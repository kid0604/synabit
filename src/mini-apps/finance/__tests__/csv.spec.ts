import { describe, it, expect } from 'vitest';
import {
  EXPORT_HEADER,
  amountForExport,
  exportCsv,
  exportFilename,
  exportRows,
  importKey,
  importRows,
  matchColumns,
  missingColumns,
  parseCsv,
  parseDate,
  parseImportedAmount,
  toCsv,
} from '../csv';
import type { Category, FinanceAccount, Transaction } from '../types';

const accounts: FinanceAccount[] = [
  { id: 'acc-1', name: 'Cash', initialBalance: 0 },
  { id: 'acc-2', name: 'Bank', initialBalance: 0 },
];

const categories: Category[] = [
  { id: 'Food & Dining', name: 'Ăn uống' },
  { id: 'Salary', name: 'Salary' },
];

let seq = 0;
const tx = (over: Partial<Transaction> & Pick<Transaction, 'type' | 'amount'>): Transaction => ({
  id: `tx-${++seq}`,
  category: 'Food & Dining',
  accountId: 'acc-1',
  date: '2026-08-15T10:00:00.000Z',
  note: '',
  ...over,
});

const ctx = { accounts, categories, currency: 'USD' };

describe('toCsv', () => {
  it('writes a header and rows', () => {
    expect(toCsv(['a', 'b'], [['1', '2']])).toBe('a,b\r\n1,2');
  });

  /** A note with a comma in it would otherwise become two columns. */
  it('quotes anything that would break a row', () => {
    expect(toCsv(['a'], [['lunch, with Mai']])).toBe('a\r\n"lunch, with Mai"');
    expect(toCsv(['a'], [['say "hi"']])).toBe('a\r\n"say ""hi"""');
    expect(toCsv(['a'], [['two\nlines']])).toBe('a\r\n"two\nlines"');
  });
});

describe('parseCsv', () => {
  it('reads plain rows', () => {
    expect(parseCsv('a,b\n1,2')).toEqual([['a', 'b'], ['1', '2']]);
  });

  it('reads the quoting it writes', () => {
    const text = toCsv(['note'], [['lunch, with Mai'], ['say "hi"'], ['two\nlines']]);
    expect(parseCsv(text)).toEqual([['note'], ['lunch, with Mai'], ['say "hi"'], ['two\nlines']]);
  });

  it('reads Windows line endings', () => {
    expect(parseCsv('a,b\r\n1,2\r\n')).toEqual([['a', 'b'], ['1', '2']]);
  });

  /** Excel puts one at the front of every file it writes. */
  it('ignores a byte-order mark', () => {
    expect(parseCsv('﻿a,b\n1,2')[0]).toEqual(['a', 'b']);
  });

  it('keeps a last row that has no newline after it', () => {
    expect(parseCsv('a\n1')).toEqual([['a'], ['1']]);
  });

  it('drops rows that are entirely empty', () => {
    expect(parseCsv('a,b\n\n1,2\n,\n')).toEqual([['a', 'b'], ['1', '2']]);
  });

  it('keeps an empty field between two commas', () => {
    expect(parseCsv('a,b,c\n1,,3')).toEqual([['a', 'b', 'c'], ['1', '', '3']]);
  });
});

describe('amountForExport', () => {
  /** The file is for a person and a spreadsheet; neither knows what a cent is. */
  it('writes whole units at the currency’s precision', () => {
    expect(amountForExport(1250, 'USD')).toBe('12.50');
    expect(amountForExport(1500000, 'VND')).toBe('1500000');
    expect(amountForExport(-1250, 'USD')).toBe('-12.50');
  });
});

describe('exportRows', () => {
  it('writes names rather than ids, because ids mean nothing elsewhere', () => {
    const [row] = exportRows([tx({ type: 'expense', amount: 4500 })], ctx);
    expect(row).toEqual([
      '2026-08-15', 'expense', '45.00', 'USD', 'Ăn uống', 'Cash', '', '', '',
    ]);
  });

  it('reads oldest first, the way a ledger reads', () => {
    const rows = exportRows(
      [
        tx({ type: 'expense', amount: 1, date: '2026-08-20T10:00:00.000Z' }),
        tx({ type: 'expense', amount: 2, date: '2026-08-01T10:00:00.000Z' }),
      ],
      ctx,
    );
    expect(rows.map((r) => r[0])).toEqual(['2026-08-01', '2026-08-20']);
  });

  it('names both ends of a transfer and gives it no category', () => {
    const [row] = exportRows(
      [tx({ type: 'transfer', amount: 500, accountId: 'acc-1', toAccountId: 'acc-2' })],
      ctx,
    );
    expect(row[4]).toBe('');
    expect(row[5]).toBe('Cash');
    expect(row[6]).toBe('Bank');
  });
});

describe('matchColumns', () => {
  it('recognises its own header', () => {
    const map = matchColumns(EXPORT_HEADER);
    expect(map.date).toBe(0);
    expect(map.amount).toBe(2);
    expect(map.note).toBe(7);
    expect(map.receipt).toBe(8);
    expect(missingColumns(map)).toEqual([]);
  });

  it('recognises other names for the same things', () => {
    const map = matchColumns(['Ngày', 'Số tiền', 'Danh mục', 'Description']);
    expect(map.date).toBe(0);
    expect(map.amount).toBe(1);
    expect(map.category).toBe(2);
    expect(map.note).toBe(3);
  });

  it('ignores case and surrounding space', () => {
    expect(matchColumns(['  DATE  ']).date).toBe(0);
  });

  /** Guessing a column wrong puts the date in the amount. */
  it('says what it cannot find rather than guessing', () => {
    const map = matchColumns(['Something', 'Else']);
    expect(map.date).toBe(-1);
    expect(missingColumns(map)).toEqual(['date', 'amount']);
  });
});

describe('parseDate', () => {
  it('reads ISO and day-first', () => {
    expect(parseDate('2026-08-15')).toBe('2026-08-15');
    expect(parseDate('2026-08-15T10:00:00Z')).toBe('2026-08-15');
    expect(parseDate('15/08/2026')).toBe('2026-08-15');
    expect(parseDate('5-8-2026')).toBe('2026-08-05');
  });

  it('refuses what it cannot read rather than inventing a date', () => {
    expect(parseDate('')).toBeNull();
    expect(parseDate('last Tuesday')).toBeNull();
  });
});

describe('parseImportedAmount', () => {
  it('reads a plain number', () => {
    expect(parseImportedAmount('45', 'USD')).toBe(4500);
    expect(parseImportedAmount('12.50', 'USD')).toBe(1250);
  });

  /** The two conventions, told apart by which separator comes last. */
  it('reads both ways of writing the same number', () => {
    expect(parseImportedAmount('1,234.56', 'USD')).toBe(123456);
    expect(parseImportedAmount('1.234,56', 'USD')).toBe(123456);
  });

  /**
   * `1.234` is the genuinely ambiguous one. Grouping is the reading that
   * matches how bank exports are written.
   */
  it('reads a lone three-digit group as grouping', () => {
    expect(parseImportedAmount('1.234', 'USD')).toBe(123400);
    expect(parseImportedAmount('1,234', 'USD')).toBe(123400);
  });

  it('reads a two-digit tail as a fraction', () => {
    expect(parseImportedAmount('1.23', 'USD')).toBe(123);
  });

  it('reads several separators as grouping', () => {
    expect(parseImportedAmount('1.234.567', 'VND')).toBe(1234567);
  });

  it('keeps a minus sign', () => {
    expect(parseImportedAmount('-12.50', 'USD')).toBe(-1250);
  });

  it('ignores a currency symbol', () => {
    expect(parseImportedAmount('$12.50', 'USD')).toBe(1250);
    expect(parseImportedAmount('1.500.000 ₫', 'VND')).toBe(1500000);
  });

  it('refuses a field with no number in it', () => {
    expect(parseImportedAmount('', 'USD')).toBeNull();
    expect(parseImportedAmount('n/a', 'USD')).toBeNull();
  });
});

describe('importRows', () => {
  const map = matchColumns(EXPORT_HEADER);
  const base = { accounts, categories, currency: 'USD', existing: [] as Transaction[] };

  const row = (over: Partial<Record<number, string>> = {}) => {
    const cells = ['2026-08-15', 'expense', '45.00', 'USD', 'Ăn uống', 'Cash', '', 'lunch', ''];
    Object.entries(over).forEach(([i, v]) => { cells[Number(i)] = v as string; });
    return cells;
  };

  it('reads a row it wrote itself', () => {
    const result = importRows([row()], map, base);
    expect(result.problems).toEqual([]);
    expect(result.ready[0]).toMatchObject({
      type: 'expense',
      amount: 4500,
      category: 'Food & Dining',
      accountId: 'acc-1',
      note: 'lunch',
    });
  });

  /** A category is matched by name and stored by id, so renames still follow. */
  it('resolves names back to ids', () => {
    const [tx] = importRows([row({ 4: 'Ăn uống', 5: 'Bank' })], map, base).ready;
    expect(tx.category).toBe('Food & Dining');
    expect(tx.accountId).toBe('acc-2');
  });

  it('keeps a category the vault has never heard of, rather than dropping it', () => {
    const [tx] = importRows([row({ 4: 'Parking' })], map, base).ready;
    expect(tx.category).toBe('Parking');
  });

  it('files against the first account when the file names one that is unknown', () => {
    const [tx] = importRows([row({ 5: 'Some other bank' })], map, base).ready;
    expect(tx.accountId).toBe('acc-1');
  });

  /** How nearly every bank writes a statement: no type column, signed amounts. */
  it('reads a signed amount as its direction when there is no type', () => {
    const bank = matchColumns(['Date', 'Amount', 'Description']);
    const result = importRows(
      [['2026-08-15', '-45.00', 'lunch'], ['2026-08-25', '2000.00', 'salary']],
      bank,
      base,
    );
    expect(result.ready.map((t) => [t.type, t.amount])).toEqual([
      ['expense', 4500],
      ['income', 200000],
    ]);
  });

  /**
   * The point of importing twice being safe: the same file, run again, adds
   * nothing.
   */
  it('recognises rows it has already imported', () => {
    const first = importRows([row()], map, base);
    const again = importRows([row()], map, { ...base, existing: first.ready });

    expect(again.ready).toEqual([]);
    expect(again.duplicates).toBe(1);
  });

  it('counts a row repeated inside one file as a duplicate too', () => {
    const result = importRows([row(), row()], map, base);
    expect(result.ready).toHaveLength(1);
    expect(result.duplicates).toBe(1);
  });

  it('reports the line of anything it cannot use', () => {
    const result = importRows([row({ 0: 'whenever' }), row({ 2: 'n/a' })], map, base);
    expect(result.ready).toEqual([]);
    expect(result.problems.map((p) => p.line)).toEqual([2, 3]);
    expect(result.problems[0].reason).toContain('date');
  });

  it('has nothing to say about an empty file', () => {
    expect(importRows([], map, base)).toEqual({ ready: [], duplicates: 0, problems: [] });
  });
});

describe('a round trip', () => {
  /**
   * The promise the whole file exists for: what leaves can come back, and
   * coming back twice changes nothing.
   */
  it('exports and re-imports the same ledger', () => {
    const ledger = [
      tx({ type: 'expense', amount: 4500, note: 'lunch, with Mai' }),
      tx({ type: 'income', amount: 200000, category: 'Salary', accountId: 'acc-2', date: '2026-08-25T10:00:00.000Z' }),
      tx({ type: 'transfer', amount: 50000, accountId: 'acc-1', toAccountId: 'acc-2', date: '2026-08-28T10:00:00.000Z' }),
    ];

    const text = exportCsv(ledger, ctx);
    const [header, ...rows] = parseCsv(text);
    const result = importRows(rows, matchColumns(header), { ...ctx, existing: [] });

    expect(result.problems).toEqual([]);
    expect(result.ready).toHaveLength(3);
    expect(result.ready.map((t) => [t.type, t.amount, t.category, t.accountId])).toEqual([
      ['expense', 4500, 'Food & Dining', 'acc-1'],
      ['income', 200000, 'Salary', 'acc-2'],
      ['transfer', 50000, 'Transfer', 'acc-1'],
    ]);
    expect(result.ready[0].note).toBe('lunch, with Mai');
    expect(result.ready[2].toAccountId).toBe('acc-2');
  });

  it('adds nothing the second time the same file is imported', () => {
    const ledger = [tx({ type: 'expense', amount: 4500 })];
    const [header, ...rows] = parseCsv(exportCsv(ledger, ctx));
    const map = matchColumns(header);

    const first = importRows(rows, map, { ...ctx, existing: [] });
    const second = importRows(rows, map, { ...ctx, existing: first.ready });

    expect(second.ready).toEqual([]);
    expect(second.duplicates).toBe(1);
  });

  it('recognises the ledger it came from as already there', () => {
    const ledger = [tx({ type: 'expense', amount: 4500 })];
    const [header, ...rows] = parseCsv(exportCsv(ledger, ctx));

    const result = importRows(rows, matchColumns(header), { ...ctx, existing: ledger });
    expect(result.duplicates).toBe(1);
  });
});

describe('receipts crossing the boundary', () => {
  const map = matchColumns(EXPORT_HEADER);
  const base = { accounts, categories, currency: 'USD', existing: [] as Transaction[] };

  /** Leaving and coming back should not lose the pictures. */
  it('survives a round trip', () => {
    const ledger = [tx({ type: 'expense', amount: 4500, receipt: 'assets/abc123.jpg' })];
    const [header, ...rows] = parseCsv(exportCsv(ledger, ctx));

    const [imported] = importRows(rows, matchColumns(header), base).ready;
    expect(imported.receipt).toBe('assets/abc123.jpg');
  });

  /**
   * An import is a file from somewhere else. Letting it name any path on this
   * machine would let it point the ledger at anything on disk.
   */
  it('refuses a path that is not inside the vault assets folder', () => {
    const rows = [
      ['2026-08-15', 'expense', '45.00', 'USD', 'Ăn uống', 'Cash', '', '', '/etc/passwd'],
      ['2026-08-16', 'expense', '45.00', 'USD', 'Ăn uống', 'Cash', '', '', 'assets/../../secret'],
    ];
    const result = importRows(rows, map, base);

    expect(result.ready).toHaveLength(2);
    expect(result.ready[0].receipt).toBeUndefined();
    expect(result.ready[1].receipt).toBeUndefined();
  });

  it('leaves a row with no receipt without one', () => {
    const rows = [['2026-08-15', 'expense', '45.00', 'USD', 'Ăn uống', 'Cash', '', '', '']];
    expect(importRows(rows, map, base).ready[0].receipt).toBeUndefined();
  });
});

describe('importKey', () => {
  it('does not depend on the id, which an imported row does not have', () => {
    const a = tx({ type: 'expense', amount: 4500 });
    const b = tx({ type: 'expense', amount: 4500 });
    expect(importKey(a)).toBe(importKey(b));
  });

  it('tells apart two amounts on the same day', () => {
    expect(importKey(tx({ type: 'expense', amount: 4500 })))
      .not.toBe(importKey(tx({ type: 'expense', amount: 4600 })));
  });
});

describe('exportFilename', () => {
  it('says what it is and when it was taken', () => {
    expect(exportFilename(new Date(2026, 7, 5))).toBe('synabit-finance-2026-08-05.csv');
  });
});
