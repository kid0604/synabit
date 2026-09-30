import { describe, it, expect } from 'vitest';
import { syncErrorKey } from '../syncErrorText';

const FALLBACK = 'shell.sync.failed';

describe('syncErrorKey', () => {
  it('names a command this build does not have', () => {
    expect(syncErrorKey('Command p2p_pair_initiate not found', FALLBACK)).toBe('shell.sync_errors.unavailable');
  });

  it('recognises the missing encryption key the backend reports', () => {
    expect(syncErrorKey('E2EE key not set up. Please set up encryption first.', FALLBACK)).toBe('shell.sync_errors.no_key');
  });

  it('reads an AppError object, not "[object Object]"', () => {
    expect(syncErrorKey({ code: 'io', message: 'Connection refused (os error 61)' }, FALLBACK)).toBe('shell.sync_errors.offline');
  });

  it('knows a bad pairing code', () => {
    expect(syncErrorKey('Pairing code expired', FALLBACK)).toBe('shell.sync_errors.bad_code');
    expect(syncErrorKey('invalid pairing code', FALLBACK)).toBe('shell.sync_errors.bad_code');
  });

  it('falls back to the general sentence for anything else', () => {
    expect(syncErrorKey('deserialize error: expected value at line 1', FALLBACK)).toBe(FALLBACK);
    expect(syncErrorKey(undefined, FALLBACK)).toBe(FALLBACK);
  });
});
