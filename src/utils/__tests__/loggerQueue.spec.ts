import { describe, it, expect, vi, beforeEach } from 'vitest';

/**
 * The deferred-log queue has a ceiling.
 *
 * It did not. Lines are queued whenever `document.visibilityState` is
 * `hidden`, and the queue drains only on the next `visibilitychange` — so an
 * app left in the background while anything logs in a loop grows a buffer of
 * formatted strings, each of which may carry a stack, with nothing to stop it.
 *
 * Found while looking for a ten-gigabyte WebKit content process. It is not
 * proof of that fault and is not claimed as one: an unbounded buffer in a
 * logger is worth closing whether or not it is the one that hurt.
 */
describe('the deferred log queue', () => {
  beforeEach(() => {
    vi.resetModules();
    vi.mock('@tauri-apps/plugin-log', () => ({
      info: vi.fn().mockResolvedValue(undefined),
      warn: vi.fn().mockResolvedValue(undefined),
      error: vi.fn().mockResolvedValue(undefined),
      debug: vi.fn().mockResolvedValue(undefined),
      trace: vi.fn().mockResolvedValue(undefined),
    }));
  });

  it('declares a ceiling and trims to it', async () => {
    const source = (await import('../logger.ts?raw')).default;

    const ceiling = Number(source.match(/MAX_QUEUED_LOGS\s*=\s*(\d+)/)?.[1]);
    expect(ceiling, 'logger should declare MAX_QUEUED_LOGS').toBeGreaterThan(0);

    // The push must be followed by a trim in the same branch, or the ceiling
    // is decoration.
    const hiddenBranch = source
      .split("document.visibilityState === 'hidden'")[1]
      ?.split('} else {')[0] ?? '';
    expect(hiddenBranch).toContain('logQueue.push');
    expect(hiddenBranch).toContain('MAX_QUEUED_LOGS');
    expect(hiddenBranch, 'a full queue must drop rather than grow').toMatch(/slice|splice|shift/);
  });

  it('says when it dropped something, so a truncated log is not read as whole', async () => {
    const source = (await import('../logger.ts?raw')).default;
    expect(source).toMatch(/dropped/i);
  });
});
