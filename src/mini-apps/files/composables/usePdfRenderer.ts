import { ref, shallowRef } from 'vue';
import * as pdfjsLib from 'pdfjs-dist';
import type { PDFDocumentProxy, PDFPageProxy } from 'pdfjs-dist';
import { convertFileSrc } from '@tauri-apps/api/core';

// Import worker URL from node_modules — Vite resolves this at build time
import pdfjsWorkerUrl from 'pdfjs-dist/build/pdf.worker.min.mjs?url';
pdfjsLib.GlobalWorkerOptions.workerSrc = pdfjsWorkerUrl;

// Polyfill for ReadableStream async iteration (required by pdf.js TextLayer on older WebKit/Tauri)
// @ts-expect-error - TS doesn't know about asyncIterator polyfill on ReadableStream
if (typeof ReadableStream !== 'undefined' && !ReadableStream.prototype[Symbol.asyncIterator]) {
  // @ts-expect-error
  ReadableStream.prototype[Symbol.asyncIterator] = async function* () {
    const reader = this.getReader();
    try {
      while (true) {
        const { done, value } = await reader.read();
        if (done) return;
        yield value;
      }
    } finally {
      reader.releaseLock();
    }
  };
}

export interface PageInfo {
  pageNumber: number;
  width: number;
  height: number;
}

export function usePdfRenderer() {
  const pdfDoc = shallowRef<PDFDocumentProxy | null>(null);
  const totalPages = ref(0);
  const currentPage = ref(1);
  const scale = ref(1.5);
  const isLoading = ref(false);
  const error = ref('');
  const pdfTitle = ref('');
  const pdfPath = ref('');

  /**
   * Load a PDF from a URL (asset protocol) or absolute file path.
   * If the input starts with http/https/asset, it's used directly as a URL.
   * Otherwise, it's treated as an absolute path and converted via convertFileSrc.
   */
  const loadPdf = async (pathOrUrl: string) => {
    isLoading.value = true;
    error.value = '';
    pdfPath.value = pathOrUrl;

    try {
      const filename = pathOrUrl.split('/').pop() || pathOrUrl;
      pdfTitle.value = filename.replace(/\.pdf$/i, '');

      const assetUrl = pathOrUrl.startsWith('http') || pathOrUrl.startsWith('asset')
        ? pathOrUrl
        : convertFileSrc(pathOrUrl);

      const response = await fetch(assetUrl);
      const arrayBuffer = await response.arrayBuffer();
      const bytes = new Uint8Array(arrayBuffer);

      // Served from the bundle, not from unpkg.
      //
      // These pointed at `https://unpkg.com/pdfjs-dist@5.7.284/...`, which could
      // not work: `connect-src` in tauri.conf.json does not list unpkg, so the
      // CSP blocked both fetches. A PDF needing a CMap — anything CJK — or a
      // standard font it does not embed rendered wrong, silently, with the
      // failure looking like a bad PDF rather than a blocked request. In an app
      // that describes itself as local-first they should not have been remote
      // at all, and the 169 CMaps and 16 fonts were already in public/pdfjs/,
      // shipping unused.
      //
      // Dropping the pinned version from the URL also means these no longer
      // have to be edited in step with the pdfjs-dist dependency, which is how
      // they came to name a version the package had already moved past.
      const loadingTask = pdfjsLib.getDocument({
        data: bytes,
        cMapUrl: '/pdfjs/cmaps/',
        cMapPacked: true,
        standardFontDataUrl: '/pdfjs/standard_fonts/',
      });
      const doc = await loadingTask.promise;
      pdfDoc.value = doc;
      totalPages.value = doc.numPages;
      currentPage.value = 1;
    } catch (e: any) {
      error.value = `Failed to load PDF: ${e.message || e}`;
      pdfDoc.value = null;
      totalPages.value = 0;
    } finally {
      isLoading.value = false;
    }
  };

  const getPage = async (pageNum: number): Promise<PDFPageProxy | null> => {
    if (!pdfDoc.value || pageNum < 1 || pageNum > totalPages.value) return null;
    return pdfDoc.value.getPage(pageNum);
  };

  const renderPage = async (
    pageNum: number,
    canvas: HTMLCanvasElement,
    textLayerDiv?: HTMLDivElement
  ) => {
    const page = await getPage(pageNum);
    if (!page) return;

    const viewport = page.getViewport({ scale: scale.value });
    const dpr = window.devicePixelRatio || 1;

    canvas.width = viewport.width * dpr;
    canvas.height = viewport.height * dpr;
    canvas.style.width = `${viewport.width}px`;
    canvas.style.height = `${viewport.height}px`;

    const ctx = canvas.getContext('2d')!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

    await page.render({ canvasContext: ctx, viewport, canvas } as any).promise;

    // Render text layer for selection
    if (textLayerDiv) {
      textLayerDiv.innerHTML = '';
      textLayerDiv.style.width = `${viewport.width}px`;
      textLayerDiv.style.height = `${viewport.height}px`;

      // pdfjs v5 TextLayer uses CSS var --total-scale-factor for sizing.
      // In the full pdfjs viewer this is set by PDFPageView; we must set it ourselves.
      // It must be exactly the logical scale (do NOT multiply by dpr, CSS handles that).
      textLayerDiv.style.setProperty('--total-scale-factor', `${scale.value}`);
      textLayerDiv.style.setProperty('--scale-round-x', '1px');
      textLayerDiv.style.setProperty('--scale-round-y', '1px');

      const textContent = await page.getTextContent();
      const { TextLayer } = await import('pdfjs-dist');
      const textLayer = new TextLayer({
        textContentSource: textContent,
        container: textLayerDiv,
        viewport,
      });
      await textLayer.render();
    }
  };

  const goToPage = (page: number) => {
    if (page >= 1 && page <= totalPages.value) {
      currentPage.value = page;
    }
  };

  const nextPage = () => goToPage(currentPage.value + 1);
  const prevPage = () => goToPage(currentPage.value - 1);

  const setScale = (newScale: number) => {
    scale.value = Math.max(0.5, Math.min(4, newScale));
  };

  const zoomIn = () => setScale(scale.value + 0.25);
  const zoomOut = () => setScale(scale.value - 0.25);
  const fitWidth = (containerWidth: number) => {
    // Will be called after getting page dimensions
    if (!pdfDoc.value) return;
    pdfDoc.value.getPage(currentPage.value).then(page => {
      const viewport = page.getViewport({ scale: 1 });
      setScale(containerWidth / viewport.width);
    });
  };

  const cleanup = () => {
    if (pdfDoc.value) {
      // `PDFDocumentProxy.destroy()` is gone in pdfjs-dist 6; teardown moved to
      // the loading task, which is what owns the worker. Caught rather than
      // ignored: it returns a promise, and a rejected one thrown away here
      // surfaces later as an unhandled rejection with no context attached.
      pdfDoc.value.loadingTask.destroy().catch(() => {});
      pdfDoc.value = null;
    }
    totalPages.value = 0;
    currentPage.value = 1;
    error.value = '';
    pdfTitle.value = '';
    pdfPath.value = '';
  };

  return {
    pdfDoc,
    totalPages,
    currentPage,
    scale,
    isLoading,
    error,
    pdfTitle,
    pdfPath,
    loadPdf,
    getPage,
    renderPage,
    goToPage,
    nextPage,
    prevPage,
    setScale,
    zoomIn,
    zoomOut,
    fitWidth,
    cleanup,
  };
}
