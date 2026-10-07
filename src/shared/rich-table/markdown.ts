/**
 * A Rich Table in a note: a GFM pipe table, and right under it an HTML
 * comment holding YAML.
 *
 *     | Ngày | Số tiền |
 *     | --- | ---: |
 *     | 2026-10-01 | 45000 |
 *     <!-- rich-table
 *     columns:
 *       Ngày: date
 *       Số tiền: { type: number, format: vnd }
 *     -->
 *
 * The values live in the table, which every Markdown reader shows; only the
 * configuration lives in the comment, which every Markdown reader hides. A
 * comment that will not parse costs the configuration and never a value: the
 * table is then shown as it is and left alone. `docs/rich-table-2026-10-05.md` §4.
 */
import YAML from 'yaml';
import {
  isColumnType, numberOf,
  type BoardSpec, type ChartSpec, type Column, type PivotSpec, type RichTable, type Rule, type View,
} from './model';

export const FORMAT_VERSION = 1;

/** Is this HTML block the comment that makes the table above it a Rich Table? */
export function isRichTableComment(html: string): boolean {
  return /^<!--\s*rich-table(\s|-->|$)/.test(html.trimStart());
}

export type ReadOnlyReason = 'meta' | 'version';

export interface ParseResult {
  table: RichTable;
  /**
   * Set when the table must not be written: its comment would not parse, or
   * was written by a later version. Editing it would rewrite the comment from
   * what this version understood of it — which is how a configuration gets
   * lost.
   */
  readOnly?: ReadOnlyReason;
  /** Things worth telling somebody, in order: a row with too many cells, say. */
  warnings: Warning[];
}

export type Warning =
  | { kind: 'extra-cells'; row: number }
  | { kind: 'orphan-column'; name: string }
  | { kind: 'duplicate-column'; name: string };

// ─── Cells ──────────────────────────────────────────────────────

/** A cell as the table line holds it: `|` escaped, line breaks as `<br>`. */
export function escapeCell(value: string): string {
  return value.trim().replace(/\|/g, '\\|').replace(/\r\n?|\n/g, '<br>');
}

export function unescapeCell(value: string): string {
  return value.trim().replace(/\\\|/g, '|').replace(/<br\s*\/?>/gi, '\n');
}

/** The cells of one table line, split on the pipes that are not escaped. */
export function splitRow(line: string): string[] {
  let s = line.trim();
  if (s.startsWith('|')) s = s.slice(1);
  if (s.endsWith('|') && !s.endsWith('\\|')) s = s.slice(0, -1);
  const cells: string[] = [];
  let current = '';
  for (let i = 0; i < s.length; i++) {
    if (s[i] === '\\' && s[i + 1] === '|') {
      current += '\\|';
      i++;
    } else if (s[i] === '|') {
      cells.push(current);
      current = '';
    } else {
      current += s[i];
    }
  }
  cells.push(current);
  return cells.map(unescapeCell);
}

/**
 * The delimiter under a column's name: numbers to the right, ticks in the
 * middle — from the type, so a file read anywhere lines up as the grid does.
 * A formula has no one type; it goes right when everything it gave is a number.
 */
function alignmentFor(column: Column, rows: string[][], at: number): string {
  if (column.foreignType) return '---';
  if (column.type === 'number') return '---:';
  if (column.type === 'checkbox') return ':---:';
  if (column.type === 'formula') {
    const values = rows.map((r) => (r[at] ?? '').trim()).filter(Boolean);
    if (values.length && values.every((v) => numberOf(v) !== null)) return '---:';
  }
  return '---';
}

// ─── Parsing ────────────────────────────────────────────────────

/** Split a block's source into its table lines and its comment. */
function splitSource(source: string): { tableLines: string[]; comment: string | null } {
  const lines = source.replace(/\r\n?/g, '\n').split('\n');
  const at = lines.findIndex((l) => isRichTableComment(l));
  if (at === -1) return { tableLines: lines.filter((l) => l.trim()), comment: null };
  return {
    tableLines: lines.slice(0, at).filter((l) => l.trim()),
    comment: lines.slice(at).join('\n'),
  };
}

/**
 * The YAML inside the comment; `null` when the comment never closes, or has
 * text after it on its last line — text a rewrite would have nowhere to put,
 * so the table is shown and left as it is instead.
 */
function commentBody(comment: string): string | null {
  const start = comment.indexOf('rich-table') + 'rich-table'.length;
  const end = comment.indexOf('-->', start);
  if (end === -1 || comment.slice(end + 3).trim()) return null;
  return comment.slice(start, end).replace(/--\\>/g, '-->');
}

const COLUMN_KEYS = ['type', 'options', 'colors', 'format', 'time', 'width', 'wrap', 'expr'];
const VIEW_KEYS = ['name', 'layout', 'filter', 'sort', 'summary', 'group', 'hide', 'rules', 'chart', 'pivot', 'board', 'rowHeight', 'widths'];
const TOP_KEYS = ['name', 'version', 'columns', 'freeze', 'views'];

const isObject = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);

function restOf(value: Record<string, unknown>, known: string[]): Record<string, unknown> | undefined {
  const rest = Object.fromEntries(Object.entries(value).filter(([k]) => !known.includes(k)));
  return Object.keys(rest).length ? rest : undefined;
}

const strings = (v: unknown): string[] | undefined =>
  Array.isArray(v) ? v.map((x) => String(x ?? '')) : undefined;

function readColumn(name: string, spec: unknown): Column {
  const column: Column = { name, type: 'text' };
  const declared = isObject(spec) ? spec.type : spec;
  if (declared !== undefined && declared !== null) {
    if (isColumnType(declared)) column.type = declared;
    else column.foreignType = String(declared);
  }
  if (!isObject(spec)) return column;
  const options = strings(spec.options);
  if (options) column.options = options;
  if (isObject(spec.colors)) {
    column.colors = Object.fromEntries(Object.entries(spec.colors).map(([k, v]) => [k, String(v)]));
  }
  if (spec.format !== undefined) column.format = String(spec.format);
  if (column.type === 'formula') column.expr = spec.expr === undefined || spec.expr === null ? '' : String(spec.expr);
  if (spec.time === true) column.time = true;
  if (typeof spec.width === 'number') column.width = spec.width;
  if (spec.wrap === true) column.wrap = true;
  column.rest = restOf(spec, COLUMN_KEYS);
  return column;
}

function readView(spec: unknown): View {
  if (!isObject(spec)) return {};
  const view: View = {};
  if (spec.name !== undefined) view.name = String(spec.name);
  if (spec.layout !== undefined) view.layout = String(spec.layout);
  if (typeof spec.filter === 'string' && spec.filter.trim()) view.filter = spec.filter;
  const sort = strings(spec.sort) ?? (typeof spec.sort === 'string' ? [spec.sort] : undefined);
  if (sort?.length) view.sort = sort;
  if (isObject(spec.summary)) {
    view.summary = Object.fromEntries(Object.entries(spec.summary).map(([k, v]) => [k, String(v)]));
  }
  if (typeof spec.group === 'string' && spec.group.trim()) view.group = spec.group;
  const hide = strings(spec.hide);
  if (hide?.length) view.hide = hide;
  if (Array.isArray(spec.rules)) view.rules = spec.rules.filter(isObject) as Rule[];
  // Kept whole, keys this version does not know included.
  if (isObject(spec.chart)) view.chart = spec.chart as ChartSpec;
  if (isObject(spec.pivot)) view.pivot = spec.pivot as PivotSpec;
  if (isObject(spec.board)) view.board = spec.board as BoardSpec;
  if (typeof spec.rowHeight === 'string') view.rowHeight = spec.rowHeight;
  if (isObject(spec.widths)) {
    view.widths = Object.fromEntries(Object.entries(spec.widths)
      .filter(([, w]) => typeof w === 'number').map(([k, w]) => [k, w as number]));
  }
  view.rest = restOf(spec, VIEW_KEYS);
  return view;
}

/**
 * Read a Rich Table from its source: the table lines and the comment, as the
 * note holds them.
 */
export function parseRichTable(source: string): ParseResult {
  const warnings: Warning[] = [];
  const { tableLines, comment } = splitSource(source);

  const header = tableLines.length ? splitRow(tableLines[0]) : [''];
  const width = header.length;
  const rows = tableLines.slice(2).map((line, row) => {
    const cells = splitRow(line);
    if (cells.length > width) {
      // GFM drops the extra cells. Dropping them here would lose them on the
      // next save, so they are folded into the last cell instead, where they
      // can be seen and moved.
      if (cells.slice(width).some((c) => c.trim())) warnings.push({ kind: 'extra-cells', row });
      const kept = cells.slice(0, width - 1);
      kept.push(cells.slice(width - 1).filter((c, i) => i === 0 || c.trim()).join(' | '));
      return kept;
    }
    while (cells.length < width) cells.push('');
    return cells;
  });

  const seen = new Set<string>();
  header.forEach((name) => {
    if (seen.has(name)) warnings.push({ kind: 'duplicate-column', name });
    seen.add(name);
  });

  let meta: Record<string, unknown> = {};
  let readOnly: ReadOnlyReason | undefined;
  const body = comment === null ? '' : commentBody(comment);
  if (body === null) {
    readOnly = 'meta';
  } else {
    try {
      const parsed = YAML.parse(body);
      if (parsed !== null && parsed !== undefined && !isObject(parsed)) readOnly = 'meta';
      else meta = parsed ?? {};
    } catch {
      readOnly = 'meta';
    }
  }
  if (!readOnly && typeof meta.version === 'number' && meta.version > FORMAT_VERSION) {
    readOnly = 'version';
  }

  const specs = isObject(meta.columns) ? meta.columns : {};
  const table: RichTable = {
    columns: header.map((name) => readColumn(name, readOnly ? undefined : specs[name])),
    rows,
    views: [{}],
  };
  if (readOnly) return { table, readOnly, warnings };

  const orphans = Object.fromEntries(Object.entries(specs).filter(([name]) => !seen.has(name)));
  if (Object.keys(orphans).length) {
    table.orphans = orphans;
    Object.keys(orphans).forEach((name) => warnings.push({ kind: 'orphan-column', name }));
  }
  if (meta.name !== undefined && meta.name !== null) table.name = String(meta.name);
  if (typeof meta.freeze === 'number' && meta.freeze > 0) table.freeze = meta.freeze;
  if (Array.isArray(meta.views) && meta.views.length) table.views = meta.views.map(readView);
  table.rest = restOf(meta, TOP_KEYS);
  return { table, warnings };
}

// ─── Writing ────────────────────────────────────────────────────

function columnSpec(column: Column): unknown {
  const type = column.foreignType ?? column.type;
  const spec: Record<string, unknown> = { type };
  if (column.type === 'formula' && !column.foreignType) spec.expr = column.expr ?? '';
  if (column.options?.length) spec.options = column.options;
  if (column.colors && Object.keys(column.colors).length) spec.colors = column.colors;
  if (column.format) spec.format = column.format;
  if (column.time) spec.time = true;
  if (column.width) spec.width = Math.round(column.width);
  if (column.wrap) spec.wrap = true;
  Object.assign(spec, column.rest);
  const keys = Object.keys(spec);
  if (keys.length === 1) return type === 'text' ? undefined : type;
  return spec;
}

function viewSpec(view: View): Record<string, unknown> | undefined {
  const spec: Record<string, unknown> = {};
  if (view.name) spec.name = view.name;
  if (view.layout) spec.layout = view.layout;
  if (view.filter) spec.filter = view.filter;
  if (view.sort?.length) spec.sort = view.sort;
  if (view.summary && Object.keys(view.summary).length) spec.summary = view.summary;
  if (view.group) spec.group = view.group;
  if (view.hide?.length) spec.hide = view.hide;
  if (view.rules?.length) spec.rules = view.rules;
  if (view.chart) spec.chart = view.chart;
  if (view.pivot) spec.pivot = view.pivot;
  if (view.board) spec.board = view.board;
  if (view.rowHeight && view.rowHeight !== 'short') spec.rowHeight = view.rowHeight;
  if (view.widths && Object.keys(view.widths).length) spec.widths = view.widths;
  Object.assign(spec, view.rest);
  return Object.keys(spec).length ? spec : undefined;
}

function metaYaml(table: RichTable): string {
  const meta: Record<string, unknown> = {};
  if (table.name) meta.name = table.name;
  meta.version = FORMAT_VERSION;

  const columns: Record<string, unknown> = {};
  for (const column of table.columns) {
    const spec = columnSpec(column);
    if (spec !== undefined) columns[column.name] = spec;
  }
  // Settings kept for a column that is gone — unless a column of that name
  // is back, whose own settings are the ones that count.
  const names = new Set(table.columns.map((c) => c.name));
  for (const [name, spec] of Object.entries(table.orphans ?? {})) if (!names.has(name)) columns[name] = spec;
  if (Object.keys(columns).length) meta.columns = columns;
  if (table.freeze) meta.freeze = table.freeze;

  const views = table.views.map(viewSpec);
  if (views.length > 1 || views[0]) meta.views = views.map((v) => v ?? {});
  Object.assign(meta, table.rest);

  // One column, one line: the declarations, option lists and sort keys are
  // written inline, the way somebody would write them by hand.
  const doc = new YAML.Document(meta);
  const flow = (path: unknown[]) => {
    const node = doc.getIn(path, true);
    if (YAML.isCollection(node)) node.flow = true;
  };
  Object.keys(columns).forEach((name) => flow(['columns', name]));
  (meta.views as unknown[] | undefined)?.forEach((_, i) => {
    for (const key of ['sort', 'summary', 'hide', 'chart', 'pivot', 'board', 'widths']) flow(['views', i, key]);
    const rules = (meta.views as Record<string, unknown>[])[i]?.rules as unknown[] | undefined;
    rules?.forEach((_, j) => {
      flow(['views', i, 'rules', j, 'style']);
      flow(['views', i, 'rules', j, 'columns']);
      flow(['views', i, 'rules', j, 'scale']);
    });
  });
  return doc.toString({ lineWidth: 0, flowCollectionPadding: true }).replace(/-->/g, '--\\>');
}

/** The source of a Rich Table, as the note will hold it. */
export function serializeRichTable(table: RichTable): string {
  const line = (cells: string[]) => `| ${cells.join(' | ')} |`;
  const lines = [
    line(table.columns.map((c) => escapeCell(c.name))),
    line(table.columns.map((c, i) => alignmentFor(c, table.rows, i))),
    ...table.rows.map((row) => line(table.columns.map((_, i) => escapeCell(row[i] ?? '')))),
  ];
  return `${lines.join('\n')}\n<!-- rich-table\n${metaYaml(table)}-->`;
}

/**
 * `next`, sharing every row and the column list with `prev` where they hold
 * the same thing. The grid memoises a row on its array, so a table read
 * afresh from its source — after an edit, or an undo — re-renders only the
 * rows that changed rather than all two thousand.
 */
export function shareRows(prev: RichTable | undefined, next: RichTable): RichTable {
  if (!prev) return next;
  const same = (a: string[] | undefined, b: string[]) => !!a && a.length === b.length && a.every((v, i) => v === b[i]);
  const rows = next.rows.map((row, i) => (same(prev.rows[i], row) ? prev.rows[i] : row));
  const columns = JSON.stringify(prev.columns) === JSON.stringify(next.columns) ? prev.columns : next.columns;
  return { ...next, rows, columns };
}
