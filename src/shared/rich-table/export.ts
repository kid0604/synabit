/**
 * A view of a Rich Table, out of the app: as CSV, or as the rows of an Excel
 * sheet. What the view shows is what goes — its columns, its rows, in its
 * order — because that is what somebody looking at it means to send.
 */
import { formulaKind, isChecked, numberOf, type RichTable } from './model';

/** The header and the cells of a view, as the file holds them. */
export function viewGrid(table: RichTable, rows: number[], columns: number[]): string[][] {
  return [
    columns.map((c) => table.columns[c].name),
    ...rows.map((r) => columns.map((c) => table.rows[r][c] ?? '')),
  ];
}

/**
 * CSV as Excel reads it: comma-separated, quoted where needed, and a byte
 * order mark in front — without it, Excel reads UTF-8 as something else and
 * every Vietnamese word comes out mangled.
 */
export function toCsv(grid: string[][]): string {
  const cell = (raw: string) => {
    // A spreadsheet opening the file runs a cell that starts like a formula —
    // `=HYPERLINK(…)` in a note pasted from anywhere. Such text gets a `'`,
    // which shows it as text; a negative number is left a number.
    const v = /^[=+\-@\t\r]/.test(raw) && !/^[+-]?\d[\d.,]*(e[+-]?\d+)?$/i.test(raw) ? `'${raw}` : raw;
    return /[",\n\r]/.test(v) ? `"${v.replace(/"/g, '""')}"` : v;
  };
  return `﻿${grid.map((row) => row.map(cell).join(',')).join('\r\n')}\r\n`;
}

/**
 * The rows of a sheet, typed: numbers as numbers and ticks as true/false, so
 * the workbook sums and filters them the moment it is opened.
 */
export function toSheetRows(table: RichTable, rows: number[], columns: number[]): (string | number | boolean | null)[][] {
  const typed = (c: number, raw: string): string | number | boolean | null => {
    const s = raw.trim();
    if (!s) return null;
    const type = table.columns[c].type;
    if (type === 'number' || (type === 'formula' && formulaKind(s) === 'number')) return numberOf(s) ?? s;
    if (type === 'checkbox' || (type === 'formula' && formulaKind(s) === 'checkbox')) return isChecked(s);
    return s;
  };
  return [
    columns.map((c) => table.columns[c].name),
    ...rows.map((r) => columns.map((c) => typed(c, table.rows[r][c] ?? ''))),
  ];
}
