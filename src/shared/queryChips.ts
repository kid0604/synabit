/**
 * The query bar's chips — which are the text itself, sliced.
 *
 * The design is `docs/nexus-lenses-2026-09-19.md` §6.1, and §10 of
 * `docs/query-grammar-2026-09-20.md` for what a chip is once the grammar has
 * brackets in it.
 *
 * # Why chips are not a model of their own
 *
 * The bar has to work both ways: edit the words and the chips follow; press a
 * person on the graph and the words follow. The obvious build — parse the text
 * into a structure, draw chips from the structure, write the structure back out
 * — needs parsing and serialising to be exact inverses of each other, for ever,
 * including for the parts neither side fully understands. The first `sort:-when`
 * that came back as `sort:when` would silently change somebody's saved lens.
 *
 * So there is no second model. **The text is the only state**, and a chip is a
 * span of it. Removing a chip removes its tokens; adding one appends tokens;
 * typing re-slices. Two-way is not a feature that had to be kept working — it
 * is the shape of the thing.
 *
 * # A chip is a top-level conjunct, not a token
 *
 * It used to be one token, and that was fine while a question could only ever
 * mean "all of these". With `OR` and brackets it is wrong, and wrong in the
 * way that matters: a chip is a thing you can take off, so a chip has to be a
 * piece that can be taken off **without changing what the rest means**.
 *
 * `(#a OR #b)` is one such piece — remove half of it and `OR` has nothing on
 * one side. So is a bare `#a OR #b`, because `OR` binds loosest: the whole
 * question is one alternative, and there is no smaller piece to remove.
 * Brackets are how somebody says otherwise, and then they get their chips
 * back: `(#a OR #b) with:khánh` is two.
 *
 * A stage is one chip for the same reason (§10). `| stats count by month` is
 * four words that mean one thing; there is no half of it that means anything.
 *
 * # Three rules copied from the Rust side, deliberately
 *
 * `tokenise` keeps quoted phrases whole and splits brackets the same way
 * `query.rs` does, including leaving `same-day-as(today)` alone. [`SINGULAR`]
 * lists the keys that replace rather than repeat, because over there `when` is
 * an `Option` and `with` is a `Vec`. And `OR` is upper case or it is a word.
 * All three are tested; if the two sides drift, a person clicking twice gets
 * an answer the engine cannot give.
 */

export interface Chip {
  /** The tokens this chip is made of, joined exactly as they appear. */
  text: string;
  /** `with`, `when`, `#`, `source`, `group`, or empty for a bare word. */
  key: string;
  /** What to show. The token's value, unquoted; a bare word shows whole. */
  label: string;
  /** Whether it asks for the absence of the thing rather than its presence. */
  negated: boolean;
  /** The first token it covers. */
  from: number;
  /** One past the last token it covers. */
  to: number;
}

/**
 * Keys where a second one replaces the first, because the engine keeps one.
 *
 * `with:`, `place:`, `about:` and `#tag` are absent on purpose: asking for two
 * people means both were there, which is a question somebody really asks.
 */
export const SINGULAR = ['is', 'type', 'when', 'shape', 'size', 'status', 'sort', 'limit', 'date'];

/**
 * The words that name a table, and only as the first of several (§4).
 *
 * `moments` is the timeline's name; `events` is the word it had until that
 * turned out to mean a calendar entry too, and is still read the same way
 * (`Source::of` in `query.rs`).
 *
 * Alone they are ordinary English, and the Rust parser reads them that way for
 * the same reason — a free-text box would otherwise turn somebody searching
 * for the word "nodes" into a listing of everything.
 *
 * `nodes` and not `notes`: the table holds people, books and tasks too, and a
 * word has to survive being read by somebody who did not write the query.
 */
export const SOURCES = ['nodes', 'moments', 'events'];

/** Structure rather than something to search for. Shouted, as in Lucene. */
const OR = 'OR';
const AND = 'AND';
const NOT = 'NOT';

/** Whether what has been read so far is a name that could take a bracket. */
function namesACall(word: string): boolean {
  const name = word.slice(word.lastIndexOf(':') + 1);
  return name.length > 0 && /[^\W\d_]/u.test(name) && /^[\p{L}\p{N}_-]+$/u.test(name);
}

/**
 * Split a query the way the parser does: whitespace separates, quotes hold,
 * brackets stand alone unless they belong to a name.
 *
 * A trailing unclosed quote is kept as typed rather than repaired — somebody
 * is mid-sentence, and rewriting what they are typing is the one thing a bar
 * like this must never do.
 */
export function tokenise(text: string): string[] {
  const tokens: string[] = [];
  let token = '';
  let quoted = false;
  // Brackets opened inside this token, so `same-day-as(today)` stays one and
  // `(a OR b)` becomes five.
  let depth = 0;

  const end = () => {
    if (token) tokens.push(token);
    token = '';
    depth = 0;
  };

  for (const ch of text) {
    if (ch === '"' || ch === '“' || ch === '”') {
      quoted = !quoted;
      token += ch;
    } else if (quoted) {
      token += ch;
    } else if (/\s/.test(ch)) {
      end();
    } else if (ch === '|') {
      // The pipe is always its own token: it is the one mark that says the
      // answer so far is about to be turned into a different answer.
      end();
      tokens.push(ch);
    } else if (ch === '(') {
      if (namesACall(token)) {
        depth += 1;
        token += ch;
      } else {
        end();
        tokens.push(ch);
      }
    } else if (ch === ')') {
      if (depth > 0) {
        depth -= 1;
        token += ch;
      } else {
        end();
        tokens.push(ch);
      }
    } else {
      token += ch;
    }
  }
  end();
  return tokens;
}

const unquote = (value: string) =>
  value.replace(/^["“”]|["“”]$/g, '').trim();

/** Whether an `OR` sits outside every bracket, making the whole thing one. */
function joinedByOr(tokens: string[]): boolean {
  let depth = 0;
  for (const token of tokens) {
    if (token === '(') depth += 1;
    else if (token === ')') depth = Math.max(0, depth - 1);
    else if (token === OR && depth === 0) return true;
  }
  return false;
}

/** One token as a chip, with `from`/`to` filled in by the caller. */
function chipOf(token: string, from: number, to: number): Chip {
  const negated = token.startsWith('-') && token.length > 1 && !token.startsWith('--');
  const body = negated ? token.slice(1) : token;
  const at = body.indexOf(':');
  // A colon at the very start is not a key, and neither is one in a bare word
  // like `19:30`; a key is what comes before the first colon when there is
  // something before it.
  const [key, label] = body.startsWith('#')
    ? ['#', body]
    : at > 0
      ? [body.slice(0, at).toLowerCase(), unquote(body.slice(at + 1)) || body]
      : ['', body];
  return { text: token, key, label, negated, from, to };
}

export function chipsOf(text: string): Chip[] {
  const tokens = tokenise(text);
  const pipe = tokens.indexOf('|');
  // Everything from the first `|` on is stages, and a stage is one chip: four
  // words that mean one thing, with no half of them that means anything.
  if (pipe >= 0) {
    return [
      ...chipsOf(tokens.slice(0, pipe).join(' ')),
      ...stageChips(tokens, pipe),
    ];
  }

  const chips: Chip[] = [];
  let at = 0;

  // The source, which is the first word or is not the source. A chip of its
  // own so the bar says which table is being read — the thing that chooses it
  // by keyword could never say.
  if (tokens.length > 1 && SOURCES.includes(tokens[0].toLowerCase())) {
    chips.push({
      text: tokens[0],
      key: 'source',
      label: tokens[0].toLowerCase(),
      negated: false,
      from: 0,
      to: 1,
    });
    at = 1;
  }

  const rest = tokens.slice(at);
  if (!rest.length) return chips;

  // An `OR` outside every bracket makes the whole question one alternative,
  // because `OR` binds loosest. There is no smaller piece to take off, so
  // there is no smaller chip.
  if (joinedByOr(rest)) {
    chips.push({
      text: rest.join(' '),
      key: 'group',
      label: rest.join(' '),
      negated: false,
      from: at,
      to: tokens.length,
    });
    return chips;
  }

  // Otherwise the chips are the top-level conjuncts.
  let i = at;
  while (i < tokens.length) {
    const start = i;
    // `AND` was written out. It belongs to the chip that follows, so taking
    // that chip off takes the word with it.
    while (i < tokens.length && tokens[i] === AND) i += 1;
    // `-x` and `NOT x` mark the chip that follows.
    while (i < tokens.length && tokens[i] === NOT) i += 1;

    if (i >= tokens.length) {
      // Only separators left — a half-typed question. Show it as it is.
      chips.push(chipOf(tokens.slice(start).join(' '), start, tokens.length));
      break;
    }

    if (tokens[i] === '(') {
      let depth = 0;
      do {
        if (tokens[i] === '(') depth += 1;
        else if (tokens[i] === ')') depth -= 1;
        i += 1;
      } while (i < tokens.length && depth > 0);
      const span = tokens.slice(start, i);
      const negated = span.some(t => t === NOT) || span[0]?.startsWith('-');
      chips.push({
        text: span.join(' '),
        key: 'group',
        label: span.join(' '),
        negated,
        from: start,
        to: i,
      });
      continue;
    }

    i += 1;
    const span = tokens.slice(start, i);
    const chip = chipOf(span[span.length - 1], start, i);
    chips.push({
      ...chip,
      text: span.join(' '),
      negated: chip.negated || span.some(t => t === NOT),
    });
  }

  return chips;
}

/**
 * One chip per `| …` run, each covering its own pipe.
 *
 * The pipe belongs to the stage that follows it, so taking the stage off takes
 * the pipe with it — otherwise dropping the last stage leaves a dangling `|`,
 * which the engine refuses.
 */
function stageChips(tokens: string[], pipe: number): Chip[] {
  const chips: Chip[] = [];
  let from = pipe;
  for (let i = pipe + 1; i <= tokens.length; i += 1) {
    if (i < tokens.length && tokens[i] !== '|') continue;
    const span = tokens.slice(from, i);
    chips.push({
      text: span.join(' '),
      key: 'stage',
      // Without the pipe: the chip's own outline already says it is a step.
      label: span.slice(1).join(' '),
      negated: false,
      from,
      to: i,
    });
    from = i;
  }
  return chips;
}

/**
 * Whether running this question would spend money.
 *
 * §13.3: a saved lens can carry `| ask`, and opening it must not pay for a
 * model call. The engine refuses that on the ordinary path and says what it
 * would have cost — but the screen has to know too, so the button that spends
 * can look like one and the button that does not can stay where it is.
 *
 * Read off the text, like everything else here. A stage is the words after a
 * `|`, and `ask` is the first of them.
 */
export function spends(text: string): boolean {
  const tokens = tokenise(text);
  return tokens.some((token, i) => tokens[i - 1] === '|' && token.toLowerCase() === 'ask');
}

/** The text with a run of tokens taken out, spacing tidied. */
export function without(text: string, from: number, to = from + 1): string {
  const tokens = tokenise(text);
  tokens.splice(from, Math.max(0, to - from));
  return tokens.join(' ');
}

/**
 * Tokens with a bare top-level `OR` wrapped in brackets.
 *
 * Without this, pressing a person while the bar reads `#a OR #b` would write
 * `#a OR #b with:khánh` — which, because `OR` binds loosest, asks for `#a`, or
 * for `#b` with Khánh. A gesture on the graph must never quietly change the
 * question that was already there.
 */
function grouped(tokens: string[]): string[] {
  return joinedByOr(tokens) ? ['(', ...tokens, ')'] : tokens;
}

/** Where a key sits at the top level, or -1. Inside a bracket is not ours. */
function topLevel(tokens: string[], key: string): number {
  let depth = 0;
  for (let i = 0; i < tokens.length; i += 1) {
    // A `|` ends the question; anything after it belongs to a stage.
    if (tokens[i] === '|') return -1;
    if (tokens[i] === '(') depth += 1;
    else if (tokens[i] === ')') depth = Math.max(0, depth - 1);
    else if (depth === 0 && tokens[i].toLowerCase().startsWith(`${key}:`)) return i;
  }
  return -1;
}

/**
 * The text with `key:value` in it — replacing any existing one when the key
 * only holds one, appending otherwise.
 *
 * Pressing the same person twice is not two filters; pressing two people is.
 */
export function withFilter(text: string, key: string, value: string): string {
  const clean = value.trim();
  if (!clean) return text;
  const token = `${key}:${quoteIfNeeded(clean)}`;
  const tokens = tokenise(text);

  if (SINGULAR.includes(key.toLowerCase())) {
    const at = topLevel(tokens, key.toLowerCase());
    if (at >= 0) {
      // Pressing the one already there takes it off, so the same gesture that
      // added a filter removes it. Without this, clicking the strip twice
      // leaves the first date behind where nobody can see it.
      if (tokens[at] === token) return without(text, at);
      tokens[at] = token;
      return tokens.join(' ');
    }
  } else {
    const at = tokens.indexOf(token);
    // Only when it stands on its own. Taking one name out of an alternative
    // leaves `OR` with nothing on one side.
    if (at >= 0 && !joinedByOr(tokens)) return without(text, at);
  }

  return withAdded(tokens, token);
}

/** A tag, which carries its own mark rather than a key. */
export function withTag(text: string, tag: string): string {
  const clean = tag.trim().replace(/^#/, '');
  if (!clean) return text;
  const token = `#${clean}`;
  const tokens = tokenise(text);
  const at = tokens.indexOf(token);
  if (at >= 0 && !joinedByOr(tokens)) return without(text, at);
  return withAdded(tokens, token);
}

/**
 * The tokens with one more filter in them.
 *
 * Added to the **question**, which is everything before the first `|`. A
 * pipeline works on the answer, so appending at the end would write
 * `… | stats count by month with:khánh` — three words the engine reads as part
 * of the stage. Pressing a person on the graph must narrow the question, not
 * corrupt the step that draws it.
 */
function withAdded(tokens: string[], token: string): string {
  const pipe = tokens.indexOf('|');
  const question = pipe >= 0 ? tokens.slice(0, pipe) : tokens;
  const stages = pipe >= 0 ? tokens.slice(pipe) : [];
  return [...grouped(question), token, ...stages].join(' ');
}

function quoteIfNeeded(value: string): string {
  return /\s/.test(value) ? `"${value}"` : value;
}

/**
 * The same question, asked of the other table.
 *
 * Two tabs over one search box need one thing: *this question, but about
 * events instead of nodes*. The source is the first word or it is not the
 * source (§4 of `docs/query-grammar-2026-09-20.md`), so this takes off the one
 * that is there and writes the one that is wanted.
 *
 * **The tab names the source, and it overrules what was typed.** Somebody who
 * wrote `events when:2019` and then presses *Nodes* is asking to see the node
 * side of that question — pressing a tab that then answered about events
 * anyway would be a tab that does nothing.
 *
 * A lone `nodes` or `events` is **a word, not a source**, exactly as the Rust
 * parser reads it — so somebody searching for the word "events" gets
 * `nodes events` on one tab and `events events` on the other, and both are
 * right.
 */
export function onSource(text: string, source: string): string {
  const tokens = tokenise(text);
  if (!tokens.length) return '';
  if (tokens.length > 1 && SOURCES.includes(tokens[0].toLowerCase())) tokens.shift();
  return [source, ...tokens].join(' ');
}

/**
 * The question with room for more rows, unless it already said how many.
 *
 * A picture of an answer has to be drawn from all of it: a chart of the
 * first page of events is a chart that says the busy months were the recent
 * ones, because the page is sorted newest first. So the timeline asks for as
 * many as the engine will give (`AT_MOST` in `timeline::query`). A `limit:`
 * somebody wrote is theirs and stays; a question with a `|` is not rows any
 * more, and `limit:` means something else after a pipe.
 */
export function withRoomFor(text: string, rows: number): string {
  const tokens = tokenise(text);
  if (!tokens.length || tokens.includes('|') || topLevel(tokens, 'limit') >= 0) return text;
  return [...tokens, `limit:${rows}`].join(' ');
}
