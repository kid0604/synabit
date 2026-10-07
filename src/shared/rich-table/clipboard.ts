/**
 * Cells in and out of the clipboard. Excel, Sheets and Numbers all put a
 * selection on it twice: as tab-separated text and as an HTML `<table>`. The
 * HTML is read first when there is one, because it is the only form that
 * keeps a cell holding a line break, or a tab, in one piece.
 */

/** Tab-separated text, quoted the way spreadsheets quote it. */
export function toTsv(grid: string[][]): string {
  const cell = (v: string) => (/[\t\n"]/.test(v) ? `"${v.replace(/"/g, '""')}"` : v);
  return grid.map((row) => row.map(cell).join('\t')).join('\n');
}

export function parseTsv(text: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let cell = '';
  let quoted = false;
  const s = text.replace(/\r\n?/g, '\n').replace(/\n$/, '');
  for (let i = 0; i < s.length; i++) {
    const ch = s[i];
    if (quoted) {
      if (ch === '"' && s[i + 1] === '"') { cell += '"'; i++; }
      else if (ch === '"') quoted = false;
      else cell += ch;
    } else if (ch === '"' && cell === '') {
      quoted = true;
    } else if (ch === '\t') {
      row.push(cell);
      cell = '';
    } else if (ch === '\n') {
      row.push(cell);
      rows.push(row);
      row = [];
      cell = '';
    } else {
      cell += ch;
    }
  }
  row.push(cell);
  rows.push(row);
  return rows;
}

const escapeHtml = (s: string) =>
  s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

export function toHtml(grid: string[][], header?: string[]): string {
  const row = (cells: string[], tag: 'td' | 'th') =>
    `<tr>${cells.map((c) => `<${tag}>${escapeHtml(c).replace(/\n/g, '<br>')}</${tag}>`).join('')}</tr>`;
  const head = header ? `<thead>${row(header, 'th')}</thead>` : '';
  return `<table>${head}<tbody>${grid.map((r) => row(r, 'td')).join('')}</tbody></table>`;
}

/**
 * The cells of the first `<table>` in some HTML, or `null` when it has none.
 * A cell's paragraphs and `<br>`s become line breaks; merged cells are
 * spread back out, so the grid stays rectangular.
 */
export function parseHtmlTable(html: string): string[][] | null {
  const doc = new DOMParser().parseFromString(html, 'text/html');
  const table = doc.querySelector('table');
  if (!table) return null;
  const grid: string[][] = [];
  Array.from(table.rows).forEach((tr, r) => {
    grid[r] ??= [];
    let c = 0;
    for (const td of Array.from(tr.cells)) {
      while (grid[r][c] !== undefined) c++;
      const text = cellText(td);
      const rowSpan = Math.max(1, td.rowSpan || 1);
      const colSpan = Math.max(1, td.colSpan || 1);
      for (let dr = 0; dr < rowSpan; dr++) {
        grid[r + dr] ??= [];
        for (let dc = 0; dc < colSpan; dc++) grid[r + dr][c + dc] = dr || dc ? '' : text;
      }
      c += colSpan;
    }
  });
  const width = Math.max(0, ...grid.map((r) => r.length));
  return grid.map((r) => Array.from({ length: width }, (_, i) => r[i] ?? ''));
}

function cellText(el: Element): string {
  const parts: string[] = [];
  const walk = (node: Node) => {
    if (node.nodeType === Node.TEXT_NODE) {
      parts.push((node.textContent ?? '').replace(/\s+/g, ' '));
      return;
    }
    if (!(node instanceof Element)) return;
    if (node.tagName === 'BR') { parts.push('\n'); return; }
    const block = /^(P|DIV|LI|H[1-6])$/.test(node.tagName);
    if (block && parts.length && !parts[parts.length - 1].endsWith('\n')) parts.push('\n');
    node.childNodes.forEach(walk);
    if (block) parts.push('\n');
  };
  el.childNodes.forEach(walk);
  return parts.join('').split('\n').map((l) => l.trim()).join('\n').trim();
}

/** What a paste brings: HTML table when there is one, else tab-separated text. */
export function readClipboard(data: Pick<DataTransfer, 'getData'>): string[][] | null {
  const html = data.getData('text/html');
  if (html) {
    const grid = parseHtmlTable(html);
    if (grid?.length) return grid;
  }
  const text = data.getData('text/plain');
  return text ? parseTsv(text) : null;
}

/** Does the first row of the HTML's table head it — `<th>` cells, or a `<thead>`? */
export function htmlHasHeader(html: string): boolean {
  if (!html) return false;
  const doc = new DOMParser().parseFromString(html, 'text/html');
  const first = doc.querySelector('table')?.rows[0];
  if (!first) return false;
  if (first.parentElement?.tagName === 'THEAD') return true;
  const cells = Array.from(first.cells);
  return cells.length > 0 && cells.every((c) => c.tagName === 'TH');
}
