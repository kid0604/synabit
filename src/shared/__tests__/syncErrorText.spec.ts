import { describe, it, expect } from 'vitest';
import { syncErrorKey } from '../syncErrorText';

const FALLBACK = 'shell.sync.failed';

describe('syncErrorKey', () => {
  it('recognises the missing encryption key the backend reports', () => {
    expect(syncErrorKey('E2EE key not set up. Please set up encryption first.', FALLBACK)).toBe('shell.sync_errors.no_key');
  });

  it('reads an AppError object, not "[object Object]"', () => {
    expect(syncErrorKey({ code: 'io', message: 'Connection refused (os error 61)' }, FALLBACK)).toBe('shell.sync_errors.offline');
  });

  it('falls back to the general sentence for anything else', () => {
    expect(syncErrorKey('deserialize error: expected value at line 1', FALLBACK)).toBe(FALLBACK);
    expect(syncErrorKey(undefined, FALLBACK)).toBe(FALLBACK);
  });
});
