import { describe, it, expect } from 'vitest';
import en from '../../../i18n/locales/en.json';
import vi from '../../../i18n/locales/vi.json';

/**
 * Guarding the translations the way an audit cannot.
 *
 * These were checked by hand once and had drifted badly by the next pass —
 * one screen with no translated strings at all, another with one. A test
 * fails on the commit that introduces the drift, which is the only moment
 * fixing it is cheap.
 */

// Read through the bundler rather than the filesystem: this suite runs under
// the app's own tsconfig, which has no Node types, and `import.meta.glob` is
// what Vite offers instead.
const sources = import.meta.glob('../*.vue', { query: '?raw', import: 'default', eager: true }) as Record<string, string>;

const componentFiles = Object.entries(sources).map(([path, source]) => ({
  name: path.replace('../', ''),
  source,
}));

const templateOf = (source: string) => {
  const at = source.indexOf('<template>');
  return at === -1 ? '' : source.slice(at);
};

/**
 * Blank out every `{{ … }}`, counting braces so a nested object survives.
 *
 * Done before anything looks for text, and that order is the whole trick: an
 * expression can contain `>` — a ternary on a number — and `{`, and a scan
 * that goes looking for `>text<` first will cut one in half and read the
 * pieces as prose.
 */
const withoutExpressions = (template: string): string => {
  let out = '';
  let i = 0;
  while (i < template.length) {
    if (template.startsWith('{{', i)) {
      let depth = 2;
      let j = i + 2;
      while (j < template.length && depth > 0) {
        if (template[j] === '{') depth++;
        else if (template[j] === '}') depth--;
        j++;
      }
      // Replaced by a space so the text either side does not run together.
      out += ' ';
      i = j;
      continue;
    }
    out += template[i];
    i++;
  }
  return out;
};

describe('the People screens speak both languages', () => {
  it('has a component to check', () => {
    expect(componentFiles.length).toBeGreaterThan(8);
  });

  it('leaves no readable text hard-coded in English', () => {
    const offenders: string[] = [];

    for (const { name, source } of componentFiles) {
      const template = templateOf(source);

      // Text between two tags, including text sitting *beside* an
      // interpolation. The first version required the run to contain no
      // braces at all, which let `Overdue ({{ count }})` through — a heading
      // in plain English that the guard called translated.
      for (const match of withoutExpressions(template).matchAll(/>([^<>]*)</g)) {
        const literal = match[1].trim();
        if (!/[A-Za-z]{3,}/.test(literal)) continue;
        if (/^[\d\s.,:%/–—-]+$/.test(literal)) continue;
        offenders.push(`${name}: ${literal}`);
      }

      // An attribute a person reads, written as a literal rather than bound.
      // The negative lookbehind skips `:title="…"`, which is already an
      // expression and so already translated.
      for (const match of template.matchAll(
        /(?<![:\w-])(title|placeholder|aria-label)="([^"]*[A-Za-z]{3,}[^"]*)"/g
      )) {
        offenders.push(`${name}: ${match[1]}="${match[2]}"`);
      }
    }

    expect(offenders, `hard-coded English:\n${offenders.join('\n')}`).toEqual([]);
  });

  it('uses only keys that exist, in both languages', () => {
    const missing: string[] = [];

    for (const { name, source } of componentFiles) {
      for (const match of source.matchAll(/\$t\(\s*'([^']+)'\s*\)/g)) {
        const path = match[1].split('.');
        for (const [language, bundle] of [['en', en], ['vi', vi]] as const) {
          let node: any = bundle;
          for (const step of path) node = node?.[step];
          if (typeof node !== 'string') {
            missing.push(`${name}: ${match[1]} missing from ${language}`);
          }
        }
      }
    }

    expect(missing, missing.join('\n')).toEqual([]);
  });

  it('translates every People key into Vietnamese, and not by copying it', () => {
    const english = (en as any).people as Record<string, string>;
    const vietnamese = (vi as any).people as Record<string, string>;

    const missing = Object.keys(english).filter(key => !(key in vietnamese));
    expect(missing, `not translated: ${missing.join(', ')}`).toEqual([]);

    // A Vietnamese value identical to the English one is usually a key that
    // was added and never translated. A short list is genuinely the same word
    // in both languages, and is named here so the exception is deliberate.
    const sameInBothLanguages = new Set([
      'email', 'linkedin', 'twitter', 'github', 'facebook', 'instagram', 'note', 'notes',
    ]);
    const untranslated = Object.keys(english).filter(
      key =>
        english[key] === vietnamese[key] &&
        english[key].length > 3 &&
        !sameInBothLanguages.has(english[key].toLowerCase())
    );
    expect(untranslated, `still English in vi.json: ${untranslated.join(', ')}`).toEqual([]);
  });
});
