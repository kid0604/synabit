import { describe, it, expect } from 'vitest';

import en from '../../../i18n/locales/en.json';
import vi from '../../../i18n/locales/vi.json';
import source from '../AskBar.vue?raw';
import { THREAD_STATES } from '../useThreads';

/**
 * Every string the ask bar shows, in both languages.
 *
 * The bar is the surface a person reaches for without thinking, which makes it
 * the worst place in the app for a raw `syn.ask_placeholder` to appear. A
 * missing key renders as its own name and nothing fails — so this is the thing
 * that fails instead.
 */
// Read as raw source, the way the other source-reading specs here do — this
// file is type-checked by `vue-tsc`, which has no Node types to offer `fs`.
const bar = source;

/**
 * The keys the component actually asks for, read from the component.
 *
 * Both spellings, because the template reaches for one of them with a template
 * literal: `t(\`syn.thread_state_${thread.state}\`)`. A regex that only caught
 * the quoted form would have passed over five keys and proved nothing about
 * them — so the states are expanded from the list that defines them.
 */
const used = [
  ...[...bar.matchAll(/t\('syn\.((?:ask|thread)_[a-z_]+)'/g)].map((m) => m[1]),
  ...THREAD_STATES.map((state) => `thread_state_${state}`),
];

describe('what the ask bar says', () => {
  /** Every state a thread can be in has a word for it in both languages. */
  it('has a word for every state a thread can be in', () => {
    for (const state of THREAD_STATES) {
      expect(en.syn, `en is missing thread_state_${state}`).toHaveProperty(`thread_state_${state}`);
      expect(vi.syn, `vi is missing thread_state_${state}`).toHaveProperty(`thread_state_${state}`);
    }
  });

  it('asks for keys at all', () => {
    // A guard on the guard: if the extraction stops matching, every assertion
    // below passes over an empty list and this file goes quiet while proving
    // nothing.
    expect(used.length).toBeGreaterThan(5);
  });

  it('has every key in both languages', () => {
    for (const key of used) {
      expect(en.syn, `en is missing syn.${key}`).toHaveProperty(key);
      expect(vi.syn, `vi is missing syn.${key}`).toHaveProperty(key);
    }
  });

  /**
   * A placeholder with nothing behind it renders as `{tool}` on screen. Both
   * languages have to want the same values, because the component passes one
   * set of them.
   */
  it('uses the same placeholders in both languages', () => {
    const placeholders = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();

    for (const key of used) {
      const e = (en.syn as Record<string, string>)[key];
      const v = (vi.syn as Record<string, string>)[key];
      expect(placeholders(v), `syn.${key} differs between languages`).toEqual(placeholders(e));
    }
  });

  /**
   * The line that says what was picked up off the screen has to name the
   * amount, or it is telling the user nothing they could not already guess.
   */
  it('says how much it can see', () => {
    expect(en.syn.ask_sees_selection).toContain('{chars}');
    expect(vi.syn.ask_sees_selection).toContain('{chars}');
    expect(en.syn.ask_sees_node).toContain('{node}');
    expect(vi.syn.ask_sees_node).toContain('{node}');
  });
});

/**
 * Coined product terms stay in English, in both languages.
 *
 * The Vietnamese locale already does this everywhere it matters — `vault`,
 * `task`, `note`, `token`, `prompt`, `recipe`, `prose` and `tool` are all
 * left alone — and translates only words that were ordinary Vietnamese to
 * begin with, like `skill` → *kỹ năng*.
 *
 * `thread` belongs in the first group and was briefly in the second. Rendered
 * as *sợi* it is a literal translation of the wrong sense of the word: a
 * strand of fibre, not a piece of work with five states. Nobody says it, and a
 * term nobody says is a term nobody reaches for.
 */
describe('what a thread is called', () => {
  const terms = ['sợi', 'Sợi', 'vạch', 'Vạch'];

  it('is never translated into a word for a strand of fibre', () => {
    for (const [lang, locale] of [['en', en], ['vi', vi]] as const) {
      for (const [key, value] of Object.entries(locale.syn as Record<string, unknown>)) {
        if (typeof value !== 'string') continue;
        for (const term of terms) {
          expect(value, `${lang}.syn.${key} calls it "${term}"`).not.toContain(term);
        }
      }
    }
  });
});
