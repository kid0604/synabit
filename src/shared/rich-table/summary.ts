/**
 * The footer: one summary per column, over the rows the view shows — so a
 * filtered view sums what it filtered to, as Notion's footer and Excel's
 * totals row do. Values that do not fit the column are left out of the sums
 * rather than read as zero.
 */
import {
  dateOf, dateValue, formatDate, formatNumber, isBlank, isChecked, numberOf,
  type Column,
} from './model';

export type SummaryKind =
  | 'count' | 'filled' | 'empty' | 'unique' | 'percent_empty' | 'percent_filled'
  | 'sum' | 'avg' | 'median' | 'min' | 'max' | 'range'
  | 'earliest' | 'latest'
  | 'checked' | 'unchecked' | 'percent_checked';

const GENERAL: SummaryKind[] = ['count', 'filled', 'empty', 'unique', 'percent_filled', 'percent_empty'];

export function summariesFor(column: Column): SummaryKind[] {
  switch (column.type) {
    case 'number':
    case 'formula':
      return ['sum', 'avg', 'median', 'min', 'max', 'range', ...GENERAL];
    case 'date': return ['earliest', 'latest', 'range', ...GENERAL];
    case 'checkbox': return ['checked', 'unchecked', 'percent_checked', 'count'];
    default: return GENERAL;
  }
}

export function isSummaryKind(column: Column, kind: string | undefined): kind is SummaryKind {
  return !!kind && summariesFor(column).includes(kind as SummaryKind);
}

const percent = (part: number, whole: number, locale: string) =>
  whole ? formatNumber(part / whole, locale, 'percent') : '—';

/**
 * A summary of `values` (the raw cells of one column), as text to show, or
 * `null` when there is nothing to say: the sum of no numbers.
 */
export function summarize(column: Column, kind: SummaryKind, values: string[], locale: string): string | null {
  const count = (n: number) => formatNumber(n, locale, 'integer');
  const filled = values.filter((v) => !isBlank(v));
  switch (kind) {
    case 'count': return count(values.length);
    case 'filled': return count(filled.length);
    case 'empty': return count(values.length - filled.length);
    case 'unique': return count(new Set(filled.map((v) => v.trim())).size);
    case 'percent_filled': return percent(filled.length, values.length, locale);
    case 'percent_empty': return percent(values.length - filled.length, values.length, locale);
    case 'checked': return count(values.filter(isChecked).length);
    case 'unchecked': return count(values.filter((v) => !isChecked(v)).length);
    case 'percent_checked': return percent(values.filter(isChecked).length, values.length, locale);
  }

  if (column.type === 'date') {
    const dates = filled.map(dateOf).filter((d) => d !== null);
    if (!dates.length) return null;
    const sorted = [...dates].sort((a, b) => dateValue(a) - dateValue(b));
    if (kind === 'earliest') return formatDate(sorted[0], locale);
    if (kind === 'latest') return formatDate(sorted[sorted.length - 1], locale);
    if (kind === 'range') {
      const days = Math.round((dateValue(sorted[sorted.length - 1]) - dateValue(sorted[0])) / 86_400_000);
      return `${count(days)} d`;
    }
    return null;
  }

  const numbers = filled.map(numberOf).filter((n) => n !== null);
  if (!numbers.length) return null;
  const fmt = (n: number) => formatNumber(n, locale, column.format);
  const sum = numbers.reduce((a, b) => a + b, 0);
  const sorted = [...numbers].sort((a, b) => a - b);
  switch (kind) {
    case 'sum': return fmt(sum);
    case 'avg': return fmt(sum / numbers.length);
    case 'median': {
      const mid = sorted.length >> 1;
      return fmt(sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2);
    }
    case 'min': return fmt(sorted[0]);
    case 'max': return fmt(sorted[sorted.length - 1]);
    case 'range': return fmt(sorted[sorted.length - 1] - sorted[0]);
    default: return null;
  }
}

/** What the status strip shows for a selection of cells: the numbers among them. */
export function selectionStats(values: { raw: string; column: Column }[]):
  { sum: number; avg: number; count: number; min: number; max: number } | null {
  const numbers = values
    .filter((v) => v.column.type === 'number')
    .map((v) => numberOf(v.raw))
    .filter((n) => n !== null);
  if (!numbers.length) return null;
  const sum = numbers.reduce((a, b) => a + b, 0);
  return {
    sum,
    avg: sum / numbers.length,
    count: numbers.length,
    // A loop, not a spread: a spread of 100 000 numbers overflows the stack.
    min: numbers.reduce((a, b) => (b < a ? b : a), Infinity),
    max: numbers.reduce((a, b) => (b > a ? b : a), -Infinity),
  };
}
