/**
 * Every change a Rich Table can undergo, as a function from one table to the
 * next. None of them touches its argument: the node view hands the result to
 * ProseMirror as a new attribute, and the editor's history keeps the old one
 * for undo.
 */
import { filterText, parseFilter, type Condition } from './filter';
import { renameInFormula, renameQualified } from '../formula';
import { parseBy, parseMeasure, renameInBy, renameInMeasure } from './aggregate';
import {
  dateOf, dateValue, formatDateRaw, isBlank, isChecked, itemsOf, numberOf,
  type Column, type ColumnType, type DateParts, type RichTable, type View,
} from './model';

export interface CellEdit { row: number; col: number; raw: string }

export function setCells(table: RichTable, edits: CellEdit[]): RichTable {
  const changed = edits.filter((e) => (table.rows[e.row]?.[e.col] ?? '') !== e.raw);
  if (!changed.length) return table;
  const rows = table.rows.slice();
  const touched = new Set<number>();
  let columns = table.columns;
  for (const { row, col, raw } of changed) {
    if (!rows[row]) continue;
    if (!touched.has(row)) {
      rows[row] = rows[row].slice();
      touched.add(row);
    }
    rows[row][col] = raw;
    columns = withOptionsFor(columns, col, raw);
  }
  return { ...table, rows, columns };
}

/**
 * A select value nobody declared becomes an option, as typing a new tag in
 * Notion does — so the column's choices are always the ones in use.
 */
function withOptionsFor(columns: Column[], col: number, raw: string): Column[] {
  const column = columns[col];
  if (!column || (column.type !== 'select' && column.type !== 'multi')) return columns;
  const values = column.type === 'multi' ? itemsOf(raw) : isBlank(raw) ? [] : [raw.trim()];
  const missing = values.filter((v) => !(column.options ?? []).includes(v));
  if (!missing.length) return columns;
  const next = columns.slice();
  next[col] = { ...column, options: [...(column.options ?? []), ...missing] };
  return next;
}

export function insertRows(table: RichTable, at: number, rows: string[][] | number): RichTable {
  const width = table.columns.length;
  const fresh = typeof rows === 'number'
    ? Array.from({ length: rows }, () => Array<string>(width).fill(''))
    : rows.map((r) => Array.from({ length: width }, (_, i) => r[i] ?? ''));
  const next = table.rows.slice();
  next.splice(at, 0, ...fresh);
  let result: RichTable = { ...table, rows: next };
  if (typeof rows !== 'number') {
    fresh.forEach((r) => r.forEach((raw, col) => {
      result = { ...result, columns: withOptionsFor(result.columns, col, raw) };
    }));
  }
  return result;
}

export function deleteRows(table: RichTable, indices: number[]): RichTable {
  const drop = new Set(indices);
  return { ...table, rows: table.rows.filter((_, i) => !drop.has(i)) };
}

export function moveRow(table: RichTable, from: number, to: number): RichTable {
  if (to < 0 || to >= table.rows.length || from === to) return table;
  const rows = table.rows.slice();
  const [row] = rows.splice(from, 1);
  rows.splice(to, 0, row);
  return { ...table, rows };
}

/** Put the rows in the order a view shows them, for good. */
export function reorderRows(table: RichTable, order: number[]): RichTable {
  const rest = table.rows.map((_, i) => i).filter((i) => !order.includes(i));
  return { ...table, rows: [...order, ...rest].map((i) => table.rows[i]) };
}

export function insertColumn(table: RichTable, at: number, column: Column): RichTable {
  const columns = table.columns.slice();
  columns.splice(at, 0, column);
  const rows = table.rows.map((r) => {
    const next = r.slice();
    next.splice(at, 0, '');
    return next;
  });
  const freeze = table.freeze && at < table.freeze ? table.freeze + 1 : table.freeze;
  return { ...table, columns, rows, freeze };
}

export function deleteColumn(table: RichTable, at: number): RichTable {
  if (table.columns.length <= 1) return table;
  const name = table.columns[at].name;
  const columns = table.columns.filter((_, i) => i !== at);
  const rows = table.rows.map((r) => r.filter((_, i) => i !== at));
  const freeze = table.freeze && at < table.freeze ? table.freeze - 1 : table.freeze;
  const views = table.views.map((v) => forgetColumn(v, name, columns));
  return { ...table, columns, rows, freeze: freeze || undefined, views };
}

function forgetColumn(view: View, name: string, columns: Column[]): View {
  const next: View = { ...view };
  if (view.sort) next.sort = view.sort.filter((s) => s.replace(/^-/, '') !== name);
  if (view.summary) {
    const { [name]: _gone, ...summary } = view.summary;
    next.summary = summary;
  }
  const conditions = parseFilter(view.filter);
  if (conditions) next.filter = filterText(conditions.filter((c) => c.column !== name), columns) || undefined;
  if (parseBy(view.group)?.column === name) delete next.group;
  if (view.widths && name in view.widths) {
    const { [name]: _w, ...widths } = view.widths;
    next.widths = widths;
  }
  if (view.hide) next.hide = view.hide.filter((h) => h !== name);
  if (view.rules) {
    next.rules = view.rules
      .filter((r) => r.column !== name)
      .map((r) => (r.columns ? { ...r, columns: r.columns.filter((c) => c !== name) } : r));
  }
  // A chart, pivot or board that loses its column keeps its other settings,
  // and shows the "choose a column" state until one is chosen.
  if (view.chart) {
    const chart = { ...view.chart };
    if (parseBy(chart.x)?.column === name) delete chart.x;
    if (parseBy(chart.series)?.column === name) delete chart.series;
    if (chart.y === name || parseMeasure(chart.y)?.column === name) delete chart.y;
    next.chart = chart;
  }
  if (view.pivot) {
    const pivot = { ...view.pivot };
    if (parseBy(pivot.rows)?.column === name) delete pivot.rows;
    if (parseBy(pivot.columns)?.column === name) delete pivot.columns;
    if (parseMeasure(pivot.value)?.column === name) delete pivot.value;
    next.pivot = pivot;
  }
  if (view.board) {
    const board = { ...view.board };
    if (board.by === name) delete board.by;
    if (board.title === name) delete board.title;
    if (board.show) board.show = board.show.filter((s) => s !== name);
    next.board = board;
  }
  return next;
}

export function moveColumn(table: RichTable, from: number, to: number): RichTable {
  if (to < 0 || to >= table.columns.length || from === to) return table;
  const move = <T>(list: T[]) => {
    const next = list.slice();
    const [item] = next.splice(from, 1);
    next.splice(to, 0, item);
    return next;
  };
  return { ...table, columns: move(table.columns), rows: table.rows.map(move) };
}

/**
 * Rename a column, and every place the table names it: sort keys, summaries,
 * the filter. A filter phase 1 cannot read is renamed by its `[Name]`
 * references, which is the one form a column takes in it.
 */
export function renameColumn(table: RichTable, at: number, name: string): RichTable {
  const old = table.columns[at].name;
  if (old === name) return table;
  const columns = table.columns.map((c, i) => {
    const renamed = i === at ? { ...c, name } : c;
    // Every formula that names the column, by its tokens — `[Old]`,
    // `table[Old]`, a bare `Old` — so the rest of the text stays as written.
    return renamed.type === 'formula' && renamed.expr
      ? { ...renamed, expr: renameInFormula(renamed.expr, old, name, table.name) }
      : renamed;
  });
  const views = table.views.map((view) => {
    const next: View = { ...view };
    if (view.sort) next.sort = view.sort.map((s) => (s.replace(/^-/, '') === old ? s.replace(old, name) : s));
    if (view.summary && old in view.summary) {
      next.summary = Object.fromEntries(Object.entries(view.summary).map(([k, v]) => [k === old ? name : k, v]));
    }
    const conditions = parseFilter(view.filter);
    if (conditions) {
      next.filter = filterText(conditions.map((c) => (c.column === old ? { ...c, column: name } : c)), columns) || undefined;
    } else if (view.filter) {
      next.filter = renameInFormula(view.filter, old, name, table.name);
    }
    const one = (s: string) => (s === old ? name : s);
    if (view.widths && old in view.widths) {
      next.widths = Object.fromEntries(Object.entries(view.widths).map(([k, w]) => [one(k), w]));
    }
    if (view.group) next.group = renameInBy(view.group, old, name);
    if (view.hide) next.hide = view.hide.map(one);
    if (view.rules) {
      next.rules = view.rules.map((r) => ({
        ...r,
        ...(r.when ? { when: renameInFormula(r.when, old, name, table.name) } : {}),
        ...(r.columns ? { columns: r.columns.map(one) } : {}),
        ...(r.column ? { column: one(r.column) } : {}),
      }));
    }
    if (view.chart) {
      next.chart = {
        ...view.chart,
        x: renameInBy(view.chart.x, old, name),
        series: renameInBy(view.chart.series, old, name),
        y: renameInMeasure(view.chart.y, old, name),
      };
      for (const k of ['x', 'series', 'y'] as const) if (next.chart[k] === undefined) delete next.chart[k];
    }
    if (view.pivot) {
      next.pivot = {
        ...view.pivot,
        rows: renameInBy(view.pivot.rows, old, name),
        columns: renameInBy(view.pivot.columns, old, name),
        value: renameInMeasure(view.pivot.value, old, name),
      };
      for (const k of ['rows', 'columns', 'value'] as const) if (next.pivot[k] === undefined) delete next.pivot[k];
    }
    if (view.board) {
      next.board = { ...view.board };
      if (view.board.by) next.board.by = one(view.board.by);
      if (view.board.title) next.board.title = one(view.board.title);
      if (view.board.show) next.board.show = view.board.show.map(one);
    }
    return next;
  });
  return { ...table, columns, views };
}

/**
 * Change a column's declaration. Turning a column into a select collects the
 * values it holds as options; into a checkbox, reads the usual ways of saying
 * yes. Nothing else is rewritten: a value the new type cannot read is kept and
 * marked, never cleared.
 */
export function updateColumn(table: RichTable, at: number, patch: Partial<Column>): RichTable {
  const before = table.columns[at];
  let column: Column = { ...before, ...patch };
  let rows = table.rows;
  if (patch.type && patch.type !== before.type) {
    column = retype(column, patch.type);
    if (patch.type === 'checkbox') {
      rows = rows.map((r) => {
        const next = r.slice();
        const v = next[at].trim();
        next[at] = /^(\[[xX]\]|x|true|yes|1|có|✓|✔)$/i.test(v) ? '[x]' : isBlank(v) || /^\[ \]$/.test(v) ? '' : v;
        return next;
      });
    } else if (before.type === 'checkbox') {
      rows = rows.map((r) => {
        const next = r.slice();
        if (/^\[[ xX]\]$/.test(next[at].trim())) next[at] = isChecked(next[at]) ? 'x' : '';
        return next;
      });
    }
    if (patch.type === 'select' || patch.type === 'multi') {
      const values = new Set(column.options ?? []);
      for (const r of rows) {
        const raw = r[at];
        (patch.type === 'multi' ? itemsOf(raw) : isBlank(raw) ? [] : [raw.trim()]).forEach((v) => values.add(v));
      }
      column = { ...column, options: [...values] };
    }
  }
  const columns = table.columns.map((c, i) => (i === at ? column : c));
  return { ...table, columns, rows };
}

function retype(column: Column, to: ColumnType): Column {
  const next: Column = { ...column, type: to };
  if (to !== 'number' && to !== 'formula') delete next.format;
  if (to !== 'date') delete next.time;
  if (to === 'formula') next.expr ??= '';
  else delete next.expr;
  if (to !== 'select' && to !== 'multi') {
    delete next.options;
    delete next.colors;
  }
  return next;
}

/**
 * Follow a column renamed in *another* table of the note: every formula,
 * filter and rule here that reads it as `that[Old]` now reads `that[New]`.
 * The same table back, untouched, when nothing here named it.
 */
export function renameForeignColumn(table: RichTable, other: string, from: string, to: string): RichTable {
  let changed = false;
  const fix = (text: string | undefined) => {
    if (!text) return text;
    const next = renameQualified(text, other, from, to);
    if (next !== text) changed = true;
    return next;
  };
  const columns = table.columns.map((c) => (c.type === 'formula' && c.expr ? { ...c, expr: fix(c.expr)! } : c));
  const views = table.views.map((v) => ({
    ...v,
    ...(v.filter ? { filter: fix(v.filter) } : {}),
    ...(v.rules ? { rules: v.rules.map((r) => (r.when ? { ...r, when: fix(r.when) } : r)) } : {}),
  }));
  return changed ? { ...table, columns, views } : table;
}

/**
 * The one column a change renamed, if that is what it did: same columns,
 * same order, one name different.
 */
export function renamedColumn(before: RichTable, after: RichTable): { from: string; to: string } | null {
  if (before.columns.length !== after.columns.length) return null;
  let found: { from: string; to: string } | null = null;
  for (let i = 0; i < before.columns.length; i++) {
    if (before.columns[i].name === after.columns[i].name) continue;
    if (found) return null;
    found = { from: before.columns[i].name, to: after.columns[i].name };
  }
  return found;
}

/** Change one view — the first, unless told which. A key set to `undefined` is removed. */
export function updateView(table: RichTable, patch: Partial<View>, at = 0): RichTable {
  const views = table.views.slice();
  const view = { ...views[at], ...patch };
  for (const key of Object.keys(patch) as (keyof View)[]) {
    if (patch[key] === undefined) delete view[key];
  }
  views[at] = view;
  return { ...table, views };
}

export function setConditions(table: RichTable, conditions: Condition[], at = 0): RichTable {
  return updateView(table, { filter: filterText(conditions, table.columns) || undefined }, at);
}

export function addView(table: RichTable, view: View, at = table.views.length): RichTable {
  const views = table.views.slice();
  views.splice(at, 0, view);
  return { ...table, views };
}

/** The last view stays: a table always has one. */
export function removeView(table: RichTable, at: number): RichTable {
  if (table.views.length <= 1) return table;
  return { ...table, views: table.views.filter((_, i) => i !== at) };
}

export function moveView(table: RichTable, from: number, to: number): RichTable {
  if (to < 0 || to >= table.views.length || from === to) return table;
  const views = table.views.slice();
  const [v] = views.splice(from, 1);
  views.splice(to, 0, v);
  return { ...table, views };
}

/**
 * What a fill handle writes past a selection: the series the selected
 * values make, or the values again, in turn. Two numbers or more, evenly
 * apart, go on by the same step (1, 2 → 3, 4); so do dates (1 Oct, 8 Oct →
 * 15 Oct); and a word ending in a number, like `Tuần 3`, counts on. Anything
 * else repeats, as Excel and Sheets do.
 */
export function fillValues(column: Column, source: string[], count: number): string[] {
  const values = source.map((v) => v.trim());
  const out: string[] = [];
  const step = (xs: number[]) => {
    if (xs.length < 2) return null;
    const d = xs[1] - xs[0];
    return xs.every((x, i) => i === 0 || Math.abs(x - xs[i - 1] - d) < 1e-9) ? d : null;
  };

  const numbers = values.map(numberOf);
  if (column.type !== 'text' && numbers.every((n) => n !== null)) {
    const d = step(numbers as number[]);
    if (d !== null) {
      const last = numbers[numbers.length - 1]!;
      for (let i = 1; i <= count; i++) out.push(String(Number((last + d * i).toPrecision(12))));
      return out;
    }
  }

  const dates = values.map(dateOf);
  if (dates.every((d) => d !== null) && dates.length) {
    const days = (dates as DateParts[]).map((d) => Math.round(dateValue(d) / 86_400_000));
    // One date goes on a day at a time, as Excel's does.
    const d = days.length === 1 ? 1 : step(days);
    if (d !== null) {
      const lastDate = dates[dates.length - 1]!;
      for (let i = 1; i <= count; i++) {
        const t = new Date((days[days.length - 1] + d * i) * 86_400_000);
        out.push(formatDateRaw({ y: t.getUTCFullYear(), m: t.getUTCMonth() + 1, d: t.getUTCDate(), h: lastDate.h, min: lastDate.min }));
      }
      return out;
    }
  }

  const counted = values.map((v) => /^(.*?)(\d+)$/.exec(v));
  const isDate = column.type === 'date' || dates.some((d) => d !== null);
  if (counted.every((m) => m && m[1] === counted[0]![1]) && column.type !== 'number' && !isDate) {
    const ns = counted.map((m) => Number(m![2]));
    const d = values.length === 1 ? 1 : step(ns);
    if (d !== null) {
      const prefix = counted[0]![1];
      const width = counted[counted.length - 1]![2].length;
      for (let i = 1; i <= count; i++) {
        const n = ns[ns.length - 1] + d * i;
        out.push(prefix + String(n).padStart(n >= 0 ? width : 0, '0'));
      }
      return out;
    }
  }

  for (let i = 0; i < count; i++) out.push(source[i % source.length]);
  return out;
}
