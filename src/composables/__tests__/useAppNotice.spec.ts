import { describe, it, expect, vi, afterEach } from 'vitest';
import { appNotices, showAppNotice } from '../useAppNotice';

describe('showAppNotice', () => {
  afterEach(() => {
    vi.useRealTimers();
    appNotices.value = [];
  });

  it('lets a plain notice leave by itself', () => {
    vi.useFakeTimers();
    showAppNotice('Saved');
    vi.advanceTimersByTime(6000);
    expect(appNotices.value).toHaveLength(0);
  });

  it('keeps a notice with actions until it is answered', () => {
    vi.useFakeTimers();
    const run = vi.fn();
    showAppNotice('Hidden by simple mode', 'info', [{ label: 'Open anyway', run }]);
    vi.advanceTimersByTime(60_000);
    expect(appNotices.value).toHaveLength(1);
    appNotices.value[0].actions![0].run();
    expect(run).toHaveBeenCalled();
  });
});
