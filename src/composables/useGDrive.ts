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
      // Path only, never the query. This URL is the OAuth redirect, so its
      // query string carries the authorization code — a live credential until
      // it is exchanged. It used to be logged whole, and also written to
      // `gdriveSyncError`, which App.vue renders on screen in red: connecting
      // Drive on a phone printed the authorization code to the display.
      logger.info(`Received deep link: ${url.split('?')[0]}`);

      if (url.includes('?code=') || url.includes('&code=')) {
          const codeMatch = url.match(/[?&]code=([^&]+)/);

          if (codeMatch && codeMatch[1]) {
              const code = decodeURIComponent(codeMatch[1]);

              // No branch on `state` any more. The `omnidrive` and
              // `omni_browse` states belonged to the Files app's Drive browser,
              // which is withdrawn: it wrote what it fetched into the legacy
              // `files` table while the list on screen reads the `nodes` table,
              // so connecting only ever produced an empty folder. Vault sync —
              // everything below — is a separate feature on a separate command
              // set and is unaffected.
              gdriveAuthLoading.value = true;
              try {
                  await invoke('gdrive_auth_complete', { authCode: code });
                  await finishConnect();
              } catch(err: any) {
                  gdriveSyncError.value = 'Error: ' + (err?.toString() || 'OAuth Exchange failed');
                  gdriveAuthLoading.value = false;
              }
          }
      } else {
          gdriveSyncError.value = 'That sign-in did not return an authorization code. Please try connecting again.';
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
