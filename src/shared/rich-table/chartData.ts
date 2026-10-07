/**
 * What a chart draws, worked out from a table and a chart spec: the groups
 * along the x axis, the series, and a number for each. `ChartView` is handed
 * this and never reads the table itself — the same contract the app's other
 * views keep ("a view is handed a result and never fetches one").
 *
 * Colours follow the thing, not its rank: a series' colour is its place among
 * the series of the whole table, so filtering some away never repaints the
 * ones left. Past eight series the rest fold into "Other"; a scatter, which
 * puts every colour next to every other, stops at three; a donut at six
 * slices — the limits of telling colours apart (the dataviz palette's own).
 */
import {
  BLANK_KEY, compareKeys, groupRows, keysOf, measure, measureFormat, parseBy, parseMeasure,
  type Bucket, type Key,
} from './aggregate';
import { numberOf, type ChartKind, type ChartSpec, type RichTable, CHART_KINDS } from './model';

export const OTHER = '\u0000other';
const SERIES_LIMIT = 8;
const SCATTER_LIMIT = 3;
const DONUT_LIMIT = 6;

export interface Category { key: string; label: string; rows: number[] }
export interface Series { key: string; label: string; slot: number | 'other' }
export interface Point { x: number; y: number; series: number; row: number }

export type ChartProblem = 'no_x' | 'no_y' | 'no_data' | 'not_numeric';

export interface ChartModel {
  kind: ChartKind;
  categories: Category[];
  series: Series[];
  /** `values[s][c]`: series s at category c. */
  values: (number | null)[][];
  /** How values are written: the measured column's format. */
  format?: string;
  /** Scatter: one point per row. */
  points: Point[];
  /** Scatter: the formats of the two axes. */
  xFormat?: string;
  /** Number: the figure, and the last period against the one before it. */
  total: number | null;
  delta?: { current: number | null; previous: number | null; label: string; previousLabel: string };
  problem?: ChartProblem;
  stack: boolean;
  /** How many groups were folded into "Other". */
  folded: number;
}

export function kindOf(spec: ChartSpec | undefined): ChartKind {
  return CHART_KINDS.includes(spec?.kind as ChartKind) ? (spec!.kind as ChartKind) : 'bar';
}

const empty = (kind: ChartKind, problem?: ChartProblem): ChartModel => ({
  kind, categories: [], series: [], values: [], points: [], total: null, problem, stack: false, folded: 0,
});

/**
 * @param rows the rows the view shows, filtered — what the chart is of.
 */
export function chartModel(table: RichTable, spec: ChartSpec | undefined, rows: number[], locale: string): ChartModel {
  const kind = kindOf(spec);
  if (kind === 'scatter') return scatter(table, spec ?? {}, rows);
  const m = parseMeasure(spec?.y) ?? { fn: 'count' as const };
  if (m.column && !table.columns.some((c) => c.name === m.column)) return empty(kind, 'no_y');
  const format = measureFormat(table, m);

  if (kind === 'number') {
    const model = { ...empty(kind), format, total: measure(table, rows, m) };
    const by = parseBy(spec?.x);
    if (by && table.columns.some((c) => c.name === by.column)) {
      const groups = groupRows(table, rows, by, locale).filter((g) => g.key !== BLANK_KEY);
      if (groups.length >= 2) {
        const [prev, last] = groups.slice(-2);
        model.delta = {
          current: measure(table, last.rows, m),
          previous: measure(table, prev.rows, m),
          label: last.label,
          previousLabel: prev.label,
        };
      }
    }
    return model;
  }

  const by = parseBy(spec?.x);
  if (!by || !table.columns.some((c) => c.name === by.column)) return empty(kind, 'no_x');
  if (!rows.length) return { ...empty(kind, 'no_data'), format };

  let categories: Category[] = groupRows(table, rows, by, locale).map((g) => ({ key: g.key, label: g.label, rows: g.rows }));
  let folded = 0;

  // ─── Donut: each slice its own colour, six at most ───
  if (kind === 'donut') {
    const all = allKeys(table, by.column, by.bucket, locale);
    let slices = categories
      .map((c) => ({ c, v: measure(table, c.rows, m) ?? 0 }))
      .filter((s) => s.v > 0)
      .sort((a, b) => b.v - a.v);
    if (slices.length > DONUT_LIMIT) {
      const kept = slices.slice(0, DONUT_LIMIT - 1);
      const rest = slices.slice(DONUT_LIMIT - 1);
      folded = rest.length;
      slices = [...kept, { c: { key: OTHER, label: '', rows: rest.flatMap((s) => s.c.rows) }, v: rest.reduce((a, s) => a + s.v, 0) }];
    }
    categories = slices.map((s) => s.c);
    return {
      ...empty(kind),
      categories,
      series: categories.map((c) => ({ key: c.key, label: c.label, slot: slotOf(all, c.key, SERIES_LIMIT) })),
      values: [slices.map((s) => s.v)],
      format,
      total: slices.reduce((a, s) => a + s.v, 0),
      folded,
    };
  }

  // ─── Bars, lines, areas: categories along x, perhaps series ───
  const seriesBy = parseBy(spec?.series);
  let series: Series[];
  let values: (number | null)[][];
  if (seriesBy && table.columns.some((c) => c.name === seriesBy.column)) {
    const all = allKeys(table, seriesBy.column, seriesBy.bucket, locale);
    const groups = groupRows(table, rows, seriesBy, locale);
    let shown: { key: string; label: string; rows: Set<number> }[] = groups.map((g) => ({ key: g.key, label: g.label, rows: new Set(g.rows) }));
    const overflow = shown.filter((g) => slotOf(all, g.key, SERIES_LIMIT) === 'other');
    if (overflow.length) {
      folded = overflow.length;
      shown = [
        ...shown.filter((g) => !overflow.includes(g)),
        { key: OTHER, label: '', rows: new Set(overflow.flatMap((g) => [...g.rows])) },
      ];
    }
    series = shown.map((g) => ({ key: g.key, label: g.label, slot: g.key === OTHER ? 'other' : slotOf(all, g.key, SERIES_LIMIT) }));
    values = shown.map((g) => categories.map((c) => {
      const inBoth = c.rows.filter((r) => g.rows.has(r));
      return inBoth.length ? measure(table, inBoth, m) : null;
    }));
  } else {
    series = [{ key: '', label: '', slot: 0 }];
    values = [categories.map((c) => measure(table, c.rows, m))];
  }

  if (spec?.cumulative) {
    values = values.map((vs) => {
      let run = 0;
      return vs.map((v) => (run += v ?? 0));
    });
  }

  return {
    ...empty(kind),
    categories,
    series,
    values,
    format,
    stack: !!spec?.stack && series.length > 1,
    folded,
  };
}

function scatter(table: RichTable, spec: ChartSpec, rows: number[]): ChartModel {
  const xAt = table.columns.findIndex((c) => c.name === parseBy(spec.x)?.column);
  const yAt = table.columns.findIndex((c) => c.name === (spec.y ?? '').replace(/^\[(.*)\]$/, '$1'));
  if (xAt === -1) return empty('scatter', 'no_x');
  if (yAt === -1) return empty('scatter', 'no_y');
  const seriesBy = parseBy(spec.series);
  const sAt = seriesBy ? table.columns.findIndex((c) => c.name === seriesBy.column) : -1;
  const all = sAt === -1 ? [] : allKeys(table, seriesBy!.column, seriesBy!.bucket, 'en');
  const series: Series[] = [];
  const indexOf = new Map<string, number>();
  const points: Point[] = [];
  for (const r of rows) {
    const x = numberOf(table.rows[r][xAt] ?? '');
    const y = numberOf(table.rows[r][yAt] ?? '');
    if (x === null || y === null) continue;
    let key = '';
    let label = '';
    if (sAt !== -1) {
      const k = keysOf(table.columns[sAt], table.rows[r][sAt] ?? '', seriesBy!.bucket, 'en')[0];
      const slot = slotOf(all, k.key, SCATTER_LIMIT);
      key = slot === 'other' ? OTHER : k.key;
      label = slot === 'other' ? '' : k.label;
    }
    if (!indexOf.has(key)) {
      indexOf.set(key, series.length);
      series.push({ key, label, slot: key === OTHER ? 'other' : sAt === -1 ? 0 : slotOf(all, key, SCATTER_LIMIT) });
    }
    points.push({ x, y, series: indexOf.get(key)!, row: r });
  }
  if (!points.length) return { ...empty('scatter', rows.length ? 'not_numeric' : 'no_data') };
  series.sort((a, b) => (a.slot === 'other' ? 1 : b.slot === 'other' ? -1 : (a.slot as number) - (b.slot as number)));
  const remap = new Map(series.map((s, i) => [s.key, i]));
  const order = [...indexOf.entries()];
  for (const p of points) p.series = remap.get(order[p.series][0])!;
  return {
    ...empty('scatter'),
    series,
    points,
    format: table.columns[yAt].format,
    xFormat: table.columns[xAt].format,
  };
}

/** Every key a column has across the whole table, in group order: what colours are assigned by. */
function allKeys(table: RichTable, column: string, bucket: Bucket | undefined, locale: string): string[] {
  const at = table.columns.findIndex((c) => c.name === column);
  if (at === -1) return [];
  const seen = new Map<string, Key>();
  for (const row of table.rows) {
    for (const k of keysOf(table.columns[at], row[at] ?? '', bucket, locale)) if (!seen.has(k.key)) seen.set(k.key, k);
  }
  return [...seen.values()].sort(compareKeys).map((k) => k.key);
}

function slotOf(all: string[], key: string, limit: number): number | 'other' {
  const i = all.indexOf(key);
  return i === -1 || i >= limit ? 'other' : i;
}
