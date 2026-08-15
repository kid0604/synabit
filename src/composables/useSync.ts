import { ref, watch, onUnmounted, computed, type Ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { emit as tauriEmit, listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { SyncResult } from '../types/ipc';
import { useAppStore } from '../stores/useAppStore';
import { logger } from '../utils/logger';
import { usePlatform } from './usePlatform';

export type SyncAdapterId = 'none' | 'local' | 'gdrive' | 'server';
export type SyncTriggerReason = 'manual' | 'server_push' | 'periodic_timer' | 'app_foreground' | 'initial_connect' | 'watcher_create_delete' | 'watcher_modified' | 'queued_retry';

export type SyncPhase = 'checking' | 'pulling' | 'applying' | 'pushing' | 'assets' | 'complete' | 'error';
export type SyncStatus = 'idle' | 'checking' | 'pulling' | 'applying' | 'pushing' | 'waiting_for_assets' | 'partial' | 'success' | 'offline' | 'error' | 'upgrade_required';

export interface SyncProgressEvent {
  runId: string;
  vaultId: string;
  provider: string;
  phase: SyncPhase;
  completedItems: number;
  totalItems?: number;
  bytesTransferred: number;
  totalBytes?: number;
  currentFile?: string;
}

export interface SyncConflictInfo {
  fileName: string;
  resolution: string;
}

export interface QuotaInfo {
  currentBytes: number;
  limitBytes: number;
}


// --- Shared Singleton State ---
const syncStatus = ref<SyncStatus>('idle');
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

let syncAgain = false;
let syncQueuedReason: SyncTriggerReason = 'queued_retry';

// References for global callbacks
let activeVaultPath: Ref<string> | null = null;
let activeVaultType: Ref<SyncAdapterId> | null = null;

async function setupEventListeners() {
  try {
    const unlistenPush = await listen('sync-server-push', () => {
      logger.info('[Sync] Received push notification. Triggering sync...');
      if (syncStatus.value === 'idle' || syncStatus.value === 'success' || syncStatus.value === 'partial' || syncStatus.value === 'error') {
        doSync('server_push');
      } else {
        syncAgain = true;
        syncQueuedReason = 'server_push';
      }
    });
    unlistenFns.push(unlistenPush);

    const unlistenProgress = await listen<SyncProgressEvent>('sync-progress', (event) => {
      syncProgress.value = event.payload;
      
      // Map progress phase to UI syncStatus
      const phase = event.payload.phase;
      if (phase === 'checking') syncStatus.value = 'checking';
      else if (phase === 'pulling') syncStatus.value = 'pulling';
      else if (phase === 'applying') syncStatus.value = 'applying';
      else if (phase === 'pushing') syncStatus.value = 'pushing';
      else if (phase === 'assets') syncStatus.value = 'waiting_for_assets';
      
      if (phase === 'error' && event.payload.currentFile) {
        syncErrors.value = [event.payload.currentFile];
      }
      
      if (phase === 'complete' || phase === 'error') {
        setTimeout(() => {
          if (syncProgress.value?.runId === event.payload.runId) {
            syncProgress.value = null;
          }
        }, 2000);
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
  const enabled = appStore.syncAutoEnabled;
  const interval = Math.max(1, Math.min(60, appStore.syncAutoInterval));

  if (enabled && document.visibilityState === 'visible') {
    autoSyncTimer = window.setInterval(() => {
      const isIdle = syncStatus.value === 'idle' || syncStatus.value === 'success' || syncStatus.value === 'partial' || syncStatus.value === 'error';
      if (isIdle && document.visibilityState === 'visible') {
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

let currentSyncAbortController: AbortController | null = null;

async function doSync(triggerReason: SyncTriggerReason = 'manual') {
  const isSyncing = !['idle', 'success', 'partial', 'error', 'offline'].includes(syncStatus.value);
  if (isSyncing || !activeVaultPath?.value) return;

  const appStore = useAppStore();
  const vType = activeVaultType?.value;
  const vPath = activeVaultPath.value;

  // KNOWN GAP (S3-10): there is no connection-type detection on any platform,
  // so this is always false. That means the "don't sync on cellular" setting
  // never takes effect and cellular transfer is always recorded as wifi.
  // Previously this called an `is_cellular_connection` command that was never
  // registered, so the failure was invisible.
  const isCellular = false;

  if (isCellular && appStore.syncCellularPolicy === 'off') {
    logger.info('Skipping sync: Cellular data is restricted.');
    syncStatus.value = 'offline';
    return;
  }

  syncStatus.value = 'checking';
  syncError.value = '';
  appStore.syncLastAttempted = new Date().toISOString();
  
  try {
    const tStart = Date.now();

    if (vType === 'none' || vType === 'local') {
      logger.info('Skipping sync: No cloud provider configured.');
      syncStatus.value = 'idle';
      return;
    }

    currentSyncAbortController = new AbortController();
    const abortSignal = currentSyncAbortController.signal;

    // Timeout logic: reject if takes longer than 60 seconds
    const timeoutPromise = new Promise<never>((_, reject) => {
      setTimeout(() => {
        reject(new Error('Sync operation timed out'));
      }, 60000);
    });

    // Each provider has its own registered command; there is no single
    // dispatching command on the Rust side.
    const syncPromise =
      vType === 'gdrive'
        ? invoke<SyncResult>('gdrive_sync_full', { vaultPath: vPath })
        : invoke<SyncResult>('sync_full', {
            vaultPath: vPath,
            isCellular,
            triggerReason,
          });

    const result = await Promise.race([syncPromise, timeoutPromise]);
    
    if (abortSignal.aborted) {
       throw new Error("Cancelled by user");
    }
    
    logger.info(`[${vType}] Sync done in ${Date.now() - tStart}ms: pulled=${result.pulled} pushed=${result.pushed} deleted=${result.deleted} tx=${result.tx_bytes}B rx=${result.rx_bytes}B`);
    
    let isPartial = false;

    if (result.errors.length > 0) {
      syncError.value = `${result.errors.length} error(s)`;
      logger.warn('Sync errors:', result.errors);
      isPartial = true;
    }
    
    if (isPartial) {
       syncStatus.value = 'partial';
    } else {
       syncStatus.value = 'success';
       appStore.syncLastSuccessful = new Date().toISOString();
    }
    
    if (result.pulled > 0) {
      await tauriEmit('vault-sync-completed', {
        pulled_files: result.pulled_files || [],
        pulled: result.pulled,
      });
    }
  } catch (e: any) {
    if (e?.toString().includes('offline') || e?.toString().includes('network')) {
       syncStatus.value = 'offline';
    } else {
       syncStatus.value = 'error';
    }
    syncError.value = e?.toString() || 'Sync failed';
    logger.error(`[${vType}] Sync failed:`, e);
  } finally {
    currentSyncAbortController = null;
    
    if (syncAgain) {
      syncAgain = false;
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

function cancelSync() {
    if (currentSyncAbortController) {
       currentSyncAbortController.abort();
    }
    syncStatus.value = 'error';
    syncError.value = 'Sync cancelled by user';
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
    appStore.syncAutoEnabled,
    appStore.syncAutoInterval,
    appStore.activeSyncProvider,
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

  const isSyncing = computed(() => ['checking', 'pulling', 'applying', 'pushing', 'waiting_for_assets'].includes(syncStatus.value));

  return {
    isSyncing,
    syncStatus,
    syncError,
    syncProgress,
    syncErrors,
    syncConflicts,
    quotaWarning,
    sync: doSync,
    cancelSync,
    setupAutoSync,
  };
}
