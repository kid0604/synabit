/**
 * What a formula computes with: numbers, text, true/false, dates, lists, and
 * blank — and errors, which are values too, so they travel through a formula
 * to the cell that shows them.
 *
 * Nothing here depends on the interface language. A formula's result is
 * written into the note, and two devices set to different languages must
 * write the same thing, or they would rewrite each other's file forever.
 */

export type ErrorCode = 'NAME' | 'TYPE' | 'DIV0' | 'CYCLE' | 'VALUE';

export class FError {
  constructor(
    readonly code: ErrorCode,
    /** What went wrong, as an i18n key under `rich_table.formula.errors`. */
    readonly key: string = code.toLowerCase(),
    readonly params: Record<string, string | number> = {},
  ) {}
  /** As the file holds it: `#DIV0`. */
  toString(): string {
    return `#${this.code}`;
  }
}

/**
 * A date, as the civil date and time it names, encoded as if it were UTC — so
 * 1 October is 1 October on every device, whatever its time zone. `time`
 * says whether the hour means anything.
 */
export class FDate {
  constructor(readonly ms: number, readonly time = false) {}
  get y() { return new Date(this.ms).getUTCFullYear(); }
  get m() { return new Date(this.ms).getUTCMonth() + 1; }
  get d() { return new Date(this.ms).getUTCDate(); }
  static of(y: number, m: number, d: number, h?: number, min?: number): FDate {
    return new FDate(Date.UTC(y, m - 1, d, h ?? 0, min ?? 0), h !== undefined);
  }
}

export type Value = number | string | boolean | null | FDate | FError | Value[];

export const DAY = 86_400_000;

export const isError = (v: Value): v is FError => v instanceof FError;
export const isList = (v: Value): v is Value[] => Array.isArray(v);
export const isDate = (v: Value): v is FDate => v instanceof FDate;
export const isBlankValue = (v: Value) => v === null || v === '';

const pad = (n: number, w = 2) => String(n).padStart(w, '0');

/**
 * A number as the file writes it: rounded to twelve significant digits, so
 * `0.1 + 0.2` is written `0.3` rather than `0.30000000000000004`.
 */
export function numberText(n: number): string {
  if (!Number.isFinite(n)) return '#VALUE';
  // An integer is exact as it is — a 13-digit order number stays that number.
  // A fraction is cut to twelve digits, so 0.1 + 0.2 is 0.3 — but never into
  // its whole part, which a large amount with cents would otherwise lose.
  const whole = Math.abs(n) < 1 ? 0 : Math.floor(Math.log10(Math.abs(n))) + 1;
  const rounded = Number.isInteger(n) ? n : Number(n.toPrecision(Math.min(15, Math.max(12, whole + 2))));
  return String(Object.is(rounded, -0) ? 0 : rounded);
}

export function dateText(d: FDate): string {
  const day = `${d.y}-${pad(d.m)}-${pad(d.d)}`;
  if (!d.time) return day;
  const t = new Date(d.ms);
  return `${day} ${pad(t.getUTCHours())}:${pad(t.getUTCMinutes())}`;
}

/** A value as text: for `&`, for text functions, and for writing into a cell. */
export function toText(v: Value): string {
  if (v === null) return '';
  if (typeof v === 'string') return v;
  if (typeof v === 'number') return numberText(v);
  if (typeof v === 'boolean') return v ? 'TRUE' : 'FALSE';
  if (v instanceof FDate) return dateText(v);
  if (v instanceof FError) return v.toString();
  return v.map(toText).filter((s) => s !== '').join(', ');
}

/**
 * A value as a cell holds it: a checkbox for true and false, so a formula
 * that answers yes or no shows as a tick.
 */
export function toCell(v: Value): string {
  if (typeof v === 'boolean') return v ? '[x]' : '[ ]';
  return toText(v);
}

const NUMBER = /^-?(\d+(\.\d*)?|\.\d+)([eE][-+]?\d+)?$/;
const DATE = /^(\d{4})-(\d{2})-(\d{2})(?:[ T](\d{2}):(\d{2}))?$/;

/**
 * A value read from a cell whose type is not declared — another formula's
 * column, written out: `45000` is a number, `2026-10-01` a date, `[x]` true.
 *
 * Text that reads like an error code — `#DIV0`, `#NAME` — stays text. An
 * error is something a formula ran into while it ran, never a word in a
 * cell: a note that mentions `#NAME` must not turn every formula reading it
 * into an error.
 */
export function autoValue(raw: string): Value {
  const s = raw.trim();
  if (!s) return null;
  if (NUMBER.test(s)) return Number(s);
  if (/^\[[xX]\]$/.test(s)) return true;
  if (s === '[ ]') return false;
  const date = DATE.exec(s);
  if (date) {
    const [, y, m, d, h, min] = date;
    return FDate.of(+y, +m, +d, h === undefined ? undefined : +h, min === undefined ? undefined : +min);
  }
  return s;
}

/** A number, or the reason there is none. Blank stays blank. */
export function toNumber(v: Value): number | null | FError {
  if (v === null || v === '') return null;
  if (typeof v === 'number') return v;
  if (typeof v === 'boolean') return v ? 1 : 0;
  if (v instanceof FError) return v;
  if (v instanceof FDate) return v.ms / DAY;
  if (typeof v === 'string') {
    const s = v.trim();
    return NUMBER.test(s) ? Number(s) : new FError('TYPE', 'not_number', { value: s });
  }
  return new FError('TYPE', 'list_not_number');
}

export function toBool(v: Value): boolean | FError {
  if (v instanceof FError) return v;
  if (v === null) return false;
  if (typeof v === 'boolean') return v;
  if (typeof v === 'number') return v !== 0;
  if (typeof v === 'string') {
    if (/^(true|\[x\])$/i.test(v.trim())) return true;
    if (/^(false|\[ \])$/i.test(v.trim())) return false;
    return v.trim() !== '';
  }
  // A condition over a list holds when it holds for any item: `[Nhãn] = "gấp"`
  // on a multi-select is a list of yes and no, and true when one tag is.
  if (Array.isArray(v)) {
    for (const x of v) {
      const b = toBool(x);
      if (b !== false) return b;
    }
    return false;
  }
  return true;
}

export function toDate(v: Value): FDate | null | FError {
  if (v === null || v === '') return null;
  if (v instanceof FDate) return v;
  if (v instanceof FError) return v;
  if (typeof v === 'string') {
    const parsed = autoValue(v);
    if (parsed instanceof FDate) return parsed;
  }
  return new FError('TYPE', 'not_date', { value: toText(v) });
}

/**
 * Compare two values for `=` `<` and the rest: numbers as numbers, dates as
 * dates, text without regard to case — Excel's `=` ignores case, and so do
 * the people writing these. Blank equals the empty string. `null` when the
 * two cannot be ordered (a number and a word).
 */
const folded = new Map<string, string>();

/**
 * Lower case and composed (NFC), remembered: a condition over a whole column
 * folds the same words thousands of times. Composed, because Vietnamese typed
 * or pasted decomposed — "ế" as e + two marks, as macOS file names have it —
 * is the same word to the person reading it.
 */
export function fold(s: string): string {
  let f = folded.get(s);
  if (f === undefined) {
    if (folded.size > 10_000) folded.clear();
    f = s.normalize('NFC').toLowerCase();
    folded.set(s, f);
  }
  return f;
}

export function compare(a: Value, b: Value): number | null {
  // The common case first, and cheaply: two words, or two numbers.
  if (typeof a === 'string' && typeof b === 'string' && a !== '' && b !== '') {
    if (a === b) return 0;
    const fa = fold(a);
    const fb = fold(b);
    return fa === fb ? 0 : fa < fb ? -1 : 1;
  }
  if (typeof a === 'number' && typeof b === 'number') return Math.sign(a - b);
  if (isBlankValue(a) && isBlankValue(b)) return 0;
  if (isBlankValue(a) || isBlankValue(b)) return isBlankValue(a) ? -1 : 1;
  if (a instanceof FDate || b instanceof FDate) {
    const da = toDate(a);
    const db = toDate(b);
    if (da instanceof FDate && db instanceof FDate) return Math.sign(da.ms - db.ms);
    return null;
  }
  if (typeof a === 'number' || typeof b === 'number' || typeof a === 'boolean' || typeof b === 'boolean') {
    const na = toNumber(a);
    const nb = toNumber(b);
    if (typeof na === 'number' && typeof nb === 'number') return Math.sign(na - nb);
    return null;
  }
  const sa = fold(toText(a));
  const sb = fold(toText(b));
  return sa === sb ? 0 : sa < sb ? -1 : 1;
}

/** Every value in `values`, lists opened up: what `SUM(a, table[B])` adds. */
export function flatten(values: Value[]): Value[] {
  const out: Value[] = [];
  const walk = (v: Value) => (Array.isArray(v) ? v.forEach(walk) : out.push(v));
  values.forEach(walk);
  return out;
}

/** The first error among the values, if any. */
export function firstError(values: Value[]): FError | undefined {
  for (const v of values) {
    if (v instanceof FError) return v;
    if (Array.isArray(v)) {
      const inner = firstError(v);
      if (inner) return inner;
    }
  }
  return undefined;
}

/** The kind of a value, for the formula editor to say what a formula gives. */
export function kindOf(v: Value): 'number' | 'text' | 'boolean' | 'date' | 'list' | 'blank' | 'error' {
  if (v === null) return 'blank';
  if (typeof v === 'number') return 'number';
  if (typeof v === 'string') return 'text';
  if (typeof v === 'boolean') return 'boolean';
  if (v instanceof FDate) return 'date';
  if (v instanceof FError) return 'error';
  return 'list';
}
