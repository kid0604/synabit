/**
 * The query bar's chips — which are the text itself, sliced.
 *
 * The design is `docs/nexus-lenses-2026-09-19.md`, §6.1 and step 3.
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
 * span of it. Removing a chip removes its token; adding one appends a token;
 * typing re-slices. Two-way is not a feature that had to be kept working — it
 * is the shape of the thing.
 *
 * # Two rules copied from the Rust side, deliberately
 *
 * `tokenise` keeps quoted phrases whole, because `search.rs` does. And
 * [`SINGULAR`] lists the keys that replace rather than repeat, because over
 * there `when` is an `Option` and `with` is a `Vec`. Both are tested; if the
 * two sides drift, a person clicking twice gets an answer the engine cannot
 * give.
 */

export interface Chip {
  /** The token exactly as it appears in the text. */
  text: string;
  /** `with`, `when`, `#`, or empty for a bare word. */
  key: string;
  /** What to show. The token's value, unquoted; a bare word shows whole. */
  label: string;
}

/**
 * Keys where a second one replaces the first, because the engine keeps one.
 *
 * `with:`, `where:`, `about:` and `#tag` are absent on purpose: asking for two
 * people means both were there, which is a question somebody really asks.
 */
export const SINGULAR = ['is', 'type', 'when', 'shape', 'size', 'status', 'sort', 'limit', 'date'];

/**
 * The two words that name a table, and only as the first of several (§4).
 *
 * Alone they are ordinary English, and the Rust parser reads them that way for
 * the same reason — a free-text box would otherwise turn somebody searching
 * for the word "notes" into a listing of every note.
 */
export const SOURCES = ['notes', 'events'];

/**
 * Split a query the way the parser does: whitespace separates, quotes hold.
 *
 * A trailing unclosed quote is kept as typed rather than repaired — somebody
 * is mid-sentence, and rewriting what they are typing is the one thing a bar
 * like this must never do.
 */
export function tokenise(text: string): string[] {
  const tokens: string[] = [];
  let token = '';
  let quoted = false;

  for (const ch of text) {
    if (ch === '"' || ch === '“' || ch === '”') {
      quoted = !quoted;
      token += ch;
    } else if (/\s/.test(ch) && !quoted) {
      if (token) tokens.push(token);
      token = '';
    } else {
      token += ch;
    }
  }
  if (token) tokens.push(token);
  return tokens;
}

const unquote = (value: string) =>
  value.replace(/^["“”]|["“”]$/g, '').trim();

export function chipsOf(text: string): Chip[] {
  const tokens = tokenise(text);
  return tokens.map((token, index) => {
    // The source, which is the first word or is not the source. Drawn as a
    // chip of its own so the bar says which table is being read — the thing
    // choosing it by keyword could never say.
    if (index === 0 && tokens.length > 1 && SOURCES.includes(token.toLowerCase())) {
      return { text: token, key: 'source', label: token.toLowerCase() };
    }
    if (token.startsWith('#')) {
      return { text: token, key: '#', label: token };
    }
    const at = token.indexOf(':');
    // A colon at the very start is not a key, and neither is one in a bare
    // word like `19:30`; a key is what comes before the first colon when
    // there is something before it.
    if (at > 0) {
      const key = token.slice(0, at).toLowerCase().replace(/^-/, '');
      return { text: token, key, label: unquote(token.slice(at + 1)) || token };
    }
    return { text: token, key: '', label: token };
  });
}

/** The text with one token taken out, spacing tidied. */
export function without(text: string, index: number): string {
  const tokens = tokenise(text);
  tokens.splice(index, 1);
  return tokens.join(' ');
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
    const at = tokens.findIndex(t => t.toLowerCase().startsWith(`${key.toLowerCase()}:`));
    if (at >= 0) {
      // Pressing the one already there takes it off, so the same gesture that
      // added a filter removes it. Without this, clicking the strip twice
      // leaves the first date behind where nobody can see it.
      if (tokens[at] === token) return without(text, at);
      tokens[at] = token;
      return tokens.join(' ');
    }
  } else if (tokens.includes(token)) {
    return without(text, tokens.indexOf(token));
  }

  tokens.push(token);
  return tokens.join(' ');
}

/** A tag, which carries its own mark rather than a key. */
export function withTag(text: string, tag: string): string {
  const clean = tag.trim().replace(/^#/, '');
  if (!clean) return text;
  const token = `#${clean}`;
  const tokens = tokenise(text);
  if (tokens.includes(token)) return without(text, tokens.indexOf(token));
  tokens.push(token);
  return tokens.join(' ');
}

function quoteIfNeeded(value: string): string {
  return /\s/.test(value) ? `"${value}"` : value;
}
