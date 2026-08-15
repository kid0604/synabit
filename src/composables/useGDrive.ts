import { ref, type Ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
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
  vaultType: Ref<'local' | 'gdrive' | 'server' | 'none'>,
) {
  const appStore = useAppStore();

  // --- State ---
  const gdriveConnected = ref(false);
  const gdriveSyncing = ref(false);
  const gdriveSyncError = ref('');
  const gdriveAuthLoading = ref(false);



  // --- Auth ---
  async function checkGDriveAuth() {
    try {
      gdriveConnected.value = await invoke<boolean>('gdrive_auth_status');
    } catch {
      gdriveConnected.value = false;
    }
  }

  async function finishConnect() {
      try {
          gdriveSyncError.value = 'Validating connection and checking health...';
          
          const isAuthed = await invoke<boolean>('gdrive_auth_status');
          if (!isAuthed) {
              throw new Error("Auth token is missing or invalid");
          }
          
          gdriveSyncError.value = 'Syncing directly to active local vault...';
          await invoke<SyncResult>('gdrive_sync_full', {
            vaultPath: vaultPath.value,
          });
          
          gdriveConnected.value = true;
          appStore.activeSyncProvider = 'gdrive';
          gdriveSyncError.value = ''; // Success!
      } catch (e: any) {
          gdriveSyncError.value = 'Error in finishConnect: ' + (e?.toString() || 'Vault initialization failed');
          gdriveConnected.value = false;
          // UI Transaction rollback: Don't change activeSyncProvider
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
                      await invoke('connect_gdrive_complete', {
                        authCode: code,
                        vaultPath: vaultPath.value,
                      });
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
    
    // UI Transaction logic: Stop current provider (if applicable).
    // In Tauri backend we don't have an explicit cancel yet, but we will
    // change the active state so the UI blocks standard syncs.
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
      if (appStore.activeSyncProvider === 'gdrive') {
          appStore.activeSyncProvider = 'none';
      }
      // Clear vault handled by caller
    } catch (e) {
      logger.error('Disconnect failed:', e);
    }
  }



  return {
    // State
    gdriveConnected,
    gdriveSyncing,
    gdriveSyncError,
    gdriveAuthLoading,
    // Actions
    checkGDriveAuth,
    connectGDrive,
    disconnectGDrive,
  };
}
