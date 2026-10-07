import { describe, it, expect, afterEach } from 'vitest';
import { Editor } from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import { Table, TableRow } from '@tiptap/extension-table';
import { Markdown } from 'tiptap-markdown';
import { CustomTableCell, CustomTableHeader } from '../extensions/customTable';
import { RichTableExtension, RichTableOrphan } from '../../extensions/RichTableExtension';
import { RichChartExtension } from '../../extensions/RichChartExtension';
import { InlineFormula } from '../../extensions/InlineFormula';
import { parseRichTable, serializeRichTable } from '../../../../shared/rich-table/markdown';
import { setCells } from '../../../../shared/rich-table/ops';
import { chartAttrs, isRichChartComment } from '../../../../shared/rich-table/markdownIt';

let editor: Editor | undefined;
afterEach(() => editor?.destroy());

function open(markdown: string): Editor {
  editor = new Editor({
    element: document.createElement('div'),
    extensions: [
      StarterKit,
      Markdown.configure({ html: true }),
      Table, TableRow, CustomTableCell, CustomTableHeader,
      RichTableExtension,
      RichChartExtension,
      RichTableOrphan,
    ],
    content: markdown,
  });
  return editor;
}

const markdownOf = (e: Editor) => (e.storage as any).markdown.getMarkdown() as string;

/** Hand-written, with padding and a blank line no writer of ours would produce. */
const HAND_WRITTEN = `| Khoản   | Số tiền |
|---------|--------:|
| Cà phê  |   45000 |
| Grab    |   62000 |

<!-- rich-table
columns:
  Số tiền: { type: number, format: vnd }
-->`;

const NOTE = `# Chi tiêu

Tháng mười.

${HAND_WRITTEN}

| Ordinary | Table |
| --- | --- |
| a | b |

The end.`;

describe('a Rich Table in a note', () => {
  it('comes through open and save without a byte changed', () => {
    const e = open(NOTE);
    expect(markdownOf(e)).toBe(NOTE);
  });

  it('is one node, beside an ordinary table that stays ordinary', () => {
    const e = open(NOTE);
    const types: string[] = [];
    e.state.doc.forEach((n) => types.push(n.type.name));
    expect(types).toEqual(['heading', 'paragraph', 'richTable', 'table', 'paragraph']);
    const table = parseRichTable(e.state.doc.child(2).attrs.source).table;
    expect(table.columns[1]).toMatchObject({ name: 'Số tiền', type: 'number', format: 'vnd' });
  });

  it('writes an edit, and undoes it in one step', () => {
    const e = open(NOTE);
    let pos = -1;
    e.state.doc.forEach((n, offset) => { if (n.type.name === 'richTable') pos = offset; });
    const before = e.state.doc.nodeAt(pos)!.attrs.source;
    const edited = setCells(parseRichTable(before).table, [{ row: 0, col: 1, raw: '50000' }]);
    e.commands.setRichTableSource(pos, serializeRichTable(edited));
    const saved = markdownOf(e);
    expect(saved).toContain('| Cà phê | 50000 |');
    expect(saved).toContain('Số tiền: { type: number, format: vnd }');
    expect(parseRichTable(e.state.doc.nodeAt(pos)!.attrs.source).table.rows[0][1]).toBe('50000');

    e.commands.undo();
    expect(e.state.doc.nodeAt(pos)!.attrs.source).toBe(before);
  });

  it('reads back what it wrote', () => {
    const e = open(NOTE);
    let source = '';
    e.state.doc.forEach((n) => { if (n.type.name === 'richTable') source = n.attrs.source; });
    const rewritten = serializeRichTable(parseRichTable(source).table);
    const again = open(`Before.\n\n${rewritten}\n\nAfter.`);
    expect(markdownOf(again)).toBe(`Before.\n\n${rewritten}\n\nAfter.`);
  });

  it('is inserted fresh from the slash command', () => {
    const e = open('');
    e.commands.insertRichTable(['Tên', 'Ghi chú']);
    expect(markdownOf(e)).toContain('| Tên | Ghi chú |\n| --- | --- |\n|  |  |');
    expect(markdownOf(e)).toContain('<!-- rich-table\nversion: 1\n-->');
  });

  it('stays out of quotes and lists', () => {
    const e = open(`${HAND_WRITTEN}\n\nafter`);
    e.commands.setNodeSelection(0);
    expect(e.commands.wrapIn('blockquote')).toBe(true);
    // The change was refused: the table is still where it was.
    expect(e.state.doc.child(0).type.name).toBe('richTable');
  });

  it('leaves a table inside a quote an ordinary table', () => {
    const quoted = HAND_WRITTEN.split('\n').map((l) => `> ${l}`).join('\n');
    const e = open(quoted);
    let found = false;
    e.state.doc.descendants((n) => { if (n.type.name === 'richTable') found = true; });
    expect(found).toBe(false);
  });
});

describe('formula columns in a note', () => {
  const PRICES = `| Mã | Giá |
| --- | ---: |
| A | 10 |
| B | 20 |
<!-- rich-table
name: gia
version: 1
columns:
  Giá: number
-->`;

  const ORDER = `| Mã | SL | Tiền |
| --- | ---: | --- |
| A | 2 |  |
| B | 1 |  |
<!-- rich-table
version: 1
columns:
  SL: number
  Tiền: { type: formula, expr: "LOOKUP([Mã], gia[Mã], gia[Giá], 0) * [SL]" }
-->`;

  const sourceAt = (e: Editor, index: number) => {
    const found: string[] = [];
    e.state.doc.forEach((n) => { if (n.type.name === 'richTable') found.push(n.attrs.source); });
    return found[index];
  };
  const posOf = (e: Editor, index: number) => {
    const found: number[] = [];
    e.state.doc.forEach((n, pos) => { if (n.type.name === 'richTable') found.push(pos); });
    return found[index];
  };

  it('writes stale values when asked to refresh, outside the history', () => {
    const e = open(`${PRICES}\n\n${ORDER}`);
    expect(sourceAt(e, 1)).toContain('| A | 2 |  |');
    e.commands.refreshRichTables();
    expect(sourceAt(e, 1)).toContain('| A | 2 | 20 |');
    expect(sourceAt(e, 1)).toContain('| B | 1 | 20 |');
    // Nothing to undo: nobody did anything.
    expect(e.can().undo()).toBe(false);
  });

  it('recomputes in the same step as the edit, and undoes both together', () => {
    const e = open(`${PRICES}\n\n${ORDER}`);
    e.commands.refreshRichTables();
    const before = sourceAt(e, 1);
    const order = parseRichTable(before).table;
    e.commands.setRichTableSource(posOf(e, 1), serializeRichTable(setCells(order, [{ row: 0, col: 1, raw: '5' }])));
    expect(sourceAt(e, 1)).toContain('| A | 5 | 50 |');
    e.commands.undo();
    expect(sourceAt(e, 1)).toBe(before);
  });

  it('follows a change in the table it reads', () => {
    const e = open(`${PRICES}\n\n${ORDER}`);
    e.commands.refreshRichTables();
    const prices = parseRichTable(sourceAt(e, 0)).table;
    e.commands.setRichTableSource(posOf(e, 0), serializeRichTable(setCells(prices, [{ row: 1, col: 1, raw: '25' }])));
    expect(sourceAt(e, 1)).toContain('| B | 1 | 25 |');
  });

  it('leaves a note with current values exactly as it was', () => {
    const e = open(`${PRICES}\n\n${ORDER}`);
    e.commands.refreshRichTables();
    const current = markdownOf(e);
    const again = open(current);
    again.commands.refreshRichTables();
    expect(markdownOf(again)).toBe(current);
  });

  it('does not fill a table with #NAME when the table it reads cannot be read', () => {
    const broken = PRICES.replace('version: 1', 'version: 1\ncolumns: [oops');
    const e = open(`${broken}\n\n${ORDER}`);
    e.commands.refreshRichTables();
    expect(sourceAt(e, 1)).toContain('| A | 2 |  |');
  });

  it('writes nothing when two tables share the name it reads', () => {
    const e = open(`${PRICES}\n\n${PRICES.replace('| A | 10 |', '| A | 99 |')}\n\n${ORDER}`);
    e.commands.refreshRichTables();
    expect(sourceAt(e, 2)).toContain('| A | 2 |  |');
  });

  it('leaves two tables that read each other in a ring as they are, open after open', () => {
    const ring = (me: string, other: string) => `| N | V |
| --- | --- |
| a | 1 |
<!-- rich-table
name: ${me}
version: 1
columns:
  V: { type: formula, expr: "SUM(${other}[V]) + 1" }
-->`;
    const note = `${ring('p', 'q')}\n\n${ring('q', 'p')}`;
    const e = open(note);
    e.commands.refreshRichTables();
    expect(markdownOf(e)).toBe(note);
  });

  it('does not write a clock on open, but does with an edit', () => {
    const clock = `| N | Lúc |
| --- | --- |
| a | 2020-01-01 00:00 |
<!-- rich-table
version: 1
columns:
  Lúc: { type: formula, expr: "NOW()" }
-->`;
    const e = open(clock);
    e.commands.refreshRichTables();
    expect(markdownOf(e)).toBe(clock);
    const table = parseRichTable(sourceAt(e, 0)).table;
    e.commands.setRichTableSource(posOf(e, 0), serializeRichTable(setCells(table, [{ row: 0, col: 0, raw: 'b' }])));
    expect(sourceAt(e, 0)).not.toContain('2020-01-01');
  });
});

describe('a chart of a table, elsewhere in the note', () => {
  const NOTE = `Above.

<!-- rich-chart of="chi-tieu" view="Theo loại" -->

Below.`;

  it('comes through open and save without a byte changed', () => {
    const e = open(NOTE);
    expect(markdownOf(e)).toBe(NOTE);
    let found: Record<string, unknown> | null = null;
    e.state.doc.forEach((n) => { if (n.type.name === 'richChart') found = n.attrs; });
    expect(found).toEqual({ of: 'chi-tieu', view: 'Theo loại', rest: '' });
  });

  it('is written fresh from the slash command', () => {
    const e = open('');
    e.commands.insertRichChart('gia', 'Tháng "này"');
    expect(markdownOf(e)).toContain('<!-- rich-chart of="gia" view="Tháng &quot;này&quot;" -->');
  });

  it('leaves an ordinary comment alone', () => {
    expect(isRichChartComment('<!-- rich-charts -->')).toBe(false);
    expect(chartAttrs('<!-- rich-chart of=gia view="A B" -->')).toEqual({ of: 'gia', view: 'A B', rest: '' });
  });

  it('keeps what it does not know, and a view whose name has dashes', () => {
    const note = '<!-- rich-chart of="gia" view="Q1 -- Q2" height="300" -->';
    const e = open(note);
    expect(markdownOf(e)).toBe('<!-- rich-chart of="gia" view="Q1 &#45;- Q2" height="300" -->');
    expect(markdownOf(open(markdownOf(e)))).toBe(markdownOf(e));
    expect(chartAttrs(markdownOf(e)).view).toBe('Q1 -- Q2');
  });
});

describe('a formula in a sentence', () => {
  it('shows the value of inline code that is a formula of this note, and leaves other code alone', () => {
    editor = new Editor({
      element: document.createElement('div'),
      extensions: [StarterKit, Markdown.configure({ html: true }), RichTableExtension, InlineFormula],
      content: `${HAND_WRITTEN.replace('columns:', 'name: chi\ncolumns:')}\n\nTổng \`=SUM(chi[Số tiền])\`, đếm \`=COUNT(chi[Số tiền])\`, và \`=x + 1\` là code.`,
    });
    const values = [...editor.view.dom.querySelectorAll('.rt-inline-value')].map((e) => e.textContent);
    expect(values).toHaveLength(2);
    expect(values[0]).toMatch(/107[.,]000/);
    expect(values[1]).toBe('2');
    // The note itself is unchanged: still the code.
    expect(markdownOf(editor)).toContain('`=SUM(chi[Số tiền])`');
  });
});

describe('undo after a change from outside', () => {
  it('takes back only its own edit, keeping a row Syn or sync added since', async () => {
    const { replaceFromOutside } = await import('../replaceFromOutside');
    const e = open(NOTE);
    let pos = -1;
    e.state.doc.forEach((n, at) => { if (n.type.name === 'richTable') pos = at; });
    const source = () => e.state.doc.nodeAt(pos)!.attrs.source as string;

    const edited = setCells(parseRichTable(source()).table, [{ row: 0, col: 1, raw: '50000' }]);
    e.commands.setRichTableSource(pos, serializeRichTable(edited));
    expect(source()).toContain('| Cà phê | 50000 |');

    // Syn appends a row; the note comes back through the same door a sync uses.
    const withSyn = markdownOf(e).replace('| Grab | 62000 |', '| Grab | 62000 |\n| SYN | 99 |');
    replaceFromOutside(e, (e.storage as any).markdown.parser.parse(withSyn));
    expect(source()).toContain('| SYN | 99 |');

    e.commands.undo();
    expect(source()).toContain('| SYN | 99 |');
    expect(source()).not.toContain('50000');
    expect(source()).toMatch(/Cà phê\s+\|\s+45000/);
    // Still one table: the row Syn added sits in it, not after a gap.
    expect(source()).toMatch(/62000 \|\n\| SYN \| 99 \|\n<!-- rich-table/);

    e.commands.redo();
    expect(source()).toContain('| SYN | 99 |');
    expect(source()).toContain('50000');
  });
});

describe('a Rich Table inside a Details block', () => {
  it('reads, writes back, and leaves the rest of the note editable', async () => {
    const { DetailsExtension } = await import('../../extensions/DetailsExtension');
    const note = `<details class="synabit-details">\n<summary>Chi tiết</summary>\n<div class="details-content">\n\n${HAND_WRITTEN}\n\n</div>\n</details>\n\nAfter.`;
    editor = new Editor({
      element: document.createElement('div'),
      extensions: [StarterKit, Markdown.configure({ html: true }), DetailsExtension, RichTableExtension],
      content: note,
    });
    let inside = false;
    editor.state.doc.descendants((n, _p, parent) => { if (n.type.name === 'richTable' && parent?.type.name === 'details') inside = true; });
    expect(inside).toBe(true);
    const size = editor.state.doc.content.size;
    editor.commands.insertContentAt(size, '<p>typed</p>');
    expect(markdownOf(editor)).toContain('typed');
    expect(markdownOf(editor)).toContain('<!-- rich-table');
  });
});

describe('settings whose table no longer reads as one', () => {
  const COMMENT = '<!-- rich-table\nversion: 1\ncolumns:\n  B: number\n-->';
  it.each([
    ['right under a list item', `- item\n| A | B |\n| --- | ---: |\n| x | 1 |\n${COMMENT}`],
    ['with a header and --- row of different widths', `| A | B | C |\n| --- | --- |\n| x | 1 | 2 |\n${COMMENT}`],
    ['inside a quote', `> | A | B |\n> | --- | --- |\n> | x | 1 |\n> ${COMMENT.split('\n').join('\n> ')}`],
    ['with a paragraph between', `| A | B |\n| --- | --- |\n| x | 1 |\n\nA paragraph.\n\n${COMMENT}`],
  ])('are kept, word for word: %s', (_, note) => {
    const e = open(note);
    expect(markdownOf(e)).toContain('  B: number');
    expect(markdownOf(e)).toContain('<!-- rich-table');
    // And again, after a second round trip.
    expect(markdownOf(open(markdownOf(e)))).toContain('  B: number');
  });
});
