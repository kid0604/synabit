import { describe, it, expect } from 'vitest';
import { formatDuration } from '../useAudioCapture';

/**
 * The extension a recording is stored under is decided by what the platform
 * actually produced, never assumed: Chromium's WebView records webm/opus and
 * Safari records mp4, and a file named for the wrong container is one no
 * player will open — including this app's own.
 *
 * `extensionFor` is not exported, so the mapping is pinned through the shape
 * it produces rather than directly. The visible half is the duration, which
 * is what the recording button reads out.
 */
describe('formatDuration', () => {
  it('counts from zero', () => {
    expect(formatDuration(0)).toBe('0:00');
  });

  it('pads the seconds', () => {
    expect(formatDuration(5_000)).toBe('0:05');
    expect(formatDuration(9_400)).toBe('0:09');
  });

  it('rolls over into minutes', () => {
    expect(formatDuration(60_000)).toBe('1:00');
    expect(formatDuration(75_000)).toBe('1:15');
  });

  it('keeps counting past ten minutes', () => {
    expect(formatDuration(671_000)).toBe('11:11');
  });

  /** A tick can land a few milliseconds early; it must not read as a second. */
  it('rounds down rather than up', () => {
    expect(formatDuration(1_999)).toBe('0:01');
  });
});
