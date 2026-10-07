/**
 * Grouping rows and measuring groups: what a chart's bars, a pivot's cells
 * and a grouped table's subtotals are all made of. One implementation, so a
 * bar and the pivot cell under it can never disagree.
 *
 *     Loại                  group by a column's values
 *     Ngày by month         group a date column by day, week, month or year
 *     count                 how many rows
 *     sum([Số tiền])        sum, avg, min, max, median of a column
 */
import {
  dateOf, formatNumber, formulaKind, isBlank, isChecked, itemsOf, numberOf,
  type Column, type RichTable,
} from './model';

export type Bucket = 'day' | 'week' | 'month' | 'year';
export const BUCKETS: readonly Bucket[] = ['day', 'week', 'month', 'year'];

export interface By { column: string; bucket?: Bucket }

export function parseBy(spec: string | undefined): By | null {
  if (!spec?.trim()) return null;
  const m = /^(.*?)\s+by\s+(day|week|month|year)$/i.exec(spec.trim());
  if (m) return { column: m[1].trim().replace(/^\[(.*)\]$/, '$1'), bucket: m[2].toLowerCase() as Bucket };
  return { column: spec.trim().replace(/^\[(.*)\]$/, '$1') };
}

export function formatBy(by: By): string {
  return by.bucket ? `${by.column} by ${by.bucket}` : by.column;
}

export type MeasureFn = 'count' | 'sum' | 'avg' | 'min' | 'max' | 'median';
export const MEASURES: readonly MeasureFn[] = ['count', 'sum', 'avg', 'min', 'max', 'median'];

export interface Measure { fn: MeasureFn; column?: string }

/** `count`, `sum([Số tiền])`, `sum(Số tiền)` — the brackets are optional. */
export function parseMeasure(spec: string | undefined): Measure | null {
  if (!spec?.trim()) return null;
  const s = spec.trim();
  if (/^count(\(\s*\))?$/i.test(s)) return { fn: 'count' };
  const m = /^(sum|avg|average|min|max|median|count)\(\s*\[?(.*?)\]?\s*\)$/i.exec(s);
  if (!m) return null;
  const fn = (m[1].toLowerCase() === 'average' ? 'avg' : m[1].toLowerCase()) as MeasureFn;
  return { fn, column: m[2].trim() || undefined };
}

export function formatMeasure(m: Measure): string {
  return m.column && m.fn !== 'count' ? `${m.fn}([${m.column}])` : m.fn === 'count' && m.column ? `count([${m.column}])` : m.fn;
}

/** A spec with one column renamed; anything it cannot read is left as it is. */
export function renameInBy(spec: string | undefined, from: string, to: string): string | undefined {
  const by = parseBy(spec);
  return by && by.column === from ? formatBy({ ...by, column: to }) : spec;
}

export function renameInMeasure(spec: string | undefined, from: string, to: string): string | undefined {
  if (spec?.trim() === from) return to;
  const m = parseMeasure(spec);
  return m && m.column === from ? formatMeasure({ ...m, column: to }) : spec;
}

/** Does a spec still name a column the table has? */
export function byFits(table: RichTable, spec: string | undefined): boolean {
  const by = parseBy(spec);
  return !!by && table.columns.some((c) => c.name === by.column);
}

// ─── Keys ───────────────────────────────────────────────────────

export interface Key {
  /** What groups are told apart by. */
  key: string;
  /** What the group is called on screen. */
  label: string;
  /** Where the group sorts: option order, date, number, or the label. */
  order: number | string;
}

export const BLANK_KEY = '\u0000blank';

const pad = (n: number) => String(n).padStart(2, '0');

function isoWeek(y: number, m: number, d: number): { year: number; week: number } {
  const t = new Date(Date.UTC(y, m - 1, d));
  const day = t.getUTCDay() || 7;
  t.setUTCDate(t.getUTCDate() + 4 - day);
  const year = t.getUTCFullYear();
  const week = Math.ceil(((t.getTime() - Date.UTC(year, 0, 1)) / 86_400_000 + 1) / 7);
  return { year, week };
}

/**
 * The groups a cell falls in: one, mostly; one per item of a multi-select;
 * the blank group for an empty cell.
 */
export function keysOf(column: Column, raw: string, bucket: Bucket | undefined, locale: string): Key[] {
  const s = raw.trim();
  // An empty checkbox is an unticked one, not a missing value.
  if (!s && column.type === 'checkbox') return [{ key: '[ ]', label: '✗', order: 1 }];
  if (!s) return [{ key: BLANK_KEY, label: '', order: '\uffff' }];
  if (column.type === 'multi') {
    return itemsOf(s).map((item) => {
      const at = column.options?.indexOf(item) ?? -1;
      return { key: item, label: item, order: at === -1 ? `~${item}` : at };
    });
  }
  if (column.type === 'checkbox' || (column.type === 'formula' && formulaKind(s) === 'checkbox')) {
    const on = isChecked(s);
    return [{ key: on ? '[x]' : '[ ]', label: on ? '✓' : '✗', order: on ? 0 : 1 }];
  }
  if (column.type === 'select') {
    const at = column.options?.indexOf(s) ?? -1;
    return [{ key: s, label: s, order: at === -1 ? `~${s}` : at }];
  }
  const date = column.type === 'date' || (column.type === 'formula' && formulaKind(s) === 'date') ? dateOf(s) : null;
  if (date) {
    const b = bucket ?? 'day';
    if (b === 'year') return [{ key: `${date.y}`, label: `${date.y}`, order: `${date.y}` }];
    if (b === 'month') {
      const label = new Intl.DateTimeFormat(locale, { month: 'short', year: 'numeric', timeZone: 'UTC' })
        .format(new Date(Date.UTC(date.y, date.m - 1, 1)));
      return [{ key: `${date.y}-${pad(date.m)}`, label, order: `${date.y}-${pad(date.m)}` }];
    }
    if (b === 'week') {
      const { year, week } = isoWeek(date.y, date.m, date.d);
      const key = `${year}-W${pad(week)}`;
      return [{ key, label: key, order: key }];
    }
    const key = `${date.y}-${pad(date.m)}-${pad(date.d)}`;
    const label = new Intl.DateTimeFormat(locale, { day: 'numeric', month: 'short', year: 'numeric', timeZone: 'UTC' })
      .format(new Date(Date.UTC(date.y, date.m - 1, date.d)));
    return [{ key, label, order: key }];
  }
  const n = column.type === 'number' || column.type === 'formula' ? numberOf(s) : null;
  if (n !== null) return [{ key: s, label: formatNumber(n, locale, column.format), order: n }];
  return [{ key: s, label: s, order: s.toLocaleLowerCase(locale) }];
}

export function compareKeys(a: Key, b: Key): number {
  if (typeof a.order === 'number' && typeof b.order === 'number') return a.order - b.order;
  if (typeof a.order === 'number') return -1;
  if (typeof b.order === 'number') return 1;
  return a.order < b.order ? -1 : a.order > b.order ? 1 : 0;
}

export interface Group extends Key {
  /** Rows of the table, in the order they were given. */
  rows: number[];
}

/**
 * Rows in groups, the groups in order. A row in a multi-select column lands
 * in a group per item, as it does in Notion's board.
 */
export function groupRows(table: RichTable, rows: number[], by: By, locale: string): Group[] {
  const at = table.columns.findIndex((c) => c.name === by.column);
  if (at === -1) return [{ key: '', label: '', order: 0, rows }];
  const column = table.columns[at];
  const groups = new Map<string, Group>();
  for (const r of rows) {
    for (const k of keysOf(column, table.rows[r][at] ?? '', by.bucket, locale)) {
      let g = groups.get(k.key);
      if (!g) {
        g = { ...k, rows: [] };
        groups.set(k.key, g);
      }
      g.rows.push(r);
    }
  }
  return [...groups.values()].sort(compareKeys);
}

// ─── Measuring ──────────────────────────────────────────────────

/** A measure of some rows, as a number; `null` when there is nothing to measure. */
export function measure(table: RichTable, rows: number[], m: Measure): number | null {
  if (m.fn === 'count' && !m.column) return rows.length;
  const at = table.columns.findIndex((c) => c.name === m.column);
  if (at === -1) return null;
  if (m.fn === 'count') return rows.filter((r) => !isBlank(table.rows[r][at] ?? '')).length;
  const xs = rows.map((r) => numberOf(table.rows[r][at] ?? '')).filter((n): n is number => n !== null);
  if (!xs.length) return null;
  switch (m.fn) {
    case 'sum': return xs.reduce((a, b) => a + b, 0);
    case 'avg': return xs.reduce((a, b) => a + b, 0) / xs.length;
    case 'min': return xs.reduce((a, b) => (b < a ? b : a), Infinity);
    case 'max': return xs.reduce((a, b) => (b > a ? b : a), -Infinity);
    case 'median': {
      const s = [...xs].sort((a, b) => a - b);
      const mid = s.length >> 1;
      return s.length % 2 ? s[mid] : (s[mid - 1] + s[mid]) / 2;
    }
    default: return null;
  }
}

/** How a measure's numbers are written: in its column's format, or as a count. */
export function measureFormat(table: RichTable, m: Measure): string | undefined {
  if (m.fn === 'count') return 'integer';
  return table.columns.find((c) => c.name === m.column)?.format;
}
