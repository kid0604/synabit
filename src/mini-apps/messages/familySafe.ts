import { invoke } from '@tauri-apps/api/core';

/**
 * Family-safe answers: whether Syn's prompts carry the app's family-safe
 * instruction (`src-tauri/src/syn/family_safe.rs`).
 *
 * # Where the switch lives
 *
 * On this device, beside the app-lock PIN hash — not in the vault's
 * `Syn/settings.json`, which anybody with the folder (or any device it syncs
 * to) can edit. The vault's old flag is read only to carry an existing "on"
 * across once. So it is per device: switching it on here does not switch it
 * on anywhere else.
 *
 * # What guards it
 *
 * The backend. `set_family_safe` refuses to turn it off without the app-lock
 * PIN when one is set, checked against the stored hash with the same attempt
 * limit as the lock screen; `syn_save_settings` ignores the field entirely.
 * The screens ask for the PIN first only so the person is asked before they
 * are refused.
 *
 * It is a lock on the app, not on the machine: somebody who can open this OS
 * account's keychain can change it, as they can remove the PIN itself.
 *
 * # For another screen that wants the switch
 *
 * ```ts
 * const on = await getFamilySafe(vaultPath);
 * if (needsPinToSave(on, wanted, appLock.isEnabled)) {
 *   // show <LockScreen @unlocked="(pin) => setFamilySafe(false, { pin, vaultPath })" />
 * } else {
 *   await setFamilySafe(wanted, { vaultPath });
 * }
 * ```
 * A refusal rejects with a code; `pinErrorKey` (in `useAppLockStore`) turns
 * it into an i18n key.
 */

/**
 * Whether asking for the PIN comes first.
 *
 * Turning family-safe *off* is the one change that is guarded. `was` is what
 * is in force (what was loaded or last saved), not what the form showed a
 * moment ago: flipping it off and on again asks nothing, because nothing
 * changes. With no PIN set there is nothing to ask for — the app lock is the
 * household's one secret.
 */
export function needsPinToSave(was: boolean, now: boolean, pinIsSet: boolean): boolean {
  return pinIsSet && was && !now;
}

/**
 * Whether family-safe answers are on, on this device.
 *
 * `vaultPath` lets a vault that had them on (before the switch moved) carry
 * that across; it can never turn them off.
 */
export function getFamilySafe(vaultPath?: string): Promise<boolean> {
  return invoke<boolean>('get_family_safe', { vaultPath: vaultPath ?? null });
}

/**
 * Switch family-safe answers on or off on this device. Off needs `pin` when
 * a PIN is set; the promise rejects with `PIN_REQUIRED`, `PIN_WRONG` or
 * `PIN_LOCKED_OUT` otherwise.
 */
export function setFamilySafe(on: boolean, opts: { pin?: string; vaultPath?: string } = {}): Promise<void> {
  return invoke<void>('set_family_safe', {
    on,
    pin: opts.pin ?? null,
    vaultPath: opts.vaultPath ?? null,
  });
}
