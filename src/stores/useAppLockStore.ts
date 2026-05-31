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

  async function toggleProtectedApp(appId: string) {
    const idx = protectedApps.value.indexOf(appId);
    if (idx >= 0) {
      protectedApps.value.splice(idx, 1);
    } else {
      protectedApps.value.push(appId);
    }
    await invoke('update_app_lock_config', {
      config: { protected_apps: protectedApps.value }
    });
  }

  async function toggleProtectedNote(noteId: string) {
    const idx = protectedNotes.value.indexOf(noteId);
    if (idx >= 0) {
      protectedNotes.value.splice(idx, 1);
      unlockedNotes.value.delete(noteId);
    } else {
      protectedNotes.value.push(noteId);
    }
    await invoke('update_app_lock_config', {
      config: { protected_notes: protectedNotes.value }
    });
  }

  async function setAutoLockTimeout(secs: number) {
    autoLockTimeoutSecs.value = secs;
    await invoke('update_app_lock_config', {
      config: { auto_lock_timeout_secs: secs }
    });
  }

  async function setAppLockActive(active: boolean) {
    appLockActive.value = active;
    await invoke('update_app_lock_config', {
      config: { app_lock_active: active }
    });
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
