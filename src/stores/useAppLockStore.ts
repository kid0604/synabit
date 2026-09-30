import { defineStore } from 'pinia';
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';

interface AppLockConfig {
  is_enabled: boolean;
  app_lock_active: boolean;
  protected_apps: string[];
  protected_notes: string[];
  auto_lock_timeout_secs: number;
}

interface VerifyResult {
  success: boolean;
  remaining_attempts: number;
  locked_until: number | null;
}

/** A forgotten-PIN reset: make a folder called `name` inside `folder`. */
export interface ResetChallenge {
  folder: string;
  name: string;
}

/**
 * The codes PIN-guarded commands answer with (`src-tauri/src/commands/app_lock.rs`),
 * and what each says to a person. The backend checks the PIN itself; these
 * only put its refusal into words.
 */
const PIN_ERROR_KEYS: Record<string, string> = {
  PIN_REQUIRED: 'shell.lock.pin_required',
  PIN_WRONG: 'shell.lock.wrong_pin_plain',
  PIN_LOCKED_OUT: 'shell.lock.too_many',
  PIN_ALREADY_SET: 'shell.lock.already_set',
  RESET_NOT_STARTED: 'shell.lock.reset_expired',
  RESET_EXPIRED: 'shell.lock.reset_expired',
  RESET_NOT_PROVEN: 'shell.lock.reset_not_found',
  RESET_NOT_ON_THIS_DEVICE: 'shell.lock.reset_not_here',
};

/**
 * Whether moving the auto-lock timeout from `current` to `next` seconds guards
 * less — the same rule as `timeout_loosens` in the backend. `0` is "never".
 */
export function timeoutLoosens(current: number, next: number): boolean {
  return current !== 0 && (next === 0 || next > current);
}

/** The i18n key for a backend PIN refusal, or `null` for any other error. */
export function pinErrorKey(e: unknown): string | null {
  const code = typeof e === 'string' ? e : e instanceof Error ? e.message : '';
  return PIN_ERROR_KEYS[code] ?? null;
}

export const useAppLockStore = defineStore('appLock', () => {
  // Config (from backend)
  const isEnabled = ref(false);       // PIN is set up (needed for any tier)
  const appLockActive = ref(false);   // Tier 1 toggle (lock whole app)
  const protectedApps = ref<string[]>([]);
  const protectedNotes = ref<string[]>([]);
  const autoLockTimeoutSecs = ref(300);

  // Runtime state
  const isAppLocked = ref(false);  // Tier 1
  // Tier 2/3: Map<id, lastAccessTimestamp> — session expires after idle timeout
  const unlockedApps = ref<Map<string, number>>(new Map());
  const unlockedNotes = ref<Map<string, number>>(new Map());
  const lastActivityTime = ref(Date.now()); // For Tier 1 global idle
  const isReady = ref(false);

  async function initialize() {
    try {
      const config = await invoke<AppLockConfig>('get_app_lock_config');
      isEnabled.value = config.is_enabled;
      appLockActive.value = config.app_lock_active;
      protectedApps.value = config.protected_apps;
      protectedNotes.value = config.protected_notes;
      autoLockTimeoutSecs.value = config.auto_lock_timeout_secs;

      // If Tier 1 is active, start locked
      if (isEnabled.value && appLockActive.value) {
        isAppLocked.value = true;
      }
    } catch (e) {
      console.error('Failed to load app lock config:', e);
    }
    isReady.value = true;
  }

  async function verifyPin(pin: string): Promise<VerifyResult> {
    const result = await invoke<VerifyResult>('verify_app_lock', { pin });
    return result;
  }

  /** Remove the PIN. The backend checks `pin` again; the screen asking is not the lock. */
  async function removeLock(pin: string) {
    await invoke('remove_app_lock', { pin });
    await refreshConfig();
  }

  /** Start a forgotten-PIN reset (desktop only; the backend refuses on a phone). */
  async function beginReset(): Promise<ResetChallenge> {
    return invoke<ResetChallenge>('app_lock_reset_begin');
  }

  /**
   * Finish it, once the folder is made. With the PIN gone every guard it held
   * goes too, so nothing stays locked.
   */
  async function finishReset() {
    await invoke('app_lock_reset_finish');
    await refreshConfig();
    unlockedApps.value.clear();
    unlockedNotes.value.clear();
    isAppLocked.value = false;
  }

  function lock() {
    if (!isEnabled.value) return;
    // Always clear session caches (Tier 2 & 3 re-lock)
    unlockedApps.value.clear();
    unlockedNotes.value.clear();
    // Only lock entire app if Tier 1 is active
    if (appLockActive.value) {
      isAppLocked.value = true;
    }
  }

  function unlockApp() {
    isAppLocked.value = false;
    resetActivity();
  }

  function unlockMiniApp(appId: string) {
    unlockedApps.value.set(appId, Date.now());
  }

  function unlockNote(noteId: string) {
    unlockedNotes.value.set(noteId, Date.now());
  }

  // Refresh session timer — call while user is actively on the resource
  function touchMiniAppSession(appId: string) {
    if (unlockedApps.value.has(appId)) {
      unlockedApps.value.set(appId, Date.now());
    }
  }

  function touchNoteSession(noteId: string) {
    if (unlockedNotes.value.has(noteId)) {
      unlockedNotes.value.set(noteId, Date.now());
    }
  }

  function isAppProtected(appId: string): boolean {
    return protectedApps.value.includes(appId);
  }

  function isMiniAppAccessible(appId: string): boolean {
    if (!isAppProtected(appId)) return true;
    const unlockedAt = unlockedApps.value.get(appId);
    if (unlockedAt === undefined) return false;
    // Check session expiry
    if (autoLockTimeoutSecs.value > 0) {
      const elapsed = (Date.now() - unlockedAt) / 1000;
      if (elapsed >= autoLockTimeoutSecs.value) {
        unlockedApps.value.delete(appId);
        return false;
      }
    }
    return true;
  }

  function isNoteProtected(noteId: string): boolean {
    return protectedNotes.value.includes(noteId);
  }

  function isNoteAccessible(noteId: string): boolean {
    if (!isNoteProtected(noteId)) return true;
    const unlockedAt = unlockedNotes.value.get(noteId);
    if (unlockedAt === undefined) return false;
    // Check session expiry
    if (autoLockTimeoutSecs.value > 0) {
      const elapsed = (Date.now() - unlockedAt) / 1000;
      if (elapsed >= autoLockTimeoutSecs.value) {
        unlockedNotes.value.delete(noteId);
        return false;
      }
    }
    return true;
  }

  function resetActivity() {
    lastActivityTime.value = Date.now();
  }

  // Changes to what the PIN guards. Each saves first and changes the store
  // only once the backend agrees, so a refusal leaves the screen as it was.
  // Loosening (unprotecting, switching the whole-app lock off, a longer or no
  // timeout) needs `pin` when one is set: `update_app_lock_config` checks it.

  async function toggleProtectedApp(appId: string, pin?: string) {
    const next = protectedApps.value.includes(appId)
      ? protectedApps.value.filter(id => id !== appId)
      : [...protectedApps.value, appId];
    await invoke('update_app_lock_config', { config: { protected_apps: next }, pin: pin ?? null });
    protectedApps.value = next;
  }

  async function toggleProtectedNote(noteId: string, pin?: string) {
    const removing = protectedNotes.value.includes(noteId);
    const next = removing
      ? protectedNotes.value.filter(id => id !== noteId)
      : [...protectedNotes.value, noteId];
    await invoke('update_app_lock_config', { config: { protected_notes: next }, pin: pin ?? null });
    protectedNotes.value = next;
    if (removing) unlockedNotes.value.delete(noteId);
  }

  async function setAutoLockTimeout(secs: number, pin?: string) {
    await invoke('update_app_lock_config', { config: { auto_lock_timeout_secs: secs }, pin: pin ?? null });
    autoLockTimeoutSecs.value = secs;
  }

  async function setAppLockActive(active: boolean, pin?: string) {
    await invoke('update_app_lock_config', { config: { app_lock_active: active }, pin: pin ?? null });
    appLockActive.value = active;
    // If turning off Tier 1, unlock app immediately
    if (!active && isAppLocked.value) {
      isAppLocked.value = false;
    }
  }

  async function refreshConfig() {
    try {
      const config = await invoke<AppLockConfig>('get_app_lock_config');
      isEnabled.value = config.is_enabled;
      appLockActive.value = config.app_lock_active;
      protectedApps.value = config.protected_apps;
      protectedNotes.value = config.protected_notes;
      autoLockTimeoutSecs.value = config.auto_lock_timeout_secs;
    } catch (e) {
      console.error('Failed to refresh app lock config:', e);
    }
  }

  return {
    isEnabled,
    appLockActive,
    protectedApps,
    protectedNotes,
    autoLockTimeoutSecs,
    isAppLocked,
    unlockedApps,
    unlockedNotes,
    lastActivityTime,
    isReady,
    initialize,
    verifyPin,
    removeLock,
    beginReset,
    finishReset,
    lock,
    unlockApp,
    unlockMiniApp,
    unlockNote,
    touchMiniAppSession,
    touchNoteSession,
    isAppProtected,
    isMiniAppAccessible,
    isNoteProtected,
    isNoteAccessible,
    resetActivity,
    toggleProtectedApp,
    toggleProtectedNote,
    setAutoLockTimeout,
    setAppLockActive,
    refreshConfig,
  };
});
