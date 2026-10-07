import { describe, it, expect } from 'vitest';
import { filterText, parseFilter, passes, type Condition } from '../filter';
import type { Column } from '../model';

const columns: Column[] = [
  { name: 'Số tiền', type: 'number' },
  { name: 'Loại', type: 'select', options: ['Ăn uống'] },
  { name: 'Ngày', type: 'date' },
  { name: 'Đã trả', type: 'checkbox' },
  { name: 'Nhãn', type: 'multi' },
  { name: 'Ghi chú', type: 'text' },
];

describe('a filter as text', () => {
  const conditions: Condition[] = [
    { column: 'Số tiền', op: 'gt', value: '100000' },
    { column: 'Loại', op: 'eq', value: 'Ăn "uống"' },
    { column: 'Ngày', op: 'gte', value: '2026-10-01' },
    { column: 'Đã trả', op: 'eq', value: '[ ]' },
    { column: 'Nhãn', op: 'not_contains', value: 'Công tác' },
    { column: 'Ghi chú', op: 'not_empty', value: '' },
  ];

  it('is written in the formula language', () => {
    expect(filterText(conditions, columns)).toBe(
      '[Số tiền] > 100000 and [Loại] = "Ăn \\"uống\\"" and [Ngày] >= DATE(2026, 10, 1)'
      + ' and [Đã trả] = false and not CONTAINS([Nhãn], "Công tác") and not ISBLANK([Ghi chú])',
    );
  });

  it('reads back to the same conditions', () => {
    expect(parseFilter(filterText(conditions, columns))).toEqual(conditions);
  });

  it('reads a hand-written filter', () => {
    expect(parseFilter('[Số tiền]<>5 AND isblank([Ghi chú])')).toEqual([
      { column: 'Số tiền', op: 'neq', value: '5' },
      { column: 'Ghi chú', op: 'empty', value: '' },
    ]);
  });

  it('refuses what phase 1 cannot show as conditions', () => {
    expect(parseFilter('[A] > 1 or [B] < 2')).toBeNull();
    expect(parseFilter('[A] * 2 > 1')).toBeNull();
    expect(parseFilter('YEAR([Ngày]) = 2026')).toBeNull();
    expect(parseFilter('')).toEqual([]);
  });
});

describe('applying a condition', () => {
  const at = (name: string) => columns.find((c) => c.name === name)!;
  const test = (c: Condition, raw: string) => passes(c, at(c.column), raw, 'vi');

  it('compares numbers and dates as such', () => {
    expect(test({ column: 'Số tiền', op: 'gt', value: '9' }, '10')).toBe(true);
    expect(test({ column: 'Số tiền', op: 'gt', value: '9' }, 'abc')).toBe(false);
    expect(test({ column: 'Ngày', op: 'eq', value: '2026-10-01' }, '2026-10-01 14:30')).toBe(true);
    expect(test({ column: 'Ngày', op: 'lt', value: '2026-10-01' }, '2026-09-30')).toBe(true);
  });

  it('ignores case, and treats a blank as not equal', () => {
    expect(test({ column: 'Loại', op: 'eq', value: 'ăn uống' }, 'Ăn uống')).toBe(true);
    expect(test({ column: 'Loại', op: 'neq', value: 'Ăn uống' }, '')).toBe(true);
    expect(test({ column: 'Ghi chú', op: 'contains', value: 'GẤP' }, 'việc gấp')).toBe(true);
  });

  it('matches a list by its items, a checkbox by its tick', () => {
    expect(test({ column: 'Nhãn', op: 'contains', value: 'a' }, 'ab, c')).toBe(false);
    expect(test({ column: 'Nhãn', op: 'contains', value: 'c' }, 'ab, c')).toBe(true);
    expect(test({ column: 'Đã trả', op: 'eq', value: '[ ]' }, '')).toBe(true);
    expect(test({ column: 'Đã trả', op: 'eq', value: '[x]' }, '[X]')).toBe(true);
  });
});
