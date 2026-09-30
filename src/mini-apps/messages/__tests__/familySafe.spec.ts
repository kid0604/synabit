import { describe, it, expect, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { needsPinToSave, getFamilySafe, setFamilySafe } from '../familySafe';
import settingsComposable from '../composables/useSynSettings.ts?raw';
import settingsPanel from '../components/SynSettings.vue?raw';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

describe('family-safe answers', () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it('asks for the PIN only to switch them off, and only when there is a PIN', () => {
    expect(needsPinToSave(true, false, true)).toBe(true);
    expect(needsPinToSave(true, false, false)).toBe(false);
    expect(needsPinToSave(false, true, true)).toBe(false);
    expect(needsPinToSave(true, true, true)).toBe(false);
    expect(needsPinToSave(false, false, true)).toBe(false);
  });

  // The instruction itself is appended in Rust (`syn::family_safe`, tested
  // there). What this side must not do is lose the switch: a field the type
  // does not carry is dropped on the next save, and Reset must not be a way
  // round the PIN.
  it('keeps the setting through a save and through Reset', () => {
    expect(settingsComposable).toMatch(/family_safe: boolean;/);
    expect(settingsComposable).toMatch(/family_safe: settings\.value\.family_safe/);
  });

  it('checks the PIN on save, the one door every change goes through', () => {
    expect(settingsPanel).toMatch(/needsPinToSave\(familySafeOnDisk\.value, settings\.value\.family_safe, appLock\.isEnabled\)/);
  });

  // The switch lives on the device, and the backend checks the PIN; the
  // settings file is not a way to change it (`syn_save_settings` ignores it).
  it('asks the backend, with the PIN, to switch them', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await setFamilySafe(false, { pin: '123456', vaultPath: '/vault' });
    expect(invoke).toHaveBeenCalledWith('set_family_safe', { on: false, pin: '123456', vaultPath: '/vault' });
    await setFamilySafe(true);
    expect(invoke).toHaveBeenLastCalledWith('set_family_safe', { on: true, pin: null, vaultPath: null });
  });

  it('reads them from the backend', async () => {
    vi.mocked(invoke).mockResolvedValue(true);
    expect(await getFamilySafe('/vault')).toBe(true);
    expect(invoke).toHaveBeenCalledWith('get_family_safe', { vaultPath: '/vault' });
  });

  it('hands the PIN the lock screen checked on to the backend', () => {
    expect(settingsPanel).toMatch(/const pinGiven = async \(pin\?: string\)/);
    expect(settingsPanel).toMatch(/setFamilySafe\(settings\.value\.family_safe, \{ pin, vaultPath: props\.vaultPath \}\)/);
  });
});
