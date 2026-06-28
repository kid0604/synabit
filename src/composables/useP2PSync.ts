import { ref, watch, computed, onUnmounted, type Ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { emit as tauriEmit, listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { SyncResult } from '../types/ipc';
import { useAppStore } from '../stores/useAppStore';
import { logger } from '../utils/logger';

// ─── Sync UX Event Types ──────────────────────────────────
export interface SyncProgressEvent {
  phase: 'pull' | 'push' | 'done' | 'error';
  current: number;
  total: number;
  current_file: string;
  errors: string[];
}

export interface SyncConflictInfo {
  merged_files: string[];
  total: number;
}

export interface QuotaInfo {
  used_bytes: number;
  total_bytes: number;
  message: string;
}

/**
 * Composable for P2P Sync via Synabit Sync Server.
 * Modeled after useGDrive — manages connection, sync, auto-sync timer.
 *
 * Uses Tauri commands:
 *   p2p_sync_connect, p2p_sync_full, p2p_sync_disconnect, p2p_sync_status
 */
export function useP2PSync(vaultPath: Ref<string>) {
  const appStore = useAppStore();

  // --- State ---
  const p2pConnected = ref(false);
  const p2pSyncing = ref(false);
  const p2pSyncError = ref('');
  const p2pConnecting = ref(false);

  // --- Sync UX State ---
  const syncProgress = ref<SyncProgressEvent | null>(null);
  const syncErrors = ref<string[]>([]);
  const syncConflicts = ref<SyncConflictInfo[]>([]);
  const quotaWarning = ref<QuotaInfo | null>(null);

  // --- Event Listeners ---
  const unlistenFns: UnlistenFn[] = [];

  async function setupEventListeners() {
    try {
      const unlistenProgress = await listen<SyncProgressEvent>('sync-progress', (event) => {
        syncProgress.value = event.payload;
        if (event.payload.errors && event.payload.errors.length > 0) {
          syncErrors.value = event.payload.errors;
        }
        if (event.payload.phase === 'done') {
          syncProgress.value = null;
        }
      });
      unlistenFns.push(unlistenProgress);

      const unlistenConflict = await listen<SyncConflictInfo>('sync-conflict', (event) => {
        syncConflicts.value.push(event.payload);
      });
      unlistenFns.push(unlistenConflict);

      const unlistenQuota = await listen<QuotaInfo>('sync-quota-exceeded', (event) => {
        quotaWarning.value = event.payload;
      });
      unlistenFns.push(unlistenQuota);
    } catch (e) {
      logger.error('Failed to setup sync event listeners:', e);
    }
  }

  // Initialize listeners immediately
  setupEventListeners();

  let autoSyncTimer: number | null = null;

  // --- Auto Sync ---
  function setupAutoSync() {
    if (autoSyncTimer !== null) {
      window.clearInterval(autoSyncTimer);
      autoSyncTimer = null;
    }
    // Only set up auto-sync if we are connected, auto-sync is enabled, and the app is visible
    if (appStore.p2pAutoSyncEnabled && p2pConnected.value && document.visibilityState === 'visible') {
      const mins = Math.max(1, Math.min(60, appStore.p2pAutoSyncInterval));
      autoSyncTimer = window.setInterval(() => {
        if (!p2pSyncing.value && p2pConnected.value && document.visibilityState === 'visible') {
          syncP2P();
        }
      }, mins * 60 * 1000);
    }
  }

  // --- Mobile Lifecycle (Background/Foreground) ---
  function onVisibilityChange() {
    if (document.visibilityState === 'visible') {
      logger.info('App foregrounded, resuming auto-sync timer');
      setupAutoSync();
      // Optionally trigger an immediate sync when returning to the app
      if (appStore.p2pAutoSyncEnabled && p2pConnected.value) {
        syncP2P();
      }
    } else {
      // logger.info('App backgrounded, pausing auto-sync timer to save battery'); // Removed to avoid WebKit IPC Fetch cancellation error
      if (autoSyncTimer !== null) {
        window.clearInterval(autoSyncTimer);
        autoSyncTimer = null;
      }
    }
  }

  document.addEventListener('visibilitychange', onVisibilityChange);

  watch(() => appStore.p2pAutoSyncEnabled, async (val) => {
    const store = appStore.getStoreInstance();
    if (store) {
      await store.set('p2pAutoSyncEnabled', val);
      await store.save();
    }
    setupAutoSync();
  });

  watch(() => appStore.p2pAutoSyncInterval, async (val) => {
    const safeVal = Math.max(1, Math.min(60, val || 5));
    if (safeVal !== val) {
      appStore.p2pAutoSyncInterval = safeVal;
      return;
    }
    const store = appStore.getStoreInstance();
    if (store) {
      await store.set('p2pAutoSyncInterval', safeVal);
      await store.save();
    }
    setupAutoSync();
  });

  // --- Connect ---
  async function connectP2P(serverAddr: string, serverIdHex: string) {
    p2pConnecting.value = true;
    p2pSyncError.value = '';
    try {
      await invoke<string>('p2p_sync_connect', {
        serverAddr,
        serverIdHex,
      });
      p2pConnected.value = true;

      // Persist config
      appStore.p2pServerAddr = serverAddr;
      appStore.p2pServerIdHex = serverIdHex;
      const store = appStore.getStoreInstance();
      if (store) {
        await store.set('p2pServerAddr', serverAddr);
        await store.set('p2pServerIdHex', serverIdHex);
        await store.save();
      }

      // Update worker config for Android Headless Sync
      if (vaultPath.value) {
        await invoke('p2p_sync_update_worker_config', {
          vaultPath: vaultPath.value,
          serverAddr,
          serverIdHex,
        }).catch(e => logger.warn('Failed to update worker config:', e));
      }

      // Initial sync after connect
      await syncP2P();
      setupAutoSync();
    } catch (e: any) {
      p2pSyncError.value = e?.toString() || 'Connection failed';
      logger.error('P2P connect failed:', e);
    } finally {
      p2pConnecting.value = false;
    }
  }

  // --- Disconnect ---
  async function disconnectP2P() {
    try {
      await invoke('p2p_sync_disconnect');
      p2pConnected.value = false;
      if (autoSyncTimer !== null) {
        window.clearInterval(autoSyncTimer);
        autoSyncTimer = null;
      }
    } catch (e) {
      logger.error('P2P disconnect failed:', e);
    }
  }

  // --- Sync ---
  async function syncP2P() {
    if (p2pSyncing.value || !vaultPath.value) return;

    // Determine network and apply policy
    // navigator.connection is non-standard but supported in Chrome/Android WebView
    const conn = (navigator as any).connection;
    const isCellular = conn ? conn.type === 'cellular' : false;
    
    if (isCellular && appStore.p2pCellularPolicy === 'off') {
      logger.info('Skipping P2P sync: Cellular data is restricted by policy.');
      return;
    }

    p2pSyncing.value = true;
    p2pSyncError.value = '';
    try {
      const result = await invoke<SyncResult>('p2p_sync_full', {
        vaultPath: vaultPath.value,
        isCellular: isCellular,
      });
      const now = new Date().toLocaleTimeString();
      appStore.p2pLastSyncTime = now;
      const store = appStore.getStoreInstance();
      if (store) {
        await store.set('p2pLastSyncTime', now);
        await store.save();
      }
      logger.info(
        `P2P Sync completed: pulled=${result.pulled} pushed=${result.pushed} deleted=${result.deleted} errors=${result.errors.length}`,
        result.pulled_files
      );
      if (result.errors.length > 0) {
        p2pSyncError.value = `${result.errors.length} error(s)`;
        logger.warn('P2P Sync errors:', result.errors);
      }
      // Emit event so all mini-apps can independently refresh their data
      if (result.pulled > 0) {
        await tauriEmit('vault-sync-completed', {
          pulled_files: result.pulled_files || [],
          pulled: result.pulled,
        });
      }
    } catch (e: any) {
      p2pSyncError.value = e?.toString() || 'Sync failed';
      logger.error('P2P Sync failed:', e);
    } finally {
      p2pSyncing.value = false;
    }
  }

  // --- Check status on init ---
  async function checkP2PStatus() {
    try {
      const status = await invoke<{ connected: boolean; server_addr: string; last_sync_time: string }>('p2p_sync_status');
      p2pConnected.value = status.connected;
    } catch {
      p2pConnected.value = false;
    }
  }

  // --- Auto-reconnect from persisted config ---
  async function autoReconnect() {
    const addr = appStore.p2pServerAddr;
    const id = appStore.p2pServerIdHex;
    if (addr && id) {
      try {
        await invoke<string>('p2p_sync_connect', {
          serverAddr: addr,
          serverIdHex: id,
        });
        p2pConnected.value = true;
        
        // Update worker config for Android Headless Sync
        if (vaultPath.value) {
          await invoke('p2p_sync_update_worker_config', {
            vaultPath: vaultPath.value,
            serverAddr: addr,
            serverIdHex: id,
          }).catch(e => logger.warn('Failed to update worker config:', e));
        }

        setupAutoSync();
        logger.info('P2P auto-reconnected to', addr);
      } catch (e) {
        logger.warn('P2P auto-reconnect failed:', e);
        p2pConnected.value = false;
      }
    }
  }

  // --- Cleanup event listeners ---
  onUnmounted(() => {
    document.removeEventListener('visibilitychange', onVisibilityChange);
    for (const unlisten of unlistenFns) {
      unlisten();
    }
  });

  return {
    // State
    p2pConnected,
    p2pSyncing,
    p2pSyncError,
    p2pConnecting,
    lastSyncTime: computed(() => appStore.p2pLastSyncTime),
    p2pAutoSyncEnabled: computed({
      get: () => appStore.p2pAutoSyncEnabled,
      set: (val) => (appStore.p2pAutoSyncEnabled = val),
    }),
    p2pAutoSyncInterval: computed({
      get: () => appStore.p2pAutoSyncInterval,
      set: (val) => (appStore.p2pAutoSyncInterval = val),
    }),
    // Sync UX State
    syncProgress,
    syncErrors,
    syncConflicts,
    quotaWarning,
    // Actions
    setupAutoSync,
    connectP2P,
    disconnectP2P,
    syncP2P,
    checkP2PStatus,
    autoReconnect,
  };
}
