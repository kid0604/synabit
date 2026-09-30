import { describe, it, expect, vi, beforeEach } from 'vitest';
import { createPinia, setActivePinia } from 'pinia';
import { invoke } from '@tauri-apps/api/core';
import { useAppLockStore, pinErrorKey, timeoutLoosens } from '../useAppLockStore';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

describe('app lock store', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.mocked(invoke).mockReset();
  });

  it('removes the lock only by handing the backend the PIN', async () => {
    vi.mocked(invoke).mockImplementation(async (command: string) =>
      command === 'get_app_lock_config'
        ? { is_enabled: false, app_lock_active: false, protected_apps: [], protected_notes: [], auto_lock_timeout_secs: 300 }
        : null);
    const store = useAppLockStore();
    store.isEnabled = true;
    await store.removeLock('123456');
    expect(invoke).toHaveBeenCalledWith('remove_app_lock', { pin: '123456' });
    expect(store.isEnabled).toBe(false);
  });

  it('leaves nothing locked after a reset', async () => {
    vi.mocked(invoke).mockImplementation(async (command: string) =>
      command === 'get_app_lock_config'
        ? { is_enabled: false, app_lock_active: false, protected_apps: [], protected_notes: [], auto_lock_timeout_secs: 300 }
        : null);
    const store = useAppLockStore();
    store.isAppLocked = true;
    await store.finishReset();
    expect(invoke).toHaveBeenCalledWith('app_lock_reset_finish');
    expect(store.isAppLocked).toBe(false);
  });

  it('puts the backend refusals into words and leaves other errors alone', () => {
    expect(pinErrorKey('PIN_WRONG')).toBe('shell.lock.wrong_pin_plain');
    expect(pinErrorKey('PIN_REQUIRED')).toBe('shell.lock.pin_required');
    expect(pinErrorKey('PIN_LOCKED_OUT')).toBe('shell.lock.too_many');
    expect(pinErrorKey('Keyring error: denied')).toBeNull();
  });

  it('hands the PIN to the backend with a loosening change', async () => {
    vi.mocked(invoke).mockResolvedValue(null);
    const store = useAppLockStore();
    store.appLockActive = true;
    store.isAppLocked = true;
    store.protectedApps = ['safe', 'journal'];
    store.protectedNotes = ['n1'];

    await store.setAppLockActive(false, '123456');
    expect(invoke).toHaveBeenLastCalledWith('update_app_lock_config', { config: { app_lock_active: false }, pin: '123456' });
    expect(store.appLockActive).toBe(false);
    expect(store.isAppLocked).toBe(false);

    await store.toggleProtectedApp('safe', '123456');
    expect(invoke).toHaveBeenLastCalledWith('update_app_lock_config', { config: { protected_apps: ['journal'] }, pin: '123456' });
    expect(store.protectedApps).toEqual(['journal']);

    store.unlockNote('n1');
    await store.toggleProtectedNote('n1', '123456');
    expect(invoke).toHaveBeenLastCalledWith('update_app_lock_config', { config: { protected_notes: [] }, pin: '123456' });
    expect(store.unlockedNotes.has('n1')).toBe(false);

    await store.setAutoLockTimeout(0, '123456');
    expect(invoke).toHaveBeenLastCalledWith('update_app_lock_config', { config: { auto_lock_timeout_secs: 0 }, pin: '123456' });
    expect(store.autoLockTimeoutSecs).toBe(0);
  });

  it('sends no PIN when tightening', async () => {
    vi.mocked(invoke).mockResolvedValue(null);
    const store = useAppLockStore();
    await store.toggleProtectedApp('safe');
    expect(invoke).toHaveBeenLastCalledWith('update_app_lock_config', { config: { protected_apps: ['safe'] }, pin: null });
    expect(store.protectedApps).toEqual(['safe']);
  });

  it('leaves everything as it was when the backend refuses', async () => {
    vi.mocked(invoke).mockRejectedValue('PIN_WRONG');
    const store = useAppLockStore();
    store.appLockActive = true;
    store.isAppLocked = true;
    store.protectedApps = ['safe'];
    store.protectedNotes = ['n1'];
    store.autoLockTimeoutSecs = 300;

    await expect(store.setAppLockActive(false, '000000')).rejects.toBe('PIN_WRONG');
    await expect(store.toggleProtectedApp('safe', '000000')).rejects.toBe('PIN_WRONG');
    await expect(store.toggleProtectedNote('n1')).rejects.toBe('PIN_WRONG');
    await expect(store.setAutoLockTimeout(0)).rejects.toBe('PIN_WRONG');

    expect(store.appLockActive).toBe(true);
    expect(store.isAppLocked).toBe(true);
    expect(store.protectedApps).toEqual(['safe']);
    expect(store.protectedNotes).toEqual(['n1']);
    expect(store.autoLockTimeoutSecs).toBe(300);
    expect(pinErrorKey('PIN_WRONG')).toBe('shell.lock.wrong_pin_plain');
  });

  it('counts a longer or no timeout as loosening, the way the backend does', () => {
    expect(timeoutLoosens(300, 900)).toBe(true);
    expect(timeoutLoosens(300, 0)).toBe(true);
    expect(timeoutLoosens(300, 60)).toBe(false);
    expect(timeoutLoosens(300, 300)).toBe(false);
    expect(timeoutLoosens(0, 60)).toBe(false);
    expect(timeoutLoosens(0, 0)).toBe(false);
  });
});
