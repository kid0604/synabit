<script setup lang="ts">
import { ref, computed, provide, onMounted, onUnmounted, watch } from 'vue';
import { FileText, FolderOpen, Calendar, CheckSquare, Zap, Globe, Cloud, RefreshCw, CloudOff, Settings, Users, Wallet, MessageSquare } from 'lucide-vue-next';
import { invoke } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { open } from '@tauri-apps/plugin-dialog';
import { documentDir } from '@tauri-apps/api/path';

import { defineAsyncComponent } from 'vue';

// Mini App Components — lazy loaded for code splitting
const NoteApp = defineAsyncComponent(() => import('./mini-apps/note/NoteApp.vue'));
const QuickCap = defineAsyncComponent(() => import('./mini-apps/quickcap/QuickCapApp.vue'));
const Tasks = defineAsyncComponent(() => import('./mini-apps/task/TaskApp.vue'));
const CalendarApp = defineAsyncComponent(() => import('./mini-apps/calendar/CalendarApp.vue'));
const Nexus = defineAsyncComponent(() => import('./mini-apps/nexus/NexusApp.vue'));
const FilesApp = defineAsyncComponent(() => import('./mini-apps/files/FilesApp.vue'));
const WhiteboardApp = defineAsyncComponent(() => import('./mini-apps/whiteboard/WhiteboardApp.vue'));
const PeopleApp = defineAsyncComponent(() => import('./mini-apps/people/PeopleApp.vue'));
const FinanceApp = defineAsyncComponent(() => import('./mini-apps/finance/FinanceApp.vue'));
const ChatApp = defineAsyncComponent(() => import('./mini-apps/chat/ChatApp.vue'));
const SettingsModal = defineAsyncComponent(() => import('./shared/components/SettingsModal.vue'));

// Composables
import { useSettings } from './composables/useSettings';
import { useGDrive } from './composables/useGDrive';
import { usePlatform } from './composables/usePlatform';

import DesktopLayout from './layouts/DesktopLayout.vue';
import MobileLayout from './layouts/MobileLayout.vue';

// Stores
import { useAppStore } from './stores/useAppStore';
import { useNavigationStore, type NavEntry } from './stores/useNavigationStore';
import { storeToRefs } from 'pinia';

// ─── Settings ─────────────────────────────────────────────
const {
  showSettingsModal, openSettings, initSettings, applyTheme, defaultApp
} = useSettings();

const appStore = useAppStore();
const { vaultPath, vaultType } = storeToRefs(appStore);

const { useMobileLayout, isMobileOS } = usePlatform();

// ─── App View State ───────────────────────────────────────
const activeTool = ref<'nexus' | 'quickcap' | 'note' | 'task' | 'calendar' | 'file' | 'whiteboard' | 'people' | 'finance' | 'chat'>('nexus');

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
  
  if (newTool === 'chat' && hasUnreadNotifications.value && vaultPath.value) {
     hasUnreadNotifications.value = false;
     try {
         const msgs = await invoke<any[]>('get_nodes', { nodeType: 'message' });
         for (const m of msgs) {
             if (m.properties && m.properties.is_read === false) {
                 m.properties.is_read = true;
                 await invoke('write_node_file', {
                     vaultPath: vaultPath.value,
                     relPath: m.id,
                     title: m.title,
                     nodeType: 'message',
                     properties: m.properties,
                     content: m.content
                 });
             }
         }
     } catch(e) { logger.error('Failed to mark messages as read', e); }
  }
});

// ─── Mini App Refs for cross-app navigation ─────────────────
const noteAppRef = ref<InstanceType<typeof NoteApp> | null>(null);
const quickCapAppRef = ref<any>(null);
const taskAppRef = ref<any>(null);
const calendarAppRef = ref<any>(null);
const whiteboardAppRef = ref<any>(null);
const peopleAppRef = ref<any>(null);
const financeAppRef = ref<any>(null);
const filesAppRef = ref<any>(null);

// ─── Floating Note (opened in new window) ─────────────────
const isFloatingView = ref(false);
const floatingNoteId = ref<string | null>(null);

// ─── GDrive (needs NoteApp's scanVault + tabContents for sync pulling) ─────
const dummyScanVault = async () => {
    if (noteAppRef.value) await noteAppRef.value.scanVault();
};
const dummyTabContents = ref<Record<string, string>>({});
const dummyLoadNoteFile = async (id: string) => {
    if (noteAppRef.value) await noteAppRef.value.loadNoteFile(id);
};
const dummyCurrentNoteId = ref<string | null>(null);

// Keep sync with NoteApp refs when NoteApp exists
watch(() => noteAppRef.value?.tabContents, (v) => { if (v) dummyTabContents.value = v; }, { deep: true });
watch(() => noteAppRef.value?.currentNoteId, (v) => { dummyCurrentNoteId.value = v ?? null; });

const gdrive = useGDrive(vaultPath, vaultType, dummyScanVault, dummyTabContents, dummyLoadNoteFile, dummyCurrentNoteId);

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
    else if (type === 'pdf' || type === 'pdf_highlight') {
        activeTool.value = 'file';
        callWhenReady(() => filesAppRef.value, 'openFileById', id);
    }
};

import { logger } from './utils/logger';

// ─── Notifications & Initial Scan ─────────────────────────
const hasUnreadNotifications = ref(false);

const checkUnreadNotifications = async () => {
    if (!vaultPath.value) return;
    try {
        const msgs = await invoke<any[]>('get_nodes', { nodeType: 'message' });
        hasUnreadNotifications.value = msgs.some(m => m.properties && m.properties.is_read === false);
    } catch(e) {
        logger.error('Failed to check unread messages', e);
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
     
     // Force sync all data types on startup so Nexus sees fresh Indexed DB data
     invoke('migrate_tasks_to_nodes', { vaultPath: vaultPath.value }).then(() => {
         invoke('migrate_events_to_nodes', { vaultPath: vaultPath.value }).then(() => {
             invoke('migrate_quickcaps_to_nodes', { vaultPath: vaultPath.value }).then(() => {
                 invoke('scan_all_nodes', { vaultPath: vaultPath.value }).then(async () => {
                     await checkUnreadNotifications();
                     // One-time migration: graph_edges → node_edges
                     invoke('migrate_graph_edges').catch(logger.error);
                 }).catch(logger.error);
             }).catch(logger.error);
         }).catch(logger.error);
     }).catch(logger.error);
     
     if (noteAppRef.value) noteAppRef.value.scanVault();
  }

  gdrive.checkGDriveAuth().then(() => { gdrive.setupAutoSync(); });

  let unlistenFns: (() => void)[] = [];

  listen('vault-file-created-deleted', (event) => {
      if (noteAppRef.value) noteAppRef.value.scanVault();
      const paths = event.payload as string[];
      if (paths && paths.length > 0) {
          invoke('scan_specific_nodes', { vaultPath: vaultPath.value, paths }).catch(logger.error);
      } else {
          invoke('scan_all_nodes', { vaultPath: vaultPath.value }).catch(logger.error);
      }
      
      setTimeout(() => checkUnreadNotifications(), 500);
      
      if (vaultType.value === 'gdrive' && gdrive.gdriveConnected.value && !gdrive.gdriveSyncing.value) {
          gdrive.syncGDrive();
      }
  }).then(fn => unlistenFns.push(fn));

  listen('vault-file-modified', (event) => {
      if (noteAppRef.value) noteAppRef.value.scanVault();
      const paths = event.payload as string[];
      if (paths && paths.length > 0) {
          invoke('scan_specific_nodes', { vaultPath: vaultPath.value, paths }).catch(logger.error);
      } else {
          invoke('scan_all_nodes', { vaultPath: vaultPath.value }).catch(logger.error);
      }
      
      setTimeout(() => checkUnreadNotifications(), 500);
  }).then(fn => unlistenFns.push(fn));

  onUnmounted(() => {
      unlistenFns.forEach(fn => fn());
      unlistenFns = [];
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
                      await invoke('write_node_file', {
                          vaultPath: vaultPath.value,
                          relPath: note.id,
                          nodeType: 'note',
                          title: note.title,
                          properties: {
                              pinned: note.pinned,
                              tags: note.tags
                          },
                          content: nApp.tabContents[noteId]
                      });
                      emit('note-updated', { id: note.id, content: nApp.tabContents[noteId] });
                  } catch(e) { logger.error('Save before close failed', e); }
              }
          }
      }
  });

  logger.info("Synabit Frontend App Mount Complete.");
});

onUnmounted(() => {
  window.matchMedia('(prefers-color-scheme: dark)').removeEventListener('change', applyTheme);
  window.removeEventListener('keydown', handleKeyboardNav);
});
</script>

<template>
  <div class="flex h-screen w-full bg-base text-text dark:bg-base-dark dark:text-text-dark font-sans overflow-hidden select-none">
       
    <!-- Application State 1: No Vault Selected -->
    <div v-if="!vaultPath" class="flex-1 flex flex-col items-center justify-center p-8 bg-base dark:bg-base-dark" data-tauri-drag-region>
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
                <button @click="activeTool = 'nexus'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'nexus' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Globe class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Nexus</span>
                </button>

                <button @click="activeTool = 'chat'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'chat' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <MessageSquare class="w-5 h-5" />
                   <div v-if="hasUnreadNotifications" class="absolute top-[8px] right-[8px] w-[6px] h-[6px] bg-red-500 rounded-full ring-2 ring-[#f8f9fa] dark:ring-[#1a1a1a]"></div>
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Chat</span>
                </button>

                <button @click="activeTool = 'quickcap'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'quickcap' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Zap class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">QuickCap</span>
                </button>
                <button @click="activeTool = 'note'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'note' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <FileText class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Notes</span>
                </button>
                <button @click="activeTool = 'task'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'task' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <CheckSquare class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Tasks</span>
                </button>
                <button @click="activeTool = 'calendar'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'calendar' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Calendar class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Calendar</span>
                </button>
                <button v-if="!useMobileLayout" @click="activeTool = 'file'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'file' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <FolderOpen class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Files</span>
                </button>
                <button @click="activeTool = 'whiteboard'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'whiteboard' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <svg class="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                     <rect x="2" y="4" width="20" height="16" rx="3" />
                     <path d="M6 14 C 7 11, 9 11, 10 14 C 11 17, 13 10, 14 13" />
                     <path d="M15.5 4 L 20 8.5 M 14 10.5 L 18.5 6" />
                   </svg>
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Whiteboard</span>
                </button>
                <button @click="activeTool = 'people'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'people' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Users class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">People</span>
                </button>

                <button @click="activeTool = 'finance'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'finance' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Wallet class="w-5 h-5" />
                   <span v-if="!useMobileLayout" class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Finance</span>
                </button>
                
                <button v-if="useMobileLayout" @click="openSettings" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', showSettingsModal ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Settings class="w-5 h-5" />
                </button>
             </div>
             
             <!-- Settings & Sync bottom icons for desktop -->
             <div v-if="!useMobileLayout" class="flex-shrink-0 w-full flex flex-col items-center gap-3 mb-2" @mousedown.stop>
                <button v-if="vaultType === 'gdrive'" @click="gdrive.syncGDrive()" :disabled="gdrive.gdriveSyncing.value" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', gdrive.gdriveSyncError.value ? 'text-red-500 hover:bg-red-100 dark:hover:bg-red-900/30' : gdrive.gdriveConnected.value ? 'text-blue-500 hover:bg-blue-100 dark:hover:bg-blue-900/30' : 'text-gray-400 hover:bg-gray-200 dark:hover:bg-gray-800']" :title="gdrive.gdriveSyncing.value ? 'Syncing...' : gdrive.lastSyncTime.value ? `Last sync: ${gdrive.lastSyncTime.value}` : 'Sync with Google Drive'">
                   <RefreshCw v-if="gdrive.gdriveSyncing.value" class="w-5 h-5 animate-spin" />
                   <CloudOff v-else-if="gdrive.gdriveSyncError.value" class="w-5 h-5" />
                   <Cloud v-else class="w-5 h-5" />
                   <span class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">{{ gdrive.gdriveSyncing.value ? 'Syncing…' : gdrive.gdriveSyncError.value ? 'Sync Error' : gdrive.lastSyncTime.value ? `Synced ${gdrive.lastSyncTime.value}` : 'Sync Now' }}</span>
                </button>
                <button @click="openSettings" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', showSettingsModal ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
                   <Settings class="w-5 h-5" />
                   <span class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Settings</span>
                </button>
             </div>
          </nav>
        </template>

        <!-- MINI APP CONTENT AREA (v-show keeps all apps mounted, instant switching) -->
        <div v-show="activeTool === 'chat'" class="flex-1 h-full overflow-hidden">
            <ChatApp :vaultPath="vaultPath" @open-node="handleEditFromNexus" />
        </div>
        <div v-show="activeTool === 'note'" class="flex-1 h-full overflow-hidden">
            <NoteApp ref="noteAppRef" :vault-path="vaultPath" :is-floating-view="isFloatingView" :floating-note-id="floatingNoteId" @open-node="handleEditFromNexus" />
        </div>
        <div v-show="activeTool === 'quickcap'" class="flex-1 h-full overflow-hidden">
            <QuickCap ref="quickCapAppRef" :vaultPath="vaultPath" />
        </div>
        <div v-show="activeTool === 'nexus'" class="flex-1 h-full overflow-hidden">
            <Nexus :vaultPath="vaultPath" @edit-item="handleEditFromNexus" />
        </div>
        <div v-show="activeTool === 'task'" class="flex-1 h-full overflow-hidden">
            <Tasks ref="taskAppRef" :vaultPath="vaultPath" />
        </div>
        <div v-show="activeTool === 'calendar'" class="flex-1 h-full overflow-hidden">
            <CalendarApp ref="calendarAppRef" :vaultPath="vaultPath" @open-node="handleEditFromNexus" />
        </div>
        <div v-show="activeTool === 'file'" class="flex-1 h-full overflow-hidden">
            <FilesApp ref="filesAppRef" :vaultPath="vaultPath" />
        </div>
        <div v-show="activeTool === 'whiteboard'" class="flex-1 h-full overflow-hidden">
            <WhiteboardApp ref="whiteboardAppRef" :vaultPath="vaultPath" />
        </div>
        <div v-show="activeTool === 'people'" class="flex-1 h-full overflow-hidden">
            <PeopleApp ref="peopleAppRef" :vaultPath="vaultPath" @open-node="handleEditFromNexus" />
        </div>
        <div v-show="activeTool === 'finance'" class="flex-1 h-full overflow-hidden">
            <FinanceApp ref="financeAppRef" :vaultPath="vaultPath" />
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
            @clear-vault="clearVault"
            @sync-gdrive="gdrive.syncGDrive()"
            @connect-gdrive="gdrive.connectGDrive()"
            @disconnect-gdrive="gdrive.disconnectGDrive().then(clearVault)"
            @update:gdrive-auto-sync-enabled="gdrive.gdriveAutoSyncEnabled.value = $event"
            @update:gdrive-auto-sync-interval="gdrive.gdriveAutoSyncInterval.value = $event"
          />
        </template>
      </component>
    </template>
  </div>
</template>

<style scoped>
[data-tauri-drag-region] {
  -webkit-app-region: drag;
}
</style>