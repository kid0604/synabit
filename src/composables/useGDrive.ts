import { ref, watch, computed, type Ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { SyncResult } from '../types/ipc';
import { useAppStore } from '../stores/useAppStore';
import { onOpenUrl } from '@tauri-apps/plugin-deep-link';
import { logger } from '../utils/logger';

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
  const appStore = useAppStore();

  // --- State ---
  const gdriveConnected = ref(false);
  const gdriveSyncing = ref(false);
  const gdriveSyncError = ref('');
  const gdriveAuthLoading = ref(false);

  let autoSyncTimer: number | null = null;

  // --- Auto Sync ---
  function setupAutoSync() {
    if (autoSyncTimer !== null) {
      window.clearInterval(autoSyncTimer);
      autoSyncTimer = null;
    }
    if (appStore.gdriveAutoSyncEnabled && vaultType.value === 'gdrive' && gdriveConnected.value) {
      const mins = Math.max(1, Math.min(60, appStore.gdriveAutoSyncInterval));
      autoSyncTimer = window.setInterval(() => {
        if (!gdriveSyncing.value && gdriveConnected.value && vaultType.value === 'gdrive') {
          syncGDrive();
        }
      }, mins * 60 * 1000);
    }
  }

  watch(() => appStore.gdriveAutoSyncEnabled, async (val) => {
    const store = appStore.getStoreInstance();
    if (store) {
      await store.set('gdriveAutoSyncEnabled', val);
      await store.save();
    }
    setupAutoSync();
  });

  watch(() => appStore.gdriveAutoSyncInterval, async (val) => {
    const safeVal = Math.max(1, Math.min(60, val || 1));
    if (safeVal !== val) {
      appStore.gdriveAutoSyncInterval = safeVal;
      return;
    }
    const store = appStore.getStoreInstance();
    if (store) {
      await store.set('gdriveAutoSyncInterval', safeVal);
      await store.save();
    }
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

  async function finishConnect() {
      gdriveConnected.value = true;
      try {
          const cachePath = await invoke<string>('gdrive_get_cache_path');
          await appStore.setVaultPath(cachePath, 'gdrive');
          await syncGDrive();
          scanVault();
          setupAutoSync();
      } catch (e: any) {
          gdriveSyncError.value = e?.toString() || 'Vault initialization failed';
      } finally {
          gdriveAuthLoading.value = false;
      }
  }

  // --- Global Deep Link Listener (For Android Cold Starts) ---
  onOpenUrl(async (urls) => {
      const url = urls[0] || '';
      if (url.includes('?code=') || url.includes('&code=')) {
          const codeMatch = url.match(/[?&]code=([^&]+)/);
          const stateMatch = url.match(/[?&]state=([^&]+)/);
          
          if (codeMatch && codeMatch[1]) {
              const code = decodeURIComponent(codeMatch[1]);
              const state = stateMatch ? decodeURIComponent(stateMatch[1]) : '';
              
              if (state === 'omnidrive') {
                  // Forward to OmniDrive (File Manager)
                  import('@tauri-apps/api/event').then(({ emit }) => {
                      emit('omnidrive-auth-code', { code });
                  });
              } else {
                  // Vault Sync flow
                  gdriveAuthLoading.value = true;
                  gdriveSyncError.value = '';
                  try {
                      await invoke('gdrive_auth_complete', { authCode: code });
                      await finishConnect();
                  } catch(err: any) {
                      gdriveSyncError.value = err?.toString() || 'OAuth Exchange failed';
                      gdriveAuthLoading.value = false;
                  }
              }
          }
      }
  }).catch(logger.error);

  async function connectGDrive() {
    gdriveAuthLoading.value = true;
    gdriveSyncError.value = '';
    try {
      const resp = await invoke<string>('gdrive_auth_start');
      if (resp === 'WAITING_DEEP_LINK') {
          // We wait for the global onOpenUrl listener to catch the redirect.
          // Don't set gdriveAuthLoading to false here.
      } else {
          // Loopback success on Desktop
          await finishConnect();
      }
    } catch (e: any) {
      gdriveSyncError.value = e?.toString() || 'Connection failed';
      gdriveAuthLoading.value = false;
    }
  }

  async function disconnectGDrive() {
    try {
      await invoke('gdrive_disconnect');
      gdriveConnected.value = false;
      // Clear vault handled by caller
    } catch (e) {
      logger.error('Disconnect failed:', e);
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
      appStore.gdriveLastSyncTime = now;
      const store = appStore.getStoreInstance();
      if (store) {
          await store.set('gdriveLastSyncTime', now);
          await store.save();
      }
      if (result.errors.length > 0) {
        gdriveSyncError.value = `${result.errors.length} error(s)`;
        logger.warn('Sync errors:', result.errors);
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
      logger.error('Sync failed:', e);
    } finally {
      gdriveSyncing.value = false;
    }
  }

  return {
    // State
    gdriveConnected,
    gdriveSyncing,
    gdriveSyncError,
    lastSyncTime: computed(() => appStore.gdriveLastSyncTime),
    gdriveAuthLoading,
    gdriveAutoSyncEnabled: computed({
      get: () => appStore.gdriveAutoSyncEnabled,
      set: (val) => appStore.gdriveAutoSyncEnabled = val
    }),
    gdriveAutoSyncInterval: computed({
      get: () => appStore.gdriveAutoSyncInterval,
      set: (val) => appStore.gdriveAutoSyncInterval = val
    }),
    // Actions
    setupAutoSync,
    checkGDriveAuth,
    connectGDrive,
    disconnectGDrive,
    syncGDrive,
  };
}
