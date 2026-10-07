import { describe, it, expect } from 'vitest';
import {
  groupRows, measure, parseBy, parseMeasure, formatMeasure, renameInBy, renameInMeasure, keysOf, BLANK_KEY,
} from '../aggregate';
import { chartModel, OTHER } from '../chartData';
import { ruleStyles, ruleProblem } from '../rules';
import { addView, deleteColumn, moveView, removeView, renameColumn, updateView } from '../ops';
import { parseRichTable, serializeRichTable } from '../markdown';
import { layoutOf, type RichTable } from '../model';

const spending = (): RichTable => ({
  name: 'chi-tieu',
  columns: [
    { name: 'Ngày', type: 'date' },
    { name: 'Khoản', type: 'text' },
    { name: 'Loại', type: 'select', options: ['Ăn uống', 'Đi lại', 'Nhà'] },
    { name: 'Số tiền', type: 'number', format: 'vnd' },
    { name: 'Nhãn', type: 'multi', options: ['a', 'b'] },
    { name: 'Xong', type: 'checkbox' },
  ],
  rows: [
    ['2026-09-28', 'Cà phê', 'Ăn uống', '45000', 'a', '[x]'],
    ['2026-10-01', 'Grab', 'Đi lại', '62000', 'a, b', ''],
    ['2026-10-02', 'Nhà', 'Nhà', '6500000', '', '[x]'],
    ['2026-10-15', 'Trà', 'Ăn uống', '30000', 'b', ''],
    ['', 'Sách', '', '120000', '', ''],
  ],
  views: [{}],
});
const all = [0, 1, 2, 3, 4];

describe('reading what to group by and what to measure', () => {
  it.each([
    ['Loại', { column: 'Loại' }],
    ['Ngày by month', { column: 'Ngày', bucket: 'month' }],
    ['[Số tiền] BY Year', { column: 'Số tiền', bucket: 'year' }],
    ['', null],
  ])('%s', (spec, expected) => expect(parseBy(spec)).toEqual(expected));

  it.each([
    ['count', { fn: 'count' }],
    ['count()', { fn: 'count' }],
    ['sum([Số tiền])', { fn: 'sum', column: 'Số tiền' }],
    ['sum(Số tiền)', { fn: 'sum', column: 'Số tiền' }],
    ['AVERAGE([Điểm])', { fn: 'avg', column: 'Điểm' }],
    ['median([X])', { fn: 'median', column: 'X' }],
    ['Số tiền', null],
  ])('%s', (spec, expected) => expect(parseMeasure(spec)).toEqual(expected));

  it('writes a measure back in one form', () => {
    expect(formatMeasure({ fn: 'sum', column: 'Số tiền' })).toBe('sum([Số tiền])');
    expect(formatMeasure({ fn: 'count' })).toBe('count');
    expect(renameInBy('Ngày by week', 'Ngày', 'Hôm')).toBe('Hôm by week');
    expect(renameInBy('Loại', 'Ngày', 'Hôm')).toBe('Loại');
    expect(renameInMeasure('sum(Số tiền)', 'Số tiền', 'Tiền')).toBe('sum([Tiền])');
    expect(renameInMeasure('Số tiền', 'Số tiền', 'Tiền')).toBe('Tiền');
  });
});

describe('grouping rows', () => {
  const t = spending();
  const g = (by: string) => groupRows(t, all, parseBy(by)!, 'vi').map((x) => [x.key, x.rows]);

  it('orders a select by its options, blank last', () => {
    expect(g('Loại')).toEqual([['Ăn uống', [0, 3]], ['Đi lại', [1]], ['Nhà', [2]], [BLANK_KEY, [4]]]);
  });

  it('buckets dates', () => {
    expect(g('Ngày by month')).toEqual([['2026-09', [0]], ['2026-10', [1, 2, 3]], [BLANK_KEY, [4]]]);
    expect(g('Ngày by week')).toEqual([['2026-W40', [0, 1, 2]], ['2026-W42', [3]], [BLANK_KEY, [4]]]);
    expect(g('Ngày by year')).toEqual([['2026', [0, 1, 2, 3]], [BLANK_KEY, [4]]]);
  });

  it('puts a row in a group per item of a multi-select', () => {
    expect(g('Nhãn')).toEqual([['a', [0, 1]], ['b', [1, 3]], [BLANK_KEY, [2, 4]]]);
  });

  it('orders numbers as numbers and ticks before blanks', () => {
    expect(g('Số tiền').map(([k]) => k)).toEqual(['30000', '45000', '62000', '120000', '6500000']);
    expect(g('Xong')).toEqual([['[x]', [0, 2]], ['[ ]', [1, 3, 4]]]);
  });

  it('labels a month in the interface language', () => {
    expect(keysOf(t.columns[0], '2026-10-01', 'month', 'en')[0].label).toBe('Oct 2026');
  });

  it('measures groups', () => {
    expect(measure(t, all, { fn: 'count' })).toBe(5);
    expect(measure(t, all, { fn: 'sum', column: 'Số tiền' })).toBe(6757000);
    expect(measure(t, [0, 3], { fn: 'avg', column: 'Số tiền' })).toBe(37500);
    expect(measure(t, [0, 3], { fn: 'count', column: 'Ngày' })).toBe(2);
    expect(measure(t, [4], { fn: 'max', column: 'Ngày' })).toBeNull();
  });
});

describe('the data of a chart', () => {
  const t = spending();

  it('sums by category, one series, coloured by its first slot', () => {
    const m = chartModel(t, { kind: 'bar', x: 'Loại', y: 'sum([Số tiền])' }, all, 'vi');
    expect(m.categories.map((c) => c.key)).toEqual(['Ăn uống', 'Đi lại', 'Nhà', BLANK_KEY]);
    expect(m.values).toEqual([[75000, 62000, 6500000, 120000]]);
    expect(m.series).toEqual([{ key: '', label: '', slot: 0 }]);
    expect(m.format).toBe('vnd');
  });

  it('counts when nothing is measured, and runs a total when asked', () => {
    expect(chartModel(t, { kind: 'line', x: 'Ngày by month' }, all, 'vi').values).toEqual([[1, 3, 1]]);
    expect(chartModel(t, { kind: 'line', x: 'Ngày by month', cumulative: true }, all, 'vi').values).toEqual([[1, 4, 5]]);
  });

  it('keeps a series its colour when a filter removes others', () => {
    const spec = { kind: 'bar', x: 'Ngày by month', series: 'Loại', y: 'sum([Số tiền])' };
    const full = chartModel(t, spec, all, 'vi');
    expect(full.series.map((s) => [s.key, s.slot])).toEqual([['Ăn uống', 0], ['Đi lại', 1], ['Nhà', 2], [BLANK_KEY, 3]]);
    const filtered = chartModel(t, spec, [2], 'vi');
    expect(filtered.series.map((s) => [s.key, s.slot])).toEqual([['Nhà', 2]]);
    expect(full.values[0]).toEqual([45000, 30000, null]);
  });

  it('folds a ninth series into Other', () => {
    const many: RichTable = {
      columns: [{ name: 'K', type: 'text' }, { name: 'V', type: 'number' }],
      rows: Array.from({ length: 10 }, (_, i) => [`k${i}`, '1']),
      views: [{}],
    };
    const m = chartModel(many, { kind: 'bar', x: 'K', series: 'K' }, many.rows.map((_, i) => i), 'en');
    expect(m.series).toHaveLength(9);
    expect(m.series[8]).toEqual({ key: OTHER, label: '', slot: 'other' });
    expect(m.folded).toBe(2);
  });

  it('keeps a donut to six slices, the biggest first', () => {
    const many: RichTable = {
      columns: [{ name: 'K', type: 'text' }, { name: 'V', type: 'number' }],
      rows: Array.from({ length: 9 }, (_, i) => [`k${i}`, String(i + 1)]),
      views: [{}],
    };
    const m = chartModel(many, { kind: 'donut', x: 'K', y: 'sum(V)' }, many.rows.map((_, i) => i), 'en');
    expect(m.categories.map((c) => c.key)).toEqual(['k8', 'k7', 'k6', 'k5', 'k4', OTHER]);
    expect(m.values[0]).toEqual([9, 8, 7, 6, 5, 10]);
    expect(m.total).toBe(45);
  });

  it('gives a number, and the last period against the one before', () => {
    const m = chartModel(t, { kind: 'number', x: 'Ngày by month', y: 'sum([Số tiền])' }, all, 'en');
    expect(m.total).toBe(6757000);
    expect(m.delta).toMatchObject({ current: 6592000, previous: 45000, label: 'Oct 2026', previousLabel: 'Sep 2026' });
  });

  it('plots a point per row for a scatter, three colours at most', () => {
    const m = chartModel(t, { kind: 'scatter', x: 'Số tiền', y: 'Số tiền', series: 'Loại' }, all, 'vi');
    expect(m.points).toHaveLength(5);
    expect(m.series.map((s) => s.slot)).toEqual([0, 1, 2, 'other']);
  });

  it('says what is missing', () => {
    expect(chartModel(t, { kind: 'bar' }, all, 'vi').problem).toBe('no_x');
    expect(chartModel(t, { kind: 'bar', x: 'Loại', y: 'sum([Nope])' }, all, 'vi').problem).toBe('no_y');
    expect(chartModel(t, { kind: 'bar', x: 'Loại' }, [], 'vi').problem).toBe('no_data');
    expect(chartModel(t, { kind: 'scatter', x: 'Khoản', y: 'Khoản' }, all, 'vi').problem).toBe('not_numeric');
  });
});

describe('colouring rules', () => {
  const t = spending();

  it('tints the rows a condition holds for, first rule winning', () => {
    const styles = ruleStyles(t, { rules: [
      { when: '[Số tiền] > 100000', style: { row: 'red' } },
      { when: '[Xong]', style: { row: 'green' } },
    ] }, all);
    expect(styles.get(2)?.row).toBe('var(--rt-tint-red)');
    expect(styles.get(0)?.row).toBe('var(--rt-tint-green)');
    expect(styles.get(4)?.row).toBe('var(--rt-tint-red)');
    expect(styles.has(1)).toBe(false);
  });

  it('tints the cells a condition names', () => {
    const styles = ruleStyles(t, { rules: [{ when: '[Loại] = "Nhà"', style: { cell: 'blue' } }] }, all);
    expect(styles.get(2)?.cells).toEqual({ 2: 'var(--rt-tint-blue)' });
  });

  it('shades a number column from low to high', () => {
    const styles = ruleStyles(t, { rules: [{ scale: ['green', 'red'], column: 'Số tiền' }] }, all);
    expect(styles.get(3)?.cells?.[3]).toBe('color-mix(in srgb, var(--rt-tint-red) 0%, var(--rt-tint-green))');
    expect(styles.get(2)?.cells?.[3]).toBe('color-mix(in srgb, var(--rt-tint-red) 100%, var(--rt-tint-green))');
  });

  it('skips a rule that does not compile, and says why', () => {
    const rule = { when: '[Nope] > 1', style: { row: 'red' } };
    expect(ruleStyles(t, { rules: [rule] }, all).size).toBe(0);
    expect(ruleProblem(t, rule)).toMatchObject({ code: 'NAME' });
    expect(ruleProblem(t, { scale: ['green', 'red'], column: 'Nope' })).toBe('no_column');
  });
});

describe('views', () => {
  const withViews = (): RichTable => ({
    ...spending(),
    views: [
      {
        name: 'Tất cả', group: 'Ngày by month', hide: ['Nhãn'],
        rules: [{ when: '[Số tiền] > 100000', style: { cell: 'red' }, columns: ['Số tiền'] }, { scale: ['green', 'red'], column: 'Số tiền' }],
      },
      { name: 'Theo loại', layout: 'chart', chart: { kind: 'bar', x: 'Loại', y: 'sum([Số tiền])', series: 'Ngày by month' } },
      { name: 'Tổng hợp', layout: 'pivot', pivot: { rows: 'Loại', columns: 'Ngày by month', value: 'sum(Số tiền)' } },
      { name: 'Việc', layout: 'board', board: { by: 'Loại', title: 'Khoản', show: ['Số tiền'] } },
    ],
  });

  it('adds, moves and removes views, keeping one', () => {
    let t = addView(spending(), { name: 'B', layout: 'chart' });
    expect(t.views.map((v) => v.name)).toEqual([undefined, 'B']);
    t = moveView(t, 1, 0);
    expect(t.views[0].name).toBe('B');
    t = removeView(removeView(t, 0), 0);
    expect(t.views).toHaveLength(1);
    expect(updateView(t, { group: 'Loại' }, 0).views[0].group).toBe('Loại');
    expect(layoutOf({ layout: 'nonsense' })).toBe('table');
  });

  it('follows a column renamed, everywhere a view names it', () => {
    const t = renameColumn(renameColumn(withViews(), 3, 'Tiền'), 0, 'Hôm');
    expect(t.views[0]).toMatchObject({
      group: 'Hôm by month',
      rules: [{ when: '[Tiền] > 100000', columns: ['Tiền'] }, { column: 'Tiền' }],
    });
    expect(t.views[1].chart).toEqual({ kind: 'bar', x: 'Loại', y: 'sum([Tiền])', series: 'Hôm by month' });
    expect(t.views[2].pivot).toEqual({ rows: 'Loại', columns: 'Hôm by month', value: 'sum([Tiền])' });
    expect(renameColumn(withViews(), 1, 'Tên').views[3].board).toEqual({ by: 'Loại', title: 'Tên', show: ['Số tiền'] });
  });

  it('forgets a column deleted, everywhere a view names it', () => {
    const t = deleteColumn(deleteColumn(withViews(), 3), 0);
    expect(t.views[0].group).toBeUndefined();
    expect(t.views[0].rules).toEqual([{ when: '[Số tiền] > 100000', style: { cell: 'red' }, columns: [] }]);
    expect(t.views[1].chart).toEqual({ kind: 'bar', x: 'Loại' });
    expect(t.views[2].pivot).toEqual({ rows: 'Loại' });
    expect(t.views[3].board).toEqual({ by: 'Loại', title: 'Khoản', show: [] });
  });

  it('round-trips every view setting through the file', () => {
    const source = serializeRichTable(withViews());
    expect(source).toContain('chart: { kind: bar, x: Loại, y: "sum([Số tiền])", series: Ngày by month }');
    expect(source).toContain('style: { cell: red }');
    const again = parseRichTable(source);
    expect(again.readOnly).toBeUndefined();
    expect(again.table.views).toEqual(withViews().views);
    expect(serializeRichTable(again.table)).toBe(source);
  });

  it('reads the design document’s own example', () => {
    const example = `| Ngày       | Khoản  | Loại    | Số tiền | Thuế |
| ---        | ---    | ---     |    ---: | ---: |
| 2026-10-01 | Cà phê | Ăn uống |   45000 | 4500 |
| 2026-10-02 | Grab   | Đi lại  |   62000 | 6200 |
<!-- rich-table
name: chi-tieu
columns:
  Ngày: date
  Loại: { type: select, options: [Ăn uống, Đi lại, Nhà] }
  Số tiền: { type: number, format: vnd }
  Thuế: { type: formula, expr: "[Số tiền] * 0.1", format: vnd }
views:
  - name: Tất cả
    sort: -Ngày
    summary: { Số tiền: sum, Thuế: sum }
  - name: Theo loại
    layout: chart
    chart: { kind: bar, x: Loại, y: "sum([Số tiền])" }
-->`;
    const { table, readOnly } = parseRichTable(example);
    expect(readOnly).toBeUndefined();
    expect(table.views.map((v) => layoutOf(v))).toEqual(['table', 'chart']);
    const m = chartModel(table, table.views[1].chart, [0, 1], 'vi');
    expect(m.values).toEqual([[45000, 62000]]);
  });
});
