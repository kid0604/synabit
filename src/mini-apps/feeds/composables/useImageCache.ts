import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { logger } from '../../../utils/logger';

/**
 * Article images, fetched by the app instead of by the page.
 *
 * A remote `<img>` in the reader tells the publisher when the article was
 * opened and from where, and a one-pixel image exists for no other reason. It
 * also means an article read offline has no pictures. So the backend fetches
 * them once into its own cache and hands back local paths, and the content
 * security policy no longer allows the webview to reach a publisher at all.
 *
 * The map is module-level: the same picture appears across articles and across
 * both panes, and it should be resolved once per session.
 */
const resolved = new Map<string, string>();

/** Whether a URL is something we would have to leave the app to load. */
function isRemote(url: string): boolean {
  return url.startsWith('http://') || url.startsWith('https://');
}

export function useImageCache() {
  /**
   * Resolve a batch of remote URLs to local ones.
   *
   * URLs that could not be cached — a 404, something that was never an image,
   * something enormous — are remembered as failures so the next article using
   * the same picture does not queue the same doomed request again.
   */
  async function resolveAll(urls: string[]): Promise<Map<string, string>> {
    const wanted = urls.filter(isRemote);
    const missing = [...new Set(wanted.filter(url => !resolved.has(url)))];

    if (missing.length > 0) {
      try {
        const paths = await invoke<Record<string, string>>('feed_cache_images', { urls: missing });
        for (const url of missing) {
          const path = paths[url];
          resolved.set(url, path ? convertFileSrc(path) : '');
        }
      } catch (e) {
        logger.error('Failed to cache article images', e);
      }
    }

    const out = new Map<string, string>();
    for (const url of wanted) {
      const local = resolved.get(url);
      if (local) out.set(url, local);
    }
    return out;
  }

  async function resolveOne(url: string): Promise<string> {
    if (!url) return '';
    if (!isRemote(url)) return url;
    return (await resolveAll([url])).get(url) ?? '';
  }

  /**
   * Point every image in a piece of article HTML at the local cache.
   *
   * Parsed rather than pattern-matched: the markup has already been sanitized
   * in Rust, so this is reading attributes off a document, not trusting one.
   * An image that could not be cached is dropped — with the policy tightened
   * it could not load anyway, and a broken-image icon is worse than no image.
   */
  async function rewriteImages(html: string): Promise<string> {
    if (!html.includes('<img')) return html;

    const doc = new DOMParser().parseFromString(html, 'text/html');
    const images = Array.from(doc.querySelectorAll('img[src]'));
    const sources = images
      .map(img => img.getAttribute('src') ?? '')
      .filter(isRemote);
    if (sources.length === 0) return html;

    const local = await resolveAll(sources);
    for (const img of images) {
      const src = img.getAttribute('src') ?? '';
      if (!isRemote(src)) continue;
      const cached = local.get(src);
      if (cached) img.setAttribute('src', cached);
      else img.remove();
    }

    return doc.body.innerHTML;
  }

  return { resolveOne, resolveAll, rewriteImages };
}
