/**
 * Which rows a view shows, in what order. Sorting here never moves a row in
 * the file; that takes `reorderRows`, on purpose.
 */
import { parseFilter, passes, type Condition } from './filter';
import { formulaFilter } from './formulas';
import { compareCells, fits, isBlank, type RichTable, type View } from './model';

export interface SortKey { column: number; descending: boolean }

export function sortKeys(table: RichTable, view: View): SortKey[] {
  return (view.sort ?? [])
    .map((s) => {
      const descending = s.startsWith('-');
      const column = table.columns.findIndex((c) => c.name === (descending ? s.slice(1) : s));
      return { column, descending };
    })
    .filter((k) => k.column !== -1);
}

export interface Shown {
  /** Indices into `table.rows`, in the order shown. */
  rows: number[];
  /** The view's filter does not compile, and so was not applied. */
  filterUnread: boolean;
}

/**
 * @param keep rows shown whatever the filter says — one just added, which
 *   would otherwise vanish the moment it was made in a filtered view.
 * @param search a quick search over every cell, never saved.
 */
export function shownRows(
  table: RichTable,
  view: View,
  locale: string,
  keep: ReadonlySet<number> = new Set(),
  search = '',
): Shown {
  const conditions = parseFilter(view.filter);
  const columnOf = (c: Condition) => table.columns.findIndex((col) => col.name === c.column);
  const active = (conditions ?? [])
    .map((c) => ({ c, at: columnOf(c) }))
    .filter(({ at }) => at !== -1);
  const needle = search.trim().toLocaleLowerCase(locale);
  // More than pills can say — `or`, arithmetic, functions — is run as a formula.
  const advanced = conditions === null && view.filter ? formulaFilter(table, view.filter) : null;
  const unread = conditions === null && !advanced;

  let rows = table.rows.map((_, i) => i).filter((i) => {
    if (keep.has(i)) return true;
    const row = table.rows[i];
    if (advanced && !advanced(i)) return false;
    if (!active.every(({ c, at }) => passes(c, table.columns[at], row[at] ?? '', locale))) return false;
    return !needle || row.some((cell) => cell.toLocaleLowerCase(locale).includes(needle));
  });

  const keys = sortKeys(table, view);
  if (keys.length) {
    const collator = new Intl.Collator(locale, { numeric: true, sensitivity: 'base' });
    rows = rows.slice().sort((a, b) => {
      for (const { column, descending } of keys) {
        const x = table.rows[a][column] ?? '';
        const y = table.rows[b][column] ?? '';
        // Blank last, whichever way the sort runs.
        const bx = isBlank(x);
        const by = isBlank(y);
        if (bx !== by) return bx ? 1 : -1;
        if (bx) continue;
        // So is a value that does not fit the column: last but for the blanks.
        const col = table.columns[column];
        const fx = fits(col, x);
        const fy = fits(col, y);
        if (fx !== fy) return fx ? -1 : 1;
        const c = compareCells(col, x, y, collator);
        if (c) return descending ? -c : c;
      }
      return a - b;
    });
  }
  return { rows, filterUnread: unread };
}
