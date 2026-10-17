/**
 * An Eisenhower matrix in a note's Markdown:
 *
 *     <!-- eisenhower -->
 *
 *     #### Làm ngay
 *
 *     - Gọi lại cho ngân hàng
 *     - [Nộp thuế quý 3](synabit://task/Tasks/a1b2.md)
 *
 *     #### Lên kế hoạch
 *
 *     #### Ủy thác
 *
 *     #### Loại bỏ
 *
 *     <!-- /eisenhower -->
 *
 * Any other Markdown reader shows four headings and their lists. The quadrant
 * a heading opens is its place in the order, not its words, so a heading
 * written in another language — or retyped by hand — still opens the same
 * quadrant.
 *
 * Inside, a paragraph that is nothing but a link to a task is the task itself,
 * drawn live (`EisenhowerTask`); every other line is text.
 */
import type MarkdownIt from 'markdown-it';
import { QUADRANTS } from '../../task/quadrant';

const INSTALLED = Symbol('eisenhower');

export const OPEN_COMMENT = '<!-- eisenhower -->';
export const CLOSE_COMMENT = '<!-- /eisenhower -->';
export const TASK_HREF = 'synabit://task/';

const isOpen = (html: string) => /^<!--\s*eisenhower\s*-->\s*$/.test(html.trim());
const isClose = (html: string) => /^<!--\s*\/eisenhower\s*-->\s*$/.test(html.trim());

function escapeAttribute(value: string): string {
  return value.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

const escapeText = (value: string) => value.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

/** A task's address in a link: its path, with what would end a Markdown link destination encoded. */
export function taskHref(id: string): string {
  return TASK_HREF + encodeURI(id).replace(/\(/g, '%28').replace(/\)/g, '%29');
}

/** The task a link points at, or `null` for a link to anything else. */
export function taskIdFromHref(href: string): string | null {
  if (!href.startsWith(TASK_HREF)) return null;
  const raw = href.slice(TASK_HREF.length);
  if (!raw) return null;
  try {
    return decodeURIComponent(raw);
  } catch {
    return raw;
  }
}

/** Link text that reads back as the same words, whatever punctuation the title has. */
export function escapeLinkText(title: string): string {
  return title.replace(/[\\`*_[\]<>]/g, (c) => `\\${c}`);
}

const cellOpen = (index: number) =>
  `<div data-type="eisenhower-cell" data-quadrant="${QUADRANTS[index]}">`;

/**
 * The task an inline run stands for, when the run is one link to a task and
 * nothing else — the shape `EisenhowerTask` is written in.
 */
function soleTaskLink(children: any[]): { id: string; title: string } | null {
  // Blank text at either end is not more than the link; inside it, a space
  // between two escapes is part of the title.
  const blank = (c: any) => c.type === 'softbreak' || (c.type === 'text' && c.content.trim() === '');
  let from = 0;
  let to = children.length;
  while (from < to && blank(children[from])) from++;
  while (to > from && blank(children[to - 1])) to--;
  const meaningful = children.slice(from, to);
  if (meaningful.length < 2) return null;
  const first = meaningful[0];
  const last = meaningful[meaningful.length - 1];
  if (first.type !== 'link_open' || last.type !== 'link_close') return null;
  const inner = meaningful.slice(1, -1);
  if (inner.some((c) => c.type === 'link_open' || c.type === 'link_close')) return null;
  const id = taskIdFromHref(first.attrGet('href') ?? '');
  if (!id) return null;
  // `text_special` is a backslash escape, not yet joined into the text
  // around it at this point of the parse.
  const title = inner.map((c) => (c.type === 'text' || c.type === 'text_special' || c.type === 'code_inline' ? c.content : '')).join('');
  return { id, title };
}

export function eisenhowerPlugin(md: MarkdownIt): void {
  // tiptap-markdown runs every extension's setup on every parse, against the
  // same instance; see `richTablePlugin`.
  const flagged = md as MarkdownIt & { [INSTALLED]?: boolean };
  if (flagged[INSTALLED]) return;
  flagged[INSTALLED] = true;

  // After `inline`, so the links inside are already tokens to look at.
  md.core.ruler.after('inline', 'eisenhower', (state) => {
    const tokens = state.tokens;
    for (let i = 0; i < tokens.length; i++) {
      const open = tokens[i];
      if (open.type !== 'html_block' || !isOpen(open.content)) continue;
      let close = i + 1;
      while (close < tokens.length && !(tokens[close].type === 'html_block' && tokens[close].level === open.level && isClose(tokens[close].content))) close++;
      // An opening with no closing is left alone: a comment, invisible.
      if (close >= tokens.length) continue;

      const isBoundary = (t: any) => t.type === 'heading_open' && t.tag === 'h4' && t.level === open.level;
      let cells = 0;
      open.content = '<div data-type="eisenhower">';
      // Lines before the first heading — written by hand — belong to the
      // first quadrant rather than to no quadrant at all.
      if (i + 1 < close && !isBoundary(tokens[i + 1])) {
        open.content += cellOpen(0);
        cells = 1;
      }
      for (let j = i + 1; j < close; j++) {
        const t = tokens[j];
        if (isBoundary(t) && cells < QUADRANTS.length) {
          // The heading becomes the boundary between two quadrants; its
          // words are the label, which the editor draws for itself.
          const html = `${cells > 0 ? '</div>' : ''}${cellOpen(cells)}`;
          cells++;
          const boundary = new state.Token('html_block', '', 0);
          boundary.content = html;
          boundary.level = t.level;
          boundary.block = true;
          tokens.splice(j, 3, boundary);
          close -= 2;
          continue;
        }
        if (t.type === 'inline' && t.children) {
          const task = soleTaskLink(t.children);
          if (task) {
            const chip = new state.Token('html_inline', '', 0);
            chip.content = `<span data-type="eisenhower-task" data-id="${escapeAttribute(task.id)}" data-title="${escapeAttribute(task.title)}">${escapeText(task.title)}</span>`;
            t.children = [chip];
          }
        }
      }
      let end = cells > 0 ? '</div>' : '';
      for (let k = cells; k < QUADRANTS.length; k++) end += `${cellOpen(k)}</div>`;
      tokens[close].content = `${end}</div>`;
      i = close;
    }
  });
}
