/**
 * The chips, run against every question the Rust grammar is pinned to.
 *
 * The gate for step 4 in `docs/query-grammar-2026-09-20.md` §15.3 is that the
 * words → chips → words round trip stays exact for every question in §15.1 —
 * that is, for the corpus in `src-tauri/src/search_gate.txt`, which is the
 * golden file the parser and both runners are held to.
 *
 * Reading that file rather than copying the questions is the point. A question
 * added on the Rust side to pin a new shape of grammar arrives here on its
 * own, so the bar cannot quietly stop being able to draw something the engine
 * can answer.
 */
import { describe, it, expect } from 'vitest';
// The golden file itself, as text. Imported rather than read off disk:
// `vite/client` declares `*?raw`, so this needs no Node types in a config
// that has none.
import goldenFile from '../../../src-tauri/src/search_gate.txt?raw';
import { chipsOf, tokenise } from '../queryChips';

/** Every question the golden file asks, in the order it asks them. */
function questionsAskedOfTheEngine(): string[] {
  return goldenFile
    .split('\n')
    // Each line is `<question padded to 34> → <answer>`.
    .map((line: string) => line.split('→')[0] ?? '')
    .map((question: string) => question.trimEnd())
    .filter((question: string) => question.length > 0);
}

/**
 * Whether a token stream is still a question: brackets balanced, and no `OR`
 * left with nothing on one side.
 *
 * Written out here rather than imported, because the app has no parser on this
 * side and must not grow one — the whole design is that the text is the state
 * and Rust does the reading. This is a test's own yardstick, and it only has
 * to be strict enough to catch a chip that cuts a question in half.
 */
function stillAQuestion(tokens: string[]): boolean {
  let depth = 0;
  for (let i = 0; i < tokens.length; i += 1) {
    const token = tokens[i];
    if (token === '(') depth += 1;
    else if (token === ')') {
      depth -= 1;
      if (depth < 0) return false;
    } else if (token === 'OR') {
      const before = tokens[i - 1];
      const after = tokens[i + 1];
      if (before === undefined || before === '(' || before === 'OR') return false;
      if (after === undefined || after === ')' || after === 'OR') return false;
    }
  }
  return depth === 0;
}

describe('The chips, against the questions the engine is pinned to', () => {
  const questions = questionsAskedOfTheEngine();

  it('reads the golden file at all', () => {
    // If this drops to nothing the rest of the file passes vacuously, which is
    // the one way a corpus test can lie.
    expect(questions.length).toBeGreaterThan(50);
    expect(questions).toContain('(with:khánh OR with:minh) when:2019');
  });

  /// The property the whole design rests on: chips are the text, so there is
  /// nothing to keep in step.
  it('slices every one of them back into exactly what was asked', () => {
    for (const question of questions) {
      const tokens = tokenise(question);
      expect(chipsOf(question).map(c => c.text).join(' '), question).toBe(tokens.join(' '));
    }
  });

  /// And a chip's own text has to tokenise the way it did inside the whole
  /// question — otherwise taking one off and putting it back would not be the
  /// same question.
  it('slices a chip the same way whether it stands alone or in company', () => {
    for (const question of questions) {
      for (const chip of chipsOf(question)) {
        expect(tokenise(chip.text).join(' '), `${question} → ${chip.text}`).toBe(chip.text);
      }
    }
  });

  /// **The property a chip exists for.** A chip is a thing you can take off,
  /// so taking one off must leave a question — not `OR` with nothing on one
  /// side, and not an unclosed bracket.
  ///
  /// This is the assertion that has teeth. The round trip above holds just as
  /// well for a *wrong* slicing: chipping `#a OR #b` into three pieces still
  /// joins back to the same words. It is only when one of those pieces is
  /// removed that the wrongness shows.
  it('leaves a question behind whichever chip is taken off', () => {
    for (const question of questions) {
      const tokens = tokenise(question);
      // Three of the golden questions are broken on purpose, to pin what the
      // engine refuses. Nothing can be promised about taking a chip off one.
      if (!stillAQuestion(tokens)) continue;

      for (const chip of chipsOf(question)) {
        const left = [...tokens];
        left.splice(chip.from, chip.to - chip.from);
        expect(stillAQuestion(left), `${question} → without "${chip.text}" → ${left.join(' ')}`)
          .toBe(true);
      }
    }
  });

  /// Every chip covers a real run of tokens, and together they cover all of
  /// them exactly once — no gaps, no overlaps. Without this, taking a chip off
  /// could take a neighbour's token with it.
  it('covers every token exactly once', () => {
    for (const question of questions) {
      const tokens = tokenise(question);
      const chips = chipsOf(question);
      let next = 0;
      for (const chip of chips) {
        expect(chip.from, question).toBe(next);
        expect(chip.to, question).toBeGreaterThan(chip.from);
        next = chip.to;
      }
      expect(next, question).toBe(tokens.length);
    }
  });
});
