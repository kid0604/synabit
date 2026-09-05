import { describe, it, expect } from 'vitest';

import { askPhrase } from '../useSynConsent';
import en from '../../../../i18n/locales/en.json';
import vi from '../../../../i18n/locales/vi.json';
import type { Capability } from '../../types';

/**
 * The sentence on a consent card comes from i18n, keyed on the capability.
 *
 * The backend also sends an English `about` string, and showing that would be
 * the easy thing to do. It would also mean a Vietnamese user reads a permission
 * prompt in English — and a permission prompt is the last place in an app to
 * fall back to the wrong language, because it is the one screen where not
 * understanding the sentence and clicking anyway is the whole failure.
 */
const every: Capability[] = [
  'VaultRead',
  'VaultWrite',
  'VaultStructural',
  { NetRead: { domain: 'example.com' } },
  { NetWrite: { domain: 'example.com', tool: 'post_message' } },
  { Spend: { cents_estimate: 250 } },
  'Execute',
];

describe('what a consent card says', () => {
  it('has a sentence for every capability, in both languages', () => {
    for (const capability of every) {
      const { key } = askPhrase(capability);
      const short = key.replace('syn.', '');

      expect(en.syn, `en is missing ${key}`).toHaveProperty(short);
      expect(vi.syn, `vi is missing ${key}`).toHaveProperty(short);
    }
  });

  /**
   * The values a sentence needs have to be the ones its text asks for. A
   * placeholder with nothing behind it renders as `{domain}` on the one screen
   * where a person is deciding whether to let something out of the building.
   */
  it('supplies every value its own sentence asks for', () => {
    for (const capability of every) {
      const { key, values } = askPhrase(capability);
      const short = key.replace('syn.', '') as keyof typeof en.syn;

      for (const locale of [en, vi]) {
        const text = locale.syn[short] as string;
        const wanted = [...text.matchAll(/\{(\w+)\}/g)].map(m => m[1]);
        for (const name of wanted) {
          expect(values, `${key} says {${name}} and nothing supplies it`).toHaveProperty(name);
        }
      }
    }
  });

  it('names the host in the sentence, not only in the log', () => {
    const { values } = askPhrase({ NetWrite: { domain: 'mail.example.com', tool: 'send' } });
    expect(values.domain).toBe('mail.example.com');
    expect(values.tool).toBe('send');
  });

  it('turns cents into the amount a person reads', () => {
    expect(askPhrase({ Spend: { cents_estimate: 250 } }).values.amount).toBe('2.50');
    expect(askPhrase({ Spend: { cents_estimate: 7 } }).values.amount).toBe('0.07');
  });
});
