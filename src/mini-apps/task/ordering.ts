/**
 * Where a task sits in a hand-ordered column.
 *
 * The board used a float and inserted at the midpoint of its neighbours. That
 * works until it does not: each insertion between the same pair halves the gap,
 * and a double runs out of mantissa after about fifty of them. What happens
 * then is not an error — the midpoint simply equals one of its neighbours, two
 * cards claim the same position, and their order starts flipping between
 * reloads with nothing in the file to explain why.
 *
 * A string key has no such limit. `keyBetween` always finds a value strictly
 * between two others, growing the key by a character when it must, so the
 * sequence can be subdivided for as long as anyone keeps dragging.
 *
 * The keys sort with a plain `<`, which is what makes them cheap: no parsing,
 * no numeric coercion, and SQLite would order them correctly too if the board
 * ever asked the database to sort instead of doing it here.
 *
 * # What this scheme costs
 *
 * This is the plain fractional form — a digit string read as a fraction — not
 * the variant with a magnitude-prefixed integer part that Figma and LexoRank
 * use. The difference shows only at the end of a column: appending climbs the
 * alphabet from the middle, so roughly every thirty appends adds a character,
 * where the prefixed form would stay short indefinitely.
 *
 * Worth naming, and not worth paying for here. Keys are minted by dragging a
 * card, never by creating one — quick add writes no key at all and falls back
 * to the creation date — so "append a thousand times" is not a gesture anyone
 * performs. A few dozen drags to the bottom of a column costs a character or
 * two of frontmatter, and the failure it replaces was cards silently swapping
 * places, which is a different order of problem.
 */

/**
 * The alphabet, in ASCII order.
 *
 * That ordering is the whole contract: `'9' < 'A' < 'a'` by character code, so
 * comparing two keys with `<` compares them digit by digit. Reordering these
 * characters would silently reorder every board in every vault.
 */
const DIGITS = '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz';
const BASE = DIGITS.length;
const MID = DIGITS[Math.floor(BASE / 2)];

const digit = (char: string): number => DIGITS.indexOf(char);

/** Whether a string is one of these keys, rather than a legacy number. */
export function isOrderKey(value: unknown): value is string {
  return typeof value === 'string'
    && value.length > 0
    && [...value].every((c) => DIGITS.includes(c));
}

/**
 * Something strictly greater than `a`, kept short.
 *
 * Increments the last digit with room rather than appending, so repeatedly
 * adding to the end of a list does not grow the key by a character each time.
 */
function keyAfter(a: string): string {
  for (let i = a.length - 1; i >= 0; i -= 1) {
    const d = digit(a[i]);
    if (d + 1 < BASE) return a.slice(0, i) + DIGITS[d + 1];
  }
  // Every digit is already the highest. Appending is always greater, because a
  // string is greater than any prefix of itself.
  return a + MID;
}

/**
 * Something strictly less than `b`.
 *
 * Walks down `b` looking for a digit with room beneath it. A digit of 0 or 1
 * has none — there is nothing strictly between 0 and 1 — so it writes a 0 in
 * that place, which already puts the answer below `b`, and keeps looking for
 * somewhere to land.
 *
 * The loop cannot run off the end of a key this module generated: every one of
 * them has a digit above 0 somewhere, so either the search finds room or a 0
 * is written under a 1 and the result is below `b` from that point on. A key
 * of nothing but zeroes has nothing beneath it and is rejected rather than
 * silently answered with a duplicate.
 */
function keyBefore(b: string): string {
  let prefix = '';
  let rest = b;
  let belowAlready = false;
  for (;;) {
    const d = rest.length ? digit(rest[0]) : BASE;
    if (d > 1) return prefix + DIGITS[Math.floor(d / 2)];
    if (d === 1) belowAlready = true;
    prefix += DIGITS[0];
    rest = rest.slice(1);
    if (rest.length === 0) {
      if (!belowAlready) {
        throw new Error(`no key sorts before "${b}"`);
      }
      return prefix + MID;
    }
  }
}

/**
 * Something strictly between `a` and `b`, where `a` is non-empty and `a < b`.
 *
 * Walks the common prefix, then splits the first digit that differs. When the
 * two digits are adjacent there is no digit between them, so it keeps `a`'s
 * and descends — anything appended after that is still below `b`, because the
 * digits already differ higher up.
 */
function midpoint(a: string, b: string): string {
  let common = 0;
  while (common < a.length && common < b.length && a[common] === b[common]) common += 1;
  if (common > 0) {
    const head = a.slice(0, common);
    const restA = a.slice(common);
    const restB = b.slice(common);
    // `a` being a prefix of `b` leaves nothing of `a` to split on. What is
    // wanted then is simply something below the rest of `b`, which is a
    // different question and has its own answer.
    return head + (restA.length ? midpoint(restA, restB) : keyBefore(restB));
  }

  const da = digit(a[0]);
  const db = b.length ? digit(b[0]) : BASE;

  if (db - da > 1) {
    return DIGITS[da + Math.floor((db - da) / 2)];
  }

  // Adjacent digits. Keep `a`'s and find something above the rest of it; `b`
  // is out of reach above, because its first digit is already larger.
  const rest = a.slice(1);
  return DIGITS[da] + (rest.length ? keyAfter(rest) : MID);
}

/**
 * A key that sorts between `before` and `after`.
 *
 * `null` on either side means "nothing there" — the start or the end of the
 * column. Both `null` is the first card in an empty column.
 */
export function keyBetween(before: string | null, after: string | null): string {
  if (before !== null && after !== null && before >= after) {
    throw new Error(`keyBetween needs before < after, got "${before}" and "${after}"`);
  }
  if (before === null && after === null) return MID;
  if (before === null) return keyBefore(after!);
  if (after === null) return keyAfter(before);
  return midpoint(before, after);
}

/**
 * Keys for a whole column, in the order given.
 *
 * Used to move a column off the old float ordering in one go: the cards are
 * already on screen in some order, and this is what writes that order down in
 * a form that can be subdivided from then on.
 */
export function keysForSequence(count: number): string[] {
  const keys: string[] = [];
  let previous: string | null = null;
  for (let i = 0; i < count; i += 1) {
    previous = keyBetween(previous, null);
    keys.push(previous);
  }
  return keys;
}
