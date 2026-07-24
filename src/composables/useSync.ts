import { ref, watch, onUnmounted, type Ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { emit as tauriEmit, listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { SyncResult } from '../types/ipc';
import { useAppStore } from '../stores/useAppStore';
import { logger } from '../utils/logger';
import { usePlatform } from './usePlatform';

export type SyncAdapterId = 'local' | 'gdrive';
export type SyncTriggerReason = 'manual' | 'server_push' | 'periodic_timer' | 'app_foreground' | 'initial_connect' | 'watcher_create_delete' | 'watcher_modified' | 'queued_retry';

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


// --- Shared Singleton State ---
const isSyncing = ref(false);
const syncError = ref('');
const syncProgress = ref<SyncProgressEvent | null>(null);
const syncErrors = ref<string[]>([]);
const syncConflicts = ref<SyncConflictInfo[]>([]);
const quotaWarning = ref<QuotaInfo | null>(null);

let isInitialized = false;
let instanceCount = 0;

let autoSyncTimer: number | null = null;
let fgTimer: number | null = null;
let queuedTimer: number | null = null;
let unlistenFns: UnlistenFn[] = [];

let syncQueued = false;
let syncQueuedReason: SyncTriggerReason = 'queued_retry';

// References for global callbacks
let activeVaultPath: Ref<string> | null = null;
let activeVaultType: Ref<SyncAdapterId> | null = null;

async function setupEventListeners() {
  try {
    const unlistenPush = await listen('sync-server-push', () => {
      logger.info('[Sync] Received push notification. Triggering sync...');
      if (!isSyncing.value) {
        doSync('server_push');
      } else {
        syncQueued = true;
        syncQueuedReason = 'server_push';
      }
    });
    unlistenFns.push(unlistenPush);

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

function clearAllTimers() {
  if (autoSyncTimer !== null) {
    window.clearInterval(autoSyncTimer);
    autoSyncTimer = null;
  }
  if (fgTimer !== null) {
    window.clearTimeout(fgTimer);
    fgTimer = null;
  }
  if (queuedTimer !== null) {
    window.clearTimeout(queuedTimer);
    queuedTimer = null;
  }
}

function setupAutoSync() {
  if (autoSyncTimer !== null) {
    window.clearInterval(autoSyncTimer);
    autoSyncTimer = null;
  }

  const appStore = useAppStore();
  let enabled = false;
  let interval = 5;
  const vType = activeVaultType?.value;

  if (vType === 'gdrive') {
    enabled = appStore.gdriveAutoSyncEnabled;
    interval = Math.max(1, Math.min(60, appStore.gdriveAutoSyncInterval));
  } else {
    enabled = appStore.p2pAutoSyncEnabled;
    interval = Math.max(1, Math.min(60, appStore.p2pAutoSyncInterval));
  }

  if (enabled && document.visibilityState === 'visible') {
    autoSyncTimer = window.setInterval(() => {
      if (!isSyncing.value && document.visibilityState === 'visible') {
        doSync('periodic_timer');
      }
    }, interval * 60 * 1000);
  }
}

function onVisibilityChange() {
  if (document.visibilityState === 'visible') {
    setupAutoSync();
    if (fgTimer !== null) window.clearTimeout(fgTimer);
    fgTimer = window.setTimeout(() => {
      fgTimer = null;
      doSync('app_foreground');
    }, 1000);
  } else {
    if (autoSyncTimer !== null) {
      window.clearInterval(autoSyncTimer);
      autoSyncTimer = null;
    }
  }
}

async function doSync(triggerReason: SyncTriggerReason = 'manual') {
  if (isSyncing.value || !activeVaultPath?.value) return;

  const appStore = useAppStore();
  const vType = activeVaultType?.value;
  const vPath = activeVaultPath.value;

  // Check network policy using Tauri command if available, else fallback safely
  // (Ignoring navigator.connection which is unreliable on iOS Safari)
  let isCellular = false;
  try {
    // If backend implements it, otherwise default to false
    isCellular = await invoke<boolean>('is_cellular_connection').catch(() => false);
  } catch (e) {
    // Fallback
  }
  
  if (isCellular && appStore.p2pCellularPolicy === 'off') {
    logger.info('Skipping sync: Cellular data is restricted.');
    return;
  }

  isSyncing.value = true;
  syncError.value = '';
  
  try {
    let result: SyncResult;
    const tStart = Date.now();

    if (vType === 'gdrive') {
      result = await invoke<SyncResult>('gdrive_sync_full', { vaultPath: vPath });
      appStore.gdriveLastSyncTime = new Date().toLocaleTimeString();
    } else {
      result = await invoke<SyncResult>('sync_full', {
        vaultPath: vPath,
        isCellular,
        triggerReason,
      });
      appStore.p2pLastSyncTime = new Date().toLocaleTimeString();
    }
    
    logger.info(`[${vType}] Sync done in ${Date.now() - tStart}ms: pulled=${result.pulled} pushed=${result.pushed} deleted=${result.deleted}`);
    
    if (result.errors.length > 0) {
      syncError.value = `${result.errors.length} error(s)`;
      logger.warn('Sync errors:', result.errors);
    }
    
    if (result.pulled > 0) {
      await tauriEmit('vault-sync-completed', {
        pulled_files: result.pulled_files || [],
        pulled: result.pulled,
      });
    }
  } catch (e: any) {
    syncError.value = e?.toString() || 'Sync failed';
    logger.error(`[${vType}] Sync failed:`, e);
  } finally {
    isSyncing.value = false;
    if (syncQueued) {
      syncQueued = false;
      const queuedReason = syncQueuedReason;
      syncQueuedReason = 'queued_retry';
      if (queuedTimer !== null) window.clearTimeout(queuedTimer);
      queuedTimer = window.setTimeout(() => {
        queuedTimer = null;
        doSync(queuedReason);
      }, 1000);
    }
  }
}

export function useSync(vaultPath: Ref<string>, vaultType: Ref<SyncAdapterId>) {
  const appStore = useAppStore();
  const { } = usePlatform();

  activeVaultPath = vaultPath;
  activeVaultType = vaultType;

  if (!isInitialized) {
    isInitialized = true;
    setupEventListeners();
    document.addEventListener('visibilitychange', onVisibilityChange);
  }

  instanceCount++;

  // Watchers for settings change
  watch(() => [
    appStore.p2pAutoSyncEnabled,
    appStore.p2pAutoSyncInterval,
    appStore.gdriveAutoSyncEnabled,
    appStore.gdriveAutoSyncInterval,
    vaultType.value
  ], () => {
    setupAutoSync();
  });

  onUnmounted(() => {
    instanceCount--;
    if (instanceCount === 0) {
      isInitialized = false;
      document.removeEventListener('visibilitychange', onVisibilityChange);
      for (const unlisten of unlistenFns) {
        unlisten();
      }
      unlistenFns = [];
      clearAllTimers();
    }
  });

  return {
    isSyncing,
    syncError,
    syncProgress,
    syncErrors,
    syncConflicts,
    quotaWarning,
    sync: doSync,
    setupAutoSync,
  };
}
