/**
 * The functions a formula can call. Names and meanings follow Excel where
 * Excel has them, because that is what people know and what they find when
 * they search; where Excel's shape is awkward (`SUMIF`'s three ranges,
 * `VLOOKUP`'s column number) there is one simpler function instead.
 *
 * Adding a function never changes the grammar: this table is open forever.
 */
import type { Node } from './parser';
import {
  DAY, FDate, FError, compare, firstError, flatten, fold, isBlankValue, numberText,
  toBool, toDate, toNumber, toText, type Value,
} from './values';

/**
 * How a `LET` name was defined: its expression, and the names in scope where
 * it was written — so it can be worked out again at another row.
 */
export interface Def {
  node: Node;
  defs: Map<string, Def>;
}

export interface EvalContext {
  /** The row being computed, counted from 0 in file order. */
  row: number;
  rows: number;
  now: Date;
  /** The `LET` names worked out at this row. */
  vars: Map<string, Value>;
  /** The `LET` names in scope, as defined: what another row works them out from. */
  defs: Map<string, Def>;
  /** Evaluate a node at another row: what `PREV` does. */
  at(row: number, node: Node, defs: Map<string, Def>): Value;
  ev(node: Node): Value;
}

interface Fn {
  min: number;
  max: number;
  /** Receives its arguments unevaluated: `IF` evaluates only the branch it takes. */
  lazy?: (args: Node[], ctx: EvalContext) => Value;
  call?: (args: Value[], ctx: EvalContext) => Value;
  volatile?: boolean;
}

const MANY = Infinity;
const valueError = (key = 'value', params: Record<string, string | number> = {}) => new FError('VALUE', key, params);

/** One number argument, or the error that stops the function. */
function num(v: Value): number | null | FError {
  return toNumber(v);
}

/** A function of one number, blank in blank out. */
function unary(f: (n: number) => number): Fn {
  const one = (a: Value): Value => {
    if (Array.isArray(a)) return a.map(one);
    const n = num(a);
    if (n === null || n instanceof FError) return n;
    const r = f(n);
    return Number.isFinite(r) ? r : valueError();
  };
  return { min: 1, max: 1, call: ([a]) => one(a) };
}

/** The numbers among a function's arguments, lists opened, blanks and words skipped. */
function numbers(args: Value[]): number[] | FError {
  const err = firstError(args);
  if (err) return err;
  const out: number[] = [];
  for (const v of flatten(args)) {
    if (typeof v === 'number') out.push(v);
    else if (typeof v === 'boolean') out.push(v ? 1 : 0);
  }
  return out;
}

function aggregate(f: (xs: number[]) => Value, empty: Value = 0): Fn {
  return {
    min: 1, max: MANY,
    call: (args) => {
      const xs = numbers(args);
      if (xs instanceof FError) return xs;
      return xs.length ? f(xs) : empty;
    },
  };
}

const sum = (xs: number[]) => xs.reduce((a, b) => a + b, 0);

function median(xs: number[]): number {
  const s = [...xs].sort((a, b) => a - b);
  const mid = s.length >> 1;
  return s.length % 2 ? s[mid] : (s[mid - 1] + s[mid]) / 2;
}

/**
 * A function whose first argument is one number, run on each item when it is
 * given a list: `ROUND(table[Giá] * 1.1)` rounds every price.
 */
function perItem(fn: Fn): Fn {
  const call = fn.call!;
  const each = (args: Value[], ctx: EvalContext): Value =>
    Array.isArray(args[0]) ? args[0].map((x) => each([x, ...args.slice(1)], ctx)) : call(args, ctx);
  return { ...fn, call: each };
}

function roundTo(n: number, digits: number, mode: 'round' | 'up' | 'down'): number {
  const f = 10 ** digits;
  const x = Number((Math.abs(n) * f).toPrecision(15));
  const r = mode === 'round' ? Math.round(x) : mode === 'up' ? Math.ceil(x) : Math.floor(x);
  return (Math.sign(n) * r) / f;
}

function roundFn(mode: 'round' | 'up' | 'down'): Fn {
  return perItem({
    min: 1, max: 2,
    call: ([a, d]) => {
      const n = num(a);
      const digits = d === undefined ? 0 : num(d);
      if (n === null) return null;
      if (n instanceof FError) return n;
      if (digits instanceof FError) return digits;
      return roundTo(n, Math.trunc(digits ?? 0), mode);
    },
  });
}

function multiple(f: (x: number) => number): Fn {
  return perItem({
    min: 1, max: 2,
    call: ([a, s]) => {
      const n = num(a);
      const step = s === undefined ? 1 : num(s);
      if (n === null) return null;
      if (n instanceof FError) return n;
      if (step instanceof FError) return step;
      if (!step) return 0;
      return Number((f(n / step) * step).toPrecision(15));
    },
  });
}

/** Pair values with a mask of the same length: what `SUMIF` and `FILTER` keep. */
function masked(values: Value, mask: Value): Value[] | FError {
  if (values instanceof FError) return values;
  if (mask instanceof FError) return mask;
  const vs = Array.isArray(values) ? values : [values];
  const ms = Array.isArray(mask) ? mask : vs.map(() => mask);
  const out: Value[] = [];
  for (let i = 0; i < vs.length; i++) {
    const keep = toBool(ms[i] ?? false);
    if (keep instanceof FError) return keep;
    if (keep) out.push(vs[i]);
  }
  return out;
}

/** A value as text for a text function: composed (NFC), so decomposed Vietnamese counts and cuts as it reads. */
function text(v: Value): string | FError {
  if (v instanceof FError) return v;
  return toText(v).normalize('NFC');
}

function date(v: Value): FDate | null | FError {
  return toDate(v);
}

/** A function of one date, blank in blank out. */
function ofDate(f: (d: FDate) => Value): Fn {
  return {
    min: 1, max: 1,
    call: ([a]) => {
      const d = date(a);
      if (d === null || d instanceof FError) return d;
      return f(d);
    },
  };
}

/**
 * Excel's `WEEKNUM`: week 1 is the week holding 1 January, and weeks start on
 * the day `type` says — 1 or 17 Sunday, 2 or 11 Monday, 12 to 16 Tuesday to
 * Saturday; 21 is the ISO week.
 */
function weekNum(d: FDate, type: number): number | FError {
  if (type === 21) return isoWeek(d);
  const first = type === 1 ? 0 : type === 2 ? 1 : type >= 11 && type <= 17 ? (type - 10) % 7 : -1;
  if (first === -1 || !Number.isInteger(type)) return valueError();
  const jan1 = Date.UTC(d.y, 0, 1);
  const shift = (new Date(jan1).getUTCDay() - first + 7) % 7;
  const day = Math.round((Date.UTC(d.y, d.m - 1, d.d) - jan1) / DAY);
  return Math.floor((day + shift) / 7) + 1;
}

/** Weekdays, Monday to Friday, from one day to another, both counted: whole weeks, then what is left. */
function weekdaysBetween(from: number, to: number): number {
  const days = Math.round((to - from) / DAY) + 1;
  let n = Math.floor(days / 7) * 5;
  const start = new Date(from).getUTCDay();
  for (let i = 0; i < days % 7; i++) {
    const wd = (start + i) % 7;
    if (wd !== 0 && wd !== 6) n++;
  }
  return n;
}

function isoWeek(d: FDate): number {
  const t = new Date(Date.UTC(d.y, d.m - 1, d.d));
  const day = t.getUTCDay() || 7;
  t.setUTCDate(t.getUTCDate() + 4 - day);
  const yearStart = Date.UTC(t.getUTCFullYear(), 0, 1);
  return Math.ceil(((t.getTime() - yearStart) / DAY + 1) / 7);
}

function addMonths(d: FDate, months: number): FDate {
  const t = new Date(d.ms);
  const target = new Date(Date.UTC(t.getUTCFullYear(), t.getUTCMonth() + months, 1, t.getUTCHours(), t.getUTCMinutes()));
  const last = new Date(Date.UTC(target.getUTCFullYear(), target.getUTCMonth() + 1, 0)).getUTCDate();
  target.setUTCDate(Math.min(t.getUTCDate(), last));
  return new FDate(target.getTime(), d.time);
}

const pad = (n: number, w = 2) => String(Math.trunc(n)).padStart(w, '0');

/** A money function: every argument a number, blank arguments as their defaults. */
function finance(f: (args: number[]) => number, min: number, max: number): Fn {
  return {
    min, max,
    call: (args) => {
      const xs: number[] = [];
      for (const a of args) {
        const n = num(a);
        if (n instanceof FError) return n;
        xs.push(n ?? 0);
      }
      const r = f(xs);
      return Number.isFinite(r) ? r : valueError();
    },
  };
}

/** The rate at which flows are worth nothing today: Newton's method, then halving if it wanders. */
function irr(flows: number[], guess: number): number | null {
  const npv = (r: number) => flows.reduce((a, v, i) => a + v / (1 + r) ** i, 0);
  const slope = (r: number) => flows.reduce((a, v, i) => a - (i * v) / (1 + r) ** (i + 1), 0);
  let r = guess;
  for (let k = 0; k < 50; k++) {
    const d = slope(r);
    if (!d) break;
    const next = r - npv(r) / d;
    if (!Number.isFinite(next) || next <= -1) break;
    if (Math.abs(next - r) < 1e-10) return next;
    r = next;
  }
  let lo = -0.9999;
  let hi = 10;
  if (Math.sign(npv(lo)) === Math.sign(npv(hi))) return null;
  for (let k = 0; k < 200; k++) {
    const mid = (lo + hi) / 2;
    if (Math.sign(npv(mid)) === Math.sign(npv(lo))) lo = mid; else hi = mid;
    if (hi - lo < 1e-12) return mid;
  }
  return (lo + hi) / 2;
}

const MONTHS = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December'];

/**
 * The parts of a date pattern, in either case: `yyyy` `yy`, `mmmm` (January)
 * `mmm` (Jan) `mm` `m`, `dd` `d`, `hh` `h`, `ss`. Text in quotes is kept as it
 * is: `dd "tháng" mm`.
 */
const DATE_TOKEN = /"[^"]*"|yyyy|yy|m{1,4}|dd|d|hh|h|ss/gi;

function formatDate(d: FDate, pattern: string): string {
  const t = new Date(d.ms);
  const parts: { tok?: string; lit: string }[] = [];
  let last = 0;
  for (const m of pattern.matchAll(DATE_TOKEN)) {
    if (m.index! > last) parts.push({ lit: pattern.slice(last, m.index) });
    parts.push(m[0].startsWith('"') ? { lit: m[0].slice(1, -1) } : { tok: m[0], lit: m[0] });
    last = m.index! + m[0].length;
  }
  if (last < pattern.length) parts.push({ lit: pattern.slice(last) });
  const kind = (p?: { tok?: string }) => p?.tok?.[0].toLowerCase();
  return parts.map((p, i) => {
    const tok = p.tok;
    if (!tok) return p.lit;
    switch (tok[0].toLowerCase()) {
      case 'y': return tok.length === 4 ? String(d.y) : pad(d.y % 100);
      case 'd': return tok.length === 2 ? pad(d.d) : String(d.d);
      case 'h': return tok.length === 2 ? pad(t.getUTCHours()) : String(t.getUTCHours());
      case 's': return pad(t.getUTCSeconds());
      default: {
        if (tok.length === 4) return MONTHS[d.m - 1];
        if (tok.length === 3) return MONTHS[d.m - 1].slice(0, 3);
        // Excel's rule: `mm` is minutes right after an hour or right before
        // seconds, and the month everywhere else — `hh:mm`, `mm:ss`, `dd/mm`.
        const before = parts.slice(0, i).reverse().find((q) => q.tok);
        const after = parts.slice(i + 1).find((q) => q.tok);
        const minutes = kind(before) === 'h' || kind(after) === 's';
        const n = minutes ? t.getUTCMinutes() : d.m;
        return tok.length === 2 ? pad(n) : String(n);
      }
    }
  }).join('');
}

/**
 * `TEXT(value, pattern)`. For numbers: `0`, `0.00`, `#.##`, `#,##0`, `0%`;
 * for dates: `DD/MM/YYYY`, `dd/mm/yyyy`, `d MMM yyyy`, `HH:mm` and the like.
 * The separators are the pattern's own, not the interface's — the result is
 * written into the note.
 */
function formatText(v: Value, pattern: string): string | FError {
  if (v instanceof FDate) return formatDate(v, pattern);
  const n = toNumber(v);
  if (n === null) return '';
  if (n instanceof FError) return n;
  const match = /^([^#0]*)([#0,]*0?)(?:\.([0#]+))?(%?)(.*)$/.exec(pattern);
  if (!match || !match[2]) return numberText(n);
  const [, before, whole, decimals = '', pct, after] = match;
  const x = pct ? n * 100 : n;
  // `0` places are always written, `#` places only when they are not zero.
  const least = (decimals.match(/0/g) ?? []).length;
  let [digits, frac = ''] = Math.abs(x).toFixed(decimals.length).split('.');
  while (frac.length > least && frac.endsWith('0')) frac = frac.slice(0, -1);
  digits = digits.padStart((whole.match(/0/g) ?? []).length, '0');
  const int = whole.includes(',') ? digits.replace(/\B(?=(\d{3})+(?!\d))/g, ',') : digits;
  // Rounded to nothing, a small negative is zero, not "-0.00".
  const negative = x < 0 && /[1-9]/.test(digits + frac);
  return `${before}${negative ? '-' : ''}${int}${frac ? `.${frac}` : ''}${pct}${after}`;
}

function today(ctx: EvalContext): FDate {
  const n = ctx.now;
  return FDate.of(n.getFullYear(), n.getMonth() + 1, n.getDate());
}

export const FUNCTIONS: Record<string, Fn> = {
  // ─── Logic ───
  IF: {
    min: 2, max: 3,
    lazy: ([c, a, b], ctx) => {
      const cond = toBool(ctx.ev(c));
      if (cond instanceof FError) return cond;
      return cond ? ctx.ev(a) : b ? ctx.ev(b) : null;
    },
  },
  IFS: {
    min: 2, max: MANY,
    lazy: (args, ctx) => {
      for (let i = 0; i + 1 < args.length; i += 2) {
        const cond = toBool(ctx.ev(args[i]));
        if (cond instanceof FError) return cond;
        if (cond) return ctx.ev(args[i + 1]);
      }
      return args.length % 2 ? ctx.ev(args[args.length - 1]) : null;
    },
  },
  SWITCH: {
    min: 3, max: MANY,
    lazy: ([subject, ...rest], ctx) => {
      const x = ctx.ev(subject);
      if (x instanceof FError) return x;
      for (let i = 0; i + 1 < rest.length; i += 2) {
        if (compare(x, ctx.ev(rest[i])) === 0) return ctx.ev(rest[i + 1]);
      }
      return rest.length % 2 ? ctx.ev(rest[rest.length - 1]) : null;
    },
  },
  AND: {
    min: 1, max: MANY,
    call: (args) => {
      for (const v of flatten(args)) {
        const b = toBool(v);
        if (b instanceof FError) return b;
        if (!b) return false;
      }
      return true;
    },
  },
  OR: {
    min: 1, max: MANY,
    call: (args) => {
      let any = false;
      for (const v of flatten(args)) {
        const b = toBool(v);
        if (b instanceof FError) return b;
        any ||= b;
      }
      return any;
    },
  },
  NOT: {
    min: 1, max: 1,
    call: ([a]) => {
      const b = toBool(a);
      return b instanceof FError ? b : !b;
    },
  },
  ISBLANK: { min: 1, max: 1, call: ([a]) => isBlankValue(a) || (Array.isArray(a) && a.length === 0) },
  ISERROR: { min: 1, max: 1, call: ([a]) => a instanceof FError },
  ISNUMBER: { min: 1, max: 1, call: ([a]) => typeof a === 'number' },
  IFERROR: {
    min: 2, max: 2,
    lazy: ([a, b], ctx) => {
      const v = ctx.ev(a);
      // Over a list, each error on its own: SUM(IFERROR(table[X] * 2, 0)).
      if (Array.isArray(v)) return v.map((x) => (x instanceof FError ? ctx.ev(b) : x));
      return v instanceof FError ? ctx.ev(b) : v;
    },
  },
  COALESCE: {
    min: 1, max: MANY,
    lazy: (args, ctx) => {
      for (const arg of args) {
        const v = ctx.ev(arg);
        if (!isBlankValue(v)) return v;
      }
      return null;
    },
  },

  // ─── Numbers ───
  ROUND: roundFn('round'),
  ROUNDUP: roundFn('up'),
  ROUNDDOWN: roundFn('down'),
  FLOOR: multiple(Math.floor),
  CEILING: multiple(Math.ceil),
  INT: unary(Math.floor),
  ABS: unary(Math.abs),
  SQRT: perItem({
    min: 1, max: 1,
    call: ([a]) => {
      const n = num(a);
      if (n === null || n instanceof FError) return n;
      return n < 0 ? valueError('negative_sqrt') : Math.sqrt(n);
    },
  }),
  EXP: unary(Math.exp),
  LN: unary(Math.log),
  LOG10: unary(Math.log10),
  SIGN: unary(Math.sign),
  MOD: perItem({
    min: 2, max: 2,
    call: ([a, b]) => {
      const x = num(a);
      const y = num(b);
      if (x instanceof FError) return x;
      if (y instanceof FError) return y;
      if (x === null || y === null) return null;
      if (y === 0) return new FError('DIV0');
      // The sign of the divisor, as Excel's MOD has it: MOD(-3, 2) is 1.
      return Number((x - y * Math.floor(x / y)).toPrecision(15));
    },
  }),
  POWER: perItem({
    min: 2, max: 2,
    call: ([a, b]) => {
      const x = num(a);
      const y = num(b);
      if (x instanceof FError) return x;
      if (y instanceof FError) return y;
      if (x === null || y === null) return null;
      const r = x ** y;
      return Number.isFinite(r) ? r : valueError();
    },
  }),
  CLAMP: perItem({
    min: 3, max: 3,
    call: ([a, lo, hi]) => {
      const [x, l, h] = [num(a), num(lo), num(hi)];
      for (const v of [x, l, h]) if (v instanceof FError) return v;
      if (x === null) return null;
      return Math.min(Math.max(x as number, (l as number | null) ?? -Infinity), (h as number | null) ?? Infinity);
    },
  }),
  PI: { min: 0, max: 0, call: () => Math.PI },

  // ─── Aggregates ───
  SUM: aggregate(sum),
  AVERAGE: aggregate((xs) => sum(xs) / xs.length, null),
  MEDIAN: aggregate(median, null),
  // A loop, not Math.min(...xs): spreading a hundred thousand values
  // overflows the stack.
  MIN: aggregate((xs) => xs.reduce((a, b) => (b < a ? b : a)), null),
  MAX: aggregate((xs) => xs.reduce((a, b) => (b > a ? b : a)), null),
  STDEV: aggregate((xs) => {
    if (xs.length < 2) return new FError('DIV0');
    const mean = sum(xs) / xs.length;
    return Math.sqrt(xs.reduce((a, x) => a + (x - mean) ** 2, 0) / (xs.length - 1));
  }, null),
  COUNT: {
    min: 1, max: MANY,
    call: (args) => flatten(args).filter((v) => typeof v === 'number').length,
  },
  COUNTA: {
    min: 1, max: MANY,
    call: (args) => flatten(args).filter((v) => !isBlankValue(v)).length,
  },
  COUNTUNIQUE: {
    min: 1, max: MANY,
    call: (args) => new Set(flatten(args).filter((v) => !isBlankValue(v)).map((v) => fold(toText(v)))).size,
  },

  // ─── Conditional aggregates and lists ───
  SUMIF: {
    min: 2, max: 2,
    call: ([values, mask]) => {
      const kept = masked(values, mask);
      if (kept instanceof FError) return kept;
      const xs = numbers(kept);
      return xs instanceof FError ? xs : sum(xs);
    },
  },
  AVERAGEIF: {
    min: 2, max: 2,
    call: ([values, mask]) => {
      const kept = masked(values, mask);
      if (kept instanceof FError) return kept;
      const xs = numbers(kept);
      if (xs instanceof FError) return xs;
      return xs.length ? sum(xs) / xs.length : null;
    },
  },
  COUNTIF: {
    min: 1, max: 1,
    call: ([mask]) => {
      const ms = Array.isArray(mask) ? mask : [mask];
      let n = 0;
      for (const m of ms) {
        const b = toBool(m);
        if (b instanceof FError) return b;
        if (b) n++;
      }
      return n;
    },
  },
  FILTER: { min: 2, max: 2, call: ([values, mask]) => masked(values, mask) },
  UNIQUE: {
    min: 1, max: 1,
    call: ([list]) => {
      const seen = new Set<string>();
      return flatten([list]).filter((v) => {
        if (isBlankValue(v)) return false;
        const key = fold(toText(v));
        if (seen.has(key)) return false;
        seen.add(key);
        return true;
      });
    },
  },
  SORT: {
    min: 1, max: 2,
    call: ([list, desc]) => {
      const descending = desc === undefined ? false : toBool(desc);
      if (descending instanceof FError) return descending;
      const items = flatten([list]).filter((v) => !isBlankValue(v));
      items.sort((a, b) => compare(a, b) ?? toText(a).localeCompare(toText(b)));
      return descending ? items.reverse() : items;
    },
  },
  LIST: { min: 0, max: MANY, call: (args) => flatten(args) },
  INDEX: {
    min: 2, max: 2,
    call: ([list, n]) => {
      const i = num(n);
      if (i instanceof FError) return i;
      if (!Array.isArray(list) || i === null) return valueError();
      const at = Math.trunc(i) - 1;
      return at >= 0 && at < list.length ? list[at] : valueError('index_out_of_range', { n: Math.trunc(i) });
    },
  },
  LOOKUP: {
    min: 3, max: 4,
    call: ([key, keys, values, fallback]) => {
      if (key instanceof FError) return key;
      // A blank key finds nothing: it would otherwise match the first blank cell.
      if (isBlankValue(key)) return fallback === undefined ? null : fallback;
      const ks = Array.isArray(keys) ? keys : [keys];
      const vs = Array.isArray(values) ? values : [values];
      const at = ks.findIndex((k) => !(k instanceof FError) && compare(k, key) === 0);
      if (at !== -1) return vs[at] ?? null;
      return fallback === undefined ? valueError('not_found', { key: toText(key) }) : fallback;
    },
  },

  // ─── Text ───
  CONCAT: {
    min: 1, max: MANY,
    call: (args) => firstError(args) ?? flatten(args).map(toText).join(''),
  },
  JOIN: {
    min: 1, max: 2,
    call: ([list, sep]) => {
      const err = firstError([list, sep ?? null]);
      if (err) return err;
      return flatten([list]).filter((v) => !isBlankValue(v)).map(toText).join(sep === undefined ? ', ' : toText(sep));
    },
  },
  LEN: {
    min: 1, max: 1,
    call: ([a]) => {
      if (Array.isArray(a)) return a.length;
      const s = text(a);
      return s instanceof FError ? s : [...s].length;
    },
  },
  LEFT: {
    min: 1, max: 2,
    call: ([a, n]) => {
      const s = text(a);
      const k = n === undefined ? 1 : num(n);
      if (s instanceof FError) return s;
      if (k instanceof FError) return k;
      return [...s].slice(0, Math.max(0, k ?? 1)).join('');
    },
  },
  RIGHT: {
    min: 1, max: 2,
    call: ([a, n]) => {
      const s = text(a);
      const k = n === undefined ? 1 : num(n);
      if (s instanceof FError) return s;
      if (k instanceof FError) return k;
      const chars = [...s];
      return chars.slice(Math.max(0, chars.length - (k ?? 1))).join('');
    },
  },
  MID: {
    min: 3, max: 3,
    call: ([a, start, n]) => {
      const s = text(a);
      const from = num(start);
      const k = num(n);
      if (s instanceof FError) return s;
      if (from instanceof FError) return from;
      if (k instanceof FError) return k;
      if (from === null || from < 1) return valueError();
      return [...s].slice(from - 1, from - 1 + Math.max(0, k ?? 0)).join('');
    },
  },
  UPPER: { min: 1, max: 1, call: ([a]) => { const s = text(a); return s instanceof FError ? s : s.toUpperCase(); } },
  LOWER: { min: 1, max: 1, call: ([a]) => { const s = text(a); return s instanceof FError ? s : s.toLowerCase(); } },
  TRIM: { min: 1, max: 1, call: ([a]) => { const s = text(a); return s instanceof FError ? s : s.trim().replace(/\s+/g, ' '); } },
  REPLACE: {
    min: 3, max: 3,
    call: ([a, find, repl]) => {
      const [s, f, r] = [text(a), text(find), text(repl)];
      for (const v of [s, f, r]) if (v instanceof FError) return v;
      if (!f) return s;
      return (s as string).split(f as string).join(r as string);
    },
  },
  CONTAINS: {
    min: 2, max: 2,
    call: ([hay, needle]) => {
      if (hay instanceof FError) return hay;
      if (needle instanceof FError) return needle;
      if (Array.isArray(hay)) return hay.some((v) => compare(v, needle) === 0);
      return fold(toText(hay)).includes(fold(toText(needle)));
    },
  },
  STARTSWITH: {
    min: 2, max: 2,
    call: ([a, b]) => firstError([a, b]) ?? fold(toText(a)).startsWith(fold(toText(b))),
  },
  SPLIT: {
    min: 1, max: 2,
    call: ([a, sep]) => {
      const s = text(a);
      if (s instanceof FError) return s;
      const by = sep === undefined ? ',' : toText(sep);
      return s ? s.split(by).map((x) => x.trim()).filter(Boolean) : [];
    },
  },
  TEXT: {
    min: 1, max: 2,
    call: ([a, pattern]) => {
      if (a instanceof FError) return a;
      if (pattern === undefined) return toText(a);
      return formatText(a, toText(pattern));
    },
  },
  VALUE: {
    min: 1, max: 1,
    call: ([a]) => {
      const n = toNumber(typeof a === 'string' ? a.replace(/[\s,]/g, '') : a);
      return n instanceof FError ? valueError('not_number', { value: toText(a) }) : n;
    },
  },

  // ─── Dates ───
  TODAY: { min: 0, max: 0, volatile: true, call: (_, ctx) => today(ctx) },
  NOW: {
    min: 0, max: 0, volatile: true,
    call: (_, ctx) => {
      const n = ctx.now;
      return FDate.of(n.getFullYear(), n.getMonth() + 1, n.getDate(), n.getHours(), n.getMinutes());
    },
  },
  DATE: {
    min: 3, max: 3,
    call: (args) => {
      const [y, m, d] = args.map(num);
      for (const v of [y, m, d]) if (v instanceof FError) return v;
      if (y === null || m === null || d === null) return null;
      const yy = y as number;
      const mm = m as number;
      const dd = d as number;
      if (mm < 1 || mm > 12 || dd < 1 || dd > 31) return valueError('bad_date');
      const out = FDate.of(yy, mm, dd);
      return out.m === mm ? out : valueError('bad_date');
    },
  },
  YEAR: ofDate((d) => d.y),
  MONTH: ofDate((d) => d.m),
  DAY: ofDate((d) => d.d),
  HOUR: ofDate((d) => new Date(d.ms).getUTCHours()),
  MINUTE: ofDate((d) => new Date(d.ms).getUTCMinutes()),
  /** 1 for Sunday … 7 for Saturday, as Excel's default. */
  WEEKDAY: ofDate((d) => new Date(d.ms).getUTCDay() + 1),
  /** Excel's default: weeks start on Sunday, week 1 holds 1 January. */
  WEEKNUM: {
    min: 1, max: 2,
    call: ([a, t]) => {
      const d = date(a);
      const type = t === undefined ? 1 : num(t);
      if (d === null || d instanceof FError) return d;
      if (type instanceof FError) return type;
      return weekNum(d, type ?? 1);
    },
  },
  /** The ISO week: weeks start on Monday, week 1 holds the year's first Thursday. */
  ISOWEEKNUM: ofDate(isoWeek),
  DAYS: {
    min: 2, max: 2,
    call: ([end, start]) => {
      const a = date(end);
      const b = date(start);
      if (a instanceof FError) return a;
      if (b instanceof FError) return b;
      if (!a || !b) return null;
      return Math.round((Date.UTC(a.y, a.m - 1, a.d) - Date.UTC(b.y, b.m - 1, b.d)) / DAY);
    },
  },
  DATEADD: {
    min: 2, max: 3,
    call: ([d0, n0, unit0]) => {
      const d = date(d0);
      const n = num(n0);
      if (d instanceof FError) return d;
      if (n instanceof FError) return n;
      if (!d || n === null) return null;
      const unit = unit0 === undefined ? 'days' : toText(unit0).toLowerCase();
      switch (unit.replace(/s$/, '')) {
        case 'day': return new FDate(d.ms + Math.round(n) * DAY, d.time);
        case 'week': return new FDate(d.ms + Math.round(n) * 7 * DAY, d.time);
        case 'month': return addMonths(d, Math.round(n));
        case 'year': return addMonths(d, Math.round(n) * 12);
        case 'hour': return new FDate(d.ms + n * 3_600_000, true);
        case 'minute': return new FDate(d.ms + n * 60_000, true);
        default: return valueError('bad_unit', { unit });
      }
    },
  },
  EOMONTH: {
    min: 1, max: 2,
    call: ([d0, n0]) => {
      const d = date(d0);
      const n = n0 === undefined ? 0 : num(n0);
      if (d instanceof FError) return d;
      if (n instanceof FError) return n;
      if (!d) return null;
      // Day 0 of the month after is the last day of the month wanted.
      return new FDate(Date.UTC(d.y, d.m + Math.round(n ?? 0), 0));
    },
  },
  NETWORKDAYS: {
    min: 2, max: 3,
    call: ([a0, b0, h0]) => {
      const a = date(a0);
      const b = date(b0);
      if (a instanceof FError) return a;
      if (b instanceof FError) return b;
      if (!a || !b) return null;
      const from = Math.min(Date.UTC(a.y, a.m - 1, a.d), Date.UTC(b.y, b.m - 1, b.d));
      const to = Math.max(Date.UTC(a.y, a.m - 1, a.d), Date.UTC(b.y, b.m - 1, b.d));
      let days = weekdaysBetween(from, to);
      // Holidays: each weekday among them, within the span, once.
      const off = new Set<number>();
      for (const h of h0 === undefined ? [] : flatten([h0])) {
        const d = date(h);
        if (d instanceof FError) return d;
        if (!d) continue;
        const t = Date.UTC(d.y, d.m - 1, d.d);
        const wd = new Date(t).getUTCDay();
        if (t >= from && t <= to && wd !== 0 && wd !== 6) off.add(t);
      }
      days -= off.size;
      return a.ms <= b.ms ? days : -days;
    },
  },

  // ─── Money ───
  // Excel's signs: money paid out is negative, money received positive.
  PMT: finance(([rate, nper, pv, fv = 0, type = 0]) => {
    if (rate === 0) return -(pv + fv) / nper;
    const g = (1 + rate) ** nper;
    return -(rate * (fv + pv * g)) / ((1 + rate * type) * (g - 1));
  }, 3, 5),
  FV: finance(([rate, nper, pmt, pv = 0, type = 0]) => {
    if (rate === 0) return -(pv + pmt * nper);
    const g = (1 + rate) ** nper;
    return -(pv * g + (pmt * (1 + rate * type) * (g - 1)) / rate);
  }, 3, 5),
  PV: finance(([rate, nper, pmt, fv = 0, type = 0]) => {
    if (rate === 0) return -(fv + pmt * nper);
    const g = (1 + rate) ** nper;
    return -(fv + (pmt * (1 + rate * type) * (g - 1)) / rate) / g;
  }, 3, 5),
  NPV: {
    min: 2, max: MANY,
    call: ([rate, ...flows]) => {
      const r = num(rate);
      if (r === null || r instanceof FError) return r;
      const xs = numbers(flows);
      if (xs instanceof FError) return xs;
      return xs.reduce((acc, v, i) => acc + v / (1 + r) ** (i + 1), 0);
    },
  },
  IRR: {
    min: 1, max: 2,
    call: ([flows, guess]) => {
      const xs = numbers([flows]);
      if (xs instanceof FError) return xs;
      if (!xs.some((v) => v > 0) || !xs.some((v) => v < 0)) return valueError('irr_signs');
      const g = guess === undefined ? 0.1 : num(guess);
      if (g instanceof FError) return g;
      return irr(xs, g ?? 0.1) ?? valueError('irr_no_answer');
    },
  },

  // ─── Rows ───
  PREV: {
    min: 1, max: 2,
    lazy: ([x, fallback], ctx) => {
      if (ctx.row === 0) return fallback ? ctx.ev(fallback) : null;
      // The row above in full: a `LET` name in `x` is its value there, not here.
      return ctx.at(ctx.row - 1, x, ctx.defs);
    },
  },
  ROW: { min: 0, max: 0, call: (_, ctx) => ctx.row + 1 },

  // ─── Variables ───
  LET: {
    min: 3, max: MANY,
    lazy: (args, ctx) => {
      const saved = new Map(ctx.vars);
      const savedDefs = new Map(ctx.defs);
      try {
        for (let i = 0; i + 1 < args.length; i += 2) {
          const name = args[i];
          if (name.k !== 'ident') return valueError('let_name');
          const key = name.name.toLowerCase();
          const def: Def = { node: args[i + 1], defs: new Map(ctx.defs) };
          ctx.vars.set(key, ctx.ev(args[i + 1]));
          ctx.defs.set(key, def);
        }
        return ctx.ev(args[args.length - 1]);
      } finally {
        ctx.vars.clear();
        saved.forEach((v, k) => ctx.vars.set(k, v));
        ctx.defs.clear();
        savedDefs.forEach((v, k) => ctx.defs.set(k, v));
      }
    },
  },
};

// Aliases people will reach for.
FUNCTIONS.AVG = FUNCTIONS.AVERAGE;
FUNCTIONS.SUBSTITUTE = FUNCTIONS.REPLACE;
FUNCTIONS.CONCATENATE = FUNCTIONS.CONCAT;
FUNCTIONS.TEXTJOIN = FUNCTIONS.JOIN;

export function isVolatile(name: string): boolean {
  return !!FUNCTIONS[name]?.volatile;
}
