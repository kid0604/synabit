import { marked } from 'marked';
import DOMPurify from 'dompurify';

/**
 * A text box's words, as the HTML to show.
 *
 * A text box holds Markdown in its `label` — the same text everything else
 * reads (search, Syn, the previews), and a plain-text label from before is
 * already valid Markdown. Shown, it is rendered and then sanitised down to
 * the handful of elements a note on a board needs: no scripts, no styles, no
 * images pulled from the network, no forms.
 */
const ALLOWED_TAGS = [
  'p', 'br', 'strong', 'em', 'b', 'i', 's', 'del', 'code', 'pre', 'blockquote', 'hr',
  'ul', 'ol', 'li', 'h1', 'h2', 'h3', 'h4', 'a', 'input',
];

export function renderRichText(markdown: string): string {
  if (!markdown.trim()) return '';
  const html = marked.parse(markdown, { async: false, breaks: true, gfm: true }) as string;
  return DOMPurify.sanitize(html, {
    ALLOWED_TAGS,
    // Task-list checkboxes come through as disabled inputs; nothing else may.
    ALLOWED_ATTR: ['href', 'title', 'type', 'checked', 'disabled'],
    ALLOWED_URI_REGEXP: /^(?:https?|mailto|synabit):/i,
  });
}
