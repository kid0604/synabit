import { describe, it, expect } from 'vitest';
import {
  setCells, insertRows, deleteRows, moveRow, reorderRows, insertColumn, deleteColumn,
  moveColumn, renameColumn, updateColumn, updateView, setConditions, renameForeignColumn, renamedColumn, fillValues,
} from '../ops';
import { shownRows } from '../view';
import { summarize, selectionStats } from '../summary';
import { parseTsv, toTsv, parseHtmlTable } from '../clipboard';
import type { Column, RichTable } from '../model';

const base = (): RichTable => ({
  columns: [
    { name: 'Khoản', type: 'text' },
    { name: 'Loại', type: 'select', options: ['Ăn uống'] },
    { name: 'Số tiền', type: 'number' },
  ],
  rows: [
    ['Cà phê', 'Ăn uống', '45000'],
    ['Grab', 'Đi lại', '62000'],
    ['Trà', 'Ăn uống', ''],
    ['Sách', '', 'abc'],
  ],
  views: [{}],
});

describe('changing a table', () => {
  it('never touches the table it was given', () => {
    const table = base();
    const frozen = JSON.stringify(table);
    setCells(table, [{ row: 0, col: 0, raw: 'x' }]);
    insertRows(table, 0, 1);
    deleteColumn(table, 0);
    renameColumn(table, 0, 'Tên');
    expect(JSON.stringify(table)).toBe(frozen);
  });

  it('makes a new select value an option', () => {
    const next = setCells(base(), [{ row: 3, col: 1, raw: 'Nhà' }]);
    expect(next.columns[1].options).toEqual(['Ăn uống', 'Nhà']);
  });

  it('returns the same table when nothing changes', () => {
    const table = base();
    expect(setCells(table, [{ row: 0, col: 0, raw: 'Cà phê' }])).toBe(table);
  });

  it('inserts, deletes and moves rows', () => {
    expect(insertRows(base(), 1, 1).rows[1]).toEqual(['', '', '']);
    expect(deleteRows(base(), [0, 2]).rows.map((r) => r[0])).toEqual(['Grab', 'Sách']);
    expect(moveRow(base(), 0, 2).rows.map((r) => r[0])).toEqual(['Grab', 'Trà', 'Cà phê', 'Sách']);
    expect(reorderRows(base(), [3, 1]).rows.map((r) => r[0])).toEqual(['Sách', 'Grab', 'Cà phê', 'Trà']);
  });

  it('inserts, deletes and moves columns with their cells', () => {
    const inserted = insertColumn(base(), 1, { name: 'Ngày', type: 'date' });
    expect(inserted.rows[0]).toEqual(['Cà phê', '', 'Ăn uống', '45000']);
    expect(deleteColumn(base(), 1).rows[0]).toEqual(['Cà phê', '45000']);
    expect(moveColumn(base(), 2, 0).rows[0]).toEqual(['45000', 'Cà phê', 'Ăn uống']);
    const only = { ...base(), columns: [base().columns[0]], rows: [['a']] };
    expect(deleteColumn(only, 0)).toBe(only);
  });

  it('renames a column everywhere the view names it', () => {
    let table = updateView(base(), { sort: ['-Số tiền'], summary: { 'Số tiền': 'sum' } });
    table = setConditions(table, [{ column: 'Số tiền', op: 'gt', value: '1' }]);
    const renamed = renameColumn(table, 2, 'Tiền');
    expect(renamed.views[0]).toEqual({ sort: ['-Tiền'], summary: { Tiền: 'sum' }, filter: '[Tiền] > 1' });
  });

  it('forgets a deleted column in the view', () => {
    let table = updateView(base(), { sort: ['Số tiền', 'Khoản'], summary: { 'Số tiền': 'sum' } });
    table = setConditions(table, [{ column: 'Số tiền', op: 'gt', value: '1' }]);
    expect(deleteColumn(table, 2).views[0]).toEqual({ sort: ['Khoản'], summary: {}, filter: undefined });
  });

  it('collects options when a column becomes a select, and keeps every value', () => {
    const next = updateColumn(base(), 0, { type: 'select' });
    expect(next.columns[0].options).toEqual(['Cà phê', 'Grab', 'Trà', 'Sách']);
    const back = updateColumn(next, 0, { type: 'number' });
    expect(back.columns[0].options).toBeUndefined();
    expect(back.rows.map((r) => r[0])).toEqual(['Cà phê', 'Grab', 'Trà', 'Sách']);
  });
});

describe('what a view shows', () => {
  it('filters, then sorts with blanks last', () => {
    const table = updateView(base(), { sort: ['-Số tiền'] });
    expect(shownRows(table, table.views[0], 'vi').rows).toEqual([1, 0, 3, 2]);
    const filtered = setConditions(table, [{ column: 'Loại', op: 'eq', value: 'Ăn uống' }]);
    expect(shownRows(filtered, filtered.views[0], 'vi').rows).toEqual([0, 2]);
  });

  it('keeps a row it was told to keep, and searches every cell', () => {
    const table = setConditions(base(), [{ column: 'Loại', op: 'eq', value: 'Ăn uống' }]);
    expect(shownRows(table, table.views[0], 'vi', new Set([3])).rows).toEqual([0, 2, 3]);
    expect(shownRows(base(), {}, 'vi', new Set(), 'grab').rows).toEqual([1]);
  });

  it('runs a filter pills cannot say as a formula', () => {
    const shown = shownRows(base(), { filter: '[Số tiền] > 50000 or [Khoản] = "Trà"' }, 'vi');
    expect(shown).toEqual({ rows: [1, 2], filterUnread: false });
  });

  it('shows everything, and says so, when the filter does not compile', () => {
    const shown = shownRows(base(), { filter: '[Nope] > 1 or' }, 'vi');
    expect(shown).toEqual({ rows: [0, 1, 2, 3], filterUnread: true });
  });

  it('renames a column inside formulas and a formula filter', () => {
    const table: RichTable = {
      ...base(),
      columns: [...base().columns, { name: 'Thuế', type: 'formula', expr: 'table[Số tiền] * 0.1 // [Số tiền]' }],
      views: [{ filter: '[Số tiền] > 1 or Khoản = "x"' }],
    };
    const renamed = renameColumn(renameColumn(table, 2, 'Tiền'), 0, 'Tên');
    expect(renamed.columns[3].expr).toBe('table[Tiền] * 0.1 // [Số tiền]');
    expect(renamed.views[0].filter).toBe('[Tiền] > 1 or Tên = "x"');
  });
});

describe('summaries', () => {
  const money = base().columns[2];
  const values = base().rows.map((r) => r[2]);
  it('sum what fits, and count everything', () => {
    expect(summarize(money, 'sum', values, 'en')).toBe('107,000');
    expect(summarize(money, 'avg', values, 'en')).toBe('53,500');
    expect(summarize(money, 'empty', values, 'en')).toBe('1');
    expect(summarize(money, 'percent_filled', values, 'en')).toBe('75%');
    expect(summarize(money, 'sum', ['', 'x'], 'en')).toBeNull();
  });

  it('sum a selection of numbers', () => {
    expect(selectionStats(values.map((raw) => ({ raw, column: money })))).toEqual({
      sum: 107000, avg: 53500, count: 2, min: 45000, max: 62000,
    });
  });
});

describe('the clipboard', () => {
  it('round-trips tab-separated text, quotes and all', () => {
    const grid = [['a', 'b\tc'], ['line\nbreak', 'say "hi"']];
    expect(parseTsv(toTsv(grid))).toEqual(grid);
    expect(parseTsv('1\t2\r\n3\t4\n')).toEqual([['1', '2'], ['3', '4']]);
  });

  it('reads an HTML table, spreading merged cells back out', () => {
    expect(parseHtmlTable(`<table>
      <tr><th>A</th><th>B</th></tr>
      <tr><td colspan="2"><p>one</p><p>two</p></td></tr>
      <tr><td>x<br>y</td><td>z</td></tr></table>`)).toEqual([
      ['A', 'B'], ['one\ntwo', ''], ['x\ny', 'z'],
    ]);
    expect(parseHtmlTable('<p>no table</p>')).toBeNull();
  });
});

describe('a column renamed in another table', () => {
  it('is followed by formulas, filters and rules that read it by that table’s name', () => {
    const order: RichTable = {
      columns: [{ name: 'Mã', type: 'text' }, { name: 'Tiền', type: 'formula', expr: 'LOOKUP([Mã], gia[Giá], gia[Giá]) + [Giá]' }],
      rows: [],
      views: [{ filter: 'SUM(gia[Giá]) > 0', rules: [{ when: 'gia[Giá] > 1', style: { row: 'red' } }] }],
    };
    const next = renameForeignColumn(order, 'gia', 'Giá', 'Đơn giá');
    expect(next.columns[1].expr).toBe('LOOKUP([Mã], gia[Đơn giá], gia[Đơn giá]) + [Giá]');
    expect(next.views[0].filter).toBe('SUM(gia[Đơn giá]) > 0');
    expect(next.views[0].rules?.[0].when).toBe('gia[Đơn giá] > 1');
    expect(renameForeignColumn(order, 'khac', 'Giá', 'X')).toBe(order);
    expect(renamedColumn(base(), renameColumn(base(), 1, 'Nhóm'))).toEqual({ from: 'Loại', to: 'Nhóm' });
    expect(renamedColumn(base(), deleteColumn(base(), 1))).toBeNull();
  });
});

describe('the fill handle', () => {
  it('goes on with a series, or repeats', () => {
    const num: Column = { name: 'n', type: 'number' };
    const date: Column = { name: 'd', type: 'date' };
    const text: Column = { name: 't', type: 'text' };
    expect(fillValues(num, ['1', '2'], 3)).toEqual(['3', '4', '5']);
    expect(fillValues(num, ['0.1', '0.2'], 2)).toEqual(['0.3', '0.4']);
    expect(fillValues(num, ['5'], 2)).toEqual(['5', '5']);
    expect(fillValues(num, ['1', '5', '6'], 4)).toEqual(['1', '5', '6', '1']);
    expect(fillValues(date, ['2026-10-01', '2026-10-08'], 2)).toEqual(['2026-10-15', '2026-10-22']);
    expect(fillValues(date, ['2026-01-31', '2026-02-01'], 1)).toEqual(['2026-02-02']);
    expect(fillValues(date, ['2026-10-31'], 2)).toEqual(['2026-11-01', '2026-11-02']);
    expect(fillValues(date, ['2026-10-01', '2026-10-05', '2026-10-06'], 2)).toEqual(['2026-10-01', '2026-10-05']);
    expect(fillValues(text, ['Tuần 3'], 2)).toEqual(['Tuần 4', 'Tuần 5']);
    expect(fillValues(text, ['A-01', 'A-03'], 2)).toEqual(['A-05', 'A-07']);
    expect(fillValues(text, ['a', 'b'], 3)).toEqual(['a', 'b', 'a']);
    expect(fillValues({ name: 's', type: 'select' }, ['Nhà'], 2)).toEqual(['Nhà', 'Nhà']);
  });
});
