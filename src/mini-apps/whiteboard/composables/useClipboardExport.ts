import { type Ref } from 'vue';
import { nextTick } from 'vue';
import { useVueFlow, getRectOfNodes } from '@vue-flow/core';
import { toPng } from 'html-to-image';
import type { WBNode } from './useWhiteboardStore';
import { logger } from '../../../utils/logger';

export function useClipboardExport(
  store: any,
  vfNodes: Ref<any[]>,
  vfEdges: Ref<any[]>,
  addNodeToCanvas: (node: WBNode) => void,
  scheduleSave: () => void,
) {
  const { setViewport, getViewport, getNodes } = useVueFlow({ id: 'whiteboard-flow' });

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

  async function exportPng() {
    const el = document.querySelector('.vue-flow') as HTMLElement;
    const nodes = getNodes.value;
    if (!el || nodes.length === 0) return;
    try {
      const nodesBounds = getRectOfNodes(nodes);
      const padding = 50;
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
      setViewport(prevViewport);

      // Trigger download
      const link = document.createElement('a');
      link.download = `${store.currentBoardData.value?.title || 'whiteboard'}.png`;
      link.href = dataUrl;
      link.click();
    } catch (err) {
      logger.error('Export PNG failed', err as string);
    }
  }

  return { copySelected, pasteClipboard, exportPng };
}
