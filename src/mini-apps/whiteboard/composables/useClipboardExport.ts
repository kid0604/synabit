import { ref, type Ref } from 'vue';
import { nextTick } from 'vue';
import { useVueFlow, getRectOfNodes } from '@vue-flow/core';
import { toPng } from 'html-to-image';
import { assetDataUri, rotatedOverhang } from '../imageAssets';
import type { WBNode } from './useWhiteboardStore';
import { logger } from '../../../utils/logger';

export function useClipboardExport(
  store: any,
  vfNodes: Ref<any[]>,
  vfEdges: Ref<any[]>,
  addNodeToCanvas: (node: WBNode) => void,
  scheduleSave: () => void,
  vaultPath: Ref<string>,
) {
  const { setViewport, getViewport, getNodes } = useVueFlow({ id: 'whiteboard-flow' });

  /**
   * True while a PNG is being taken.
   *
   * The canvas only builds the items that are on screen, which is what keeps
   * a large board responsive — and it is exactly wrong for an export, where
   * the point is to capture the parts that are not on screen. The canvas
   * binds this to turn culling off for the duration.
   */
  const isExporting = ref(false);

  let clipboard: { type: string; data: any; position: { x: number; y: number } } | null = null;

  function copySelected() {
    const selectedNode = vfNodes.value.find((n: any) => n.selected);
    if (selectedNode) {
      clipboard = {
        type: selectedNode.type,
        data: JSON.parse(JSON.stringify(selectedNode.data)),
        position: { ...selectedNode.position },
      };
    }
  }

  function pasteClipboard() {
    if (clipboard) {
      const offset = 30;
      const newNode: WBNode = {
        id: store.generateId(clipboard.type === 'shape' ? 'sh' : 'n'),
        type: clipboard.type as any,
        position: {
          x: clipboard.position.x + offset,
          y: clipboard.position.y + offset,
        },
        data: JSON.parse(JSON.stringify(clipboard.data)),
      };
      addNodeToCanvas(newNode);
      // Move clipboard position so next paste offsets further
      clipboard.position.x += offset;
      clipboard.position.y += offset;
    }
  }

  /**
   * Put the bytes of every vault picture into the document itself.
   *
   * The screenshot is taken by cloning the document and re-fetching whatever
   * the clone points at. Vault files are served over the webview's asset
   * protocol, which the app's `connect-src` policy does not allow to be
   * fetched — so those pictures would come out of the export blank. Reading
   * them here, over the same channel the rest of the app reads files with,
   * hands the exporter something it cannot fail on.
   *
   * Returns the undo, because these are the live elements on screen.
   */
  async function inlineVaultImages(el: HTMLElement): Promise<() => void> {
    const images = Array.from(el.querySelectorAll<HTMLImageElement>('img[data-asset]'));
    const restores: Array<() => void> = [];

    await Promise.all(
      images.map(async (img) => {
        const assetPath = img.dataset.asset;
        if (!assetPath) return;
        const dataUri = await assetDataUri(vaultPath.value, assetPath);
        if (!dataUri) return;

        const original = img.getAttribute('src') ?? '';
        restores.push(() => img.setAttribute('src', original));
        img.setAttribute('src', dataUri);
        // Decoded before the capture, or the picture is still blank when the
        // screenshot is taken.
        await img.decode().catch(() => {});
      })
    );

    return () => restores.forEach((restore) => restore());
  }

  async function exportPng() {
    const el = document.querySelector('.vue-flow') as HTMLElement;
    if (!el) return;
    try {
      // 0. Put every node back in the document first. The canvas only builds
      //    what is on screen, and `getNodes` is that same culled list — so
      //    both the picture and the bounds it is measured against have to be
      //    taken after culling is off, or an export of a board bigger than
      //    the window is a picture of the window.
      isExporting.value = true;
      await nextTick();
      // The canvas takes the flag through its own props watcher, and the list
      // below is computed from it. One frame, so the read that follows is of
      // the whole board rather than of the board as it was a tick ago.
      await new Promise((r) => requestAnimationFrame(() => r(null)));

      const nodes = getNodes.value;
      if (nodes.length === 0) return;

      const nodesBounds = getRectOfNodes(nodes);

      // A turned picture reaches past the box the bounds are measured from,
      // so cropping to those bounds would cut its corners off. Widen the
      // margin by the worst overhang on the board — the same on every side,
      // because a picture is turned about its own middle.
      const overhang = ((store.currentBoardData.value?.nodes ?? []) as any[])
        .filter((n) => n.type === 'image' && n.data?.rotation)
        .reduce(
          (worst: number, n: any) =>
            Math.max(worst, rotatedOverhang(n.data.width || 320, n.data.height || 240, n.data.rotation)),
          0
        );
      const padding = 50 + Math.ceil(overhang);
      const exportWidth = nodesBounds.width + padding * 2;
      const exportHeight = nodesBounds.height + padding * 2;

      const prevViewport = getViewport();

      // 1. Force the viewport to perfectly fit the export area
      setViewport({
        x: -nodesBounds.x + padding,
        y: -nodesBounds.y + padding,
        zoom: 1
      });

      // 2. Wait for VueFlow to apply transform to DOM
      await nextTick();
      await new Promise(r => setTimeout(r, 100)); // allow transitions to finish

      // 3. Inject explicit styles to fix html-to-image dropping CSS variables
      const isDark = document.documentElement.classList.contains('dark');
      const edgeColor = isDark ? '#71717a' : '#8b8b8b';
      const bgColor = store.backgroundColor.value === 'transparent' 
        ? (isDark ? '#242424' : '#ffffff') 
        : store.backgroundColor.value;
      const textColor = isDark ? '#a1a1aa' : '#52525b';
      
      const restoreImages = await inlineVaultImages(el);

      const styleEl = document.createElement('style');
      styleEl.innerHTML = `
        .vue-flow__edge-path, .vue-flow__connection-path { stroke: ${edgeColor} !important; stroke-width: 2 !important; fill: none !important; }
        .vue-flow__edge-textbg { fill: ${bgColor} !important; }
        .vue-flow__edge-text { fill: ${textColor} !important; }
        .vue-flow__arrowhead { fill: ${edgeColor} !important; }
      `;
      el.appendChild(styleEl);

      // 4. Capture
      const dataUrl = await toPng(el, {
        backgroundColor: store.backgroundColor.value === 'transparent' ? '#ffffff' : store.backgroundColor.value,
        width: exportWidth,
        height: exportHeight,
        pixelRatio: 2,
        style: {
          width: `${exportWidth}px`,
          height: `${exportHeight}px`,
        },
        filter: (node) => {
          // Exclude UI controls
          if (node.classList?.contains('vue-flow__controls')) return false;
          if (node.classList?.contains('vue-flow__panel')) return false;
          return true;
        }
      });

      // 5. Cleanup and restore
      el.removeChild(styleEl);
      restoreImages();
      setViewport(prevViewport);
      isExporting.value = false;

      // Trigger download
      const link = document.createElement('a');
      link.download = `${store.currentBoardData.value?.title || 'whiteboard'}.png`;
      link.href = dataUrl;
      link.click();
    } catch (err) {
      logger.error('Export PNG failed', err as string);
    } finally {
      // A failed export must not leave the board rendering every node for the
      // rest of the session.
      isExporting.value = false;
    }
  }

  return { copySelected, pasteClipboard, exportPng, isExporting };
}
