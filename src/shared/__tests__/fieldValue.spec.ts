import { describe, it, expect } from 'vitest';
import { kindOf, toText, parseText, valueOf } from '../fieldValue';

describe('a frontmatter value on its way to the screen and back', () => {
  it('recognises what it is looking at', () => {
    expect(kindOf(false)).toBe('boolean');
    expect(kindOf(1781838764424)).toBe('number');
    expect(kindOf(['mdp', 'network'])).toBe('list');
    expect(kindOf('2025-05-26')).toBe('date');
    expect(kindOf('2025-05-26 09:30')).toBe('date');
    expect(kindOf({ a: 1 })).toBe('json');
    expect(kindOf('Harari')).toBe('text');
    expect(kindOf('')).toBe('text');
  });

  /** A version string is not a number and a name is not a date. */
  it('does not mistake text that merely looks structured', () => {
    expect(kindOf('1.2.3')).toBe('text');
    expect(kindOf('2025-5-26')).toBe('text');
  });

  it('shows a list as its JSON rather than as [object Object]', () => {
    expect(toText(['mdp', 'network'])).toBe('["mdp","network"]');
    expect(toText({ done: 2 })).toBe('{"done":2}');
    expect(toText(null)).toBe('');
    expect(toText(undefined)).toBe('');
  });

  /**
   * The bug this whole module exists for.
   *
   * The old pair only parsed text starting with `[` or `{`, so `false` came
   * back as the string `"false"` — which YAML writes as `'false'` and
   * JavaScript reads as true. Three task files in this vault already carry a
   * quoted boolean, so the failure is not theoretical: an unpinned note comes
   * back pinned.
   */
  it('reads an edited boolean back as a boolean', () => {
    expect(parseText('false')).toBe(false);
    expect(parseText('true')).toBe(true);
    expect(parseText(' true ')).toBe(true);
    expect(typeof parseText('false')).toBe('boolean');
  });

  it('reads an edited number back as a number', () => {
    expect(parseText('5')).toBe(5);
    expect(parseText('-2.5')).toBe(-2.5);
    expect(parseText('1.2.3')).toBe('1.2.3');
  });

  it('keeps malformed JSON as the text somebody typed', () => {
    expect(parseText('["a",')).toBe('["a",');
    expect(parseText('["a","b"]')).toEqual(['a', 'b']);
  });

  /**
   * Opening a node and saving it must not change the file.
   *
   * This is the guarantee, and it is stronger than parsing well: an untouched
   * field is written back as the very value that was read, so nothing this
   * module infers can be applied to a field nobody edited. Inference is a
   * guess, and a guess belongs only where somebody typed.
   */
  describe('an untouched field', () => {
    const unchanged = (value: unknown) =>
      expect(valueOf(toText(value), value)).toBe(value);

    it('goes back exactly as it came', () => {
      unchanged(false);
      unchanged(true);
      unchanged(0);
      unchanged(1781838764424);
      unchanged('');
      unchanged('Harari');
      unchanged('2025-05-26');
    });

    /** Including the values whose text form would parse to something else. */
    it('survives even when its own text would be read as another type', () => {
      // A string that says "true" stays that string, because nobody edited it.
      expect(valueOf('true', 'true')).toBe('true');
      expect(typeof valueOf('true', 'true')).toBe('string');
      // A string "5" stays a string.
      expect(valueOf('5', '5')).toBe('5');
      expect(typeof valueOf('5', '5')).toBe('string');
    });

    it('keeps a list identical rather than rebuilding it', () => {
      const tags = ['mdp', 'network'];
      expect(valueOf(toText(tags), tags)).toBe(tags);
    });
  });

  it('parses only what was actually edited', () => {
    // Switched off in the UI: text differs from the original, so it parses.
    expect(valueOf('false', true)).toBe(false);
    // Typed into a brand-new field, which has no original at all.
    expect(valueOf('7', undefined)).toBe(7);
  });
});
