/**
 * A text cell as HTML: its inline Markdown rendered, its wikilinks made links.
 *
 * Most cells hold plain words, and those skip Markdown entirely — a table of
 * two thousand rows would otherwise run the parser twenty thousand times on
 * every render. What is rendered is remembered by its text.
 */
import { marked } from 'marked';
import DOMPurify from 'dompurify';

const MARKUP = /[*_`~[\]<>\\!]|https?:/;
const cache = new Map<string, string>();
const LIMIT = 5000;

const escapeHtml = (s: string) =>
  s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

export function cellHtml(raw: string): string {
  let html = cache.get(raw);
  if (html !== undefined) return html;
  if (!MARKUP.test(raw)) {
    html = escapeHtml(raw).replace(/\n/g, '<br>');
  } else {
    const linked = raw.replace(/\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g, (_, target: string, alias?: string) =>
      `<a class="rt-wikilink" data-note="${escapeHtml(target.trim())}">${escapeHtml((alias ?? target).trim())}</a>`);
    const rendered = marked.parseInline(linked.replace(/\n/g, '  \n'), { async: false }) as string;
    html = DOMPurify.sanitize(rendered, {
      ALLOWED_TAGS: ['a', 'strong', 'em', 'b', 'i', 'code', 'del', 's', 'br', 'span', 'mark', 'u'],
      ALLOWED_ATTR: ['href', 'class', 'data-note'],
    });
  }
  if (cache.size >= LIMIT) cache.clear();
  cache.set(raw, html);
  return html;
}
