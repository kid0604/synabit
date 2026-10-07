/**
 * A view's filter: conditions joined by `and`, written in the formula
 * language the table will have in phase 2 —
 *
 *     [Số tiền] > 100000 and [Loại] = "Ăn uống" and not ISBLANK([Hạn])
 *
 * Phase 1 reads and writes only that much of the language: one column, one
 * comparison, `and` between. A filter it cannot read — `or`, arithmetic, a
 * later version's functions — is kept exactly as written and not applied,
 * because applying half of somebody's filter is worse than showing them all
 * the rows and saying so. `docs/rich-table-2026-10-05.md` §3.6, §7.2.
 */
import {
  dateOf, dateValue, formatDateRaw, formulaKind, isBlank, isChecked, itemsOf, numberOf,
  type Column, type ColumnType,
} from './model';

export type Op =
  | 'eq' | 'neq' | 'gt' | 'gte' | 'lt' | 'lte'
  | 'contains' | 'not_contains' | 'empty' | 'not_empty';

export interface Condition {
  column: string;
  op: Op;
  /** The value as a cell of the column would hold it: `45000`, `2026-10-01`, `[x]`. */
  value: string;
}

export function opsFor(type: ColumnType): Op[] {
  switch (type) {
    case 'number': return ['eq', 'neq', 'gt', 'gte', 'lt', 'lte', 'empty', 'not_empty'];
    case 'date': return ['eq', 'lt', 'gt', 'lte', 'gte', 'empty', 'not_empty'];
    case 'select': return ['eq', 'neq', 'empty', 'not_empty'];
    case 'multi': return ['contains', 'not_contains', 'empty', 'not_empty'];
    case 'checkbox': return ['eq'];
    case 'formula': return ['eq', 'neq', 'gt', 'gte', 'lt', 'lte', 'contains', 'not_contains', 'empty', 'not_empty'];
    default: return ['contains', 'not_contains', 'eq', 'neq', 'empty', 'not_empty'];
  }
}

export function needsValue(op: Op): boolean {
  return op !== 'empty' && op !== 'not_empty';
}

// ─── Writing ────────────────────────────────────────────────────

const SYMBOL: Partial<Record<Op, string>> = {
  eq: '=', neq: '!=', gt: '>', gte: '>=', lt: '<', lte: '<=',
};

function quote(s: string): string {
  return `"${s.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`;
}

function literal(column: Column | undefined, value: string): string {
  switch (column?.type) {
    case 'number': {
      const n = numberOf(value);
      return n === null ? quote(value) : String(n);
    }
    case 'checkbox':
      return isChecked(value) ? 'true' : 'false';
    case 'date': {
      const d = dateOf(value);
      return d ? `DATE(${d.y}, ${d.m}, ${d.d})` : quote(value);
    }
    default:
      return quote(value);
  }
}

export function conditionText(condition: Condition, column: Column | undefined): string {
  const ref = `[${condition.column}]`;
  switch (condition.op) {
    case 'empty': return `ISBLANK(${ref})`;
    case 'not_empty': return `not ISBLANK(${ref})`;
    case 'contains': return `CONTAINS(${ref}, ${quote(condition.value)})`;
    case 'not_contains': return `not CONTAINS(${ref}, ${quote(condition.value)})`;
    default: return `${ref} ${SYMBOL[condition.op]} ${literal(column, condition.value)}`;
  }
}

export function filterText(conditions: Condition[], columns: Column[]): string {
  return conditions
    .map((c) => conditionText(c, columns.find((col) => col.name === c.column)))
    .join(' and ');
}

// ─── Reading ────────────────────────────────────────────────────

type Token =
  | { t: 'ref'; v: string }
  | { t: 'str'; v: string }
  | { t: 'num'; v: string }
  | { t: 'word'; v: string }
  | { t: 'op'; v: string }
  | { t: 'punct'; v: string };

function tokenize(text: string): Token[] | null {
  const tokens: Token[] = [];
  let i = 0;
  while (i < text.length) {
    const ch = text[i];
    if (/\s/.test(ch)) { i++; continue; }
    if (ch === '[') {
      const end = text.indexOf(']', i);
      if (end === -1) return null;
      tokens.push({ t: 'ref', v: text.slice(i + 1, end) });
      i = end + 1;
    } else if (ch === '"') {
      let v = '';
      i++;
      while (i < text.length && text[i] !== '"') {
        if (text[i] === '\\' && i + 1 < text.length) i++;
        v += text[i++];
      }
      if (text[i] !== '"') return null;
      tokens.push({ t: 'str', v });
      i++;
    } else if (/[\d.-]/.test(ch) && /^-?(\d+\.?\d*|\.\d+)/.test(text.slice(i))) {
      const v = /^-?(\d+\.?\d*|\.\d+)([eE][-+]?\d+)?/.exec(text.slice(i))![0];
      tokens.push({ t: 'num', v });
      i += v.length;
    } else if (/[=!<>]/.test(ch)) {
      const v = /^(!=|<>|<=|>=|=|<|>)/.exec(text.slice(i))?.[0];
      if (!v) return null;
      tokens.push({ t: 'op', v: v === '<>' ? '!=' : v });
      i += v.length;
    } else if (/[(),]/.test(ch)) {
      tokens.push({ t: 'punct', v: ch });
      i++;
    } else if (/[\p{L}_]/u.test(ch)) {
      const v = /^[\p{L}\p{N}_]+/u.exec(text.slice(i))![0];
      tokens.push({ t: 'word', v });
      i += v.length;
    } else {
      return null;
    }
  }
  return tokens;
}

const OP_OF: Record<string, Op> = { '=': 'eq', '!=': 'neq', '>': 'gt', '>=': 'gte', '<': 'lt', '<=': 'lte' };

/**
 * The conditions a filter is made of, or `null` when it says something phase 1
 * cannot show as conditions. An empty filter is no conditions.
 */
export function parseFilter(text: string | undefined): Condition[] | null {
  if (!text?.trim()) return [];
  const tokens = tokenize(text);
  if (!tokens) return null;
  let i = 0;
  const peek = () => tokens[i];
  const word = (w: string) => peek()?.t === 'word' && peek().v.toLowerCase() === w;
  const punct = (p: string) => {
    if (peek()?.t === 'punct' && peek().v === p) { i++; return true; }
    return false;
  };

  const value = (): string | null => {
    const tok = tokens[i];
    if (!tok) return null;
    if (tok.t === 'str') { i++; return tok.v; }
    if (tok.t === 'num') { i++; return String(Number(tok.v)); }
    if (tok.t === 'word' && /^(true|false)$/i.test(tok.v)) { i++; return /^true$/i.test(tok.v) ? '[x]' : '[ ]'; }
    if (tok.t === 'word' && tok.v.toUpperCase() === 'DATE') {
      i++;
      if (!punct('(')) return null;
      const parts: number[] = [];
      for (let k = 0; k < 3; k++) {
        if (tokens[i]?.t !== 'num') return null;
        parts.push(Number(tokens[i++].v));
        if (k < 2 && !punct(',')) return null;
      }
      if (!punct(')')) return null;
      const raw = formatDateRaw({ y: parts[0], m: parts[1], d: parts[2] });
      return dateOf(raw) ? raw : null;
    }
    return null;
  };

  const condition = (): Condition | null => {
    let negated = false;
    if (word('not')) { negated = true; i++; }
    const tok = tokens[i];
    if (tok?.t === 'word') {
      const fn = tok.v.toUpperCase();
      if (fn !== 'ISBLANK' && fn !== 'CONTAINS') return null;
      i++;
      if (!punct('(')) return null;
      const ref = tokens[i++];
      if (ref?.t !== 'ref') return null;
      let arg = '';
      if (fn === 'CONTAINS') {
        if (!punct(',')) return null;
        const s = tokens[i++];
        if (s?.t !== 'str') return null;
        arg = s.v;
      }
      if (!punct(')')) return null;
      const op: Op = fn === 'ISBLANK' ? (negated ? 'not_empty' : 'empty') : (negated ? 'not_contains' : 'contains');
      return { column: ref.v, op, value: arg };
    }
    if (negated || tok?.t !== 'ref') return null;
    i++;
    const op = tokens[i++];
    if (op?.t !== 'op') return null;
    const v = value();
    if (v === null) return null;
    return { column: tok.v, op: OP_OF[op.v], value: v };
  };

  const conditions: Condition[] = [];
  for (;;) {
    const c = condition();
    if (!c) return null;
    conditions.push(c);
    if (i === tokens.length) return conditions;
    if (!word('and')) return null;
    i++;
  }
}

// ─── Applying ───────────────────────────────────────────────────

const fold = (s: string, locale: string) => s.trim().toLocaleLowerCase(locale);

/** Does a cell of `column` pass `condition`? */
export function passes(condition: Condition, column: Column, raw: string, locale: string): boolean {
  const blank = isBlank(raw);
  switch (condition.op) {
    case 'empty': return blank;
    case 'not_empty': return !blank;
  }
  if (column.type === 'checkbox' || (column.type === 'formula' && formulaKind(raw) === 'checkbox')) {
    return isChecked(raw) === isChecked(condition.value);
  }
  if (condition.op === 'contains' || condition.op === 'not_contains') {
    const needle = fold(condition.value, locale);
    const found = column.type === 'multi'
      ? itemsOf(raw).some((item) => fold(item, locale) === needle)
      : fold(raw, locale).includes(needle);
    return condition.op === 'contains' ? found : !found;
  }
  if (condition.op === 'neq' && blank) return true;
  if (blank) return false;

  let cmp: number | null = null;
  if (column.type === 'number' || column.type === 'formula') {
    const a = numberOf(raw);
    const b = numberOf(condition.value);
    if (a !== null && b !== null) cmp = a - b;
  }
  if (cmp === null && (column.type === 'date' || column.type === 'formula')) {
    const a = dateOf(raw);
    const b = dateOf(condition.value);
    // A date with a time still equals the day it falls on.
    if (a && b) cmp = dateValue({ ...a, h: undefined, min: undefined }) - dateValue(b);
  }
  if (cmp === null) {
    if (condition.op !== 'eq' && condition.op !== 'neq') return false;
    const same = fold(raw, locale) === fold(condition.value, locale);
    return condition.op === 'eq' ? same : !same;
  }
  switch (condition.op) {
    case 'eq': return cmp === 0;
    case 'neq': return cmp !== 0;
    case 'gt': return cmp > 0;
    case 'gte': return cmp >= 0;
    case 'lt': return cmp < 0;
    case 'lte': return cmp <= 0;
    default: return true;
  }
}
