import { describe, it, expect } from 'vitest';

// Imported rather than read off disk: `?raw` and JSON imports are Vite's own,
// already typed by `vite/client`, and they keep this spec inside the same
// module graph the app is built from. Reading with `node:fs` worked under
// vitest and failed `vue-tsc`, which has no Node types.
import source from '../RunInspector.vue?raw';
import en from '../../../../i18n/locales/en.json';
import vi from '../../../../i18n/locales/vi.json';

/**
 * Every tab the panel declares is a tab somebody can reach.
 *
 * The `Tab` union and the `v-for` that renders the buttons are two lists of
 * the same fact, written eight lines apart, and nothing links them. They
 * drifted: the Memory tab was built — its branch, its data, its labels, all of
 * it — and the button row still said `['runs', 'prompt']`. The panel opened
 * with two buttons and a third screen nobody could get to.
 *
 * Nothing caught it. `vue-tsc` is happy, because a shorter array is still a
 * `Tab[]`. ESLint is happy. Every unit test is happy. It is only visible by
 * opening the app and counting buttons, which is how it was found.
 *
 * Read out of the source rather than by mounting, because mounting needs the
 * i18n plugin, a Tauri `invoke` mock and a vault path, and none of that has
 * anything to do with the question being asked.
 */
describe('the inspector panel', () => {

  /** The members of `type Tab = 'a' | 'b'`. */
  const declared = (): string[] => {
    const union = source.split('type Tab =')[1]?.split(';')[0];
    expect(union, 'RunInspector should still declare a `Tab` union').toBeTruthy();
    return [...union.matchAll(/'([^']+)'/g)].map(m => m[1]);
  };

  /** The members of the array the button row iterates. */
  const rendered = (): string[] => {
    const list = source.split('v-for="option in (')[1]?.split(' as Tab[])')[0];
    expect(list, 'the button row should still iterate a literal array').toBeTruthy();
    return [...list.matchAll(/'([^']+)'/g)].map(m => m[1]);
  };

  it('renders a button for every tab it declares', () => {
    expect(rendered()).toEqual(declared());
  });

  it('has a label for every tab, in both languages', () => {
    for (const [locale, messages] of [['en', en], ['vi', vi]] as const) {
      const labels = (messages as { syn: Record<string, string> }).syn;
      for (const tab of declared()) {
        expect(
          labels[`inspector_tab_${tab}`],
          `${locale}: syn.inspector_tab_${tab} is missing, so that tab renders a raw key`,
        ).toBeTruthy();
      }
    }
  });
});
