import { provide, ref, type InjectionKey, type Ref } from 'vue';
import { nextTick } from 'vue';
import { useVueFlow } from '@vue-flow/core';
import { toJpeg, toPng, toSvg } from 'html-to-image';
import { dataUrlBytes, jpegToPdf } from '../pdf';
import { assetDataUri, rotatedOverhang } from '../imageAssets';
import { logger } from '../../../utils/logger';
import { inkBox, unionBox } from '../inkLayer';
import { slidesOf } from '../presentation';
import { sharePage, svgFromDataUrl, type SharePageInput } from '../sharePage';

/**
 * What an export shares with the ink layer, which the canvas places but this
 * composable cannot reach: whether a picture is being taken (so the layer
 * leaves its selection box out of it), and which loose ink is selected (so it
 * is selected again afterwards — the canvas's own selection does not have it).
 */
export interface ExportInkLink {
  capturing: Ref<boolean>;
  selectedInk: (read: (() => string[]) | null) => void;
}
export const EXPORT_INK: InjectionKey<ExportInkLink> = Symbol('whiteboard-export-ink');

/**
 * The items an export pictures, out of what the canvas holds: those asked
 * for, less comments (notes for whoever edits the board, not part of it) and
 * what a folded mind-map branch hides — neither is drawn, so neither may
 * widen the picture, and a hidden item is never measured.
 */
export function exportable<T extends { id: string; type?: string; hidden?: boolean }>(nodes: T[], only: Set<string> | null): T[] {
  return nodes.filter((n) => n.type !== 'comment' && !n.hidden && (!only || only.has(n.id)));
}

export function useClipboardExport(
  store: any,
  vaultPath: Ref<string>,
  canvasEl: () => HTMLElement | null,
  page?: { lang: () => string; labels: () => SharePageInput['labels'] },
  /**
   * Select these again after the export. The canvas's own reselect finds only
   * what it still holds, and selected ink goes back to the ink layer the
   * moment the export clears the selection.
   */
  reselect?: (ids: string[]) => void,
  /** Tell the person an export did not happen; it used to fail without a word. */
  onFail?: () => void,
) {
  const { setViewport, getViewport, getNodes, getSelectedNodes, removeSelectedElements, addSelectedNodes, findNode } =
    useVueFlow({ id: 'whiteboard-flow' });

  /**
   * True while a PNG is being taken.
   *
   * The canvas only builds the items that are on screen, which is what keeps
   * a large board responsive — and it is exactly wrong for an export, where
   * the point is to capture the parts that are not on screen. The canvas
   * binds this to turn culling off for the duration.
   */
  const isExporting = ref(false);
  /** While exporting part of the board: the items in the picture. */
  const exportOnly = ref<Set<string> | null>(null);
  /**
   * True for the whole of every export, the quick one included — unlike
   * `isExporting`, which only the whole-board way through sets.
   */
  const capturing = ref(false);
  let readSelectedInk: (() => string[]) | null = null;
  provide(EXPORT_INK, { capturing, selectedInk: (read) => { readSelectedInk = read; } });
  /**
   * Exports run one after another. Two at once — Syn looking at a sketch
   * while the person saves a PNG — each moved the view and turned culling on
   * and off under the other.
   */
  let queue: Promise<unknown> = Promise.resolve();

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

  /**
   * Write every SVG mark's paint into its own `style`, as the browser resolved it.
   *
   * The screenshot is a clone of the document, and the clone loses what the
   * marks inherit through the canvas: the theme's edge grey, the ink colour,
   * an arrowhead's colour. Exporting used to paper over that by forcing every
   * edge to one grey at one width, which threw away the colours and widths
   * the user had chosen. Resolving each mark's own paint keeps them.
   *
   * Returns the undo, because these are the live elements on screen.
   */
  function freezeSvgPaint(el: HTMLElement): () => void {
    const marks = Array.from(el.querySelectorAll<SVGElement>('path, polyline, polygon, rect, line, circle, ellipse, text'));
    // Every read first, then every write. Interleaved, each read after a
    // write made the page work its styles out again — once per mark, which on
    // a drawing of thousands of strokes was seconds.
    const paints = marks.map((mark) => {
      const computed = getComputedStyle(mark);
      return {
        mark,
        original: mark.getAttribute('style'),
        fill: computed.fill,
        stroke: computed.stroke,
        strokeWidth: computed.strokeWidth,
        opacity: computed.opacity,
      };
    });
    for (const p of paints) {
      p.mark.style.fill = p.fill;
      p.mark.style.stroke = p.stroke;
      p.mark.style.strokeWidth = p.strokeWidth;
      p.mark.style.opacity = p.opacity;
    }
    return () => {
      for (const p of paints) {
        if (p.original === null) p.mark.removeAttribute('style');
        else p.mark.setAttribute('style', p.original);
      }
    };
  }

  /**
   * How much finer than the screen the picture is drawn.
   *
   * Twice, for a sharp picture — unless that makes a canvas bigger than the
   * webview will draw. WebKit gives back a blank image rather than an error
   * past its limit, so a large board is drawn at whatever scale fits.
   */
  function pixelRatioFor(width: number, height: number): number {
    const MAX_SIDE = 16384;
    const MAX_AREA = 100_000_000;
    return Math.max(
      0.25,
      Math.min(2, MAX_SIDE / width, MAX_SIDE / height, Math.sqrt(MAX_AREA / (width * height))),
    );
  }

  /**
   * Save the board — or some of it — as a picture or a document.
   *
   * `only` limits it to those items (a selection, a frame and what is in it):
   * the picture is cropped to them and everything else is left out of it,
   * lines included unless both their ends are in.
   */
  /**
   * A turned item reaches past the box the bounds are measured from, so
   * cropping to those bounds would cut its corners off: the margin is widened
   * by the worst overhang — the same on every side, because a picture is
   * turned about its own middle.
   */
  function worstOverhang(only: Set<string> | null): number {
    return Math.ceil(((store.currentBoardData.value?.nodes ?? []) as any[])
      .filter((n) => n.data?.rotation && n.type !== 'comment' && (!only || only.has(n.id)))
      .reduce((worst: number, n: any) => Math.max(worst, rotatedOverhang(n.data.width || 320, n.data.height || 240, n.data.rotation)), 0));
  }

  /** Where some items are, from the board and what the canvas has measured: enough to bring them into view. */
  function roughBounds(only: Set<string>) {
    const boxes = ((store.currentBoardData.value?.nodes ?? []) as any[])
      .filter((n) => only.has(n.id) && n.type !== 'comment')
      .map((n) => (n.type === 'stroke' ? inkBox(n) : {
        x: n.position.x,
        y: n.position.y,
        width: findNode(n.id)?.dimensions?.width || Number(n.data?.width) || 160,
        height: findNode(n.id)?.dimensions?.height || Number(n.data?.height) || 80,
      }));
    return unionBox(boxes);
  }

  type ExportOptions = { format?: 'png' | 'svg' | 'pdf' | 'html'; transparent?: boolean; only?: string[]; keep?: boolean };

  function exportBoard(options: ExportOptions = {}): Promise<string | undefined> {
    const run = queue.then(() => exportNow(options));
    queue = run.catch(() => {});
    return run;
  }

  async function exportNow(options: ExportOptions): Promise<string | undefined> {
    const el = canvasEl();
    if (!el) return;
    const format = options.format ?? 'png';
    const only = options.only?.length ? new Set(options.only) : null;
    // A PDF page has no "transparent", and a shared page shows the board as
    // it looks.
    const transparent = format !== 'pdf' && format !== 'html' && !!options.transparent;
    const edgesIn = new Set(
      ((store.currentBoardData.value?.edges ?? []) as any[])
        .filter((e) => !only || (only.has(e.source) && only.has(e.target)))
        .map((e) => e.id),
    );
    // What was selected, put back afterwards: the picture is of the board,
    // not of its selection outlines and handles.
    // Loose ink is not the canvas's to report: a large selection of it stays
    // on the ink layer, which says what of it is selected.
    const wasSelected = [...new Set([...getSelectedNodes.value.map((n) => n.id), ...(readSelectedInk?.() ?? [])])];

    // Everything the export changes on the live canvas, undone in `finally`
    // whether or not the picture was taken.
    let prevViewport: ReturnType<typeof getViewport> | null = null;
    let restoreImages: (() => void) | null = null;
    let restorePaint: (() => void) | null = null;
    let culledOff = false;

    try {
      capturing.value = true;
      removeSelectedElements();
      exportOnly.value = only;
      // 0. Part of the board that fits on screen — one sticky copied as a
      //    picture, a frame, what Syn is shown of a sketch — is brought into
      //    view and taken as the canvas has it, built only as far as the
      //    screen goes. Building the whole board for it took as long on a
      //    board of thousands of items as exporting all of them did.
      const near = only ? roughBounds(only) : null;
      const margin = 2 * (50 + worstOverhang(only));
      const fits = !!near && near.width + margin <= el.clientWidth && near.height + margin <= el.clientHeight;
      if (fits) {
        prevViewport = getViewport();
        setViewport({ x: -near!.x + margin / 2, y: -near!.y + margin / 2, zoom: 1 });
      } else {
        // Otherwise every node goes back in the document first. The canvas
        // only builds what is on screen, and `getNodes` is that same culled
        // list — so both the picture and the bounds it is measured against
        // have to be taken after culling is off, or an export of a board
        // bigger than the window is a picture of the window.
        isExporting.value = true;
        culledOff = true;
      }
      await nextTick();
      // The canvas takes the flag through its own props watcher, and the list
      // below is computed from it. One frame, so the read that follows is of
      // the whole board rather than of the board as it was a tick ago.
      await new Promise((r) => requestAnimationFrame(() => r(null)));

      // Items just put back into the document have not been measured yet,
      // and an unmeasured item has no size to crop the picture to. Wait for
      // them — briefly: the measuring is a frame or two away.
      const pick = () => exportable(getNodes.value, only);
      for (let i = 0; i < 20 && pick().some((n) => !n.dimensions?.width || !n.dimensions?.height); i++) {
        await new Promise((r) => setTimeout(r, 25));
      }
      const nodes = pick();
      // Loose ink is drawn by the ink layer, not the canvas, so the canvas's
      // list does not have it: it is measured from the board.
      const ink = ((store.currentBoardData.value?.nodes ?? []) as any[])
        .filter((n) => n.type === 'stroke' && (!only || only.has(n.id)) && !findNode(n.id))
        .map(inkBox);
      if (nodes.length === 0 && ink.length === 0) return;

      // The box the items cover, measured as drawn, or by the size the board
      // gives them where the canvas has not measured one.
      const sizeOf = (n: (typeof nodes)[number]) => ({
        w: n.dimensions?.width || Number(n.data?.width) || 160,
        h: n.dimensions?.height || Number(n.data?.height) || 80,
      });
      const nodesBounds = unionBox([
        ...nodes.map((n) => ({
          x: n.computedPosition?.x ?? n.position.x,
          y: n.computedPosition?.y ?? n.position.y,
          width: sizeOf(n).w,
          height: sizeOf(n).h,
        })),
        ...ink,
      ])!;

      // A turned item reaches past the box the bounds are measured from,
      // so cropping to those bounds would cut its corners off. Widen the
      // margin by the worst overhang on the board — the same on every side,
      // because a picture is turned about its own middle.
      const padding = 50 + worstOverhang(only);
      const exportWidth = nodesBounds.width + padding * 2;
      const exportHeight = nodesBounds.height + padding * 2;

      prevViewport ??= getViewport();

      // 1. Force the viewport to perfectly fit the export area
      setViewport({
        x: -nodesBounds.x + padding,
        y: -nodesBounds.y + padding,
        zoom: 1
      });

      // 2. Wait for VueFlow to apply transform to DOM
      await nextTick();
      await new Promise(r => setTimeout(r, 100)); // allow transitions to finish

      // 3. The background the user sees: the board's colour, or the theme's
      //    canvas under it. White behind a dark-theme board made its light
      //    ink unreadable.
      const background = transparent
        ? undefined
        : store.backgroundColor.value !== 'transparent'
          ? store.backgroundColor.value
          : getComputedStyle(el.parentElement ?? el).backgroundColor;

      restoreImages = await inlineVaultImages(el);
      restorePaint = freezeSvgPaint(el);

      // 4. Capture
      const ratio = pixelRatioFor(exportWidth, exportHeight);
      const capture = format === 'svg' || format === 'html' ? toSvg : format === 'pdf' ? toJpeg : toPng;
      const dataUrl = await capture(el, {
        backgroundColor: background,
        width: exportWidth,
        height: exportHeight,
        pixelRatio: ratio,
        quality: 0.95,
        style: {
          width: `${exportWidth}px`,
          height: `${exportHeight}px`,
          // The canvas's own fill, and the pattern drawn over it, are the
          // screen's; the picture's background is the one chosen above.
          background: 'transparent',
        },
        filter: (node) => {
          // Exclude UI controls
          if (node.classList?.contains('vue-flow__controls')) return false;
          if (node.classList?.contains('vue-flow__panel')) return false;
          // Comments are notes for whoever edits the board, not part of it.
          if (node.classList?.contains('vue-flow__node-comment')) return false;
          // The dot or line pattern belongs to the board as shown; a
          // transparent picture is meant to go on top of something else.
          if (transparent && node.classList?.contains('vue-flow__background')) return false;
          if (only && node.classList?.contains('vue-flow__node') && !only.has(node.getAttribute('data-id') ?? '')) return false;
          // An <svg> is copied whole, never looked into: each line is its own
          // <svg>, judged by the line inside it. (Loose ink is one <svg>, so
          // the ink layer draws only what is wanted instead: `exportOnly`.)
          if (only && node.classList?.contains('vue-flow__edges')) {
            const line = node.querySelector?.('.vue-flow__edge')?.getAttribute('data-id');
            if (line && !edgesIn.has(line)) return false;
          }
          if (only && node.dataset?.edgeLabel && !edgesIn.has(node.dataset.edgeLabel)) return false;
          return true;
        }
      });

      // Wanted as a picture rather than a file — for Syn to look at.
      if (options.keep) return dataUrl;

      // Trigger download
      const boardTitle: string = store.currentBoardData.value?.title ?? '';
      const title = boardTitle || 'whiteboard';
      const link = document.createElement('a');
      link.download = `${title}.${format}`;
      if (format === 'pdf') {
        // Pixels to points: a PDF page is measured at 72 to the inch, a
        // screen at 96. The pixel size is only a fallback: the PDF reads the
        // picture's real size from the JPEG, which the capture rounds and,
        // past what the webview draws, shrinks.
        const pdf = jpegToPdf(
          dataUrlBytes(dataUrl),
          Math.round(exportWidth * ratio),
          Math.round(exportHeight * ratio),
          exportWidth * 0.75,
          exportHeight * 0.75,
        );
        const url = URL.createObjectURL(new Blob([pdf as BlobPart], { type: 'application/pdf' }));
        link.href = url;
        link.click();
        setTimeout(() => URL.revokeObjectURL(url), 10_000);
      } else if (format === 'html' && page) {
        // The frames to step through, where they are in the picture.
        const left = nodesBounds.x - padding;
        const top = nodesBounds.y - padding;
        const frames = slidesOf(((store.currentBoardData.value?.nodes ?? []) as any[]).filter((n) => !only || only.has(n.id)))
          .map((f) => ({ title: f.title, box: { ...f.box, x: f.box.x - left, y: f.box.y - top } }));
        const html = sharePage({
          // Untitled boards take the page's own word for it, in its language.
          title: boardTitle,
          svg: svgFromDataUrl(dataUrl),
          width: exportWidth,
          height: exportHeight,
          background,
          frames,
          lang: page.lang(),
          labels: page.labels(),
        });
        const url = URL.createObjectURL(new Blob([html], { type: 'text/html' }));
        link.href = url;
        link.click();
        setTimeout(() => URL.revokeObjectURL(url), 10_000);
      } else {
        link.href = dataUrl;
        link.click();
      }
    } catch (err) {
      logger.error('Export failed', err as string);
      onFail?.();
    } finally {
      restorePaint?.();
      restoreImages?.();
      if (prevViewport) setViewport(prevViewport);
      if (reselect) {
        if (wasSelected.length) reselect(wasSelected);
      } else {
        const back = wasSelected.map((id) => findNode(id)).filter((n): n is NonNullable<typeof n> => !!n);
        if (back.length) addSelectedNodes(back);
      }
      // A failed export must not leave the board rendering every node for the
      // rest of the session.
      if (culledOff) isExporting.value = false;
      exportOnly.value = null;
      capturing.value = false;
    }
  }

  return { exportBoard, isExporting, exportOnly };
}
