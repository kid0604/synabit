import { Extension } from '@tiptap/core';

/**
 * A blank line after a block the note keeps as HTML, put back when a note is
 * opened.
 *
 * Toggles, PDFs, maps, boards, audio, video and galleries are saved as HTML,
 * and until 2026-10-15 their serializers ended without a blank line. In
 * markdown an HTML block runs until a blank line, so the paragraph after one
 * became part of it: "Thread về việc rà soát domain, TLS cert: <https://…>"
 * was read back as raw HTML, `<https://…>` as an unknown tag, and the link
 * was gone on the device that opened the note — and gone for good once that
 * device saved it. The serializers now close the block (`closeBlock`); this is
 * for the notes already written the old way, eleven in the author's vault.
 *
 * Only after a line that closes a block — `</details>`, `</div>`, or one of the
 * single-line embeds — and only when the next line is text rather than more
 * HTML. An autolink is text: `<https://…>` has no tag name. Never inside a
 * fenced code block, where the lines are what they say.
 */
const CLOSES_A_BLOCK = /^\s*(<\/details>|<\/div>|<(div|audio|video|iframe)\b[^>]*>\s*<\/\2>)\s*$/i;
const STARTS_A_TAG = /^\s*<\/?[a-z][\w-]*(\s|>|\/|$)/i;
const FENCE = /^\s*(```|~~~)/;
const INSTALLED = Symbol('htmlBlockBreaks');

export function separateHtmlBlocks(markdown: string): string {
  const lines = markdown.split('\n');
  const out: string[] = [];
  let fence: string | null = null;
  lines.forEach((line, i) => {
    out.push(line);
    const opens = line.match(FENCE)?.[1];
    if (opens) fence = fence === null ? opens : fence === opens ? null : fence;
    if (fence !== null) return;
    const next = lines[i + 1];
    if (next === undefined || next.trim() === '') return;
    if (CLOSES_A_BLOCK.test(line) && !STARTS_A_TAG.test(next)) out.push('');
  });
  return out.join('\n');
}

export const HtmlBlockBreaks = Extension.create({
  name: 'htmlBlockBreaks',

  addStorage() {
    return {
      markdown: {
        parse: {
          setup(markdownit: any) {
            // Every parse runs this on the same instance; once is enough.
            // See `richTablePlugin`.
            if (markdownit[INSTALLED]) return;
            markdownit[INSTALLED] = true;
            markdownit.core.ruler.before('normalize', 'html_block_breaks', (state: any) => {
              state.src = separateHtmlBlocks(state.src);
            });
          },
        },
      },
    };
  },
});
