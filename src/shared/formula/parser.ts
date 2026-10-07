/**
 * The formula language, read into a tree. `docs/rich-table-2026-10-05.md` §6.
 *
 *     ROUND([Đơn giá] * [Số lượng] * (1 - [Giảm giá]), 0)
 *     [Số tiền] / SUM(table[Số tiền])
 *     IF([Hạn] < TODAY() and not [Xong], "Trễ", "")
 *
 * Every node keeps where it came from in the text, so an error can underline
 * the part that is wrong rather than say "invalid formula".
 */

export interface Span { from: number; to: number }

export type Node =
  | { k: 'num'; v: number; s: Span }
  | { k: 'str'; v: string; s: Span }
  | { k: 'bool'; v: boolean; s: Span }
  /** `[Số tiền]`: a column of this table, at this row. */
  | { k: 'ref'; name: string; s: Span }
  /** `Gia` or `net`: a column written without brackets, or a `LET` name. */
  | { k: 'ident'; name: string; s: Span }
  /** `table[Số tiền]`, `gia[Giá]`: a whole column. */
  | { k: 'col'; table: string; name: string; s: Span }
  | { k: 'call'; name: string; args: Node[]; s: Span; nameSpan: Span; memo?: string[] }
  | { k: 'un'; op: '-' | 'not'; x: Node; s: Span }
  | { k: 'bin'; op: BinOp; a: Node; b: Node; s: Span; memo?: string[] };

export type BinOp = '+' | '-' | '*' | '/' | '%' | '^' | '&' | '=' | '!=' | '<' | '<=' | '>' | '>=' | 'and' | 'or';

export type TokenKind = 'num' | 'str' | 'ref' | 'ident' | 'op' | '(' | ')' | ',' | 'comment' | 'end';

export interface Token { t: TokenKind; v: string; s: Span }

export class SyntaxProblem {
  constructor(readonly key: string, readonly s: Span, readonly params: Record<string, string> = {}) {}
}

const IDENT_START = /[\p{L}_]/u;
const IDENT_PART = /[\p{L}\p{N}_]/u;

/**
 * The tokens of a formula, comments included — the editor colours them and
 * renaming a column rewrites them, and both want everything that is there.
 */
export function tokenize(text: string): Token[] {
  const tokens: Token[] = [];
  let i = 0;
  while (i < text.length) {
    const ch = text[i];
    const start = i;
    if (/\s/.test(ch)) { i++; continue; }
    if (ch === '/' && text[i + 1] === '/') {
      while (i < text.length && text[i] !== '\n') i++;
      tokens.push({ t: 'comment', v: text.slice(start, i), s: { from: start, to: i } });
      continue;
    }
    if (ch === '[') {
      const end = text.indexOf(']', i + 1);
      const nl = text.indexOf('\n', i + 1);
      if (end === -1 || (nl !== -1 && nl < end)) {
        throw new SyntaxProblem('unclosed_bracket', { from: start, to: nl === -1 ? text.length : nl });
      }
      tokens.push({ t: 'ref', v: text.slice(i + 1, end), s: { from: start, to: end + 1 } });
      i = end + 1;
      continue;
    }
    if (ch === '"') {
      let v = '';
      i++;
      while (i < text.length && text[i] !== '"') {
        if (text[i] === '\\' && i + 1 < text.length) {
          i++;
          v += text[i] === 'n' ? '\n' : text[i];
        } else {
          v += text[i];
        }
        i++;
      }
      if (i >= text.length) throw new SyntaxProblem('unclosed_string', { from: start, to: text.length });
      i++;
      tokens.push({ t: 'str', v, s: { from: start, to: i } });
      continue;
    }
    const num = /^(\d+(\.\d*)?|\.\d+)([eE][-+]?\d+)?/.exec(text.slice(i));
    if (num) {
      i += num[0].length;
      tokens.push({ t: 'num', v: num[0], s: { from: start, to: i } });
      continue;
    }
    // A table's name may have hyphens — `chi-tieu[Số tiền]` — when it is
    // right against a bracket; anywhere else `a-b` is a subtraction.
    const qualified = /^[\p{L}_][\p{L}\p{N}_-]*(?=\[)/u.exec(text.slice(i));
    if (qualified && qualified[0].includes('-') && !qualified[0].endsWith('-')) {
      i += qualified[0].length;
      tokens.push({ t: 'ident', v: qualified[0], s: { from: start, to: i } });
      continue;
    }
    if (IDENT_START.test(ch)) {
      while (i < text.length && IDENT_PART.test(text[i])) i++;
      tokens.push({ t: 'ident', v: text.slice(start, i), s: { from: start, to: i } });
      continue;
    }
    const op = /^(<=|>=|!=|<>|==|[-+*/%^&=<>])/.exec(text.slice(i));
    if (op) {
      i += op[0].length;
      const v = op[0] === '<>' ? '!=' : op[0] === '==' ? '=' : op[0];
      tokens.push({ t: 'op', v, s: { from: start, to: i } });
      continue;
    }
    if (ch === '(' || ch === ')' || ch === ',') {
      i++;
      tokens.push({ t: ch, v: ch, s: { from: start, to: i } });
      continue;
    }
    throw new SyntaxProblem('unexpected_char', { from: start, to: start + 1 }, { char: ch });
  }
  tokens.push({ t: 'end', v: '', s: { from: text.length, to: text.length } });
  return tokens;
}

const word = (tok: Token, w: string) => tok.t === 'ident' && tok.v.toLowerCase() === w;

/** Binding power of each infix operator; higher binds tighter. */
const INFIX: Record<string, number> = {
  or: 1, and: 2,
  '=': 4, '!=': 4, '<': 4, '<=': 4, '>': 4, '>=': 4,
  '&': 5, '+': 6, '-': 6, '*': 7, '/': 7, '%': 7, '^': 8,
};
const NOT_POWER = 3;
const NEG_POWER = 9;

/** Read a formula. Throws a `SyntaxProblem` saying where it stops making sense. */
export function parse(text: string): Node {
  const tokens = tokenize(text).filter((t) => t.t !== 'comment');
  let i = 0;
  const peek = () => tokens[i];
  const next = () => tokens[i++];

  const expect = (t: TokenKind, key: string) => {
    const tok = peek();
    if (tok.t !== t) throw new SyntaxProblem(key, tok.s, { near: tok.v });
    return next();
  };

  const infixOf = (tok: Token): string | null => {
    if (tok.t === 'op' && tok.v in INFIX) return tok.v;
    if (word(tok, 'and')) return 'and';
    if (word(tok, 'or')) return 'or';
    return null;
  };

  function expr(min: number): Node {
    let left = prefix();
    for (;;) {
      const op = infixOf(peek());
      if (!op) break;
      const power = INFIX[op];
      if (power < min) break;
      next();
      // `^` groups to the right, as in Excel: 2^3^2 is 2^9.
      const right = expr(op === '^' ? power : power + 1);
      left = { k: 'bin', op: op as BinOp, a: left, b: right, s: { from: left.s.from, to: right.s.to } };
    }
    return left;
  }

  function prefix(): Node {
    const tok = peek();
    if (tok.t === 'op' && (tok.v === '-' || tok.v === '+')) {
      next();
      const x = expr(NEG_POWER);
      if (tok.v === '+') return x;
      return { k: 'un', op: '-', x, s: { from: tok.s.from, to: x.s.to } };
    }
    if (word(tok, 'not')) {
      next();
      const x = expr(NOT_POWER + 1);
      return { k: 'un', op: 'not', x, s: { from: tok.s.from, to: x.s.to } };
    }
    return primary();
  }

  function primary(): Node {
    const tok = next();
    switch (tok.t) {
      case 'num': return { k: 'num', v: Number(tok.v), s: tok.s };
      case 'str': return { k: 'str', v: tok.v, s: tok.s };
      case 'ref': return { k: 'ref', name: tok.v.trim(), s: tok.s };
      case '(': {
        const inner = expr(0);
        const close = expect(')', 'expected_close');
        return { ...inner, s: { from: tok.s.from, to: close.s.to } } as Node;
      }
      case 'ident': {
        if (word(tok, 'true') || word(tok, 'false')) return { k: 'bool', v: word(tok, 'true'), s: tok.s };
        const after = peek();
        if (after.t === '(') {
          next();
          const args: Node[] = [];
          if (peek().t !== ')') {
            for (;;) {
              args.push(expr(0));
              if (peek().t === ',') { next(); continue; }
              break;
            }
          }
          const close = expect(')', 'expected_close_call');
          return { k: 'call', name: tok.v.toUpperCase(), args, s: { from: tok.s.from, to: close.s.to }, nameSpan: tok.s };
        }
        // `table[X]` — the bracket right against the name, no space between.
        if (after.t === 'ref' && after.s.from === tok.s.to) {
          next();
          return { k: 'col', table: tok.v, name: after.v.trim(), s: { from: tok.s.from, to: after.s.to } };
        }
        return { k: 'ident', name: tok.v, s: tok.s };
      }
      case 'end':
        throw new SyntaxProblem('unexpected_end', tok.s);
      default:
        throw new SyntaxProblem('unexpected_token', tok.s, { near: tok.v });
    }
  }

  if (peek().t === 'end') throw new SyntaxProblem('empty', peek().s);
  const tree = expr(0);
  if (peek().t !== 'end') throw new SyntaxProblem('unexpected_token', peek().s, { near: peek().v });
  return tree;
}

/** Is `name` writable without brackets: one word, not a keyword? */
export function isBareName(name: string): boolean {
  return /^[\p{L}_][\p{L}\p{N}_]*$/u.test(name) && !/^(and|or|not|true|false|table)$/i.test(name);
}

/**
 * Rename a column of another table where a formula names it through that
 * table — `gia[Giá]` — leaving this table's own `[Giá]` alone.
 */
export function renameQualified(text: string, table: string, from: string, to: string): string {
  let tokens: Token[];
  try {
    tokens = tokenize(text);
  } catch {
    return text.split(`${table}[${from}]`).join(`${table}[${to}]`);
  }
  let out = text;
  for (let i = tokens.length - 1; i > 0; i--) {
    const tok = tokens[i];
    const prev = tokens[i - 1];
    if (tok.t === 'ref' && tok.v.trim() === from && prev.t === 'ident' && prev.v === table && prev.s.to === tok.s.from) {
      out = out.slice(0, tok.s.from) + `[${to}]` + out.slice(tok.s.to);
    }
  }
  return out;
}

/**
 * Where a bare name in the formula is a `LET` name rather than a column: the
 * name as `LET` writes it, and every use of it inside that `LET`. As spans,
 * which for a name in brackets — `(net)` — take in the brackets too. `null`
 * when the formula does not parse.
 */
function letNames(text: string): Span[] | null {
  let tree: Node;
  try {
    tree = parse(text);
  } catch {
    return null;
  }
  const out: Span[] = [];
  const walk = (node: Node, scope: Set<string>): void => {
    switch (node.k) {
      case 'ident':
        if (scope.has(node.name.toLowerCase())) out.push(node.s);
        return;
      case 'un':
        return walk(node.x, scope);
      case 'bin':
        walk(node.a, scope);
        return walk(node.b, scope);
      case 'call': {
        if (node.name !== 'LET') return node.args.forEach((a) => walk(a, scope));
        // As `compile` scopes them: each value sees the names before it.
        const inner = new Set(scope);
        for (let i = 0; i + 1 < node.args.length; i += 2) {
          walk(node.args[i + 1], inner);
          const name = node.args[i];
          if (name.k === 'ident') {
            out.push(name.s);
            inner.add(name.name.toLowerCase());
          }
        }
        return walk(node.args[node.args.length - 1], inner);
      }
      default:
        return;
    }
  };
  walk(tree, new Set());
  return out;
}

/**
 * Rename a column everywhere a formula names it — `[Old]`, `Old`, and
 * `table[Old]` — by its tokens, so the rest of the text, spacing and comments
 * and all, is left as it was. A reference to another table's column of the
 * same name is not this column, and is left alone; nor is a `LET` name that
 * happens to be spelt like it.
 */
export function renameInFormula(text: string, from: string, to: string, ownTable?: string): string {
  let tokens: Token[];
  try {
    tokens = tokenize(text);
  } catch {
    return text.split(`[${from}]`).join(`[${to}]`);
  }
  const bound = letNames(text);
  const edits: { s: Span; v: string }[] = [];
  tokens.forEach((tok, i) => {
    const prev = tokens[i - 1];
    const after = tokens[i + 1];
    if (tok.t === 'ref' && tok.v.trim() === from) {
      const qualified = prev?.t === 'ident' && prev.s.to === tok.s.from;
      if (qualified) {
        const q = prev.v;
        if (q.toLowerCase() !== 'table' && q !== ownTable) return;
      }
      edits.push({ s: tok.s, v: `[${to}]` });
    } else if (
      tok.t === 'ident' && tok.v === from && after?.t !== '(' && !(after?.t === 'ref' && after.s.from === tok.s.to)
      && !bound?.some((b) => b.from <= tok.s.from && tok.s.to <= b.to)
    ) {
      edits.push({ s: tok.s, v: isBareName(to) ? to : `[${to}]` });
    }
  });
  let out = text;
  for (const e of edits.reverse()) out = out.slice(0, e.s.from) + e.v + out.slice(e.s.to);
  return out;
}
