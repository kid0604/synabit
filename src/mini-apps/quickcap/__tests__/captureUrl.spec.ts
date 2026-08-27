import { describe, it, expect } from 'vitest';
import { isComposeUrl } from '../captureUrl';

/**
 * The launcher shortcut fires this URL, and so will the desktop hotkey. It
 * shares a scheme with the Google Drive OAuth redirect and with capture URLs
 * that carry text, so being exact about which is which is what keeps a
 * shortcut from swallowing a sign-in.
 */
describe('isComposeUrl', () => {
  it('recognises the URL the launcher shortcut fires', () => {
    expect(isComposeUrl('com.synabit.app://quickcap/compose')).toBe(true);
  });

  it('accepts the shorter scheme too', () => {
    expect(isComposeUrl('synabit://quickcap/compose')).toBe(true);
  });

  it('tolerates a trailing slash and a query it does not need', () => {
    expect(isComposeUrl('com.synabit.app://quickcap/compose/')).toBe(true);
    expect(isComposeUrl('com.synabit.app://quickcap/compose?from=widget')).toBe(true);
  });

  it('leaves the OAuth redirect alone', () => {
    expect(isComposeUrl('com.synabit.app://oauth2redirect?code=abc')).toBe(false);
  });

  /** A capture carries text and is queued by Rust; this one must not claim it. */
  it('is not a capture URL', () => {
    expect(isComposeUrl('com.synabit.app://quickcap/new?text=hi')).toBe(false);
  });

  it('rejects anything else', () => {
    expect(isComposeUrl('https://example.com/quickcap/compose')).toBe(false);
    expect(isComposeUrl('com.synabit.app://quickcap')).toBe(false);
    expect(isComposeUrl('')).toBe(false);
  });
});
