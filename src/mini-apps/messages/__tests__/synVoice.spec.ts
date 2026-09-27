import { describe, it, expect } from 'vitest';
import en from '../../../i18n/locales/en.json';
import vi from '../../../i18n/locales/vi.json';

/**
 * What Syn's screens say, in both languages (review §5, U6).
 */

/** Every string under `syn`, however deep. */
const strings = (node: unknown, path = 'syn'): Array<[string, string]> =>
  typeof node === 'string'
    ? [[path, node]]
    : Object.entries(node as Record<string, unknown>).flatMap(([k, v]) => strings(v, `${path}.${k}`));

describe('Syn’s Vietnamese', () => {
  /**
   * One register. Ten keys said *tao/mày* beside the rest saying *bạn* — the
   * same assistant switching between intimate and rude from one card to the
   * next.
   */
  it('says bạn, never tao or mày', () => {
    const rude = strings(vi.syn).filter(([, s]) => /(^|[^\p{L}])(tao|mày)(?![\p{L}])/iu.test(s));
    expect(rude.map(([k]) => k)).toEqual([]);
  });
});

describe('the choice card’s explanation', () => {
  /**
   * A pick used to go into the composer and wait for Enter. It carries on
   * now, and the card must not tell somebody to send something that has
   * already gone.
   */
  it('no longer says the pick waits in the composer', () => {
    expect(en.syn.choice_explainer).not.toMatch(/box below|until you send/i);
    expect(vi.syn.choice_explainer).not.toMatch(/ô soạn|cho tới khi .* gửi/i);
  });

  it('says it carries on', () => {
    expect(en.syn.choice_explainer).toMatch(/carries on/i);
    expect(vi.syn.choice_explainer).toMatch(/làm tiếp ngay/i);
  });
});
