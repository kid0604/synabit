/**
 * `[n]` in an answer, made into the source it names.
 *
 * # Where the numbers come from
 *
 * Retrieved context reaches the model numbered (`rag::format_context`), and
 * the prompt asks it to cite what it uses as `[2]`. After the answer,
 * `commands::syn::settle` reads the citations back, puts the cited sources
 * first under the message and moves the numbers in the text to match, so that
 * `[n]` is the n-th chip under the answer. A number that matched no source is
 * left as written and flagged under the answer by the backend.
 *
 * # Only what can be opened becomes a button
 *
 * A mark is linked only when every number in it has a source on this message.
 * While an answer streams it has no sources yet, and an older message may have
 * fewer than it cites; in both cases the text stays exactly as the model wrote
 * it. A button that opens nothing — or opens the wrong note — would be worse
 * than the plain `[3]`.
 *
 * # Why the HTML string and not the DOM
 *
 * Because that is where the bubble already turns `[[Title]]` into links, after
 * DOMPurify and before `v-html`. What this adds is built here, from a number
 * and an escaped title — never from anything the model wrote — so it does not
 * need sanitising again. Text inside `code`, `pre`, links and buttons is left
 * alone: `v[1]` in a code span is an index, not a source.
 */

import type { SourceRef } from './types';

/** Elements whose text is never a citation. */
const LEFT_ALONE = new Set(['code', 'pre', 'a', 'button', 'kbd', 'samp']);

/** `[1]`, `[12]`, `[1, 3]` — up to three digits, as the backend reads them. */
const MARK = /\[(\d{1,3}(?:\s*,\s*\d{1,3})*)\]/g;

const escapeAttr = (text: string): string =>
  text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');

/**
 * The numbers a mark cites, or `null` when it is not one this message can open.
 *
 * Refused when it is half of a wiki-link (`[[1]]`), when it is followed by `(`
 * or `]` — a link or the other half — or when any number in it has no source.
 */
export const citedIn = (
  text: string,
  at: number,
  whole: string,
  inside: string,
  sources: number,
): number[] | null => {
  const before = text[at - 1];
  const after = text[at + whole.length];
  if (before === '[' || after === ']' || after === '(') return null;
  const numbers = inside.split(',').map(n => Number.parseInt(n.trim(), 10));
  if (numbers.some(n => !Number.isInteger(n) || n < 1 || n > sources)) return null;
  return numbers;
};

/**
 * Turn the citations in rendered HTML into buttons that open their source.
 *
 * `label` names each button for somebody who cannot see which chip it matches:
 * "Source 2: Pricing decision".
 */
export const linkCitations = (
  html: string,
  sources: SourceRef[] | null | undefined,
  label: (n: number, title: string) => string,
): string => {
  if (!sources?.length || !html.includes('[')) return html;

  // Tags and the text between them, in order. The sanitised HTML has no `<`
  // inside text, so a tag is everything from `<` to the next `>`.
  const parts = html.split(/(<[^>]*>)/);
  const open: string[] = [];

  return parts
    .map(part => {
      if (part.startsWith('<')) {
        const tag = /^<\s*(\/)?\s*([a-zA-Z0-9-]+)/.exec(part);
        if (tag) {
          const name = tag[2].toLowerCase();
          if (LEFT_ALONE.has(name)) {
            if (tag[1]) {
              const i = open.lastIndexOf(name);
              if (i >= 0) open.splice(i, 1);
            } else if (!part.endsWith('/>')) {
              open.push(name);
            }
          }
        }
        return part;
      }
      if (open.length || !part.includes('[')) return part;

      return part.replace(MARK, (whole, inside: string, at: number) => {
        const numbers = citedIn(part, at, whole, inside, sources.length);
        if (!numbers) return whole;
        const buttons = numbers
          .map(n => {
            const title = sources[n - 1].title;
            return (
              `<button type="button" class="cite" data-cite="${n}"` +
              ` aria-label="${escapeAttr(label(n, title))}" title="${escapeAttr(title)}">${n}</button>`
            );
          })
          .join('');
        return `<sup class="cite-mark">${buttons}</sup>`;
      });
    })
    .join('');
};
