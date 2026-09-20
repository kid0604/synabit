import { describe, it, expect } from 'vitest';
import { createI18n } from 'vue-i18n';
import en from '../../i18n/locales/en.json';
import vi from '../../i18n/locales/vi.json';
import { refusalText, wasRefused } from '../refusal';

const translator = (locale: 'en' | 'vi') =>
  createI18n({ legacy: false, locale, messages: { en, vi } }).global.t;

/** What `AppError::Refused` serialises to. */
const refused = (name: string, message: string, args: string[] = []) => ({
  code: `REFUSED:${name}`,
  message,
  args,
});

describe('A refusal, in the reader’s own language', () => {
  it('says it in Vietnamese, with the arguments in it', () => {
    const e = refused('renamed', "'notes' is now 'nodes'", ['notes', 'nodes']);
    expect(refusalText(e, translator('vi'))).toBe("'notes' bây giờ là 'nodes'");
    expect(refusalText(e, translator('en'))).toBe("'notes' is now 'nodes'");
  });

  /// The one the whole guardrail rests on: somebody has to see the price.
  it('carries the numbers through into the translation', () => {
    const e = refused('would_spend', 'ask 15 would send 208 lines…', ['15', '208']);
    const vi = refusalText(e, translator('vi'));
    expect(vi).toContain('15');
    expect(vi).toContain('208');
    expect(vi).toContain('model');
  });

  it('knows a refusal from a failure', () => {
    expect(wasRefused(refused('nothing_to_match', 'x'))).toBe(true);
    expect(wasRefused({ code: 'GENERAL_ERROR', message: 'Query error: disk I/O' })).toBe(false);
  });

  /// An SQL failure is a bug report, not a sentence to act on — it comes
  /// through as itself.
  it('passes anything that is not a refusal straight through', () => {
    const bug = { code: 'GENERAL_ERROR', message: 'Query error: disk I/O' };
    expect(refusalText(bug, translator('vi'))).toBe('Query error: disk I/O');
  });

  /// If a code ever slips past the Rust-side gate, English beats a key.
  it('falls back to the engine’s own words rather than showing a key', () => {
    const e = refused('a_code_nobody_translated', 'the engine still said this');
    expect(refusalText(e, translator('vi'))).toBe('the engine still said this');
  });

  /// Every code the engine can produce has a Vietnamese sentence. The Rust
  /// side asserts the same thing from its end; this asserts the file the app
  /// actually loads parses and is reachable through `t`.
  it('has a Vietnamese line for every code the engine knows', () => {
    const t = translator('vi');
    const codes = Object.keys((en as Record<string, any>).query.refused);
    expect(codes.length).toBeGreaterThan(30);
    for (const code of codes) {
      const key = `query.refused.${code}`;
      expect(t(key), code).not.toBe(key);
      expect(t(key), code).not.toBe((en as Record<string, any>).query.refused[code]);
    }
  });
});
