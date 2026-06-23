import { ref, watch, computed, type Ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { emit as tauriEmit } from '@tauri-apps/api/event';
import type { SyncResult } from '../types/ipc';
import { useAppStore } from '../stores/useAppStore';
import { onOpenUrl, getCurrent } from '@tauri-apps/plugin-deep-link';
import { logger } from '../utils/logger';

/**
 * Composable for Google Drive sync state and operations.
 * Decoupled from specific mini-apps — emits 'vault-sync-completed' event
 * so each app can independently handle post-sync data refresh.
 */
export function useGDrive(
  vaultPath: Ref<string>,
  vaultType: Ref<'local' | 'gdrive'>,
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
          gdriveSyncError.value = 'Step 3: Getting cache path...';
          const cachePath = await invoke<string>('gdrive_get_cache_path');
          gdriveSyncError.value = `Step 4: Setting vault path: ${cachePath}`;
          await appStore.setVaultPath(cachePath, 'gdrive');
          gdriveSyncError.value = 'Step 5: Calling syncGDrive()...';
          await syncGDrive();
          setupAutoSync();
          gdriveSyncError.value = ''; // Success!
      } catch (e: any) {
          gdriveSyncError.value = 'Error in finishConnect: ' + (e?.toString() || 'Vault initialization failed');
      } finally {
          gdriveAuthLoading.value = false;
      }
  }

  // --- Global Deep Link Listener (For Android Cold Starts) ---
  async function handleDeepLink(url: string) {
      if (!url) return;
      logger.info(`Received deep link: ${url}`);
      // DEBUG: surface the deep link visually to verify intent reception
      gdriveSyncError.value = `Intent captured: ${url}`;
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
              } else if (state === 'omni_browse') {
                  // File manager browse flow
                  gdriveAuthLoading.value = true;
                  gdriveSyncError.value = 'OmniBrowse Exchange started...';
                  try {
                      await invoke('gdrive_browse_auth_complete', { authCode: code });
                      gdriveConnected.value = true;
                      window.dispatchEvent(new CustomEvent('gdrive-browse-connected'));
                  } catch(err: any) {
                      gdriveSyncError.value = err?.toString() || 'OmniDrive OAuth failed';
                  } finally {
                      gdriveAuthLoading.value = false;
                  }
              } else {
                  // Vault Sync flow
                  gdriveAuthLoading.value = true;
                  gdriveSyncError.value = 'Step 1: Exchanging Token...';
                  try {
                      await invoke('gdrive_auth_complete', { authCode: code });
                      gdriveSyncError.value = 'Step 2: Token exchanged! Finishing connect...';
                      await finishConnect();
                  } catch(err: any) {
                      gdriveSyncError.value = 'Error: ' + (err?.toString() || 'OAuth Exchange failed');
                      gdriveAuthLoading.value = false;
                  }
              }
          }
      } else {
          gdriveSyncError.value = `Intent captured but NO CODE: ${url}`;
      }
  }

  onOpenUrl(async (urls) => {
      const url = urls[0] || '';
      await handleDeepLink(url);
  });

  // Check initial deep link in case app was cold-started from browser redirect
  getCurrent().then((urls) => {
      if (urls && urls.length > 0) {
          handleDeepLink(urls[0] || '');
      }
  }).catch(e => {
      logger.error('Failed to get current deep link: ' + e?.toString());
  });

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
      logger.info(`Sync completed: pulled=${result.pulled} pushed=${result.pushed} deleted=${result.deleted} errors=${result.errors.length}`, result.pulled_files);
      if (result.errors.length > 0) {
        gdriveSyncError.value = `${result.errors.length} error(s)`;
        logger.warn('Sync errors:', result.errors);
      }
      // Emit event so all mini-apps can independently refresh their data
      if (result.pulled > 0) {
        await tauriEmit('vault-sync-completed', {
          pulled_files: result.pulled_files || [],
          pulled: result.pulled,
        });
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
