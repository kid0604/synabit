import { describe, it, expect } from 'vitest';
import { parseRichTable, serializeRichTable, shareRows, splitRow, escapeCell, isRichTableComment } from '../markdown';
import { emptyTable, validColumnName, freshColumnName } from '../model';

const EXPENSES = `| Ngày | Khoản | Loại | Số tiền | Đã trả |
| --- | --- | --- | ---: | :---: |
| 2026-10-01 | Cà phê | Ăn uống | 45000 | [x] |
| 2026-10-02 | Grab | Đi lại | 62000 | [ ] |
<!-- rich-table
name: chi-tieu
version: 1
columns:
  Ngày: date
  Loại: { type: select, options: [ Ăn uống, Đi lại, Nhà ] }
  Số tiền: { type: number, format: vnd }
  Đã trả: checkbox
views:
  - sort: [ -Ngày ]
    summary: { Số tiền: sum }
-->`;

describe('reading a Rich Table', () => {
  it('reads the columns, their types and the rows', () => {
    const { table, readOnly, warnings } = parseRichTable(EXPENSES);
    expect(readOnly).toBeUndefined();
    expect(warnings).toEqual([]);
    expect(table.name).toBe('chi-tieu');
    expect(table.columns.map((c) => [c.name, c.type])).toEqual([
      ['Ngày', 'date'], ['Khoản', 'text'], ['Loại', 'select'], ['Số tiền', 'number'], ['Đã trả', 'checkbox'],
    ]);
    expect(table.columns[2].options).toEqual(['Ăn uống', 'Đi lại', 'Nhà']);
    expect(table.columns[3].format).toBe('vnd');
    expect(table.rows[1]).toEqual(['2026-10-02', 'Grab', 'Đi lại', '62000', '[ ]']);
    expect(table.views[0]).toEqual({ sort: ['-Ngày'], summary: { 'Số tiền': 'sum' } });
  });

  it('writes back, byte for byte, what it wrote', () => {
    const once = serializeRichTable(parseRichTable(EXPENSES).table);
    expect(serializeRichTable(parseRichTable(once).table)).toBe(once);
    expect(once).toBe(EXPENSES);
  });

  it('keeps a pipe and a line break inside a cell', () => {
    const table = emptyTable(['A', 'B'], 1);
    table.rows[0] = ['x | y', 'one\ntwo'];
    const source = serializeRichTable(table);
    expect(source).toContain('| x \\| y | one<br>two |');
    expect(parseRichTable(source).table.rows[0]).toEqual(['x | y', 'one\ntwo']);
  });

  it('splits only on pipes that are not escaped', () => {
    expect(splitRow('| a \\| b | c |')).toEqual(['a | b', 'c']);
    expect(splitRow('a|b')).toEqual(['a', 'b']);
    expect(escapeCell('  a|b  ')).toBe('a\\|b');
  });

  it('pads a short row and folds a long one into its last cell', () => {
    const { table, warnings } = parseRichTable(`| A | B |
| --- | --- |
| 1 |
| 1 | 2 | 3 |
<!-- rich-table
-->`);
    expect(table.rows).toEqual([['1', ''], ['1', '2 | 3']]);
    expect(warnings).toEqual([{ kind: 'extra-cells', row: 1 }]);
  });

  it('will not write a table whose comment does not parse', () => {
    const broken = EXPENSES.replace('columns:', 'columns: [');
    const { table, readOnly } = parseRichTable(broken);
    expect(readOnly).toBe('meta');
    // The values are still all there to show.
    expect(table.rows).toHaveLength(2);
    expect(table.columns.every((c) => c.type === 'text')).toBe(true);
  });

  it('will not write a table from a later version', () => {
    expect(parseRichTable(EXPENSES.replace('version: 1', 'version: 2')).readOnly).toBe('version');
  });

  it('will not write a table whose comment never closes', () => {
    expect(parseRichTable(EXPENSES.replace(/-->$/, '')).readOnly).toBe('meta');
  });

  it('will not write a table with text after its comment, which a rewrite would drop', () => {
    expect(parseRichTable(EXPENSES.replace(/-->$/, '--> ghi chú')).readOnly).toBe('meta');
  });

  it('keeps the settings of a column that went missing, but not over a column of that name', () => {
    const parsed = parseRichTable(EXPENSES);
    const table = { ...parsed.table, orphans: { [parsed.table.columns[1].name]: 'date', Cũ: 'number' } };
    const again = parseRichTable(serializeRichTable(table)).table;
    expect(again.columns[1].type).toBe(parsed.table.columns[1].type);
    expect(again.orphans).toEqual({ Cũ: 'number' });
    expect(validColumnName(again, 'Cũ')).toBe(false);
    expect(freshColumnName(again, 'Cũ')).toBe('Cũ 2');
  });

  it('keeps a type it does not know, and everything it does not know', () => {
    const later = `| Giá | Thuế |
| ---: | ---: |
| 100 | 10 |
<!-- rich-table
version: 1
columns:
  Giá: number
  Thuế: { type: rollup, of: "[Giá]" }
charts:
  - kind: bar
-->`;
    const { table } = parseRichTable(later);
    expect(table.columns[1].foreignType).toBe('rollup');
    const again = serializeRichTable(table);
    expect(again).toContain('Thuế: { type: rollup, of: "[Giá]" }');
    expect(again).toContain('charts:');
  });

  it('reads and writes a formula column', () => {
    const source = `| Giá | Thuế |
| ---: | ---: |
| 100 | 10 |
<!-- rich-table
version: 1
columns:
  Giá: number
  Thuế: { type: formula, expr: "[Giá] * 0.1", format: vnd }
-->`;
    const { table } = parseRichTable(source);
    expect(table.columns[1]).toEqual({ name: 'Thuế', type: 'formula', expr: '[Giá] * 0.1', format: 'vnd' });
    expect(serializeRichTable(table)).toBe(source);
  });

  it('keeps the declaration of a column the header no longer has', () => {
    const renamed = EXPENSES.replace('| Ngày |', '| Day |');
    const { table, warnings } = parseRichTable(renamed);
    expect(warnings).toContainEqual({ kind: 'orphan-column', name: 'Ngày' });
    expect(serializeRichTable(table)).toContain('Ngày: date');
  });

  it('never closes its own comment early', () => {
    const table = emptyTable(['A'], 0);
    table.views = [{ filter: 'CONTAINS([A], "-->")' }];
    const source = serializeRichTable(table);
    expect(source.match(/-->/g)).toHaveLength(1);
    expect(parseRichTable(source).table.views[0].filter).toBe('CONTAINS([A], "-->")');
  });

  it('writes a plain new table with only a version', () => {
    expect(serializeRichTable(emptyTable(['Tên', 'Ghi chú'], 1))).toBe(`| Tên | Ghi chú |
| --- | --- |
|  |  |
<!-- rich-table
version: 1
-->`);
  });

  it('knows its comment', () => {
    expect(isRichTableComment('<!-- rich-table\n-->')).toBe(true);
    expect(isRichTableComment('<!-- rich-tables -->')).toBe(false);
    expect(isRichTableComment('<!-- a comment -->')).toBe(false);
  });
});

describe('reading a table again', () => {
  it('shares the rows that did not change', () => {
    const before = parseRichTable(EXPENSES).table;
    const changed = { ...before, rows: [before.rows[0], ['2026-10-02', 'Grab', 'Đi lại', '1', '[ ]']] };
    const again = shareRows(before, parseRichTable(serializeRichTable(changed)).table);
    expect(again.rows[0]).toBe(before.rows[0]);
    expect(again.rows[1]).not.toBe(before.rows[1]);
    expect(again.columns).toBe(before.columns);
  });
});
