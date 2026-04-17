import { ref, watch, type Ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { SyncResult } from '../types/ipc';

/**
 * Composable for Google Drive sync state and operations.
 * Accepts external dependencies as refs to avoid circular coupling.
 */
export function useGDrive(
  vaultPath: Ref<string>,
  vaultType: Ref<'local' | 'gdrive'>,
  scanVault: () => Promise<void>,
  tabContents: Ref<Record<string, string>>,
  loadNoteFile: (id: string) => Promise<void>,
  currentNoteId: Ref<string | null>,
) {
  // --- State ---
  const gdriveConnected = ref(false);
  const gdriveSyncing = ref(false);
  const gdriveSyncError = ref('');
  const lastSyncTime = ref(localStorage.getItem('synabitLastSyncTime') || '');
  const gdriveAuthLoading = ref(false);

  const gdriveAutoSyncEnabled = ref(localStorage.getItem('synabitGDriveAutoSync') === 'true');
  const gdriveAutoSyncInterval = ref(Number(localStorage.getItem('synabitGDriveInterval') || '15'));
  let autoSyncTimer: number | null = null;

  // --- Auto Sync ---
  function setupAutoSync() {
    if (autoSyncTimer !== null) {
      window.clearInterval(autoSyncTimer);
      autoSyncTimer = null;
    }
    if (gdriveAutoSyncEnabled.value && vaultType.value === 'gdrive' && gdriveConnected.value) {
      const mins = Math.max(1, Math.min(60, gdriveAutoSyncInterval.value));
      autoSyncTimer = window.setInterval(() => {
        if (!gdriveSyncing.value && gdriveConnected.value && vaultType.value === 'gdrive') {
          syncGDrive();
        }
      }, mins * 60 * 1000);
    }
  }

  watch(gdriveAutoSyncEnabled, (val) => {
    localStorage.setItem('synabitGDriveAutoSync', String(val));
    setupAutoSync();
  });

  watch(gdriveAutoSyncInterval, (val) => {
    const safeVal = Math.max(1, Math.min(60, val || 1));
    if (safeVal !== val) {
      gdriveAutoSyncInterval.value = safeVal;
      return;
    }
    localStorage.setItem('synabitGDriveInterval', String(safeVal));
    setupAutoSync();
  });

  // --- Auth ---
  async function checkGDriveAuth() {
    try {
      gdriveConnected.value = await invoke<boolean>('gdrive_auth_status');
    } catch {
      gdriveConnected.value = false;
    }
  }

  async function connectGDrive() {
    gdriveAuthLoading.value = true;
    gdriveSyncError.value = '';
    try {
      await invoke<string>('gdrive_auth_start');
      gdriveConnected.value = true;
      const cachePath = await invoke<string>('gdrive_get_cache_path');
      vaultPath.value = cachePath;
      vaultType.value = 'gdrive';
      localStorage.setItem('synabitVaultPath', cachePath);
      localStorage.setItem('synabitVaultType', 'gdrive');
      await syncGDrive();
      scanVault();
      setupAutoSync();
    } catch (e: any) {
      gdriveSyncError.value = e?.toString() || 'Connection failed';
    } finally {
      gdriveAuthLoading.value = false;
    }
  }

  async function disconnectGDrive() {
    try {
      await invoke('gdrive_disconnect');
      gdriveConnected.value = false;
      // Clear vault handled by caller
    } catch (e) {
      console.error('Disconnect failed:', e);
    }
  }

  // --- Sync ---
  async function syncGDrive() {
    if (gdriveSyncing.value || !vaultPath.value) return;
    gdriveSyncing.value = true;
    gdriveSyncError.value = '';
    try {
      const result = await invoke<SyncResult>('gdrive_sync_full', {
        vaultPath: vaultPath.value,
      });
      const now = new Date().toLocaleTimeString();
      lastSyncTime.value = now;
      localStorage.setItem('synabitLastSyncTime', now);
      if (result.errors.length > 0) {
        gdriveSyncError.value = `${result.errors.length} error(s)`;
        console.warn('Sync errors:', result.errors);
      }
      // Re-scan vault after sync to pick up pulled changes
      if (result.pulled > 0) {
        if (result.pulled_files) {
          result.pulled_files.forEach((p) => {
            delete tabContents.value[p];
          });
        }
        await scanVault();
        if (
          currentNoteId.value &&
          result.pulled_files &&
          result.pulled_files.includes(currentNoteId.value)
        ) {
          await loadNoteFile(currentNoteId.value);
        }
      }
    } catch (e: any) {
      gdriveSyncError.value = e?.toString() || 'Sync failed';
      console.error('Sync failed:', e);
    } finally {
      gdriveSyncing.value = false;
    }
  }

  return {
    // State
    gdriveConnected,
    gdriveSyncing,
    gdriveSyncError,
    lastSyncTime,
    gdriveAuthLoading,
    gdriveAutoSyncEnabled,
    gdriveAutoSyncInterval,
    // Actions
    setupAutoSync,
    checkGDriveAuth,
    connectGDrive,
    disconnectGDrive,
    syncGDrive,
  };
}
