/**
 * Teaches markdown-it to see a Rich Table: a top-level table followed by a
 * `<!-- rich-table` comment becomes one token carrying the source of both,
 * exactly as the note holds it. Without this the table would arrive as an
 * ordinary table, and the comment — which tiptap-markdown passes through a
 * DOM parser that keeps no comments — would be gone by the next save.
 *
 * Only top-level tables: inside a list or a quote the lines carry their `> `
 * and indentation, and the node is kept out of those places anyway (see
 * `RichTableExtension`).
 */
import type MarkdownIt from 'markdown-it';
import { isRichTableComment } from './markdown';

const INSTALLED = Symbol('rich-table');
const CHART_INSTALLED = Symbol('rich-chart');

/** The attribute-safe form of a source, for `data-source`. */
function escapeAttribute(value: string): string {
  return value.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

export function richTablePlugin(md: MarkdownIt): void {
  // tiptap-markdown runs every extension's setup on every parse, against the
  // same instance; a rule added each time would run once per note opened.
  const flagged = md as MarkdownIt & { [INSTALLED]?: boolean };
  if (flagged[INSTALLED]) return;
  flagged[INSTALLED] = true;

  md.core.ruler.after('block', 'rich_table', (state) => {
    const tokens = state.tokens;
    let lines: string[] | null = null;
    for (let i = 0; i < tokens.length; i++) {
      const open = tokens[i];
      if (open.type !== 'table_open' || open.level !== 0 || !open.map) continue;
      let close = i + 1;
      while (close < tokens.length && !(tokens[close].type === 'table_close' && tokens[close].level === 0)) close++;
      const comment = tokens[close + 1];
      if (!comment || comment.type !== 'html_block' || comment.level !== 0 || !comment.map) continue;
      if (!isRichTableComment(comment.content)) continue;

      lines ??= state.src.split('\n');
      const token = new state.Token('rich_table', 'div', 0);
      token.block = true;
      token.map = [open.map[0], comment.map[1]];
      token.content = lines.slice(open.map[0], comment.map[1]).join('\n').replace(/\s+$/, '');
      tokens.splice(i, close + 2 - i, token);
    }

    // A `<!-- rich-table` comment with no table to belong to — the table
    // above it did not read as one, or sits in a list. Left as an HTML block,
    // it would go through a DOM parser that keeps no comments, and the
    // table's whole configuration would be gone on the next save. Kept
    // instead, word for word, as a block of its own.
    for (const t of tokens) {
      if (t.type === 'html_block' && isRichTableComment(t.content)) {
        t.type = 'rich_table_orphan';
      }
    }
  });

  md.renderer.rules.rich_table_orphan = (tokens, idx) =>
    `<div data-type="rich-table-orphan" data-source="${escapeAttribute(tokens[idx].content.replace(/\n$/, ''))}"></div>`;

  md.renderer.rules.rich_table = (tokens, idx) =>
    `<div data-type="rich-table" data-source="${escapeAttribute(tokens[idx].content)}"></div>`;
}

/**
 * A chart of a Rich Table, placed anywhere in the note on a line of its own:
 *
 *     <!-- rich-chart of="chi-tieu" view="Theo loại" -->
 *
 * A comment, so any other Markdown reader shows nothing there. It names the
 * table by its `name:` and the view by its name; the chart is that view's.
 */
export function isRichChartComment(html: string): boolean {
  return /^<!--\s*rich-chart(\s|-->)/.test(html.trimStart());
}

/** The `key="value"` pairs of a rich-chart comment. */
/** An attribute's value as the comment holds it: no `"`, and no `--`, on which a comment could end. */
const quote = (s: string) => `"${s.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/-(?=-)/g, '&#45;')}"`;
const unquote = (s: string) => s.replace(/&#45;/g, '-').replace(/&quot;/g, '"').replace(/&amp;/g, '&');

/**
 * A chart comment's attributes. `rest` is whatever else it says — written by
 * a later Synabit, or by hand — kept word for word, to be written back.
 */
export function chartAttrs(html: string): { of: string; view: string; rest: string } {
  const out: Record<string, string> = {};
  const rest: string[] = [];
  const body = html.trim().replace(/^<!--\s*rich-chart/, '').replace(/-->\s*$/, '');
  for (const m of body.matchAll(/([\w-]+)\s*=\s*(?:"([^"]*)"|(\S+))/g)) {
    if (m[1] === 'of' || m[1] === 'view') out[m[1]] = unquote(m[2] ?? m[3] ?? '');
    else rest.push(m[0]);
  }
  return { of: out.of ?? '', view: out.view ?? '', rest: rest.join(' ') };
}

export function chartComment(of: string, view: string, rest = ''): string {
  return `<!-- rich-chart of=${quote(of)}${view ? ` view=${quote(view)}` : ''}${rest ? ` ${rest}` : ''} -->`;
}

export function richChartPlugin(md: MarkdownIt): void {
  const flagged = md as MarkdownIt & { [CHART_INSTALLED]?: boolean };
  if (flagged[CHART_INSTALLED]) return;
  flagged[CHART_INSTALLED] = true;
  md.core.ruler.after('block', 'rich_chart', (state) => {
    for (const token of state.tokens) {
      if (token.type !== 'html_block' || token.level !== 0 || !isRichChartComment(token.content)) continue;
      token.type = 'rich_chart';
      token.meta = chartAttrs(token.content);
    }
  });
  md.renderer.rules.rich_chart = (tokens, idx) => {
    const { of, view, rest } = tokens[idx].meta as { of: string; view: string; rest: string };
    return `<div data-type="rich-chart" data-of="${escapeAttribute(of)}" data-view="${escapeAttribute(view)}" data-rest="${escapeAttribute(rest)}"></div>`;
  };
}
