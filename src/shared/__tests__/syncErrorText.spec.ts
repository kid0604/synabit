import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { isSyncBusy, syncErrorKey } from '../syncErrorText';

declare const __dirname: string;

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

/**
 * What `sync_full` answers when it turns a second sync away, and when the
 * keychain is waiting on a macOS dialog (`commands::sync`). The texts are
 * Rust's; these keep the page reading them.
 */
describe('a sync that did not run', () => {
  const RUST = readFileSync(resolve(__dirname, '../../../src-tauri/src/commands/sync.rs'), 'utf8');
  const constant = (name: string) => new RegExp(`pub const ${name}: &str = "([^"]+)"`).exec(RUST)?.[1] ?? '';

  it('is busy, not failed, while another runs', () => {
    expect(isSyncBusy(constant('SYNC_BUSY'))).toBe(true);
    expect(isSyncBusy({ code: 'general', message: constant('SYNC_BUSY') })).toBe(true);
    expect(isSyncBusy('Connection refused')).toBe(false);
  });

  it('says to answer the keychain dialog when it is waiting', () => {
    expect(syncErrorKey(constant('KEYCHAIN_WAITING'), FALLBACK)).toBe('shell.sync_errors.keychain_waiting');
  });
});
