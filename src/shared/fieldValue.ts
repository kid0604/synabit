/**
 * What a frontmatter value *is*, so it can be shown as that rather than as text.
 *
 * A properties panel that prints `String(value)` shows `false`, `[]` and
 * `1781838764424` — three things a person has to decode. Worse, it then reads
 * the decoded text back on save, and `false` returns as the string `"false"`,
 * which YAML writes as `'false'` and JavaScript treats as true. Three task
 * files in this vault already carry a quoted boolean, so this is not a
 * hypothetical.
 */

export type FieldKind = 'boolean' | 'number' | 'list' | 'date' | 'json' | 'text';

/**
 * Every kind there is, in one place.
 *
 * There were three copies of this list, which is two more than a set of things
 * a component switches on can safely have. `json` is in it and is never
 * offered to anybody: values arrive as objects and are shown as the JSON they
 * are, but nobody sits down to declare a field an object.
 */
export const FIELD_KINDS: readonly FieldKind[] = [
  'text', 'number', 'boolean', 'date', 'list', 'json',
];

/** Kinds a person is offered when declaring a field. */
export const DECLARABLE_KINDS: readonly FieldKind[] = [
  'text', 'number', 'boolean', 'date', 'list',
];

/** A kind from anywhere unverified — a file, a caller — or `text`. */
export function asFieldKind(value: unknown): FieldKind {
  return FIELD_KINDS.includes(value as FieldKind) ? (value as FieldKind) : 'text';
}

/** A date, as YAML writes them: `2025-05-26`, optionally with a time. */
const DATE = /^\d{4}-\d{2}-\d{2}([ T]\d{2}:\d{2}(:\d{2})?)?$/;
const NUMBER = /^-?\d+(\.\d+)?$/;

export function kindOf(value: unknown): FieldKind {
  if (typeof value === 'boolean') return 'boolean';
  if (typeof value === 'number') return 'number';
  if (Array.isArray(value)) return 'list';
  if (value !== null && typeof value === 'object') return 'json';
  if (typeof value === 'string' && DATE.test(value.trim())) return 'date';
  return 'text';
}

/**
 * A value as one line of text.
 *
 * `String(value)` turns a list into `a,b` and an object into
 * `[object Object]`, and saving that writes the mangling to disk. Anything
 * structured is shown as the JSON it is, which round-trips.
 */
export function toText(value: unknown): string {
  if (value === null || value === undefined) return '';
  if (typeof value === 'object') return JSON.stringify(value);
  return String(value);
}

/**
 * Text back to a value, inferring the way YAML itself does.
 *
 * Only reached for a field somebody actually edited — see `valueOf` — because
 * inference is a guess, and a guess is the wrong thing to apply to a field
 * nobody touched.
 */
export function parseText(text: string): unknown {
  const t = text.trim();
  if (t === 'true') return true;
  if (t === 'false') return false;
  if (NUMBER.test(t) && Number.isFinite(Number(t))) return Number(t);
  if (t.startsWith('[') || t.startsWith('{')) {
    try {
      return JSON.parse(t);
    } catch {
      return text;
    }
  }
  return text;
}

/**
 * What to write for a row, given what was read for it.
 *
 * The rule that matters: **an untouched field is written back exactly as it
 * came**, not re-parsed from its own display text. Opening a node and saving
 * it must be a no-op, and a round trip through inference is not one — it
 * turns anything the inference misreads into something else entirely, and it
 * is how `is_transferred: false` becomes `is_transferred: 'false'`.
 *
 * Inference applies only where the person typed, where it is a fair reading of
 * what they meant.
 */
export function valueOf(text: string, original: unknown): unknown {
  return text === toText(original) ? original : parseText(text);
}
