/**
 * A Rich Table: typed columns over a Markdown pipe table.
 *
 * Every cell is kept as the text the file holds — `45000`, `2026-10-01`,
 * `[x]` — and given a type only when it is read. That is what lets a value
 * that does not fit its column survive: `abc` in a number column stays `abc`,
 * shown with a mark, and skipped when summing. Nothing here turns a value
 * into something else behind anybody's back. See
 * `docs/rich-table-2026-10-05.md` §4.3.
 */

export type ColumnType =
  | 'text'
  | 'number'
  | 'select'
  | 'multi'
  | 'date'
  | 'checkbox'
  | 'url'
  | 'note'
  | 'formula';

export const COLUMN_TYPES: readonly ColumnType[] = [
  'text', 'number', 'select', 'multi', 'date', 'checkbox', 'url', 'note', 'formula',
];

export const NUMBER_FORMATS = [
  'number', 'integer', 'decimal:1', 'decimal:2', 'percent', 'vnd', 'usd', 'eur',
] as const;

/** The colours an option can carry; the names are what the file holds. */
export const OPTION_COLORS = [
  'gray', 'red', 'orange', 'yellow', 'green', 'blue', 'purple', 'pink',
] as const;

export interface Column {
  name: string;
  type: ColumnType;
  /** Select and multi: the choices, in the order they sort. */
  options?: string[];
  colors?: Record<string, string>;
  /** Number, and a formula that gives one: one of `NUMBER_FORMATS`. */
  format?: string;
  /** Formula: what it computes, in the formula language (`src/shared/formula`). */
  expr?: string;
  /** Date: carries a time as well. */
  time?: boolean;
  width?: number;
  wrap?: boolean;
  /**
   * A type the file names and this version does not know — `formula`, from a
   * later one. The column is shown as text, cannot be edited, and is written
   * back exactly as it was declared.
   */
  foreignType?: string;
  /** Keys of the declaration this version does not know, kept as found. */
  rest?: Record<string, unknown>;
}

export type Layout = 'table' | 'chart' | 'pivot' | 'board';
export const LAYOUTS: readonly Layout[] = ['table', 'chart', 'pivot', 'board'];

export type ChartKind = 'bar' | 'hbar' | 'line' | 'area' | 'donut' | 'scatter' | 'number';
export const CHART_KINDS: readonly ChartKind[] = ['bar', 'hbar', 'line', 'area', 'donut', 'scatter', 'number'];

/**
 * A chart, as the file holds it. `x` and `series` are a column, a date column
 * with its bucket (`Ngày by month`); `y` is what is measured (`count`,
 * `sum([Số tiền])`) — or, for a scatter, a second column.
 */
export interface ChartSpec {
  kind?: string;
  x?: string;
  y?: string;
  series?: string;
  stack?: boolean;
  cumulative?: boolean;
  [key: string]: unknown;
}

/** A pivot: rows by one column, columns by another, each cell one measure. */
export interface PivotSpec {
  rows?: string;
  columns?: string;
  value?: string;
  [key: string]: unknown;
}

/** A board: one lane per option of a select column, one card per row. */
export interface BoardSpec {
  by?: string;
  /** The column whose value heads a card; the first text column when unset. */
  title?: string;
  /** Columns shown on a card under its title. */
  show?: string[];
  [key: string]: unknown;
}

/**
 * A colouring rule. Either a condition — `when` — colouring the row, or the
 * cells of `columns` (the columns the condition names, when unset); or a
 * `scale` over one number column, from its first colour at the lowest value
 * to its second at the highest.
 */
export interface Rule {
  when?: string;
  style?: { row?: string; cell?: string };
  columns?: string[];
  scale?: string[];
  column?: string;
  [key: string]: unknown;
}

export interface View {
  name?: string;
  layout?: string;
  /** A condition in the formula language; see `filter.ts`. */
  filter?: string;
  /** Column names, `-` in front for descending. */
  sort?: string[];
  /** Column name → summary kind; see `summary.ts`. */
  summary?: Record<string, string>;
  /** A column to group rows by, with its bucket when it is a date: `Ngày by month`. */
  group?: string;
  /** Columns this view does not show. */
  hide?: string[];
  rules?: Rule[];
  /** For a chart view, its chart; for a table view, a chart shown above the grid. */
  chart?: ChartSpec;
  pivot?: PivotSpec;
  board?: BoardSpec;
  /** How tall rows are: `short` (one line, the default), `medium`, `tall`, `extra`. */
  rowHeight?: string;
  /** Column widths this view sets, by column name; a column without one keeps its own. */
  widths?: Record<string, number>;
  rest?: Record<string, unknown>;
}

/**
 * A table's name: one word, hyphens allowed between its parts — `chi-tieu`.
 * Never `--` or a trailing hyphen, which an HTML comment could end on.
 */
export const TABLE_NAME = /^[\p{L}_][\p{L}\p{N}_]*(?:-[\p{L}\p{N}_]+)*$/u;

export const ROW_HEIGHTS = ['short', 'medium', 'tall', 'extra'] as const;
export type RowHeight = typeof ROW_HEIGHTS[number];
export function rowHeightOf(view: View | undefined): RowHeight {
  return ROW_HEIGHTS.includes(view?.rowHeight as RowHeight) ? (view!.rowHeight as RowHeight) : 'short';
}

export function layoutOf(view: View | undefined): Layout {
  return LAYOUTS.includes(view?.layout as Layout) ? (view!.layout as Layout) : 'table';
}

export interface RichTable {
  name?: string;
  columns: Column[];
  /** One array per row, one string per column, as the file holds them. */
  rows: string[][];
  /** How many columns stay put on the left while the rest scroll. */
  freeze?: number;
  /** Always at least one. Phase 1 shows only the first. */
  views: View[];
  /**
   * Declarations for columns the table no longer has — renamed in another
   * editor, say. Kept and written back, because the alternative is losing a
   * formula to a typo in a header.
   */
  orphans?: Record<string, unknown>;
  /** Top-level keys this version does not know. */
  rest?: Record<string, unknown>;
}

export function isColumnType(value: unknown): value is ColumnType {
  return COLUMN_TYPES.includes(value as ColumnType);
}

/**
 * A column that cannot be typed into: a formula's, which computes its cells,
 * and one whose type this version does not know.
 */
export function isReadOnlyColumn(column: Column): boolean {
  return column.foreignType !== undefined || column.type === 'formula';
}

const ERROR_CELL = /^#(NAME|TYPE|DIV0|CYCLE|VALUE)$/;

/** Is this cell a formula's error, as written: `#DIV0`? */
export function isErrorCell(raw: string): boolean {
  return ERROR_CELL.test(raw.trim());
}

/**
 * What a formula cell holds, read without a declared type: a number, a date,
 * a tick, an error — or text. A formula's column has no one type; each of its
 * cells is shown and sorted as what it is.
 */
export function formulaKind(raw: string): 'number' | 'date' | 'checkbox' | 'error' | 'text' | 'blank' {
  const s = raw.trim();
  if (!s) return 'blank';
  if (numberOf(s) !== null) return 'number';
  if (/^\[[ xX]\]$/.test(s)) return 'checkbox';
  if (ERROR_CELL.test(s)) return 'error';
  if (dateOf(s)) return 'date';
  return 'text';
}

// ─── Reading a cell ─────────────────────────────────────────────

const NUMBER = /^-?(\d+(\.\d*)?|\.\d+)([eE][-+]?\d+)?$/;

/** The number a cell holds, or `null` when it holds something else. */
export function numberOf(raw: string): number | null {
  const s = raw.trim();
  if (!NUMBER.test(s)) return null;
  const n = Number(s);
  return Number.isFinite(n) ? n : null;
}

const DATE = /^(\d{4})-(\d{2})-(\d{2})(?:[ T](\d{2}):(\d{2})(?::(\d{2}))?)?$/;

export interface DateParts {
  y: number; m: number; d: number;
  h?: number; min?: number;
}

export function dateOf(raw: string): DateParts | null {
  const match = DATE.exec(raw.trim());
  if (!match) return null;
  const [, y, m, d, h, min] = match;
  const parts: DateParts = { y: +y, m: +m, d: +d };
  // Through a Date and back, so 2026-02-31 is refused rather than read as March.
  const day = new Date(Date.UTC(parts.y, parts.m - 1, parts.d));
  if (day.getUTCMonth() !== parts.m - 1 || day.getUTCDate() !== parts.d) return null;
  if (h !== undefined) {
    parts.h = +h;
    parts.min = +min;
  }
  return parts;
}

/** A date as milliseconds, for comparing; the time is read as UTC on purpose. */
export function dateValue(parts: DateParts): number {
  return Date.UTC(parts.y, parts.m - 1, parts.d, parts.h ?? 0, parts.min ?? 0);
}

export function isChecked(raw: string): boolean {
  return /^\[[xX]\]$/.test(raw.trim());
}

export function itemsOf(raw: string): string[] {
  return raw.split(',').map((s) => s.trim()).filter(Boolean);
}

export function isBlank(raw: string): boolean {
  return raw.trim() === '';
}

/** Does the cell hold what its column says it holds? A blank cell always does. */
export function fits(column: Column, raw: string): boolean {
  if (isBlank(raw)) return true;
  switch (column.type) {
    case 'number': return numberOf(raw) !== null;
    case 'date': return dateOf(raw) !== null;
    case 'checkbox': return /^\[[ xX]\]$/.test(raw.trim());
    default: return true;
  }
}

// ─── Writing a cell from what somebody typed ────────────────────

/**
 * The group and decimal separators of a locale: `.` and `,` in Vietnamese,
 * the other way round in English.
 */
export function separators(locale: string): { group: string; decimal: string } {
  const parts = new Intl.NumberFormat(locale).formatToParts(12345.6);
  return {
    group: parts.find((p) => p.type === 'group')?.value ?? ',',
    decimal: parts.find((p) => p.type === 'decimal')?.value ?? '.',
  };
}

/**
 * A number as somebody would type it, read in their locale.
 *
 * The file always holds `45000.5`. What gets typed may be `45.000,5`
 * (Vietnamese), `45,000.5` (English), `45000.5`, `7,5%` or `45.000 ₫`. The
 * rule for a lone separator: if it is the locale's decimal separator, it is
 * one; if it is the group separator and every group after it has exactly
 * three digits, it groups; otherwise it is a decimal point after all. So in
 * Vietnamese `45.000` is forty-five thousand and `1.5` is one and a half.
 */
export function parseNumberInput(input: string, locale: string): number | null {
  let s = input.trim();
  if (!s) return null;
  const percent = s.endsWith('%');
  s = s.replace(/%$/, '').replace(/[\s  ]/g, '')
    .replace(/^[₫$€]|[₫$€]$|VND$|USD$|EUR$|đ$/gi, '');
  const { group, decimal } = separators(locale);
  // `45.000` is the file's own form for forty-five, and a Vietnamese hand's
  // forty-five thousand; only the locale can say which, so the shortcut is
  // taken only where a dot cannot be a group separator.
  if (NUMBER.test(s) && !(group === '.' && s.includes('.'))) return scale(Number(s), percent);

  const negative = s.startsWith('-');
  if (negative) s = s.slice(1);
  if (!/^[\d.,]+$/.test(s) || !/\d/.test(s)) return null;

  const hasDot = s.includes('.');
  const hasComma = s.includes(',');
  let normalized: string;
  if (hasDot && hasComma) {
    const dec = s.lastIndexOf('.') > s.lastIndexOf(',') ? '.' : ',';
    const grp = dec === '.' ? ',' : '.';
    normalized = s.split(grp).join('').replace(dec, '.');
    if (normalized.split('.').length > 2) return null;
  } else {
    const sep = hasDot ? '.' : ',';
    const pieces = s.split(sep);
    const grouped = pieces.length > 1
      && pieces.slice(1).every((p) => p.length === 3)
      && pieces[0].length > 0 && pieces[0].length <= 3;
    if (pieces.length > 2 || (sep === group && grouped && sep !== decimal)) {
      if (!grouped) return null;
      normalized = pieces.join('');
    } else {
      normalized = pieces.join('.');
    }
  }
  if (!NUMBER.test(normalized)) return null;
  const n = Number(normalized);
  return Number.isFinite(n) ? scale(negative ? -n : n, percent) : null;
}

function scale(n: number, percent: boolean): number {
  // Divided by 100 as a decimal shift, so 7.5% is 0.075 and not 0.07500000000000001.
  return percent ? Number((n / 100).toPrecision(15)) : n;
}

const pad = (n: number) => String(n).padStart(2, '0');

export function formatDateRaw(parts: DateParts): string {
  const day = `${parts.y}-${pad(parts.m)}-${pad(parts.d)}`;
  return parts.h === undefined ? day : `${day} ${pad(parts.h)}:${pad(parts.min ?? 0)}`;
}

/** A date typed as `1/10/2026`: day first in Vietnamese, month first in English. */
export function parseDateInput(input: string, locale: string): DateParts | null {
  const s = input.trim();
  const iso = dateOf(s.replace('T', ' ').slice(0, 16));
  if (iso) return iso;
  const match = /^(\d{1,2})[/.-](\d{1,2})[/.-](\d{4})(?:\s+(\d{1,2}):(\d{2}))?$/.exec(s);
  if (!match) return null;
  const [, a, b, y, h, min] = match;
  const dayFirst = !locale.startsWith('en');
  const parts: DateParts = { y: +y, m: dayFirst ? +b : +a, d: dayFirst ? +a : +b };
  if (h !== undefined) {
    parts.h = +h;
    parts.min = +min;
  }
  return dateOf(formatDateRaw(parts));
}

/**
 * What the file should hold for something typed into a cell. A value that
 * cannot be read as the column's type is kept as typed — marked, never
 * dropped.
 */
export function rawFromInput(column: Column, input: string, locale: string): string {
  const s = input.replace(/\r\n?/g, '\n').trim();
  if (!s) return '';
  switch (column.type) {
    case 'number': {
      const n = parseNumberInput(s, locale);
      return n === null ? s : String(n);
    }
    case 'date': {
      const parts = parseDateInput(s, locale);
      if (!parts) return s;
      if (!column.time) {
        delete parts.h;
        delete parts.min;
      }
      return formatDateRaw(parts);
    }
    case 'checkbox':
      return /^(\[[xX]\]|x|true|yes|1|có|✓|✔)$/i.test(s) ? '[x]' : '[ ]';
    case 'multi':
      return itemsOf(s).join(', ');
    case 'note':
      return /^\[\[.*\]\]$/.test(s) ? s : `[[${s}]]`;
    default:
      return s;
  }
}

// ─── Showing a cell ─────────────────────────────────────────────

const numberFormats = new Map<string, Intl.NumberFormat>();

function numberFormat(locale: string, format: string | undefined): Intl.NumberFormat {
  const key = `${locale}|${format ?? ''}`;
  let f = numberFormats.get(key);
  if (!f) {
    const decimals = /^decimal:(\d)$/.exec(format ?? '');
    const options: Intl.NumberFormatOptions =
      format === 'integer' ? { maximumFractionDigits: 0 }
        : decimals ? { minimumFractionDigits: +decimals[1], maximumFractionDigits: +decimals[1] }
          : format === 'percent' ? { style: 'percent', maximumFractionDigits: 2 }
            : format === 'vnd' ? { style: 'currency', currency: 'VND', maximumFractionDigits: 0 }
              : format === 'usd' ? { style: 'currency', currency: 'USD' }
                : format === 'eur' ? { style: 'currency', currency: 'EUR' }
                  : { maximumFractionDigits: 6 };
    f = new Intl.NumberFormat(locale, options);
    numberFormats.set(key, f);
  }
  return f;
}

export function formatNumber(n: number, locale: string, format?: string): string {
  return numberFormat(locale, format).format(n);
}

export function formatDate(parts: DateParts, locale: string): string {
  const date = new Date(Date.UTC(parts.y, parts.m - 1, parts.d, parts.h ?? 0, parts.min ?? 0));
  return new Intl.DateTimeFormat(locale, {
    year: 'numeric', month: 'short', day: 'numeric',
    ...(parts.h !== undefined ? { hour: '2-digit', minute: '2-digit' } : {}),
    timeZone: 'UTC',
  }).format(date);
}

/**
 * What an editor opens with. A number is written the way the locale writes
 * it, without grouping — `1,234` in Vietnamese for the file's `1.234` — so
 * that reading it back with `parseNumberInput` gives the same number, which
 * the file's own form would not: Vietnamese reads `1.234` as a thousand.
 */
export function draftOf(column: Column, raw: string, locale: string): string {
  if (column.type === 'number') {
    const n = numberOf(raw);
    if (n !== null) {
      return new Intl.NumberFormat(locale, { useGrouping: false, maximumFractionDigits: 20 }).format(n);
    }
  }
  return raw;
}

/** A cell as text for showing, or for a value somebody copies out. */
export function displayText(column: Column, raw: string, locale: string): string {
  if (column.type === 'formula') {
    switch (formulaKind(raw)) {
      case 'number': return formatNumber(numberOf(raw)!, locale, column.format);
      case 'date': return formatDate(dateOf(raw)!, locale);
      case 'checkbox': return isChecked(raw) ? '✓' : '';
      default: return raw;
    }
  }
  if (isBlank(raw) || !fits(column, raw)) return raw;
  switch (column.type) {
    case 'number': return formatNumber(numberOf(raw)!, locale, column.format);
    case 'date': return formatDate(dateOf(raw)!, locale);
    case 'checkbox': return isChecked(raw) ? '✓' : '';
    case 'note': return raw.replace(/^\[\[(?:[^\]|]*\|)?([^\]]*)\]\]$/, '$1');
    default: return raw;
  }
}

/** The target of a note cell: `Sách/Sapiens` for `[[Sách/Sapiens|Sapiens]]`. */
export function noteTarget(raw: string): string | null {
  const match = /^\[\[([^\]|]+)(?:\|[^\]]*)?\]\]$/.exec(raw.trim());
  return match ? match[1].trim() : null;
}

// ─── Comparing, for sorting ─────────────────────────────────────

/**
 * Compare two cells of a column, ascending. Blank cells are not compared
 * here: the caller puts them last whichever way the sort runs, as Notion and
 * every spreadsheet do. A value that does not fit the column sorts after the
 * ones that do.
 */
export function compareCells(column: Column, a: string, b: string, collator: Intl.Collator): number {
  if (column.type === 'formula') {
    const ka = formulaKind(a);
    const kb = formulaKind(b);
    if (ka === 'number' && kb === 'number') return numberOf(a)! - numberOf(b)!;
    if (ka === 'date' && kb === 'date') return dateValue(dateOf(a)!) - dateValue(dateOf(b)!);
    if (ka === 'error' || kb === 'error') return ka === kb ? 0 : ka === 'error' ? 1 : -1;
    return collator.compare(a, b);
  }
  const fa = fits(column, a);
  const fb = fits(column, b);
  if (fa !== fb) return fa ? -1 : 1;
  if (!fa) return collator.compare(a, b);
  switch (column.type) {
    case 'number': return numberOf(a)! - numberOf(b)!;
    case 'date': return dateValue(dateOf(a)!) - dateValue(dateOf(b)!);
    case 'checkbox': return Number(isChecked(a)) - Number(isChecked(b));
    case 'select': {
      const options = column.options ?? [];
      const ia = options.indexOf(a.trim());
      const ib = options.indexOf(b.trim());
      if (ia !== ib) return (ia === -1 ? Infinity : ia) - (ib === -1 ? Infinity : ib) || 0;
      return collator.compare(a, b);
    }
    case 'multi': return collator.compare(itemsOf(a)[0] ?? '', itemsOf(b)[0] ?? '');
    default: return collator.compare(a, b);
  }
}

// ─── Building ───────────────────────────────────────────────────

export function emptyTable(columnNames: string[], rows = 3): RichTable {
  return {
    columns: columnNames.map((name) => ({ name, type: 'text' })),
    rows: Array.from({ length: rows }, () => columnNames.map(() => '')),
    views: [{}],
  };
}

/** A column name not yet taken: `Cột`, then `Cột 2`, `Cột 3`… */
export function freshColumnName(table: RichTable, base: string): string {
  const taken = new Set([...table.columns.map((c) => c.name), ...Object.keys(table.orphans ?? {})]);
  if (!taken.has(base)) return base;
  for (let i = 2; ; i++) if (!taken.has(`${base} ${i}`)) return `${base} ${i}`;
}

/**
 * Whether a name can head a column. The header row is the column's identity,
 * so it must be there and unique; `|` would split it, and `[` `]` are how a
 * filter refers to it.
 */
export function validColumnName(table: RichTable, name: string, except?: number): boolean {
  const n = name.trim();
  if (!n || /[|[\]\n]/.test(n)) return false;
  // Settings kept for a column that went missing hold that name too: a column
  // named so would take them on, or be overwritten by them.
  if (table.orphans && n in table.orphans) return false;
  return !table.columns.some((c, i) => i !== except && c.name === n);
}

/**
 * The type a column of pasted values most likely has: a number if every value
 * reads as one, a date if every value does, a checkbox if every value is a
 * tick or nothing; text otherwise. Blank values say nothing either way.
 */
export function inferType(values: string[], locale: string): ColumnType {
  const filled = values.map((v) => v.trim()).filter(Boolean);
  if (!filled.length) return 'text';
  if (filled.every((v) => /^(\[[ xX]\]|x|✓|✔|true|false)$/i.test(v))) return 'checkbox';
  if (filled.every((v) => parseNumberInput(v, locale) !== null)) return 'number';
  if (filled.every((v) => parseDateInput(v, locale) !== null)) return 'date';
  return 'text';
}
