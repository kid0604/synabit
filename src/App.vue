<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue';
import { FileText, FolderOpen, Calendar, CheckSquare, Zap, Globe, Cloud, RefreshCw, CloudOff, Settings } from 'lucide-vue-next';
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
const FileManager = defineAsyncComponent(() => import('./mini-apps/file/FileApp.vue'));
const SettingsModal = defineAsyncComponent(() => import('./shared/components/SettingsModal.vue'));

// Composables
import { useSettings } from './composables/useSettings';
import { useGDrive } from './composables/useGDrive';
import { usePlatform } from './composables/usePlatform';

import DesktopLayout from './layouts/DesktopLayout.vue';
import MobileLayout from './layouts/MobileLayout.vue';

// Stores
import { useAppStore } from './stores/useAppStore';
import { storeToRefs } from 'pinia';

// ─── Settings ─────────────────────────────────────────────
const {
  showSettingsModal, openSettings, initSettings, applyTheme, defaultApp
} = useSettings();

const appStore = useAppStore();
const { vaultPath, vaultType } = storeToRefs(appStore);

const { useMobileLayout, isMac, isWindows, isMobileOS } = usePlatform();

// ─── App View State ───────────────────────────────────────
const activeTool = ref<'nexus' | 'quickcap' | 'note' | 'task' | 'calendar' | 'file'>('nexus');

// ─── Mini App Refs for cross-app navigation ─────────────────
const noteAppRef = ref<InstanceType<typeof NoteApp> | null>(null);
const quickCapAppRef = ref<any>(null);
const taskAppRef = ref<any>(null);

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
            invoke('start_vault_watcher', { vaultPath: vaultPath.value }).catch(console.error);
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
            invoke('start_vault_watcher', { vaultPath: vaultPath.value }).catch(console.error);
        }
    } catch(err) { console.error(err); }
};

const clearVault = () => {
    vaultPath.value = '';
    vaultType.value = 'local';
    activeTool.value = 'nexus';
    gdrive.setupAutoSync();
};

// ─── Cross-app Navigation (Nexus → Note/Task/QuickCap) ───
const handleEditFromNexus = (id: string, type: string) => {
    if (type === 'note') { 
        activeTool.value = 'note'; 
        nextTick(() => noteAppRef.value?.openNoteById(id)); 
    }
    else if (type === 'quickcap') { 
        activeTool.value = 'quickcap'; 
        nextTick(() => quickCapAppRef.value?.openEditById(id)); 
    }
    else if (type === 'task') { 
        activeTool.value = 'task'; 
        nextTick(() => taskAppRef.value?.openEditById(id)); 
    }
};

import { nextTick } from 'vue';

// ─── Lifecycle ────────────────────────────────────────────
onMounted(async () => {
  await appStore.initialize();
  await initSettings();
  applyTheme();
  window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', applyTheme);

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
     invoke('start_vault_watcher', { vaultPath: vaultPath.value }).catch(console.error);
     
     // Force sync all data types on startup so Nexus sees fresh Indexed DB data
     invoke('scan_tasks', { vaultPath: vaultPath.value }).catch(console.error);
     invoke('scan_events', { vaultPath: vaultPath.value }).catch(console.error);
     invoke('scan_quick_caps', { vaultPath: vaultPath.value }).catch(console.error);
     if (noteAppRef.value) noteAppRef.value.scanVault();
  }

  gdrive.checkGDriveAuth().then(() => { gdrive.setupAutoSync(); });

  let unlistenFns: (() => void)[] = [];

  listen('vault-file-created-deleted', () => {
      if (noteAppRef.value) noteAppRef.value.scanVault();
      invoke('scan_tasks', { vaultPath: vaultPath.value }).catch(console.error);
      invoke('scan_events', { vaultPath: vaultPath.value }).catch(console.error);
      invoke('scan_quick_caps', { vaultPath: vaultPath.value }).catch(console.error);
      
      if (vaultType.value === 'gdrive' && gdrive.gdriveConnected.value && !gdrive.gdriveSyncing.value) {
          gdrive.syncGDrive();
      }
  }).then(fn => unlistenFns.push(fn));

  listen('vault-file-modified', () => {
      if (noteAppRef.value) noteAppRef.value.scanVault();
      invoke('scan_tasks', { vaultPath: vaultPath.value }).catch(console.error);
      invoke('scan_events', { vaultPath: vaultPath.value }).catch(console.error);
      invoke('scan_quick_caps', { vaultPath: vaultPath.value }).catch(console.error);
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
                  const fm = `---\ntitle: "${note.title}"\npinned: ${note.pinned}\ntags: [${note.tags.map((t: string) => `"${t}"`).join(', ')}]\n---`;
                  try {
                      await invoke('update_note', { vaultPath: vaultPath.value, path: note.id, content: `${fm}\n\n${nApp.tabContents[noteId]}` });
                      emit('note-updated', { id: note.id, content: nApp.tabContents[noteId] });
                  } catch(e) { console.error('Save before close failed', e); }
              }
          }
      }
  });
});

onUnmounted(() => {
  window.matchMedia('(prefers-color-scheme: dark)').removeEventListener('change', applyTheme);
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
                <div v-if="!useMobileLayout" class="w-8 h-px bg-border dark:bg-border-dark my-1 rounded"></div>
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

        <!-- MINI APP CONTENT AREA -->
        <template v-if="activeTool === 'note'">
          <NoteApp ref="noteAppRef" :vault-path="vaultPath" :is-floating-view="isFloatingView" :floating-note-id="floatingNoteId" />
        </template>
        <template v-else-if="activeTool === 'quickcap'">
           <QuickCap ref="quickCapAppRef" :vaultPath="vaultPath" />
        </template>
        <template v-else-if="activeTool === 'nexus'">
           <Nexus :vaultPath="vaultPath" @edit-item="handleEditFromNexus" />
        </template>
        <template v-else-if="activeTool === 'task'">
           <Tasks ref="taskAppRef" :vaultPath="vaultPath" />
        </template>
        <template v-else-if="activeTool === 'calendar'">
          <CalendarApp :vaultPath="vaultPath" />
        </template>
        <template v-else-if="activeTool === 'file'">
           <FileManager :vaultPath="vaultPath" />
        </template>

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