/**
 * The way in and the way out.
 *
 * Synabit says it has no vendor lock-in, and until now the only way out of
 * Finance was to read the JSON yourself. That is a promise with an asterisk.
 * Import matters for the same reason from the other side: nobody starts a
 * ledger empty, and "type in the last two years by hand" is how a switch stops
 * halfway.
 *
 * # What crosses the boundary
 *
 * Amounts leave in whole units — `12.50`, not `1250` — because the file is for
 * a person and a spreadsheet, neither of which knows what a minor unit is. The
 * currency travels in its own column so the number is never ambiguous.
 *
 * Accounts and categories travel as **names**, not ids. An id means nothing in
 * another program, and the whole point of leaving is that the file has to be
 * readable somewhere else.
 */

import { currencyScale, currentCurrency, toMajor } from './currency';
import { categoryName } from './categories';
import type { Category, FinanceAccount, Transaction } from './types';

// ---------------------------------------------------------------------------
// The format itself
// ---------------------------------------------------------------------------

/** One field, quoted if it holds anything that would otherwise break a row. */
function quote(value: string): string {
  return /[",\n\r]/.test(value) ? `"${value.replace(/"/g, '""')}"` : value;
}

/** Rows to CSV text, with the header first. */
export function toCsv(header: string[], rows: string[][]): string {
  return [header, ...rows].map((row) => row.map(quote).join(',')).join('\r\n');
}

/**
 * CSV text to rows.
 *
 * Written out rather than pulled in as a dependency, because the subset that
 * matters is small and well defined: quoted fields, doubled quotes inside
 * them, and newlines that belong to a field rather than ending the row.
 * Everything a spreadsheet exports fits in that.
 */
export function parseCsv(text: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let field = '';
  let quoted = false;
  let started = false;

  const endField = () => {
    row.push(field);
    field = '';
    started = false;
  };
  const endRow = () => {
    endField();
    rows.push(row);
    row = [];
  };

  // A byte-order mark at the front of a file Excel wrote is not data.
  const body = text.replace(/^﻿/, '');

  for (let i = 0; i < body.length; i += 1) {
    const char = body[i];

    if (quoted) {
      if (char === '"') {
        if (body[i + 1] === '"') {
          field += '"';
          i += 1;
        } else {
          quoted = false;
        }
      } else {
        field += char;
      }
      continue;
    }

    if (char === '"' && !started) {
      quoted = true;
      started = true;
    } else if (char === ',') {
      endField();
    } else if (char === '\r') {
      // Swallowed; the newline that follows ends the row.
    } else if (char === '\n') {
      endRow();
    } else {
      field += char;
      started = true;
    }
  }

  // A file that does not end in a newline still has a last row in it.
  if (field !== '' || row.length > 0) endRow();

  return rows.filter((r) => r.some((cell) => cell.trim() !== ''));
}

// ---------------------------------------------------------------------------
// Out
// ---------------------------------------------------------------------------

export const EXPORT_HEADER = [
  'Date',
  'Type',
  'Amount',
  'Currency',
  'Category',
  'Account',
  'To account',
  'Note',
  // Kept so that leaving and coming back does not lose the receipts. It is a
  // vault-relative path, which means nothing to a spreadsheet and everything
  // to an import back into Synabit.
  'Receipt',
];

export interface ExportContext {
  accounts: FinanceAccount[];
  categories: Category[];
  currency: string;
}

const accountName = (accounts: FinanceAccount[], id: string | undefined): string =>
  id ? (accounts.find((a) => a.id === id)?.name ?? id) : '';

/** An amount as a person writes it: whole units, with the currency's precision. */
export function amountForExport(minor: number, currency: string): string {
  return toMajor(minor, currency).toFixed(currencyScale(currency));
}

/** Every transaction as a row, oldest first — the order a ledger reads in. */
export function exportRows(transactions: Transaction[], ctx: ExportContext): string[][] {
  return [...transactions]
    .sort((a, b) => a.date.localeCompare(b.date))
    .map((tx) => [
      tx.date.slice(0, 10),
      tx.type,
      amountForExport(tx.amount, ctx.currency),
      ctx.currency,
      tx.type === 'transfer' ? '' : categoryName(ctx.categories, tx.category),
      accountName(ctx.accounts, tx.accountId),
      accountName(ctx.accounts, tx.toAccountId),
      tx.note ?? '',
      tx.receipt ?? '',
    ]);
}

/** The whole ledger as one CSV file. */
export function exportCsv(transactions: Transaction[], ctx: ExportContext): string {
  return toCsv(EXPORT_HEADER, exportRows(transactions, ctx));
}

// ---------------------------------------------------------------------------
// In
// ---------------------------------------------------------------------------

/** Which column of the file holds what. `-1` means the file does not have it. */
export interface ColumnMap {
  date: number;
  type: number;
  amount: number;
  category: number;
  account: number;
  toAccount: number;
  note: number;
  receipt: number;
}

/** The header names this recognises, beyond its own. */
const ALIASES: Record<keyof ColumnMap, string[]> = {
  date: ['date', 'ngày', 'transaction date', 'time'],
  type: ['type', 'loại', 'kind'],
  amount: ['amount', 'số tiền', 'value', 'sum'],
  category: ['category', 'danh mục', 'hạng mục'],
  account: ['account', 'tài khoản', 'from account', 'wallet'],
  toAccount: ['to account', 'tài khoản đến', 'destination'],
  note: ['note', 'ghi chú', 'description', 'memo', 'notes'],
  receipt: ['receipt', 'hoá đơn', 'attachment'],
};

/**
 * Work out which column is which from the header row.
 *
 * By name rather than by position, because the file was probably written by
 * something else. What it cannot recognise it reports as missing, and the
 * caller asks the user rather than guessing — a column guessed wrong puts the
 * date in the amount.
 */
export function matchColumns(header: string[]): ColumnMap {
  const normalised = header.map((h) => h.trim().toLowerCase());
  const find = (names: string[]) =>
    normalised.findIndex((h) => names.includes(h));

  return {
    date: find(ALIASES.date),
    type: find(ALIASES.type),
    amount: find(ALIASES.amount),
    category: find(ALIASES.category),
    account: find(ALIASES.account),
    toAccount: find(ALIASES.toAccount),
    note: find(ALIASES.note),
    receipt: find(ALIASES.receipt),
  };
}

/** The columns without which a row cannot become a transaction. */
export function missingColumns(map: ColumnMap): string[] {
  return (['date', 'amount'] as const).filter((key) => map[key] < 0);
}

export interface ImportContext {
  accounts: FinanceAccount[];
  categories: Category[];
  currency: string;
  /** What is already in the ledger, so the same row is not imported twice. */
  existing: Transaction[];
}

export interface ImportProblem {
  /** The row's line in the file, counting the header as line 1. */
  line: number;
  reason: string;
}

export interface ImportResult {
  ready: Transaction[];
  duplicates: number;
  problems: ImportProblem[];
}

/**
 * What makes two transactions the same import.
 *
 * Deliberately not the id: a row from another program has no id, and an import
 * run twice has to recognise its own work. Day, direction, amount and account
 * is what a person would compare, and two genuinely separate coffees on one day
 * from one account for one amount are indistinguishable to anybody — including
 * the person who bought them.
 */
export function importKey(tx: Pick<Transaction, 'date' | 'type' | 'amount' | 'accountId' | 'category'>): string {
  return [tx.date.slice(0, 10), tx.type, tx.amount, tx.accountId, tx.category].join('|');
}

/** Read a date in any of the shapes a spreadsheet is likely to have written. */
export function parseDate(raw: string): string | null {
  const text = raw.trim();
  if (!text) return null;

  const iso = /^(\d{4})-(\d{2})-(\d{2})/.exec(text);
  if (iso) return `${iso[1]}-${iso[2]}-${iso[3]}`;

  // Day-first, which is what this app writes and what most of the world does.
  const dayFirst = /^(\d{1,2})[/-](\d{1,2})[/-](\d{4})$/.exec(text);
  if (dayFirst) {
    const [, d, m, y] = dayFirst;
    return `${y}-${m.padStart(2, '0')}-${d.padStart(2, '0')}`;
  }
  return null;
}

/**
 * An amount from a file, in minor units.
 *
 * A file can come from anywhere, so both conventions have to be readable:
 * `1,234.56` and `1.234,56` are the same number. Which separator is the
 * decimal point is decided by three rules, in order:
 *
 * 1. If both appear, the later one divides and the other groups.
 * 2. If one appears more than once, it groups — nothing has two decimal points.
 * 3. If one appears once with exactly three digits after it, it groups. That is
 *    the genuinely ambiguous case (`1.234`), and grouping is the reading that
 *    matches how bank exports are written.
 */
export function parseImportedAmount(raw: string, currency: string): number | null {
  const text = raw.trim().replace(/[^\d.,-]/g, '');
  if (!/\d/.test(text)) return null;

  const negative = text.trimStart().startsWith('-');
  const body = text.replace(/-/g, '');

  const dots = (body.match(/\./g) ?? []).length;
  const commas = (body.match(/,/g) ?? []).length;

  let decimal: string | null = null;
  if (dots > 0 && commas > 0) {
    decimal = body.lastIndexOf('.') > body.lastIndexOf(',') ? '.' : ',';
  } else if (dots === 1 || commas === 1) {
    const mark = dots === 1 ? '.' : ',';
    const after = body.length - body.lastIndexOf(mark) - 1;
    decimal = after === 3 ? null : mark;
  }

  const cut = decimal ? body.lastIndexOf(decimal) : -1;
  const whole = (cut >= 0 ? body.slice(0, cut) : body).replace(/\D/g, '');
  const fractionText = cut >= 0 ? body.slice(cut + 1).replace(/\D/g, '') : '';

  const scale = currencyScale(currency);
  const fraction = fractionText.slice(0, scale).padEnd(scale, '0');

  const value = Number(`${whole || '0'}${fraction}`);
  if (!Number.isFinite(value)) return null;
  return negative ? -value : value;
}

/** Turn parsed rows into transactions, reporting what could not be used. */
export function importRows(
  rows: string[][],
  map: ColumnMap,
  ctx: ImportContext,
): ImportResult {
  const seen = new Set(ctx.existing.map(importKey));
  const byAccountName = new Map(ctx.accounts.map((a) => [a.name.trim().toLowerCase(), a.id]));
  const byCategoryName = new Map(ctx.categories.map((c) => [c.name.trim().toLowerCase(), c.id]));
  const fallbackAccount = ctx.accounts[0]?.id ?? '';

  const ready: Transaction[] = [];
  const problems: ImportProblem[] = [];
  let duplicates = 0;

  const cell = (row: string[], index: number) => (index >= 0 ? (row[index] ?? '').trim() : '');

  rows.forEach((row, index) => {
    const line = index + 2;

    const date = parseDate(cell(row, map.date));
    if (!date) {
      problems.push({ line, reason: `Could not read the date "${cell(row, map.date)}"` });
      return;
    }

    const amount = parseImportedAmount(cell(row, map.amount), ctx.currency);
    if (amount === null || amount === 0) {
      problems.push({ line, reason: `Could not read the amount "${cell(row, map.amount)}"` });
      return;
    }

    // A negative amount in a file that has no type column is money going out;
    // that is how nearly every bank writes a statement.
    const rawType = cell(row, map.type).toLowerCase();
    const type: Transaction['type'] =
      rawType === 'income' || rawType === 'transfer' || rawType === 'expense'
        ? rawType
        : amount < 0
          ? 'expense'
          : 'income';

    const accountId = byAccountName.get(cell(row, map.account).toLowerCase()) ?? fallbackAccount;
    if (!accountId) {
      problems.push({ line, reason: 'There is no account to file this against' });
      return;
    }

    const categoryText = cell(row, map.category);
    const category =
      type === 'transfer'
        ? 'Transfer'
        : (byCategoryName.get(categoryText.toLowerCase()) ?? categoryText);

    const tx: Transaction = {
      id: `tx-import-${date}-${Math.abs(amount)}-${index}`,
      type,
      amount: Math.abs(amount),
      category,
      accountId,
      date: `${date}T12:00:00.000Z`,
      note: cell(row, map.note),
    };

    const toAccount = byAccountName.get(cell(row, map.toAccount).toLowerCase());
    if (type === 'transfer' && toAccount) tx.toAccountId = toAccount;

    // Only a path inside the vault's own assets folder: an import is a file
    // from somewhere else, and letting it name any path on this machine would
    // let it point the ledger at anything.
    const receipt = cell(row, map.receipt);
    if (receipt.startsWith('assets/') && !receipt.includes('..')) tx.receipt = receipt;

    const key = importKey(tx);
    if (seen.has(key)) {
      duplicates += 1;
      return;
    }
    seen.add(key);
    ready.push(tx);
  });

  return { ready, duplicates, problems };
}

/** A filename that says what is in it and when it was taken. */
export function exportFilename(now: Date = new Date()): string {
  const pad = (n: number) => String(n).padStart(2, '0');
  return `synabit-finance-${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}.csv`;
}

/** The currency an export was taken in, for the caller that has no context. */
export const defaultExportCurrency = () => currentCurrency.value;
