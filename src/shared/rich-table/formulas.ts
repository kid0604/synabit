/**
 * Formula columns, computed over a Rich Table and written into its cells.
 *
 * The values are written, not only shown, so that the note read anywhere —
 * GitHub, Obsidian, by Syn, by the search index — has the right numbers in it
 * (`docs/rich-table-2026-10-05.md` §4.5). Written only where they differ from
 * what the cell already holds: a table whose values are current comes back
 * as the very same object, and the note is not touched.
 *
 * Columns are computed in the order their formulas need: a column that reads
 * another comes after it. A column that ends up needing itself — through
 * `table[Self]`, or round a ring of columns — is `#CYCLE`, every column of the
 * ring. `PREV` of a column is the row above, already done, so a running
 * balance may name itself.
 */
import {
  compile, run, tokenize, FDate, FError, Problem, autoValue, toBool, toCell,
  type Compiled, type ErrorCode, type Schema, type Source, type Value,
} from '../formula';
import { dateOf, isBlank, isChecked, isErrorCell, itemsOf, numberOf, type Column, type RichTable } from './model';

/** A cell as a formula sees it, by the type of its column. */
export function valueOf(column: Column, raw: string): Value {
  if (isBlank(raw)) return column.type === 'checkbox' ? false : null;
  const s = raw.trim();
  switch (column.type) {
    case 'number': return numberOf(s) ?? s;
    case 'date': {
      const d = dateOf(s);
      return d ? FDate.of(d.y, d.m, d.d, d.h, d.min) : s;
    }
    case 'checkbox': return isChecked(s);
    case 'multi': return itemsOf(s);
    // A formula's stored error is that error — read from another table, it
    // carries on as one. Typed into a text cell, `#DIV0` is only text.
    case 'formula': return isErrorCell(s) ? new FError(s.slice(1) as ErrorCode) : autoValue(s);
    case 'text': case 'select': case 'url': case 'note': return s;
    default: return autoValue(s);
  }
}

export interface Computed {
  /** The table with every formula cell holding its current value. */
  table: RichTable;
  /** Formula columns that cannot run at all, by column index: a bad name, a cycle. */
  problems: Map<number, Problem>;
  /** Cells whose formula ran into an error, `"row:col"` → the error. */
  errors: Map<string, FError>;
}

export function schemaOf(table: RichTable, others: Record<string, RichTable> = {}): Schema {
  return {
    columns: table.columns.map((c) => c.name),
    self: table.name,
    tables: Object.fromEntries(Object.entries(others).map(([name, t]) => [name, t.columns.map((c) => c.name)])),
  };
}

/** Where formulas read from: data cells by their type, formula cells as computed so far. */
function sourceOf(
  table: RichTable,
  others: Record<string, RichTable>,
  now: Date,
  computed: Map<number, Value[]>,
): Source {
  const index = new Map(table.columns.map((c, i) => [c.name, i]));
  const data = new Map<number, Value[]>();
  const dataColumn = (i: number) => {
    let values = data.get(i);
    if (!values) {
      values = table.rows.map((r) => valueOf(table.columns[i], r[i] ?? ''));
      data.set(i, values);
    }
    return values;
  };
  const otherColumns = new Map<string, Value[]>();
  return {
    rows: table.rows.length,
    now,
    memo: new Map(),
    cell(column, row) {
      const i = index.get(column)!;
      // A formula that came out blank is blank — not what the file said last time.
      return computed.has(i) ? computed.get(i)![row] ?? null : dataColumn(i)[row] ?? null;
    },
    column(column) {
      const i = index.get(column)!;
      return computed.get(i) ?? dataColumn(i);
    },
    other(name, column) {
      const key = `${name}\u0000${column}`;
      let values = otherColumns.get(key);
      if (!values) {
        const t = others[name];
        const i = t?.columns.findIndex((c) => c.name === column) ?? -1;
        values = i === -1 ? [] : t.rows.map((r) => valueOf(t.columns[i], r[i] ?? ''));
        otherColumns.set(key, values);
      }
      return values;
    },
  };
}

/** The formula columns in the order they can be computed, and those caught in a cycle. */
function order(table: RichTable, compiled: Map<number, Compiled>): { order: number[]; cyclic: number[] } {
  const index = new Map(table.columns.map((c, i) => [c.name, i]));
  const needs = new Map<number, Set<number>>();
  for (const [i, c] of compiled) {
    const own = table.columns[i].name;
    const names = new Set([...c.deps.row, ...c.deps.whole, ...[...c.deps.prev].filter((n) => n !== own)]);
    needs.set(i, new Set([...names].map((n) => index.get(n)!).filter((j) => compiled.has(j))));
  }
  const done: number[] = [];
  const left = new Set(compiled.keys());
  let progress = true;
  while (left.size && progress) {
    progress = false;
    for (const i of [...left]) {
      if ([...needs.get(i)!].every((j) => !left.has(j))) {
        done.push(i);
        left.delete(i);
        progress = true;
      }
    }
  }
  return { order: done, cyclic: [...left] };
}

/**
 * Compute every formula column of `table`.
 *
 * @param others the other named tables of the note, for `gia[Giá]`.
 * @param now what `TODAY()` and `NOW()` are.
 */
/**
 * Whether a computed value is what the file already holds. A cell is stored
 * trimmed, with its line breaks as `<br>`; a value that only differs in that
 * would otherwise be written again on every open, and never stick.
 */
function sameCell(stored: string, cell: string): boolean {
  if (stored === cell) return true;
  const norm = (s: string) => s.replace(/<br\s*\/?>/gi, '\n').replace(/\r\n?/g, '\n').trim();
  return norm(stored) === norm(cell);
}

export function computeTable(table: RichTable, others: Record<string, RichTable> = {}, now = new Date()): Computed {
  const problems = new Map<number, Problem>();
  const errors = new Map<string, FError>();
  const formulaColumns = table.columns
    .map((c, i) => ({ c, i }))
    .filter(({ c }) => c.type === 'formula');
  if (!formulaColumns.length) return { table, problems, errors };

  const schema = schemaOf(table, others);
  const compiled = new Map<number, Compiled>();
  for (const { c, i } of formulaColumns) {
    const result = compile(c.expr ?? '', schema);
    if (result instanceof Problem) problems.set(i, result);
    else compiled.set(i, result);
  }

  const { order: sequence, cyclic } = order(table, compiled);
  for (const i of cyclic) {
    problems.set(i, new Problem('CYCLE', 'cycle', { from: 0, to: (table.columns[i].expr ?? '').length }, {
      names: cyclic.map((j) => table.columns[j].name).join(', '),
    }));
    compiled.delete(i);
  }

  const computed = new Map<number, Value[]>();
  for (const [i, p] of problems) {
    computed.set(i, table.rows.map(() => new FError(p.code, p.key, p.params)));
  }
  const source = sourceOf(table, others, now, computed);
  for (const i of sequence) {
    if (!compiled.has(i)) continue;
    const values: Value[] = [];
    computed.set(i, values);
    for (let r = 0; r < table.rows.length; r++) {
      let v: Value;
      try {
        v = run(compiled.get(i)!, source, r);
      } catch {
        // A formula never takes the editor down with it.
        v = new FError('VALUE');
      }
      values[r] = v;
      if (v instanceof FError) errors.set(`${r}:${i}`, v);
    }
  }

  let rows = table.rows;
  for (const [i, values] of computed) {
    for (let r = 0; r < rows.length; r++) {
      const cell = toCell(values[r] ?? null);
      if (sameCell(rows[r][i] ?? '', cell)) continue;
      if (rows === table.rows) rows = rows.slice();
      if (rows[r] === table.rows[r]) rows[r] = rows[r].slice();
      rows[r][i] = cell;
    }
  }
  return { table: rows === table.rows ? table : { ...table, rows }, problems, errors };
}

/**
 * A view's filter, when it is more than phase 1's pills can say — `or`,
 * arithmetic, functions — as a test of a row. `null` when it does not compile.
 */
export function formulaFilter(table: RichTable, text: string, now = new Date()): ((row: number) => boolean) | null {
  const compiled = compile(text, schemaOf(table));
  if (compiled instanceof Problem) return null;
  const source = sourceOf(table, {}, now, new Map());
  return (row) => {
    const b = toBool(run(compiled, source, row));
    return b === true;
  };
}

/** The other tables of the note a table's formulas read: `gia` for `gia[Giá]`. */
export function tablesRead(table: RichTable): Set<string> {
  const names = new Set<string>();
  for (const c of table.columns) {
    if (c.type !== 'formula' || !c.expr) continue;
    let tokens;
    try {
      tokens = tokenize(c.expr);
    } catch {
      continue;
    }
    tokens.forEach((tok, i) => {
      const next = tokens[i + 1];
      if (tok.t === 'ident' && next?.t === 'ref' && next.s.from === tok.s.to
        && tok.v.toLowerCase() !== 'table' && tok.v !== table.name) names.add(tok.v);
    });
  }
  return names;
}

/**
 * One formula at one row of a table, as a value: what the formula editor
 * shows as "this gives…", and what the tests check. The table's own formula
 * columns are read as computed.
 */
export function evaluateAt(
  table: RichTable,
  text: string,
  row: number,
  others: Record<string, RichTable> = {},
  now = new Date(),
): Value | Problem {
  const compiled = compile(text, schemaOf(table, others));
  if (compiled instanceof Problem) return compiled;
  const current = computeTable(table, others, now).table;
  return run(compiled, sourceOf(current, others, now, new Map()), row);
}

/** One table of a note, as the note's other formulas see it. */
export interface NoteTable {
  /** `null` when it cannot be read — its settings are broken, or from a later version. */
  table: RichTable | null;
  name: string | undefined;
}

/**
 * Which name means which table, across one note — and which tables must not
 * be written. A name two tables share, or the name of a table that cannot be
 * read, says nothing a formula can trust: a table reading it is left as the
 * file has it, rather than filled with `#NAME` for a table that is there. And
 * tables that read each other in a ring would add to each other on every
 * open; they are left as they are too.
 */
export function noteTables(entries: NoteTable[]): {
  byName: Map<string, number>;
  ambiguous: Set<string>;
  unreadable: Set<string>;
  /** Indexes of tables that must not be written, and why. */
  held: Map<number, { kind: 'ambiguous' | 'unreadable' | 'cycle'; name: string }>;
} {
  const byName = new Map<string, number>();
  const ambiguous = new Set<string>();
  const unreadable = new Set<string>();
  entries.forEach((e, i) => {
    if (!e.name) return;
    if (!e.table) unreadable.add(e.name);
    if (byName.has(e.name)) ambiguous.add(e.name);
    else byName.set(e.name, i);
  });
  const reads = entries.map((e) => (e.table ? tablesRead(e.table) : new Set<string>()));
  const held = new Map<number, { kind: 'ambiguous' | 'unreadable' | 'cycle'; name: string }>();
  reads.forEach((names, i) => {
    for (const n of names) {
      if (ambiguous.has(n)) held.set(i, { kind: 'ambiguous', name: n });
      else if (unreadable.has(n)) held.set(i, { kind: 'unreadable', name: n });
    }
  });
  // A ring: a table that reaches itself through the tables it reads.
  const next = (i: number) => [...reads[i]].map((n) => byName.get(n)).filter((j): j is number => j !== undefined && j !== i);
  entries.forEach((_, start) => {
    if (held.has(start) || !reads[start].size) return;
    const seen = new Set<number>();
    const stack = next(start);
    while (stack.length) {
      const j = stack.pop()!;
      if (j === start) {
        held.set(start, { kind: 'cycle', name: entries[start].name ?? '' });
        return;
      }
      if (seen.has(j)) continue;
      seen.add(j);
      stack.push(...next(j));
    }
  });
  return { byName, ambiguous, unreadable, held };
}

/** A table's name from its source, when its settings cannot be read as a whole. */
export function nameInSource(source: string): string | undefined {
  const comment = /<!--\s*rich-table\b([\s\S]*?)(?:-->|$)/.exec(source);
  const m = comment && /^name:[ \t]*(.+?)[ \t]*$/m.exec(comment[1]);
  return m ? m[1].replace(/^(["'])(.*)\1$/, '$2') : undefined;
}
