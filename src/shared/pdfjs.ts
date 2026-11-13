/**
 * pdf.js, the one way the app loads it.
 *
 * **The legacy build, not the standard one.** pdfjs-dist 6's standard build
 * calls `Promise.try` (Safari 18.2), `URL.parse` (18) and `Uint8Array#toHex`
 * (18.2) on the way to opening any document, in the page and in the worker
 * alike, so `getDocument` threw before a page was drawn. The legacy build is
 * the same code with core-js added by Mozilla, which supplies a missing method
 * and leaves alone one the WebView already has. The worker has to be the legacy
 * one too, since it is copied as it is, not compiled for `build.target`.
 *
 * **Its floor is the app's.** From 6.1 the legacy build no longer covers Safari
 * 16.4: `ArrayBuffer#transferToFixedLength` (17.4), used for every font, has no
 * stand-in, and pages drew with no text. That is why `minimumSystemVersion` is
 * 14.4 (Safari 17.4) — see CLAUDE.md. Before upgrading pdf.js, check what its
 * legacy build stops covering.
 *
 * The ESLint config sends every other `pdfjs-dist` import here, so a new
 * viewer cannot pick up the standard build by importing the package's name.
 */
import * as pdfjsLib from 'pdfjs-dist/legacy/build/pdf.mjs';
import type { PDFPageProxy } from 'pdfjs-dist/legacy/build/pdf.mjs';
import workerUrl from 'pdfjs-dist/legacy/build/pdf.worker.min.mjs?url';

pdfjsLib.GlobalWorkerOptions.workerSrc = workerUrl;

export { pdfjsLib };
export type { PDFDocumentProxy, PDFPageProxy } from 'pdfjs-dist/legacy/build/pdf.mjs';

export type TextContent = Awaited<ReturnType<PDFPageProxy['getTextContent']>>;

/**
 * A page's text, as `page.getTextContent()` gives it.
 *
 * `getTextContent()` reads its stream with `for await`, and no Safari can
 * iterate a `ReadableStream` that way (WebKit bug 194379; not in Safari 26
 * either), legacy build or not. Reading the same stream with a reader needs
 * nothing the WebView lacks, so nothing has to patch `ReadableStream` for it.
 */
export async function pageText(page: PDFPageProxy): Promise<TextContent> {
  const content: TextContent = { items: [], styles: Object.create(null), lang: null };
  const reader = page.streamTextContent().getReader();
  for (;;) {
    const { done, value } = await reader.read();
    if (done) return content;
    content.lang ??= value.lang;
    Object.assign(content.styles, value.styles);
    content.items.push(...value.items);
  }
}
