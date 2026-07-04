<script setup lang="ts">
import { ref, computed, provide, onMounted, onUnmounted, watch } from 'vue';
import { FileText, FolderOpen, Calendar, CheckSquare, Zap, Globe, Cloud, RefreshCw, CloudOff, Settings, Users, Wallet, MessageSquare, MessageCircle, Palette, MoreHorizontal, Rss, Server, Shield, TerminalSquare } from 'lucide-vue-next';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { initEventBus, destroyEventBus, useEventBus } from './composables/useEventBus';
import { useNodeService } from './composables/useNodeService';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { open } from '@tauri-apps/plugin-dialog';
import { documentDir } from '@tauri-apps/api/path';

import { defineAsyncComponent } from 'vue';
import { useRouter, useRoute } from 'vue-router';

// Settings Modal is the only async component kept here
const SettingsModal = defineAsyncComponent(() => import('./shared/components/SettingsModal.vue'));
const E2eeOnboarding = defineAsyncComponent(() => import('./shared/components/E2eeOnboarding.vue'));
const LockScreen = defineAsyncComponent(() => import('./shared/components/LockScreen.vue'));
const SetupPinModal = defineAsyncComponent(() => import('./shared/components/SetupPinModal.vue'));
const SyncConflictToast = defineAsyncComponent(() => import('./shared/components/SyncConflictToast.vue'));
const GDriveMigrationModal = defineAsyncComponent(() => import('./shared/components/GDriveMigrationModal.vue'));

// Composables
import { useSettings } from './composables/useSettings';
import { useGDrive } from './composables/useGDrive';
import { useP2PSync } from './composables/useP2PSync';
import { useAppLock } from './composables/useAppLock';
import { usePlatform } from './composables/usePlatform';
import { useAppUpdate } from './composables/useAppUpdate';

import SynIcon from './shared/icons/SynIcon.vue';

import DesktopLayout from './layouts/DesktopLayout.vue';
import MobileLayout from './layouts/MobileLayout.vue';

// Stores
import { useAppStore } from './stores/useAppStore';
import { useNavigationStore, type NavEntry } from './stores/useNavigationStore';
import { useAppLockStore } from './stores/useAppLockStore';
import { useLicenseStore } from './stores/useLicenseStore';
import { storeToRefs } from 'pinia';

const LicenseModal = defineAsyncComponent(() => import('./shared/components/LicenseModal.vue'));
const showLicenseModal = ref(false);

const licenseStore = useLicenseStore();

const bus = useEventBus();
const ns = useNodeService();

// ─── Auto-Update ──────────────────────────────────────────
const {
  updateAvailable, updateVersion, updateNotes,
  isDownloading: updateDownloading,
  downloadProgress: updateProgress,
  downloadAndInstall, dismissUpdate,
} = useAppUpdate();

// ─── Settings ─────────────────────────────────────────────
const {
  showSettingsModal, openSettings, initSettings, applyTheme, defaultApp, hiddenSidebarApps, showE2eeOnboarding
} = useSettings();

const ALL_APPS = [
  { id: 'nexus', name: 'Nexus', icon: Globe },
  { id: 'messages', name: 'Messages', icon: MessageCircle },
  { id: 'quickcap', name: 'QuickCap', icon: Zap },
  { id: 'note', name: 'Notes', icon: FileText },
  { id: 'task', name: 'Tasks', icon: CheckSquare },
  { id: 'calendar', name: 'Calendar', icon: Calendar },
  { id: 'file', name: 'Files', icon: FolderOpen },
  { id: 'whiteboard', name: 'Whiteboard', icon: Palette },
  { id: 'people', name: 'People', icon: Users },
  { id: 'finance', name: 'Finance', icon: Wallet },
  { id: 'feeds', name: 'Feeds', icon: Rss },
];

const getAppName = (appId: string): string => {
  return ALL_APPS.find(a => a.id === appId)?.name || appId;
};

const mobileVisibleApps = computed(() => {
    return ALL_APPS
        .filter(a => !hiddenSidebarApps.value.includes(a.id))
        .slice(0, 4)
        .map(a => a.id);
});

const isAppVisible = (appId: string) => {
    if (hiddenSidebarApps.value.includes(appId)) return false;
    if (useMobileLayout.value && !mobileVisibleApps.value.includes(appId)) return false;
    return true;
};

const moreMenuApps = computed(() => {
    return ALL_APPS.filter(a => {
        const isUserHidden = hiddenSidebarApps.value.includes(a.id);
        const isMobileHidden = useMobileLayout.value && !mobileVisibleApps.value.includes(a.id);
        return isUserHidden || isMobileHidden;
    });
});

// ─── App Lock ─────────────────────────────────────────────
const appLockStore = useAppLockStore();
const currentAppIdRef = computed(() => (route.name as string) || null);
useAppLock(currentAppIdRef); // Activity monitoring + session refresh
const showSetupPinModal = ref(false);
const setupPinMode = ref<'setup' | 'change'>('setup');

const showHiddenAppsMenu = ref(false);

const appStore = useAppStore();
const { vaultPath, vaultType } = storeToRefs(appStore);

const { useMobileLayout, isMobileOS } = usePlatform();

// ─── App View State (Vue Router) ──────────────────────────
const router = useRouter();
const route = useRoute();

const activeTool = computed({
  get: () => (route.name as string) || 'nexus',
  set: (val: string) => { 
      if (route.name !== val) {
          router.push({ name: val }).catch(err => {
              logger.warn('Router navigation error:', err);
          });
      }
  }
});

// ─── Navigation History (Back/Forward) — declared early so watcher can use them ─────
const navStore = useNavigationStore();
let isRestoringNav = false;

const getItemIdForApp = (app: string): string | undefined => {
    switch (app) {
        case 'note': return noteAppRef.value?.currentNoteId || undefined;
        case 'whiteboard': return whiteboardAppRef.value?.currentBoardId || undefined;
        case 'file': return filesAppRef.value?.activeTabId || undefined;
        default: return undefined;
    }
};

const getCurrentItemId = (): string | undefined => getItemIdForApp(activeTool.value);

const getCurrentScrollTop = (): number => {
    const el = document.querySelector('[data-app-scroll]') as HTMLElement;
    return el?.scrollTop || 0;
};

watch(activeTool, async (newTool, oldTool) => {
  if (oldTool !== newTool) {
    logger.debug(`Navigated to mini-app: ${newTool} (from ${oldTool})`);
    // Push old location onto the back stack (unless we're restoring from nav history)
    if (!isRestoringNav && oldTool) {
      navStore.pushNavigation({
        app: oldTool,
        itemId: getItemIdForApp(oldTool),
        scrollTop: getCurrentScrollTop(),
      });
    }
  }
  
  if (newTool === 'messages' && vaultPath.value) {
     if (messagesAppRef.value) {
         messagesAppRef.value.fetchNotifications();
     }
     
     if (unreadNotificationCount.value > 0) {
         unreadNotificationCount.value = 0;
         try {
             await invoke('mark_chat_read', { vaultPath: vaultPath.value });
         } catch (e) {
             logger.error('Failed to mark chat as read', e);
         }
     }
  }

  if (newTool === 'whiteboard' && vaultPath.value) {
     if (whiteboardAppRef.value && typeof whiteboardAppRef.value.refreshBoards === 'function') {
         whiteboardAppRef.value.refreshBoards();
     }
  }
});


// ─── Mini App Refs for cross-app navigation ─────────────────
const messagesAppRef = ref<any>(null);
const noteAppRef = ref<any>(null);
const quickCapAppRef = ref<any>(null);
const taskAppRef = ref<any>(null);
const calendarAppRef = ref<any>(null);
const whiteboardAppRef = ref<any>(null);
const peopleAppRef = ref<any>(null);
const financeAppRef = ref<any>(null);
const feedsAppRef = ref<any>(null);
const filesAppRef = ref<any>(null);

const setAppRef = (el: any, name: string) => {
    if (!el) return;
    if (name === 'messages') messagesAppRef.value = el;
    else if (name === 'note') noteAppRef.value = el;
    else if (name === 'quickcap') quickCapAppRef.value = el;
    else if (name === 'task') taskAppRef.value = el;
    else if (name === 'calendar') calendarAppRef.value = el;
    else if (name === 'whiteboard') whiteboardAppRef.value = el;
    else if (name === 'people') peopleAppRef.value = el;
    else if (name === 'finance') financeAppRef.value = el;
    else if (name === 'feeds') feedsAppRef.value = el;
    else if (name === 'file') filesAppRef.value = el;
};

// ─── Floating Note (opened in new window) ─────────────────
const isSidebarCollapsed = ref(false);

watch(activeTool, (newTool) => {
    if (newTool === 'task') {
        taskAppRef.value?.refresh?.();
    }
});
const floatingNoteId = ref<string | null>(null);
const isFloatingView = ref(false);

// ─── GDrive ─────────────────────────────────────────────────
const gdrive = useGDrive(vaultPath, vaultType);
const showGDriveMigrationModal = ref(false);

const handleGDriveMigrated = async (newPath: string) => {
    showGDriveMigrationModal.value = false;
    await appStore.setVaultPath(newPath, 'local');

    invoke('start_vault_watcher', { vaultPath: newPath }).catch(logger.error);
    gdrive.syncGDrive();
};

// ─── P2P Sync ────────────────────────────────────────────────
const p2p = useP2PSync(vaultPath);

import { appDataDir } from '@tauri-apps/api/path';

const selectVault = async () => {
    try {
        if (isMobileOS.value) {
            // On mobile, directory picker is not supported. Use app data dir implicitly.
            const dataDir = await appDataDir();
            const vaultDir = `${dataDir}/vault`;
            await appStore.setVaultPath(vaultDir, 'local');
            invoke('start_vault_watcher', { vaultPath: vaultPath.value }).catch(logger.error);
            return;
        }

        const defaultPath = await documentDir().catch(() => undefined);
        const selected = await open({ 
            title: 'Select Note Vault Directory', 
            defaultPath,
            directory: true, 
            multiple: false 
        });
        if (selected) {
            await appStore.setVaultPath(selected as string, 'local');
            invoke('start_vault_watcher', { vaultPath: vaultPath.value }).catch(logger.error);
        }
    } catch(err) { logger.error(String(err)); }
};

const clearVault = () => {
    vaultPath.value = '';
    vaultType.value = 'local';
    activeTool.value = 'nexus';
    gdrive.setupAutoSync();
};

// ─── Navigation History (Back/Forward) — continued ───────

/** Build a NavEntry snapshot of the current state */
const buildCurrentEntry = (): NavEntry => ({
    app: activeTool.value,
    itemId: getCurrentItemId(),
    scrollTop: getCurrentScrollTop(),
});

/** Navigate to a NavEntry — switch tool and restore item + scroll */
const navigateToEntry = (entry: NavEntry) => {
    isRestoringNav = true;
    activeTool.value = entry.app as any;
    if (entry.itemId) {
        navigateToItem(entry.app, entry.itemId, entry.scrollTop, true);
    } else if (entry.scrollTop) {
        setTimeout(() => {
            const el = document.querySelector('[data-app-scroll]') as HTMLElement;
            if (el) el.scrollTop = entry.scrollTop!;
        }, 150);
    }
    setTimeout(() => { isRestoringNav = false; }, 300);
};

const handleGoBack = () => {
    const entry = navStore.goBack(buildCurrentEntry());
    if (entry) navigateToEntry(entry);
};

const handleGoForward = () => {
    const entry = navStore.goForward(buildCurrentEntry());
    if (entry) navigateToEntry(entry);
};

// Provide navigation to all child mini-apps via inject
// NOTE: Pinia auto-unwraps computed refs, so navStore.canGoBack returns a plain boolean.
// We must wrap in computed() to keep reactivity through provide/inject.
provide('canGoBack', computed(() => navStore.canGoBack));
provide('canGoForward', computed(() => navStore.canGoForward));
provide('goBack', handleGoBack);
provide('goForward', handleGoForward);
provide('pushNavigation', (entry?: NavEntry) => {
    navStore.pushNavigation(entry || buildCurrentEntry());
});

// ─── Cross-app Navigation (Nexus → Note/Task/QuickCap) ───

const callWhenReady = (getRef: () => any, method: string, ...args: any[]) => {
    let attempts = 0;
    const interval = setInterval(() => {
        const componentRef = getRef();
        if (componentRef && typeof componentRef[method] === 'function') {
            clearInterval(interval);
            componentRef[method](...args);
        } else if (attempts >= 40) { // 2 seconds max
            clearInterval(interval);
            logger.warn(`Component ref or method ${method} not ready after 2s`);
        }
        attempts++;
    }, 50);
};

/** Navigate to a specific item within an app, optionally restoring scroll */
const navigateToItem = (app: string, itemId: string, scrollTop?: number, skipNavPush = false) => {
    const restoreScroll = () => {
        if (scrollTop) {
            setTimeout(() => {
                const el = document.querySelector('[data-app-scroll]') as HTMLElement;
                if (el) el.scrollTop = scrollTop;
            }, 200);
        }
    };

    if (app === 'note') { callWhenReady(() => noteAppRef.value, 'openNoteById', itemId, skipNavPush); restoreScroll(); }
    else if (app === 'quickcap') { callWhenReady(() => quickCapAppRef.value, 'openEditById', itemId); }
    else if (app === 'task') { callWhenReady(() => taskAppRef.value, 'openEditById', itemId); }
    else if (app === 'calendar') { callWhenReady(() => calendarAppRef.value, 'openEventById', itemId); }
    else if (app === 'whiteboard') { callWhenReady(() => whiteboardAppRef.value, 'openBoardById', itemId, skipNavPush); restoreScroll(); }
    else if (app === 'people') { callWhenReady(() => peopleAppRef.value, 'openPersonById', itemId); }
    else if (app === 'finance') { callWhenReady(() => financeAppRef.value, 'openMonthById', itemId); }
    else if (app === 'feeds') { callWhenReady(() => feedsAppRef.value, 'openFeedById', itemId); }
    else if (app === 'file') { callWhenReady(() => filesAppRef.value, 'openFileById', itemId, skipNavPush); }
};

const handleEditFromNexus = async (id: string, type: string) => {
    logger.debug(`App.vue: handleEditFromNexus received id: ${id}, type: ${type}`);
    // Note: watcher on activeTool now handles pushing to back stack automatically
    if (type === 'note') { 
        activeTool.value = 'note'; 
        callWhenReady(() => noteAppRef.value, 'openNoteById', id);
    }
    else if (type === 'quickcap') { 
        activeTool.value = 'quickcap'; 
        callWhenReady(() => quickCapAppRef.value, 'openEditById', id);
    }
    else if (type === 'task') { 
        activeTool.value = 'task'; 
        callWhenReady(() => taskAppRef.value, 'openEditById', id);
    }
    else if (type === 'calendar') { 
        activeTool.value = 'calendar'; 
        callWhenReady(() => calendarAppRef.value, 'openEventById', id);
    }
    else if (type === 'whiteboard') {
        activeTool.value = 'whiteboard';
        callWhenReady(() => whiteboardAppRef.value, 'openBoardById', id);
    }
    else if (type === 'person') {
        activeTool.value = 'people';
        callWhenReady(() => peopleAppRef.value, 'openPersonById', id);
    }
    else if (type === 'finance_month') {
        activeTool.value = 'finance';
        callWhenReady(() => financeAppRef.value, 'openMonthById', id);
    }
    else if (type === 'feed_source') {
        activeTool.value = 'feeds';
        callWhenReady(() => feedsAppRef.value, 'openFeedById', id);
    }
    else if (type === 'project') {
        activeTool.value = 'task';
        callWhenReady(() => taskAppRef.value, 'openProjectById', id);
    }
    else if (type === 'pdf' || type === 'pdf_highlight' || type === 'file') {
        activeTool.value = 'file';
        callWhenReady(() => filesAppRef.value, 'openFileById', id);
    }
};

import { logger } from './utils/logger';

// ─── Notifications & Initial Scan ─────────────────────────
const unreadNotificationCount = ref(0);
const feedsUnreadCount = ref(0);

const checkUnreadNotifications = async () => {
    if (!vaultPath.value) return;
    try {
        const msgs = await invoke<any[]>('get_chat_history', { vaultPath: vaultPath.value });
        unreadNotificationCount.value = msgs.filter(m => m.read_receipt === false).length;
    } catch(e) {
        logger.error('Failed to check unread messages', e);
    }
};

const updateFeedsUnreadCount = async () => {
    if (!vaultPath.value) return;
    try {
        feedsUnreadCount.value = await invoke<number>('feed_get_total_unread', { vaultPath: vaultPath.value });
    } catch(e) {
        logger.error('Failed to check feeds unread count', e);
    }
};

// ─── Keyboard shortcuts for navigation ───────────────────
const handleKeyboardNav = (e: KeyboardEvent) => {
    const isMeta = e.metaKey || e.ctrlKey;
    if (isMeta && e.key === '[') {
        e.preventDefault();
        handleGoBack();
    } else if (isMeta && e.key === ']') {
        e.preventDefault();
        handleGoForward();
    }
};



// ─── Lifecycle ────────────────────────────────────────────
onMounted(async () => {
  logger.info("Synabit Frontend App Mounting...");
  await appStore.initialize();
  await initSettings();
  await initEventBus();
  await licenseStore.checkState();
  if (licenseStore.licenseStatus.type === 'NoLicense') {
      showLicenseModal.value = true;
  }
  applyTheme();
  window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', applyTheme);
  window.addEventListener('keydown', handleKeyboardNav);

  const params = new URLSearchParams(window.location.search);
  const floatingId = params.get('floatingNote');
  if (floatingId) {
      isFloatingView.value = true;
      floatingNoteId.value = floatingId;
      activeTool.value = 'note';
  } else {
      activeTool.value = defaultApp.value;
  }

  if (vaultPath.value) {
     invoke('start_vault_watcher', { vaultPath: vaultPath.value }).catch(logger.error);
     
     // Scan all nodes on startup so Nexus sees fresh Indexed DB data
     invoke('scan_all_nodes', { vaultPath: vaultPath.value }).then(async () => {
         await checkUnreadNotifications();
         await updateFeedsUnreadCount();
     }).catch(logger.error);
     
     // Trigger GC for FTS5 on startup
     invoke('reindex_sources', { vaultPath: vaultPath.value }).catch(logger.error);
     invoke('scan_whiteboards', { vaultPath: vaultPath.value }).catch(logger.error);
     
     // Feeds unread count polling (every 60s)
     const feedsUnreadInterval = setInterval(() => updateFeedsUnreadCount(), 60 * 1000);
     
     if (noteAppRef.value) noteAppRef.value.scanVault();
  }

  gdrive.checkGDriveAuth().then(() => { gdrive.setupAutoSync(); });
  p2p.autoReconnect();

  if (vaultType.value === 'gdrive') {
      showGDriveMigrationModal.value = true;
  }

  bus.on('vault:file-created-deleted', (payload: any) => {
      if (noteAppRef.value) noteAppRef.value.scanVault();
      const paths = (payload as string[] | undefined) || [];
      if (paths && paths.length > 0) {
          invoke('scan_specific_nodes', { vaultPath: vaultPath.value, paths }).catch(logger.error);
          
          const hasFiles = paths.some(p => p.startsWith('assets/') || p.includes('Files/'));
          const hasWhiteboards = paths.some(p => p.startsWith('Whiteboards/'));
          if (hasFiles) invoke('reindex_sources', { vaultPath: vaultPath.value }).catch(logger.error);
          if (hasWhiteboards) invoke('scan_whiteboards', { vaultPath: vaultPath.value }).catch(logger.error);
      } else {
          invoke('scan_all_nodes', { vaultPath: vaultPath.value }).catch(logger.error);
          invoke('reindex_sources', { vaultPath: vaultPath.value }).catch(logger.error);
          invoke('scan_whiteboards', { vaultPath: vaultPath.value }).catch(logger.error);
      }
      
      setTimeout(() => checkUnreadNotifications(), 500);
      
      if (gdrive.gdriveConnected.value && !gdrive.gdriveSyncing.value) {
          gdrive.syncGDrive();
      }
      if (p2p.p2pConnected.value && !p2p.p2pSyncing.value) {
          p2p.syncP2P();
      }
  });

  bus.on('vault:file-modified', (payload: any) => {
      if (noteAppRef.value) noteAppRef.value.scanVault();
      const paths = (payload as string[] | undefined) || [];
      if (paths && paths.length > 0) {
          invoke('scan_specific_nodes', { vaultPath: vaultPath.value, paths }).catch(logger.error);
          
          const hasFiles = paths.some(p => p.startsWith('assets/') || p.includes('Files/'));
          const hasWhiteboards = paths.some(p => p.startsWith('Whiteboards/'));
          if (hasFiles) invoke('reindex_sources', { vaultPath: vaultPath.value }).catch(logger.error);
          if (hasWhiteboards) invoke('scan_whiteboards', { vaultPath: vaultPath.value }).catch(logger.error);
      } else {
          invoke('scan_all_nodes', { vaultPath: vaultPath.value }).catch(logger.error);
          invoke('reindex_sources', { vaultPath: vaultPath.value }).catch(logger.error);
          invoke('scan_whiteboards', { vaultPath: vaultPath.value }).catch(logger.error);
      }
      
      setTimeout(() => checkUnreadNotifications(), 500);

      if (gdrive.gdriveConnected.value && !gdrive.gdriveSyncing.value) {
          gdrive.syncGDrive();
      }
      if (p2p.p2pConnected.value && !p2p.p2pSyncing.value) {
          p2p.syncP2P();
      }
  });

  bus.on('chat:new-message', () => {
      checkUnreadNotifications();
      if (messagesAppRef.value) {
          messagesAppRef.value.fetchNotifications();
      }
  });

  // ─── Feeds Unread Badge via Event Bus ──────────────────
  bus.on('feed:refreshed', () => updateFeedsUnreadCount());
  bus.on('node:updated', ({ nodeType }: any) => {
      if (nodeType === 'feed_article') updateFeedsUnreadCount();
  });

  // ─── Cross-App Navigation via Event Bus ──────────────────
  bus.on('navigate:to-item', ({ app, itemId }) => {
      activeTool.value = app;
      navigateToItem(app, itemId);
  });

  getCurrentWindow().onCloseRequested(async () => {
      // NoteApp handles its own save-on-close internally
      // But we trigger a final save here for safety
      if (noteAppRef.value?.currentNoteId) {
          const nApp = noteAppRef.value;
          const noteId = nApp.currentNoteId;
          if (noteId && nApp.tabContents[noteId]) {
              const note = nApp.notes.find((n: any) => n.id === noteId);
              if (note) {
                  try {
                      await ns.writeNode({
                          relPath: note.id,
                          nodeType: 'note',
                          title: note.title,
                          properties: {
                              pinned: note.pinned,
                              tags: note.tags
                          },
                          content: nApp.tabContents[noteId],
                          silent: true,
                      });
                      emit('note-updated', { id: note.id, content: nApp.tabContents[noteId] });
                  } catch(e) { logger.error('Save before close failed', e); }
              }
          }
      }
  });

  logger.info("Synabit Frontend App Mount Complete.");
  
  // Show window smoothly after everything is initialized
  setTimeout(() => {
      getCurrentWindow().show().catch(logger.error);
  }, 100);
});

onUnmounted(() => {
  window.matchMedia('(prefers-color-scheme: dark)').removeEventListener('change', applyTheme);
  window.removeEventListener('keydown', handleKeyboardNav);
  destroyEventBus();
  // Note: feedsUnreadInterval is scoped inside onMounted, cleaned up via component lifecycle
});
</script>

<template>
  <div class="flex h-screen w-full bg-base text-text dark:bg-base-dark dark:text-text-dark font-sans overflow-hidden select-none">

    <!-- ═══ Auto-Update Banner ═══ -->
    <Transition name="slide-down">
      <div v-if="updateAvailable && !updateDownloading"
           class="fixed top-0 left-0 right-0 z-[9999] bg-indigo-600 text-white px-4 py-2.5 flex items-center justify-between shadow-lg">
        <div class="flex items-center gap-2.5 min-w-0">
          <svg class="w-4 h-4 flex-shrink-0 animate-bounce" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"/>
          </svg>
          <div class="min-w-0">
            <span class="text-sm font-medium truncate block">{{ $t('update.available', { version: updateVersion }) }}</span>
            <span v-if="updateNotes" class="text-xs text-indigo-200 truncate block mt-0.5">{{ updateNotes.split('\n')[0] }}</span>
          </div>
        </div>
        <div class="flex items-center gap-2 flex-shrink-0">
          <button @click="downloadAndInstall"
                  class="bg-white text-indigo-600 px-3 py-1 rounded-md text-xs font-semibold hover:bg-indigo-50 transition cursor-pointer">
            {{ $t('update.installNow') }}
          </button>
          <button @click="dismissUpdate"
                  class="text-indigo-200 hover:text-white px-2 py-1 text-xs transition cursor-pointer">
            {{ $t('update.later') }}
          </button>
        </div>
      </div>
    </Transition>

    <!-- ═══ Update Download Progress ═══ -->
    <div v-if="updateDownloading"
         class="fixed top-0 left-0 right-0 z-[9999] bg-indigo-600 text-white px-4 py-2.5 shadow-lg">
      <div class="flex items-center justify-between mb-1.5">
        <span class="text-xs font-medium">{{ $t('update.downloading') }}</span>
        <span class="text-xs tabular-nums">{{ updateProgress }}%</span>
      </div>
      <div class="w-full bg-indigo-400/50 rounded-full h-1.5">
        <div class="bg-white h-1.5 rounded-full transition-all duration-300 ease-out"
             :style="{ width: updateProgress + '%' }"/>
      </div>
    </div>

    <!-- Application State 0: Initializing -->
    <div v-if="!appStore.isReady" class="flex-1 flex flex-col items-center justify-center p-8 bg-base dark:bg-base-dark" data-tauri-drag-region>
    </div>

    <!-- Application State 1: No Vault Selected -->
    <div v-else-if="!vaultPath" class="flex-1 flex flex-col items-center justify-center p-8 bg-base dark:bg-base-dark" data-tauri-drag-region>
        <div class="max-w-lg w-full text-center space-y-8">
            <div class="w-20 h-20 bg-gray-100 dark:bg-gray-800 rounded-full flex items-center justify-center mx-auto shadow-inner">
               <FileText class="w-10 h-10 text-gray-400" />
            </div>
            <div>
               <h1 class="text-2xl font-bold mb-2">Welcome to Synabit</h1>
               <p class="text-text-secondary dark:text-text-secondary-dark text-sm">Choose how you want to store your vault.</p>
            </div>
            
            <div class="flex gap-4 justify-center" @mousedown.stop>
              <button @click="selectVault" class="group flex flex-col items-center gap-3 p-6 w-48 rounded-2xl border-2 border-border dark:border-[#333] hover:border-black dark:hover:border-white bg-surface dark:bg-surface-dark transition-all hover:shadow-lg active:scale-[0.98] cursor-pointer">
                <div class="w-12 h-12 rounded-xl bg-gray-100 dark:bg-gray-800 flex items-center justify-center group-hover:bg-gray-200 dark:group-hover:bg-gray-700 transition-colors">
                  <FolderOpen class="w-6 h-6 text-gray-600 dark:text-gray-300" />
                </div>
                <div>
                  <p class="font-semibold text-sm">Local Folder</p>
                  <p class="text-[11px] text-gray-400 mt-1">Store on this computer</p>
                </div>
              </button>
              
              <button @click="gdrive.connectGDrive()" :disabled="gdrive.gdriveAuthLoading.value" class="group flex flex-col items-center gap-3 p-6 w-48 rounded-2xl border-2 border-border dark:border-[#333] hover:border-blue-500 dark:hover:border-blue-400 bg-surface dark:bg-surface-dark transition-all hover:shadow-lg active:scale-[0.98] cursor-pointer disabled:opacity-60 disabled:pointer-events-none">
                <div class="w-12 h-12 rounded-xl bg-blue-50 dark:bg-blue-900/30 flex items-center justify-center group-hover:bg-blue-100 dark:group-hover:bg-blue-900/50 transition-colors">
                  <Cloud v-if="!gdrive.gdriveAuthLoading.value" class="w-6 h-6 text-blue-500" />
                  <RefreshCw v-else class="w-6 h-6 text-blue-500 animate-spin" />
                </div>
                <div>
                  <p class="font-semibold text-sm">Google Drive</p>
                  <p class="text-[11px] text-gray-400 mt-1">Sync across devices</p>
                </div>
              </button>
            </div>
            
            <p v-if="gdrive.gdriveSyncError.value" class="text-red-500 text-xs px-4">{{ gdrive.gdriveSyncError.value }}</p>
        </div>
    </div>

    <!-- Application State 2: Vault Selected -->
    <template v-else>

      <component :is="useMobileLayout ? MobileLayout : DesktopLayout" :activeTool="activeTool" @update:activeTool="activeTool = $event">
        
        <!-- SIDEBAR / BOTTOMBAR -->
        <template v-if="!isFloatingView" #[useMobileLayout?'bottombar':'sidebar']>
          <nav :class="useMobileLayout ? 'w-full flex justify-around items-center h-full' : 'w-16 flex-shrink-0 bg-sidebar dark:bg-sidebar-dark border-r border-border dark:border-border-dark flex flex-col items-center py-4 z-20 h-full'" data-tauri-drag-region>
              <div :class="useMobileLayout ? 'flex justify-around items-center w-full' : 'flex-1 flex flex-col items-center gap-3 mt-4 w-full'" @mousedown.stop>
                <button v-if="isAppVisible('nexus')" @click="activeTool = 'nexus'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'nexus' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Globe class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Nexus</span>
                </button>

                <button v-if="isAppVisible('messages')" @click="activeTool = 'messages'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'messages' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <MessageCircle class="w-5 h-5" />
                   <div v-if="unreadNotificationCount > 0" class="absolute -top-1 -right-1 min-w-[18px] h-[18px] px-1 bg-red-500 text-white text-[10px] font-bold rounded-full flex items-center justify-center ring-2 ring-[#f8f9fa] dark:ring-[#1a1a1a] shadow-sm">{{ unreadNotificationCount > 99 ? '99+' : unreadNotificationCount }}</div>
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Messages</span>
                </button>

                <button v-if="isAppVisible('quickcap')" @click="activeTool = 'quickcap'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'quickcap' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Zap class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">QuickCap</span>
                </button>
                <button v-if="isAppVisible('note')" @click="activeTool = 'note'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'note' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <FileText class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Notes</span>
                </button>
                <button v-if="isAppVisible('task')" @click="activeTool = 'task'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'task' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <CheckSquare class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Tasks</span>
                </button>
                <button v-if="isAppVisible('calendar')" @click="activeTool = 'calendar'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'calendar' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Calendar class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Calendar</span>
                </button>
                <button v-if="isAppVisible('file')" @click="activeTool = 'file'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'file' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <FolderOpen class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Files</span>
                </button>
                <button v-if="isAppVisible('whiteboard')" @click="activeTool = 'whiteboard'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'whiteboard' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Palette class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Whiteboard</span>
                </button>
                <button v-if="isAppVisible('people')" @click="activeTool = 'people'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'people' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Users class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">People</span>
                </button>

                <button v-if="isAppVisible('finance')" @click="activeTool = 'finance'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'finance' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Wallet class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Finance</span>
                </button>

                <button v-if="isAppVisible('feeds')" @click="activeTool = 'feeds'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'feeds' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Rss class="w-5 h-5" />
                   <span v-if="feedsUnreadCount > 0" class="absolute -top-1 -right-1 min-w-[18px] h-[18px] bg-orange-500 text-white text-[10px] font-bold rounded-full flex items-center justify-center px-1 shadow-sm ring-2 ring-[#f8f9fa] dark:ring-[#1a1a1a]">{{ feedsUnreadCount > 99 ? '99+' : feedsUnreadCount }}</span>
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Feeds</span>
                </button>

                
                <div v-if="moreMenuApps.length > 0" class="relative flex justify-center">
                  <button @click="showHiddenAppsMenu = !showHiddenAppsMenu" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', showHiddenAppsMenu ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                    <MoreHorizontal class="w-5 h-5" />
                    <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">More Apps</span>
                  </button>
                  
                  <!-- Overlay for clicking outside -->
                  <div v-if="showHiddenAppsMenu" class="fixed inset-0 z-40" @click="showHiddenAppsMenu = false"></div>
                  
                  <div v-if="showHiddenAppsMenu" :class="useMobileLayout ? 'absolute bottom-full mb-4 right-0 w-48' : 'absolute left-full top-0 ml-2 w-48'" class="py-2 bg-white dark:bg-[#1a1a1a] rounded-xl shadow-xl border border-gray-200 dark:border-[#2c2c2c] z-50 max-h-[60vh] overflow-y-auto">
                    <button v-for="app in moreMenuApps" :key="app.id" @click="activeTool = app.id; showHiddenAppsMenu = false" class="w-full flex items-center gap-3 px-4 py-3 text-sm text-[#1c1c1e] dark:text-[#f4f4f5] hover:bg-gray-100 dark:hover:bg-[#2c2c2c] transition-colors">
                      <component :is="app.icon" class="w-5 h-5 text-gray-500" />
                      <span class="font-medium">{{ app.name }}</span>
                    </button>
                  </div>
                </div>
                
                <button v-if="useMobileLayout" @click="openSettings" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', showSettingsModal ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Settings class="w-5 h-5" />
                </button>
             </div>
             
             <!-- Settings & Sync bottom icons for desktop -->
             <div v-if="!useMobileLayout" class="flex-shrink-0 w-full flex flex-col items-center gap-3 mb-2" @mousedown.stop>
                <button v-if="gdrive.gdriveConnected.value" @click="gdrive.syncGDrive()" :disabled="gdrive.gdriveSyncing.value" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', gdrive.gdriveSyncError.value ? 'text-red-500 hover:bg-red-100 dark:hover:bg-red-900/30' : gdrive.gdriveConnected.value ? 'text-blue-500 hover:bg-blue-100 dark:hover:bg-blue-900/30' : 'text-gray-400 hover:bg-gray-200 dark:hover:bg-gray-800']" :title="gdrive.gdriveSyncing.value ? 'Syncing...' : gdrive.lastSyncTime.value ? `Last sync: ${gdrive.lastSyncTime.value}` : 'Sync with Google Drive'">
                   <RefreshCw v-if="gdrive.gdriveSyncing.value" class="w-5 h-5 animate-spin" />
                   <CloudOff v-else-if="gdrive.gdriveSyncError.value" class="w-5 h-5" />
                   <Cloud v-else class="w-5 h-5" />
                   <span class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">{{ gdrive.gdriveSyncing.value ? 'Syncing…' : gdrive.gdriveSyncError.value ? 'Sync Error' : gdrive.lastSyncTime.value ? `Synced ${gdrive.lastSyncTime.value}` : 'Sync Now' }}</span>
                </button>
                <button v-if="p2p.p2pConnected.value" @click="p2p.syncP2P()" :disabled="p2p.p2pSyncing.value" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', p2p.p2pSyncError.value ? 'text-red-500 hover:bg-red-100 dark:hover:bg-red-900/30' : 'text-emerald-500 hover:bg-emerald-100 dark:hover:bg-emerald-900/30']" :title="p2p.p2pSyncing.value ? 'Syncing...' : p2p.lastSyncTime.value ? `P2P synced ${p2p.lastSyncTime.value}` : 'P2P Sync'">
                   <RefreshCw v-if="p2p.p2pSyncing.value" class="w-5 h-5 animate-spin" />
                   <Server v-else class="w-5 h-5" />
                   <span class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">{{ p2p.p2pSyncing.value ? 'P2P Syncing…' : p2p.p2pSyncError.value ? 'P2P Error' : p2p.lastSyncTime.value ? `P2P ${p2p.lastSyncTime.value}` : 'P2P Sync' }}</span>
                </button>
                 <button @click="showLicenseModal = true" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', showLicenseModal ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <div v-if="licenseStore.isPro" class="text-green-500 font-bold text-xs"><Shield class="w-5 h-5"/></div>
                   <div v-else-if="licenseStore.isTrial" class="text-orange-500 font-bold text-xs flex flex-col items-center leading-none">
                       <span>{{ licenseStore.daysLeft }}</span>
                       <span class="text-[8px]">days</span>
                   </div>
                   <TerminalSquare v-else-if="licenseStore.isDev" class="w-5 h-5 text-blue-500" />
                   <Shield v-else class="w-5 h-5 text-red-500" />
                   <span class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">License</span>
                </button>
                 <button @click="openSettings" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', showSettingsModal ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Settings class="w-5 h-5" />
                   <span class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Settings</span>
                </button>
             </div>
          </nav>
        </template>

        <!-- MINI APP CONTENT AREA (Vue Router + KeepAlive) -->
        <div class="flex-1 h-full overflow-hidden relative">
            <router-view v-slot="{ Component, route }">
                <!-- Tier 2: Show PIN pad directly for protected mini-apps -->
                <LockScreen
                    v-if="appLockStore.isEnabled && appLockStore.isAppProtected(route.name as string) && !appLockStore.isMiniAppAccessible(route.name as string)"
                    :title="`Enter PIN to access ${getAppName(route.name as string)}`"
                    @unlocked="appLockStore.unlockMiniApp(route.name as string)"
                    @cancelled="router.back()"
                />
                <keep-alive v-else>
                    <component 
                        :is="Component" 
                        :key="route.name"
                        :vault-path="vaultPath" 
                        :is-floating-view="isFloatingView" 
                        :floating-note-id="floatingNoteId" 
                        @open-node="handleEditFromNexus"
                        @edit-item="handleEditFromNexus"
                        :ref="(el: any) => setAppRef(el, route.name as string)"
                    />
                </keep-alive>
            </router-view>
            
            <!-- Conflict Toast -->
            <SyncConflictToast />

            <!-- GDrive Migration Modal -->
            <GDriveMigrationModal 
              :show="showGDriveMigrationModal" 
              @migrated="handleGDriveMigrated" 
            />
        </div>


        <!-- SETTINGS MODAL -->
        <template #modal>
          <SettingsModal
            :vault-path="vaultPath"
            :vault-type="vaultType"
            :gdrive-connected="gdrive.gdriveConnected.value"
            :gdrive-syncing="gdrive.gdriveSyncing.value"
            :gdrive-sync-error="gdrive.gdriveSyncError.value"
            :last-sync-time="gdrive.lastSyncTime.value"
            :gdrive-auto-sync-enabled="gdrive.gdriveAutoSyncEnabled.value"
            :gdrive-auto-sync-interval="gdrive.gdriveAutoSyncInterval.value"
            :p2p-connected="p2p.p2pConnected.value"
            :p2p-syncing="p2p.p2pSyncing.value"
            :p2p-sync-error="p2p.p2pSyncError.value"
            :p2p-connecting="p2p.p2pConnecting.value"
            :p2p-last-sync-time="p2p.lastSyncTime.value"
            :p2p-auto-sync-enabled="p2p.p2pAutoSyncEnabled.value"
            :p2p-auto-sync-interval="p2p.p2pAutoSyncInterval.value"
            :p2p-server-addr="appStore.p2pServerAddr"
            :p2p-server-id-hex="appStore.p2pServerIdHex"
            @clear-vault="clearVault"
            @sync-gdrive="gdrive.syncGDrive()"
            @connect-gdrive="gdrive.connectGDrive()"
            @disconnect-gdrive="gdrive.disconnectGDrive()"
            @update:gdrive-auto-sync-enabled="gdrive.gdriveAutoSyncEnabled.value = $event"
            @update:gdrive-auto-sync-interval="gdrive.gdriveAutoSyncInterval.value = $event"
            @p2p-connect="(addr: string, id: string) => p2p.connectP2P(addr, id)"
            @p2p-disconnect="p2p.disconnectP2P()"
            @p2p-sync="p2p.syncP2P()"
            @update:p2p-auto-sync-enabled="p2p.p2pAutoSyncEnabled.value = $event"
            @update:p2p-auto-sync-interval="p2p.p2pAutoSyncInterval.value = $event"
            @show-setup-pin="(mode: 'setup' | 'change') => { setupPinMode = mode; showSetupPinModal = true; }"
          />
        </template>
      </component>

      <!-- Sync Conflict Toast (floating bottom-right) -->
      <SyncConflictToast />
    </template>

    <!-- E2EE Onboarding Modal -->
    <E2eeOnboarding v-if="showE2eeOnboarding" @done="showE2eeOnboarding = false" />
    <LicenseModal :isOpen="showLicenseModal" @close="showLicenseModal = false" />

    <!-- Tier 1: App Lock Screen -->
    <LockScreen
      v-if="appLockStore.isEnabled && appLockStore.isAppLocked"
      title="Enter PIN to unlock Synabit"
      :cancellable="false"
      @unlocked="appLockStore.unlockApp()"
    />

    <!-- Setup PIN Modal -->
    <SetupPinModal
      v-if="showSetupPinModal"
      :mode="setupPinMode"
      @done="showSetupPinModal = false; appLockStore.refreshConfig();"
      @cancel="showSetupPinModal = false"
    />

  </div>
</template>

<style scoped>
[data-tauri-drag-region] {
  -webkit-app-region: drag;
}

/* Auto-Update banner slide transition */
.slide-down-enter-active,
.slide-down-leave-active {
  transition: transform 0.3s ease, opacity 0.3s ease;
}
.slide-down-enter-from,
.slide-down-leave-to {
  transform: translateY(-100%);
  opacity: 0;
}
</style>