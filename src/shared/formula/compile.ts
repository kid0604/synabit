/**
 * A formula checked against the table it belongs to, then run row by row.
 *
 * Checking is where the names are looked up: a column that is not there, a
 * function that does not exist, the wrong number of arguments. All of that
 * is found once, while the formula is typed, with the place in the text it
 * happened — never at row 1,347 of a run.
 */
import { parse, SyntaxProblem, type BinOp, type Node, type Span } from './parser';
import { FUNCTIONS, isVolatile, type Def, type EvalContext } from './functions';
import {
  DAY, FDate, FError, compare, isBlankValue, toBool, toNumber, toText, type ErrorCode, type Value,
} from './values';

/** What a formula can name: this table's columns, and the other named tables in the note. */
export interface Schema {
  columns: string[];
  /** This table's own `name:`, which works as well as `table`. */
  self?: string;
  tables?: Record<string, string[]>;
}

export interface Deps {
  /** Columns read at this row. */
  row: Set<string>;
  /** Columns read whole: `table[X]`. */
  whole: Set<string>;
  /** Columns read at the row above, through `PREV`. */
  prev: Set<string>;
  /** Other tables, by name, whose columns are read. */
  tables: Set<string>;
}

export interface Compiled {
  tree: Node;
  deps: Deps;
  volatile: boolean;
}

/** Why a formula cannot run, and where in its text. */
export class Problem {
  constructor(
    readonly code: ErrorCode,
    readonly key: string,
    readonly span: Span,
    readonly params: Record<string, string | number> = {},
  ) {}
}

/** Where a formula reads its values from as it runs. */
export interface Source {
  rows: number;
  /** The value of a column of this table at a row. */
  cell(column: string, row: number): Value;
  /** A whole column of this table. */
  column(column: string): Value[];
  /** A whole column of another table. */
  other(table: string, column: string): Value[];
  now: Date;
  /**
   * Results remembered across the rows of one run, for the parts of a
   * formula that read whole columns (see `markMemo`). Shared by every row the
   * source serves; a new source starts empty.
   */
  memo?: Map<Node, Map<string, Value>>;
}

const isThisTable = (name: string, schema: Schema) =>
  name.toLowerCase() === 'table' || (!!schema.self && name === schema.self);

export function compile(text: string, schema: Schema): Compiled | Problem {
  let tree: Node;
  try {
    tree = parse(text);
  } catch (e) {
    if (e instanceof SyntaxProblem) return new Problem('VALUE', `syntax.${e.key}`, e.s, e.params);
    throw e;
  }
  const deps: Deps = { row: new Set(), whole: new Set(), prev: new Set(), tables: new Set() };
  const columns = new Set(schema.columns);
  let volatile = false;

  const check = (node: Node, scope: Set<string>, inPrev: boolean): Problem | null => {
    switch (node.k) {
      case 'num': case 'str': case 'bool':
        return null;
      case 'ref':
        if (!columns.has(node.name)) return new Problem('NAME', 'unknown_column', node.s, { name: node.name });
        (inPrev ? deps.prev : deps.row).add(node.name);
        return null;
      case 'ident': {
        if (scope.has(node.name.toLowerCase())) return null;
        if (columns.has(node.name)) {
          (inPrev ? deps.prev : deps.row).add(node.name);
          return null;
        }
        if (node.name.toLowerCase() === 'table') return new Problem('NAME', 'table_needs_column', node.s);
        return new Problem('NAME', FUNCTIONS[node.name.toUpperCase()] ? 'function_needs_parens' : 'unknown_name', node.s, { name: node.name });
      }
      case 'col': {
        if (isThisTable(node.table, schema)) {
          if (!columns.has(node.name)) return new Problem('NAME', 'unknown_column', node.s, { name: node.name });
          // The table's own name means `table`; said once here, so running need not know it.
          node.table = 'table';
          deps.whole.add(node.name);
          return null;
        }
        const other = schema.tables?.[node.table];
        if (!other) return new Problem('NAME', 'unknown_table', node.s, { name: node.table });
        if (!other.includes(node.name)) return new Problem('NAME', 'unknown_column_in', node.s, { name: node.name, table: node.table });
        deps.tables.add(node.table);
        return null;
      }
      case 'un':
        return check(node.x, scope, inPrev);
      case 'bin':
        return check(node.a, scope, inPrev) ?? check(node.b, scope, inPrev);
      case 'call': {
        const fn = FUNCTIONS[node.name];
        if (!fn) return new Problem('NAME', 'unknown_function', node.nameSpan, { name: node.name });
        if (node.args.length < fn.min || node.args.length > fn.max) {
          return new Problem('VALUE', 'arity', node.s, {
            name: node.name, min: fn.min, max: fn.max === Infinity ? '…' : fn.max, got: node.args.length,
          });
        }
        if (isVolatile(node.name)) volatile = true;
        if (node.name === 'LET') {
          if (node.args.length % 2 === 0) return new Problem('VALUE', 'let_shape', node.s);
          const inner = new Set(scope);
          for (let i = 0; i + 1 < node.args.length; i += 2) {
            const name = node.args[i];
            if (name.k !== 'ident') return new Problem('VALUE', 'let_name', name.s);
            const p = check(node.args[i + 1], inner, inPrev);
            if (p) return p;
            inner.add(name.name.toLowerCase());
          }
          return check(node.args[node.args.length - 1], inner, inPrev);
        }
        for (let i = 0; i < node.args.length; i++) {
          // What PREV looks at is the row above: a column it names is not
          // needed at this row, so a running balance may name itself.
          const p = check(node.args[i], scope, inPrev || (node.name === 'PREV' && i === 0));
          if (p) return p;
        }
        return null;
      }
    }
  };

  const problem = check(tree, new Set(), false);
  if (problem) return problem;
  markMemo(tree, new Set());
  return { tree, deps, volatile };
}

interface Reach { whole: boolean; rows: Set<string>; free: Set<string>; impure: boolean }

/**
 * Mark the parts of a formula worth remembering from row to row.
 *
 * `SUMIF(table[Giá], table[Loại] = [Loại])` reads two whole columns at every
 * row — two thousand rows, two thousand times — yet gives the same answer at
 * every row with the same Loại. A part that reads a whole column, and whose
 * only link to its row is the cells it names (no `PREV`, no `ROW()`, no
 * outside `LET` name), is remembered by the values of those cells: computed
 * once per distinct Loại, not once per row.
 */
function markMemo(node: Node, vars: Set<string>): Reach {
  const none = (): Reach => ({ whole: false, rows: new Set(), free: new Set(), impure: false });
  const merge = (into: Reach, r: Reach) => {
    into.whole ||= r.whole;
    into.impure ||= r.impure;
    r.rows.forEach((x) => into.rows.add(x));
    r.free.forEach((x) => into.free.add(x));
  };
  switch (node.k) {
    case 'num': case 'str': case 'bool':
      return none();
    case 'ref':
      return { ...none(), rows: new Set([node.name]) };
    case 'ident': {
      const key = node.name.toLowerCase();
      return vars.has(key) ? { ...none(), free: new Set([key]) } : { ...none(), rows: new Set([node.name]) };
    }
    case 'col':
      return { ...none(), whole: true };
    case 'un':
      return markMemo(node.x, vars);
    case 'bin': {
      const r = none();
      merge(r, markMemo(node.a, vars));
      merge(r, markMemo(node.b, vars));
      if (r.whole && !r.impure && !r.free.size) node.memo = [...r.rows];
      return r;
    }
    case 'call': {
      const r = none();
      if (node.name === 'PREV' || node.name === 'ROW') r.impure = true;
      if (node.name === 'LET') {
        const inner = new Set(vars);
        const own: string[] = [];
        for (let i = 0; i + 1 < node.args.length; i += 2) {
          merge(r, markMemo(node.args[i + 1], inner));
          const arg = node.args[i];
          const name = arg.k === 'ident' ? arg.name.toLowerCase() : '';
          inner.add(name);
          own.push(name);
        }
        merge(r, markMemo(node.args[node.args.length - 1], inner));
        own.forEach((n) => { if (!vars.has(n)) r.free.delete(n); });
      } else {
        node.args.forEach((arg) => merge(r, markMemo(arg, vars)));
      }
      if (r.whole && !r.impure && !r.free.size) node.memo = [...r.rows];
      return r;
    }
  }
}

/** A cell's value as part of a memo key: its kind and its text. */
function keyOf(v: Value): string {
  if (Array.isArray(v)) return `[${v.map(keyOf).join('\u0002')}]`;
  // Exact, not as shown: two amounts that print alike are still two keys.
  if (typeof v === 'number') return `number:${v}`;
  if (v instanceof FDate) return `d:${v.ms}:${v.time ? 1 : 0}`;
  return `${v === null ? 'n' : v instanceof FError ? 'e' : typeof v}:${toText(v)}`;
}

// ─── Running ────────────────────────────────────────────────────

/** Apply `f` to two values, element by element where either is a list. */
function lift(a: Value, b: Value, f: (x: Value, y: Value) => Value): Value {
  if (Array.isArray(a) || Array.isArray(b)) {
    const as = Array.isArray(a) ? a : null;
    const bs = Array.isArray(b) ? b : null;
    const n = Math.max(as?.length ?? 0, bs?.length ?? 0);
    return Array.from({ length: n }, (_, i) => f(as ? as[i] ?? null : a, bs ? bs[i] ?? null : b));
  }
  return f(a, b);
}

function arithmetic(op: BinOp, a: Value, b: Value): Value {
  if (a instanceof FError) return a;
  if (b instanceof FError) return b;
  if (a === null || a === '' || b === null || b === '') return null;

  // Dates: a date and a number of days, or two dates apart.
  if (a instanceof FDate || b instanceof FDate) {
    if (op === '+' && a instanceof FDate && !(b instanceof FDate)) {
      const n = toNumber(b);
      return n instanceof FError || n === null ? n : new FDate(a.ms + n * DAY, a.time);
    }
    if (op === '+' && b instanceof FDate && !(a instanceof FDate)) {
      const n = toNumber(a);
      return n instanceof FError || n === null ? n : new FDate(b.ms + n * DAY, b.time);
    }
    if (op === '-' && a instanceof FDate) {
      if (b instanceof FDate) return (a.ms - b.ms) / DAY;
      const n = toNumber(b);
      return n instanceof FError || n === null ? n : new FDate(a.ms - n * DAY, a.time);
    }
    return new FError('TYPE', 'date_arithmetic', { op });
  }

  const x = toNumber(a);
  const y = toNumber(b);
  if (x instanceof FError) return x;
  if (y instanceof FError) return y;
  if (x === null || y === null) return null;
  switch (op) {
    case '+': return x + y;
    case '-': return x - y;
    case '*': return x * y;
    case '/': return y === 0 ? new FError('DIV0') : x / y;
    case '%': return y === 0 ? new FError('DIV0') : x - y * Math.floor(x / y);
    case '^': {
      const r = x ** y;
      return Number.isFinite(r) ? r : new FError('VALUE');
    }
    default: return new FError('VALUE');
  }
}

function comparison(op: BinOp, a: Value, b: Value): Value {
  if (a instanceof FError) return a;
  if (b instanceof FError) return b;
  // Nothing is before or after a blank: a task with no due date is not late.
  // Blank still equals blank, and equals "".
  if ((isBlankValue(a) || isBlankValue(b)) && op !== '=' && op !== '!=') return null;
  const c = compare(a, b);
  // A number and a word are neither equal nor in order. Not an error: one
  // cell holding the wrong kind of thing must not turn a whole column's
  // condition — `SUMIF(…, table[X] > 5)` — into an error.
  if (c === null) return op === '!=';
  switch (op) {
    case '=': return c === 0;
    case '!=': return c !== 0;
    case '<': return c < 0;
    case '<=': return c <= 0;
    case '>': return c > 0;
    case '>=': return c >= 0;
    default: return new FError('VALUE');
  }
}

function logical(op: BinOp, a: Value, b: Value): Value {
  const x = toBool(a);
  const y = toBool(b);
  if (x instanceof FError) return x;
  if (y instanceof FError) return y;
  return op === 'and' ? x && y : x || y;
}

/** A context, and the `LET` names it has worked out at its row so far. */
type Ctx = EvalContext & { lazy: Map<Def, Value> };

/** Run a checked formula at one row. */
export function run(compiled: Compiled, source: Source, row: number): Value {
  // Another row starts with no values, only the definitions: a `LET` name
  // read there is worked out again at that row, not carried from this one.
  const at = (r: number, node: Node, defs: Map<string, Def>): Value =>
    context(r, new Map(), new Map(defs)).ev(node);

  function context(r: number, vars: Map<string, Value>, defs: Map<string, Def>): Ctx {
    const ctx: Ctx = {
      row: r,
      rows: source.rows,
      now: source.now,
      vars,
      defs,
      lazy: new Map(),
      at,
      ev: (node) => ev(node, ctx),
    };
    return ctx;
  }

  /** A `LET` name at a row where it has no value yet: its definition, run there. */
  function define(def: Def, ctx: Ctx): Value {
    if (ctx.lazy.has(def)) return ctx.lazy.get(def)!;
    const inner = context(ctx.row, new Map(), new Map(def.defs));
    inner.lazy = ctx.lazy;
    const value = inner.ev(def.node);
    ctx.lazy.set(def, value);
    return value;
  }

  function ev(node: Node, ctx: Ctx): Value {
    if ((node.k === 'call' || node.k === 'bin') && node.memo && source.memo) {
      const key = node.memo.map((name) => keyOf(source.cell(name, ctx.row))).join('\u0001');
      let seen = source.memo.get(node);
      if (!seen) {
        seen = new Map();
        source.memo.set(node, seen);
      }
      if (seen.has(key)) return seen.get(key)!;
      const value = evaluate(node, ctx);
      seen.set(key, value);
      return value;
    }
    return evaluate(node, ctx);
  }

  function evaluate(node: Node, ctx: Ctx): Value {
    switch (node.k) {
      case 'num': return node.v;
      case 'str': return node.v;
      case 'bool': return node.v;
      case 'ref': return source.cell(node.name, ctx.row);
      case 'ident': {
        const key = node.name.toLowerCase();
        if (ctx.vars.has(key)) return ctx.vars.get(key)!;
        const def = ctx.defs.get(key);
        if (def) return define(def, ctx);
        return source.cell(node.name, ctx.row);
      }
      case 'col':
        return node.table.toLowerCase() === 'table' ? source.column(node.name) : source.other(node.table, node.name);
      case 'un': {
        const x = ev(node.x, ctx);
        if (node.op === 'not') {
          return lift(x, null, (v) => {
            const b = toBool(v);
            return b instanceof FError ? b : !b;
          });
        }
        return lift(x, null, (v) => arithmetic('-', 0, v === null ? null : v));
      }
      case 'bin': {
        const a = ev(node.a, ctx);
        const b = ev(node.b, ctx);
        if (node.op === 'and' || node.op === 'or') return lift(a, b, (x, y) => logical(node.op, x, y));
        if (node.op === '&') {
          return lift(a, b, (x, y) => (x instanceof FError ? x : y instanceof FError ? y : toText(x) + toText(y)));
        }
        if (['=', '!=', '<', '<=', '>', '>='].includes(node.op)) {
          // All the way down: `table[Nhãn] = "gấp"` compares each tag of each
          // row, and a row holds when any of its tags does (see `toBool`).
          const cmp = (x: Value, y: Value): Value =>
            Array.isArray(x) || Array.isArray(y) ? lift(x, y, cmp) : comparison(node.op, x, y);
          return cmp(a, b);
        }
        return lift(a, b, (x, y) => arithmetic(node.op, x, y));
      }
      case 'call': {
        const fn = FUNCTIONS[node.name];
        if (fn.lazy) return fn.lazy(node.args, ctx);
        return fn.call!(node.args.map((arg) => ev(arg, ctx)), ctx);
      }
    }
  }

  return context(row, new Map(), new Map()).ev(compiled.tree);
}
