import { describe, expect, it, vi } from 'vitest';
import { onBeforeQuit, runBeforeQuit } from '../useBeforeQuit';

describe('work before quitting', () => {
  it('waits for every task, and a failing one does not hold up the rest', async () => {
    const done: string[] = [];
    const a = onBeforeQuit(async () => { await new Promise((r) => setTimeout(r, 20)); done.push('board'); });
    const b = onBeforeQuit(() => { throw new Error('no'); });
    await runBeforeQuit();
    expect(done).toEqual(['board']);
    a(); b();
  });

  it('never waits longer than its limit', async () => {
    vi.useFakeTimers();
    const stop = onBeforeQuit(() => new Promise(() => {}));
    const run = runBeforeQuit(100);
    vi.advanceTimersByTime(100);
    await run;
    stop();
    vi.useRealTimers();
  });
});
