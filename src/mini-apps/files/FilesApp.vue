<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch, nextTick, inject } from 'vue';
import { convertFileSrc } from '@tauri-apps/api/core';
import { useVirtualList, useWindowSize } from '@vueuse/core';
import { Search, LayoutGrid, List, FolderSync, Menu, ArrowLeft, FileText, ImageIcon, Video, Music, Code, FileArchive, FileType, Info, Copy, X } from 'lucide-vue-next';
import { useFileStore, type FileMetadata, type FileReference } from './composables/useFileStore';
import { getViewer, getViewerType } from './composables/useViewerRegistry';
import FilesSidebar from './components/FilesSidebar.vue';
import NavButtons from '../../shared/components/NavButtons.vue';
import FilesTabs, { type FileTab } from './components/FilesTabs.vue';
import FilesInfoPanel from './components/FilesInfoPanel.vue';
import type { NavEntry } from '../../stores/useNavigationStore';
import { invoke } from '@tauri-apps/api/core';

// ─── People Autocomplete ─────────────────────────────────────
const getPersonName = (link: string) => {
  const match = link.match(/\[([^\]]*)\]/);
  return match ? match[1] : link;
};

interface PersonNode { id: string; title: string; }
const allPeople = ref<PersonNode[]>([]);
const searchPeopleQuery = ref('');
const showPeopleDropdown = ref(false);
const peopleInputRef = ref<HTMLInputElement | null>(null);

const fetchAllPeople = async () => {
  try {
    const nodes = await invoke<any[]>('get_nodes', { nodeType: 'person' });
    allPeople.value = nodes.map(n => ({ id: n.id, title: n.title }));
  } catch (e) {
    console.error('Failed to fetch people', e);
  }
};

const filteredPeople = computed(() => {
  const q = searchPeopleQuery.value.toLowerCase();
  return allPeople.value.filter(p => {
    if (!p.title.toLowerCase().includes(q)) return false;
    if (selectedFile.value?.people?.some(link => link.includes(p.id))) return false;
    return true;
  });
});

const handleSelectPerson = async (person: PersonNode) => {
  if (!selectedFile.value) return;
  const link = `[${person.title}](synabit://person/${person.id})`;
  await store.addPerson(selectedFile.value, link);
  searchPeopleQuery.value = '';
  showPeopleDropdown.value = false;
};

const handleRemovePerson = async (link: string) => {
  if (!selectedFile.value) return;
  await store.removePerson(selectedFile.value, link);
};

const props = defineProps<{ vaultPath: string }>();
const store = useFileStore(() => props.vaultPath);

// ─── Mode ────────────────────────────────────────────────────
const mode = ref<'browse' | 'focus' | 'duplicates'>('browse');
const isSidebarOpen = ref(false);
const viewMode = ref<'grid' | 'list'>('list');

// ─── Tabs (Focus mode) ──────────────────────────────────────
const openTabs = ref<FileTab[]>([]);
const activeTabId = ref<string | null>(null);

// ─── Intra-app navigation ──────────────────────────────────
const pushNavigation = inject<(entry?: NavEntry) => void>('pushNavigation');
let skipNavPush = false;

const activeTab = computed(() => openTabs.value.find(t => t.id === activeTabId.value) || null);
const activeViewer = computed(() => activeTab.value ? getViewer(activeTab.value.extension) : null);
const showInfoPanel = ref(false);
const activeFileMetadata = computed(() => {
  if (!activeTab.value) return null;
  return store.files.value.find(f => f.id === activeTab.value!.id) || null;
});

const openFileInFocus = (file: FileMetadata) => {
  if (activeTabId.value && activeTabId.value !== file.id && !skipNavPush) {
    pushNavigation?.({ app: 'file', itemId: activeTabId.value });
  }
  const existing = openTabs.value.find(t => t.id === file.id);
  if (existing) {
    activeTabId.value = existing.id;
  } else {
    const tab: FileTab = { id: file.id, filename: file.filename, extension: file.extension, path: file.path };
    openTabs.value.push(tab);
    activeTabId.value = tab.id;
  }
  mode.value = 'focus';
};

const closeTab = (id: string) => {
  const idx = openTabs.value.findIndex(t => t.id === id);
  if (idx === -1) return;
  openTabs.value.splice(idx, 1);
  if (activeTabId.value === id) {
    activeTabId.value = openTabs.value[Math.min(idx, openTabs.value.length - 1)]?.id || null;
    if (!activeTabId.value) mode.value = 'browse';
  }
};

const goBack = () => { mode.value = 'browse'; };

const showDuplicates = async () => {
  mode.value = 'duplicates';
  selectedFile.value = null;
  fileRefs.value = [];
  await store.scanDuplicates();
};

// ─── File References (for Duplicates) ────────────────────────
const fileRefs = ref<FileReference[]>([]);
const isLoadingRefs = ref(false);

const selectDupFile = async (file: FileMetadata) => {
  selectedFile.value = file;
};

const handleDeleteFile = async (file: FileMetadata) => {
  const deleted = await store.deleteFile(file);
  if (deleted) {
    selectedFile.value = null;
    fileRefs.value = [];
    if (mode.value === 'duplicates') {
      await store.scanDuplicates();
    }
  }
};

// ─── Browse mode ─────────────────────────────────────────────
const selectedFile = ref<FileMetadata | null>(null);
const isRenaming = ref(false);
const renameInput = ref('');
const renameInputRef = ref<HTMLInputElement | null>(null);

const startRename = async () => {
  if (isLoadingRefs.value || fileRefs.value.length > 0) return;
  if (!selectedFile.value) return;
  isRenaming.value = true;
  renameInput.value = selectedFile.value.filename;
  await nextTick();
  if (renameInputRef.value) {
    renameInputRef.value.focus();
    const extIdx = renameInput.value.lastIndexOf('.');
    if (extIdx > 0) {
      renameInputRef.value.setSelectionRange(0, extIdx);
    } else {
      renameInputRef.value.select();
    }
  }
};

const handleRename = async () => {
  if (!isRenaming.value || !selectedFile.value) return;
  const newName = renameInput.value.trim();
  if (newName && newName !== selectedFile.value.filename) {
    await store.saveFileName(selectedFile.value, newName);
  }
  isRenaming.value = false;
};

watch(selectedFile, async (newFile) => {
  isRenaming.value = false;
  if (newFile) {
    fileRefs.value = [];
    isLoadingRefs.value = true;
    try {
      fileRefs.value = await store.getFileReferences(newFile.filename);
    } catch (e) {
      console.error(e);
    } finally {
      isLoadingRefs.value = false;
    }
  } else {
    fileRefs.value = [];
  }
});
const isAddingTag = ref(false);
const newTagInput = ref('');
const tagInputRef = ref<HTMLInputElement | null>(null);

const startAddingTag = async () => {
  isAddingTag.value = true;
  await nextTick();
  tagInputRef.value?.focus();
};

const handleAddTag = async () => {
  if (!newTagInput.value.trim() || !selectedFile.value) return;
  await store.addTag(selectedFile.value, newTagInput.value.trim());
  newTagInput.value = '';
  isAddingTag.value = false;
};

const handleRemoveTag = async (tag: string) => {
  if (!selectedFile.value) return;
  await store.removeTag(selectedFile.value, tag);
};

const getFileIcon = (ext: string) => {
  const e = ext.toLowerCase();
  if (['jpg','jpeg','png','gif','webp','bmp','svg','heic'].includes(e)) return ImageIcon;
  if (['mp4','mkv','avi','mov','webm'].includes(e)) return Video;
  if (['mp3','wav','flac','ogg','m4a'].includes(e)) return Music;
  if (['pdf','doc','docx','txt','md'].includes(e)) return FileText;
  if (['zip','rar','7z','tar','gz'].includes(e)) return FileArchive;
  if (['js','ts','vue','json','html','css','rs','py'].includes(e)) return Code;
  return FileType;
};

const isPreviewable = (ext: string) => {
  const e = ext.toLowerCase();
  return ['jpg','jpeg','png','gif','svg','webp','bmp','heic','mp4','mov','webm','mp3','wav','ogg','m4a','pdf'].includes(e);
};

// ─── Virtual Lists ───────────────────────────────────────────
const { list: virtualListItems, containerProps, wrapperProps } = useVirtualList(
  computed(() => store.filteredFiles.value), { itemHeight: 57 }
);

const { width } = useWindowSize();
const gridCols = computed(() => { if (width.value >= 1536) return 5; if (width.value >= 1280) return 4; if (width.value >= 768) return 3; return 2; });
const gridRows = computed(() => {
  const r: FileMetadata[][] = []; const c = gridCols.value;
  for (let i = 0; i < store.filteredFiles.value.length; i += c) r.push(store.filteredFiles.value.slice(i, i + c));
  return r;
});
const { list: virtualGridRows, containerProps: gridContainerProps, wrapperProps: gridWrapperProps } = useVirtualList(gridRows, { itemHeight: 180 });

// ─── Public API ──────────────────────────────────────────────
const openFileById = (id: string, _skipNavPush = false) => {
  if (!_skipNavPush && activeTabId.value && activeTabId.value !== id && !skipNavPush) {
    pushNavigation?.({ app: 'file', itemId: activeTabId.value });
  }
  
  const tryOpen = () => {
    const file = store.files.value.find(f => f.id === id || f.path === id);
    if (file) {
      skipNavPush = true;
      openFileInFocus(file);
      skipNavPush = false;
    } else if (store.isLoading.value) {
      const unwatch = watch(() => store.isLoading.value, (loading) => {
        if (!loading) {
          unwatch();
          const f = store.files.value.find(f => f.id === id || f.path === id);
          if (f) {
            skipNavPush = true;
            openFileInFocus(f);
            skipNavPush = false;
          }
        }
      });
    }
  };
  
  tryOpen();
};

defineExpose({ openFileById, activeTabId });

// ─── Session Persistence ─────────────────────────────────────
const SESSION_KEY = 'files_app_state';

const saveSession = () => {
  try {
    sessionStorage.setItem(SESSION_KEY, JSON.stringify({
      mode: mode.value, activeTabId: activeTabId.value,
      openTabs: openTabs.value, viewMode: viewMode.value,
    }));
  } catch (_) {}
};

watch([mode, activeTabId, openTabs, viewMode], saveSession, { deep: true });

// Auto-switch to browse when sidebar filters change in non-browse modes
watch([store.activeSourceId, store.activeType, store.activeTag], () => {
  if (mode.value !== 'browse') {
    mode.value = 'browse';
    selectedFile.value = null;
  }
});

const restoreSession = () => {
  try {
    const raw = sessionStorage.getItem(SESSION_KEY);
    if (!raw) return;
    const s = JSON.parse(raw);
    if (s.viewMode) viewMode.value = s.viewMode;
    if (s.openTabs?.length) { openTabs.value = s.openTabs; activeTabId.value = s.activeTabId; mode.value = s.mode || 'browse'; }
  } catch (_) {}
};

// ─── Lifecycle ───────────────────────────────────────────────
let unlisten: (() => void) | null = null;
onMounted(async () => { 
  await store.init(); 
  unlisten = (await store.setupAuthListener()) as any; 
  restoreSession(); 
  fetchAllPeople();
});
onUnmounted(() => { if (unlisten) unlisten(); });
</script>

<template>
  <div class="h-full w-full flex relative bg-[#f5f5f7] dark:bg-[#0a0a0a] font-sans text-gray-900 dark:text-gray-100 overflow-hidden">

    <!-- Sidebar Overlay (mobile) -->
    <div v-if="isSidebarOpen" @click="isSidebarOpen = false" class="md:hidden absolute inset-0 bg-black/20 dark:bg-black/40 z-30" />

    <!-- Sidebar (browse + duplicates modes) -->
    <FilesSidebar v-if="mode === 'browse' || mode === 'duplicates'" :store="store" :isOpen="isSidebarOpen" @update:isOpen="isSidebarOpen = $event" @showDuplicates="showDuplicates" />

    <!-- ═══ BROWSE MODE ═══ -->
    <template v-if="mode === 'browse'">
      <div class="flex-1 flex flex-col relative z-10 min-w-0">
        <!-- Header -->
        <div class="h-14 px-4 md:px-8 flex items-center gap-3 justify-between border-b border-gray-200/50 dark:border-white/5 bg-white/30 dark:bg-black/20 backdrop-blur-md">
          <NavButtons />
          <button @click="isSidebarOpen = true" class="md:hidden p-2 -ml-2 rounded-xl hover:bg-gray-100 dark:hover:bg-white/5 text-gray-600 dark:text-gray-300 cursor-pointer"><Menu class="w-5 h-5" /></button>
          <div class="flex-1 max-w-xl relative group">
            <Search class="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-gray-400 group-focus-within:text-indigo-500" />
            <input v-model="store.searchQuery.value" :placeholder="$t('file.search_placeholder')" class="w-full pl-9 pr-10 py-2 bg-white/50 dark:bg-white/5 border border-gray-200/50 dark:border-white/10 rounded-xl text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500/50 text-gray-800 dark:text-gray-200 placeholder:text-gray-400" />
            <button v-if="store.searchQuery.value" @click="store.searchQuery.value = ''" class="absolute right-2 top-1/2 -translate-y-1/2 text-gray-400 hover:text-gray-600 dark:hover:text-gray-200 p-1 rounded-md hover:bg-gray-100 dark:hover:bg-white/10 cursor-pointer transition-colors">
              <X class="w-3.5 h-3.5" />
            </button>
          </div>
          <div class="flex items-center gap-1 bg-white/50 dark:bg-white/5 p-1 rounded-lg border border-gray-200/50 dark:border-white/10 flex-shrink-0">
            <button @click="viewMode = 'grid'" class="p-1.5 rounded-md transition-colors cursor-pointer" :class="viewMode === 'grid' ? 'bg-white dark:bg-white/10 shadow-sm text-indigo-500' : 'text-gray-400 hover:text-gray-600'"><LayoutGrid class="w-4 h-4" /></button>
            <button @click="viewMode = 'list'" class="p-1.5 rounded-md transition-colors cursor-pointer" :class="viewMode === 'list' ? 'bg-white dark:bg-white/10 shadow-sm text-indigo-500' : 'text-gray-400 hover:text-gray-600'"><List class="w-4 h-4" /></button>
          </div>
        </div>

        <!-- Scanning -->
        <div v-if="store.isScanning.value" class="w-full bg-indigo-500/10 text-indigo-600 dark:text-indigo-400 px-8 py-2.5 text-sm font-medium flex items-center gap-3">
          <FolderSync class="w-4 h-4 animate-spin" /> Scanning and indexing files...
        </div>

        <!-- File List -->
        <div class="flex-1 overflow-y-auto p-4 md:p-6">
          <div v-if="store.isLoading.value" class="flex justify-center py-20"><div class="w-8 h-8 border-2 border-indigo-500 border-t-transparent rounded-full animate-spin" /></div>
          <div v-else-if="store.filteredFiles.value.length === 0" class="flex flex-col items-center justify-center h-full text-gray-400">
            <FileArchive class="w-16 h-16 mb-4 opacity-20" /><p class="text-lg font-medium text-gray-500">{{ $t('file.no_files') }}</p>
          </div>

          <!-- Grid View -->
          <div v-else-if="viewMode === 'grid'" v-bind="gridContainerProps" class="h-full overflow-y-auto">
            <div v-bind="gridWrapperProps">
              <div v-for="{ index, data: row } in virtualGridRows" :key="index" class="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-3 mb-3">
                <div v-for="file in row" :key="file.id"
                  @click="selectedFile = file" @dblclick="openFileInFocus(file)"
                  class="group bg-white/60 dark:bg-white/[0.03] border border-gray-200/50 dark:border-white/5 rounded-2xl p-4 cursor-pointer hover:bg-white dark:hover:bg-white/10 transition-all hover:shadow-xl hover:-translate-y-1"
                  :class="{'ring-2 ring-indigo-500 border-transparent': selectedFile?.id === file.id}">
                  <div class="aspect-square rounded-xl bg-gray-100/50 dark:bg-black/20 mb-3 flex items-center justify-center overflow-hidden">
                    <img v-if="isPreviewable(file.extension) && ['jpg','jpeg','png','gif','webp','svg','bmp'].includes(file.extension.toLowerCase())" :src="convertFileSrc(file.path)" class="w-full h-full object-cover" loading="lazy" />
                    <component v-else :is="getFileIcon(file.extension)" class="w-10 h-10 text-gray-400 dark:text-gray-500 group-hover:text-indigo-500 transition-colors" />
                  </div>
                  <h4 class="file-name text-sm font-bold truncate mb-1">{{ file.filename }}</h4>
                  <div class="flex items-center justify-between text-xs text-gray-500"><span>{{ file.extension.toUpperCase() }}</span><span>{{ store.formatSize(file.size) }}</span></div>
                </div>
              </div>
            </div>
          </div>

          <!-- List View -->
          <div v-else class="bg-white/60 dark:bg-white/[0.03] border border-gray-200/50 dark:border-white/5 rounded-2xl overflow-hidden shadow-sm flex flex-col h-full">
            <div class="hidden md:grid grid-cols-[2fr_1fr_1fr_2fr] gap-4 px-6 py-3 bg-gray-50/50 dark:bg-black/20 file-meta font-medium border-b border-gray-200/50 dark:border-white/5 text-sm">
              <div>Name</div><div>Size</div><div>Modified</div><div>{{ $t('file.tags') }}</div>
            </div>
            <div v-bind="containerProps" class="flex-1 overflow-y-auto">
              <div v-bind="wrapperProps">
                <div v-for="{ data: file } in virtualListItems" :key="file.id"
                  @click="selectedFile = file" @dblclick="openFileInFocus(file)"
                  class="flex flex-col md:grid md:grid-cols-[2fr_1fr_1fr_2fr] gap-1 md:gap-4 px-4 md:px-6 py-3 hover:bg-white dark:hover:bg-white/5 cursor-pointer transition-colors border-b border-gray-100/50 dark:border-white/5 text-sm"
                  :class="{'bg-indigo-50/50 dark:bg-indigo-500/10': selectedFile?.id === file.id}">
                  <div class="flex items-center gap-3 overflow-hidden">
                    <component :is="getFileIcon(file.extension)" class="w-5 h-5 flex-shrink-0 text-indigo-500" />
                    <span class="file-name font-medium truncate">{{ file.filename }}</span>
                  </div>
                  <div class="flex items-center gap-3 md:contents text-xs md:text-sm pl-8 md:pl-0">
                    <div class="file-meta truncate font-mono md:font-sans">{{ store.formatSize(file.size) }}</div>
                    <div class="file-meta truncate">{{ file.modified_at.split(' ')[0] }}</div>
                    <div class="flex gap-1 overflow-hidden ml-auto md:ml-0">
                      <span v-for="t in file.tags.slice(0,2)" :key="t" class="file-tag px-1.5 py-0.5 bg-gray-100 dark:bg-white/10 rounded text-xs truncate">#{{ t }}</span>
                      <span v-if="file.tags.length > 2" class="file-tag px-1.5 py-0.5 bg-gray-100 dark:bg-white/10 rounded text-xs">+{{ file.tags.length - 2 }}</span>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- Detail Panel (Browse) -->
      <div v-if="selectedFile" class="w-80 xl:w-96 flex-shrink-0 bg-white/70 dark:bg-white/[0.03] backdrop-blur-2xl border-l border-gray-200/50 dark:border-white/5 flex flex-col z-20">
        <div class="h-14 px-5 flex items-center justify-between border-b border-gray-200/50 dark:border-white/5">
          <h2 class="font-bold text-sm text-gray-900 dark:text-white">{{ $t('file.details') }}</h2>
          <button @click="selectedFile = null" class="p-1.5 hover:bg-gray-100 dark:hover:bg-white/10 rounded-full text-gray-500 cursor-pointer"><svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/></svg></button>
        </div>
        <div class="flex-1 overflow-y-auto p-5 space-y-5">
          <!-- Preview -->
          <div class="aspect-square rounded-2xl bg-gradient-to-br from-indigo-50 to-purple-50 dark:from-indigo-500/10 dark:to-purple-500/10 border border-indigo-100 dark:border-indigo-500/20 flex items-center justify-center overflow-hidden">
            <img v-if="['jpg','jpeg','png','gif','svg','webp','bmp','heic'].includes(selectedFile.extension.toLowerCase())" :src="convertFileSrc(selectedFile.path)" class="w-full h-full object-contain" />
            <video v-else-if="['mp4','mov','webm','mkv'].includes(selectedFile.extension.toLowerCase())" :src="convertFileSrc(selectedFile.path)" controls class="w-full h-full object-contain bg-black/5" />
            <audio v-else-if="['mp3','wav','ogg','m4a'].includes(selectedFile.extension.toLowerCase())" :src="convertFileSrc(selectedFile.path)" controls class="w-full px-4" />
            <component v-else :is="getFileIcon(selectedFile.extension)" class="w-20 h-20 text-indigo-500/50" />
          </div>
          <!-- Name -->
          <input v-if="isRenaming" ref="renameInputRef" v-model="renameInput" @blur="handleRename" @keydown.enter="handleRename" @keydown.esc="isRenaming = false" class="w-full font-extrabold text-lg break-words leading-tight text-gray-900 dark:text-white bg-transparent border-b-2 border-indigo-500 focus:outline-none" />
          <h3 v-else @click="startRename" class="font-extrabold text-lg break-words leading-tight text-gray-900 dark:text-white" :class="(isLoadingRefs || fileRefs.length > 0) ? '' : 'cursor-text hover:underline decoration-dashed decoration-gray-400 underline-offset-4'" :title="(isLoadingRefs || fileRefs.length > 0) ? $t('file.cannot_rename') : $t('file.click_to_rename')">{{ selectedFile.filename }}</h3>
          <!-- Metadata -->
          <div class="p-4 rounded-xl bg-gray-50/50 dark:bg-black/20 border border-gray-100 dark:border-white/5 space-y-2 text-sm">
            <div class="flex justify-between"><span class="text-gray-500">Type</span><span class="font-medium uppercase text-gray-900 dark:text-white">{{ selectedFile.extension }}</span></div>
            <div class="flex justify-between"><span class="text-gray-500">Size</span><span class="font-medium text-gray-900 dark:text-white">{{ store.formatSize(selectedFile.size) }}</span></div>
            <div class="flex justify-between"><span class="text-gray-500">Modified</span><span class="font-medium text-gray-900 dark:text-white">{{ selectedFile.modified_at.split(' ')[0] }}</span></div>
          </div>
          <!-- Tags -->
          <div>
            <h4 class="text-xs font-bold text-gray-400 uppercase tracking-wider mb-2">{{ $t('file.tags') }}</h4>
            <div class="flex flex-wrap items-center gap-1.5">
              <span v-for="tag in selectedFile.tags" :key="tag" class="group relative px-2.5 py-1 bg-indigo-50 dark:bg-indigo-500/10 text-indigo-600 dark:text-indigo-400 rounded-lg text-xs font-medium border border-indigo-100 dark:border-indigo-500/20 flex items-center gap-1">
                #{{ tag }}
                <button @click="handleRemoveTag(tag)" class="opacity-0 group-hover:opacity-100 hover:text-red-500 transition-opacity cursor-pointer">
                  <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/></svg>
                </button>
              </span>
              <input v-if="isAddingTag" ref="tagInputRef" v-model="newTagInput" @keydown.enter="handleAddTag" @keydown.esc="isAddingTag = false; newTagInput = ''" @blur="handleAddTag"
                type="text" :placeholder="$t('file.tag_placeholder')"
                class="px-2 py-1 bg-white dark:bg-black/40 border border-indigo-300 dark:border-indigo-500/50 rounded-lg text-xs font-medium focus:outline-none w-20" />
              <button v-else @click="startAddingTag" class="px-2.5 py-1 bg-white dark:bg-white/5 border border-dashed border-gray-300 dark:border-gray-600 rounded-lg text-xs font-medium text-gray-400 hover:text-indigo-500 hover:border-indigo-300 cursor-pointer transition-colors">
                + Add
              </button>
            </div>
          </div>
          <!-- Linked People -->
          <div>
            <h4 class="text-xs font-bold text-gray-400 uppercase tracking-wider mb-2">{{ $t('file.people') }}</h4>
            <div class="flex flex-wrap items-center gap-1.5 mb-2">
              <span v-for="link in (selectedFile.people || [])" :key="link" class="group relative px-2.5 py-1 bg-emerald-50 dark:bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 rounded-lg text-xs font-medium border border-emerald-100 dark:border-emerald-500/20 flex items-center gap-1">
                @{{ getPersonName(link) }}
                <button @click="handleRemovePerson(link)" class="opacity-0 group-hover:opacity-100 hover:text-red-500 transition-opacity cursor-pointer">
                  <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/></svg>
                </button>
              </span>
            </div>
            <div class="relative">
              <input 
                v-if="showPeopleDropdown"
                ref="peopleInputRef"
                v-model="searchPeopleQuery"
                type="text" 
                :placeholder="$t('file.search_person_placeholder')"
                class="w-full px-2 py-1.5 bg-white dark:bg-black/40 border border-emerald-300 dark:border-emerald-500/50 rounded-lg text-xs font-medium focus:outline-none"
                @blur="setTimeout(() => showPeopleDropdown = false, 150)"
              />
              <button v-else @click="() => { showPeopleDropdown = true; nextTick(() => peopleInputRef?.focus()) }" class="px-2.5 py-1 bg-white dark:bg-white/5 border border-dashed border-gray-300 dark:border-gray-600 rounded-lg text-xs font-medium text-gray-400 hover:text-emerald-500 hover:border-emerald-300 cursor-pointer transition-colors">
                + Link Person
              </button>
              
              <!-- Dropdown -->
              <div v-if="showPeopleDropdown && filteredPeople.length > 0" class="absolute z-10 w-full mt-1 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl shadow-lg max-h-40 overflow-y-auto">
                <button
                  v-for="person in filteredPeople"
                  :key="person.id"
                  @click="handleSelectPerson(person)"
                  class="w-full text-left px-3 py-2 text-xs font-medium hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-700 dark:text-gray-300 cursor-pointer"
                >
                  {{ person.title }}
                </button>
              </div>
            </div>
          </div>
          <!-- Used by -->
          <div>
            <h4 class="text-xs font-bold text-gray-400 uppercase tracking-wider mb-2">{{ $t('file.used_by') }}</h4>
            <div v-if="isLoadingRefs" class="text-xs text-gray-400">{{ $t('file.checking_refs') }}</div>
            <div v-else-if="fileRefs.length === 0" class="flex items-center gap-2 p-3 rounded-xl bg-green-50 dark:bg-green-500/10 border border-green-200 dark:border-green-500/20">
              <svg class="w-4 h-4 text-green-500 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"/></svg>
              <span class="text-xs font-medium text-green-600 dark:text-green-400">{{ $t('file.not_used') }}</span>
            </div>
            <div v-else class="space-y-1.5">
              <div class="flex items-center gap-2 p-2 rounded-lg bg-red-50 dark:bg-red-500/10 border border-red-200 dark:border-red-500/20 mb-2">
                <svg class="w-3.5 h-3.5 text-red-500 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L4.082 16.5c-.77.833.192 2.5 1.732 2.5z"/></svg>
                <span class="text-[10px] font-bold text-red-600 dark:text-red-400">Referenced by {{ fileRefs.length }} node(s)</span>
              </div>
              <div v-for="ref_ in fileRefs" :key="ref_.node_id" class="flex items-center gap-2 px-3 py-2 bg-white dark:bg-black/30 rounded-lg border border-gray-200/50 dark:border-white/5">
                <span class="px-1.5 py-0.5 bg-indigo-100 dark:bg-indigo-500/20 text-indigo-600 dark:text-indigo-400 rounded text-[9px] font-bold uppercase flex-shrink-0">{{ ref_.node_type }}</span>
                <span class="text-xs text-gray-700 dark:text-gray-300 truncate">{{ ref_.title || 'Untitled' }}</span>
              </div>
            </div>
          </div>
        </div>
        <!-- Action -->
        <div class="p-5 border-t border-gray-200/50 dark:border-white/5 space-y-2">
          <button @click="openFileInFocus(selectedFile!)" class="w-full py-2.5 rounded-xl bg-gray-900 dark:bg-white text-white dark:text-gray-900 font-bold text-sm shadow-xl hover:scale-[1.02] active:scale-[0.98] transition-all cursor-pointer">
            Open File
          </button>
          <button
            @click="handleDeleteFile(selectedFile!)"
            :disabled="isLoadingRefs || fileRefs.length > 0"
            class="w-full py-2.5 rounded-xl font-bold text-sm transition-all cursor-pointer"
            :class="isLoadingRefs || fileRefs.length > 0
              ? 'bg-gray-100 dark:bg-white/5 text-gray-400 dark:text-gray-600 cursor-not-allowed'
              : 'bg-red-50 dark:bg-red-500/10 text-red-500 hover:bg-red-100 dark:hover:bg-red-500/20 border border-red-200 dark:border-red-500/20'">
            {{ fileRefs.length > 0 ? 'In use — cannot delete' : 'Delete File' }}
          </button>
        </div>
      </div>
    </template>

    <!-- ═══ FOCUS MODE ═══ -->
    <template v-if="mode === 'focus'">
      <div class="flex-1 flex flex-col min-w-0">
        <!-- Back + Tabs + Info toggle -->
        <div class="flex items-center bg-[#f5f5f7] dark:bg-[#0f0f0f] border-b border-gray-200/50 dark:border-white/5">
          <button @click="goBack" class="p-2.5 hover:bg-gray-200 dark:hover:bg-white/10 text-gray-600 dark:text-gray-300 cursor-pointer flex-shrink-0 mx-1" :title="$t('file.back_to_browse')">
            <ArrowLeft class="w-4 h-4" />
          </button>
          <FilesTabs :tabs="openTabs" :activeTabId="activeTabId" @select="activeTabId = $event" @close="closeTab" class="flex-1 min-w-0" />
          <button @click="showInfoPanel = !showInfoPanel" class="p-2.5 hover:bg-gray-200 dark:hover:bg-white/10 cursor-pointer flex-shrink-0 mx-1 rounded-lg transition-colors"
            :class="showInfoPanel ? 'text-indigo-500' : 'text-gray-400'" :title="$t('file.toggle_info_panel')">
            <Info class="w-4 h-4" />
          </button>
        </div>

        <!-- Viewer + Info Panel -->
        <div v-if="activeTab" class="flex-1 flex overflow-hidden">
          <component
            :is="activeViewer!"
            :fileId="activeTab.id"
            :filePath="activeTab.path"
            :vaultPath="vaultPath"
            :key="activeTab.id"
            class="flex-1 min-w-0"
          />
          <FilesInfoPanel
            v-if="showInfoPanel && activeFileMetadata"
            :file="activeFileMetadata"
            :store="store"
            @close="showInfoPanel = false"
          />
        </div>
        <div v-else class="flex-1 flex items-center justify-center text-gray-400">
          <p class="text-sm">{{ $t('file.no_file_open') }}</p>
        </div>
      </div>
    </template>

    <!-- ═══ DUPLICATES MODE ═══ -->
    <template v-if="mode === 'duplicates'">
      <div class="flex-1 flex flex-col min-w-0">
        <!-- Header -->
        <div class="h-14 px-4 md:px-8 flex items-center gap-3 border-b border-gray-200/50 dark:border-white/5 bg-white/30 dark:bg-black/20 backdrop-blur-md">
          <Copy class="w-5 h-5 text-amber-500" />
          <h2 class="font-bold text-sm text-gray-900 dark:text-white">{{ $t('file.duplicate_finder') }}</h2>
          <button @click="store.scanDuplicates()" :disabled="store.isScanningDuplicates.value"
            class="ml-auto px-3 py-1.5 rounded-lg bg-amber-50 dark:bg-amber-500/10 text-amber-600 dark:text-amber-400 text-xs font-semibold hover:bg-amber-100 dark:hover:bg-amber-500/20 transition-colors cursor-pointer disabled:opacity-50">
            <FolderSync v-if="store.isScanningDuplicates.value" class="w-3.5 h-3.5 animate-spin inline mr-1" />
            Rescan
          </button>
        </div>

        <!-- Loading -->
        <div v-if="store.isScanningDuplicates.value" class="flex-1 flex items-center justify-center">
          <div class="text-center">
            <div class="w-10 h-10 border-2 border-amber-500 border-t-transparent rounded-full animate-spin mx-auto mb-3" />
            <p class="text-sm text-gray-500">{{ $t('file.scanning_duplicates') }}</p>
          </div>
        </div>

        <!-- No duplicates -->
        <div v-else-if="!store.duplicateReport.value || store.duplicateReport.value.total_groups === 0" class="flex-1 flex items-center justify-center">
          <div class="text-center">
            <Copy class="w-16 h-16 text-gray-300 dark:text-gray-600 mx-auto mb-4" />
            <p class="text-lg font-bold text-gray-500 mb-1">{{ $t('file.no_duplicates') }}</p>
            <p class="text-sm text-gray-400">{{ $t('file.all_unique') }}</p>
          </div>
        </div>

        <!-- Results -->
        <template v-else>
          <!-- Stats Banner -->
          <div class="px-6 py-4 bg-amber-50/50 dark:bg-amber-500/5 border-b border-amber-100 dark:border-amber-500/10">
            <div class="grid grid-cols-3 gap-4 max-w-lg">
              <div class="text-center">
                <p class="text-2xl font-extrabold text-amber-600 dark:text-amber-400">{{ store.duplicateReport.value.total_groups }}</p>
                <p class="text-[10px] font-bold text-gray-400 uppercase tracking-wider">{{ $t('file.groups') }}</p>
              </div>
              <div class="text-center">
                <p class="text-2xl font-extrabold text-amber-600 dark:text-amber-400">{{ store.duplicateReport.value.total_duplicate_files }}</p>
                <p class="text-[10px] font-bold text-gray-400 uppercase tracking-wider">{{ $t('file.extra_files') }}</p>
              </div>
              <div class="text-center">
                <p class="text-2xl font-extrabold text-amber-600 dark:text-amber-400">{{ store.formatSize(store.duplicateReport.value.total_wasted_bytes) }}</p>
                <p class="text-[10px] font-bold text-gray-400 uppercase tracking-wider">{{ $t('file.wasted') }}</p>
              </div>
            </div>
          </div>

          <!-- Groups + Detail Panel -->
          <div class="flex-1 flex overflow-hidden">
            <!-- Groups List -->
            <div class="flex-1 overflow-y-auto p-4 md:p-6 space-y-3 min-w-0">
              <div v-for="(group, gi) in store.duplicateReport.value.groups" :key="gi"
                class="bg-white/60 dark:bg-white/[0.03] border border-gray-200/50 dark:border-white/5 rounded-2xl overflow-hidden">
                <!-- Group Header -->
                <div class="px-5 py-3 flex items-center gap-3 bg-gray-50/50 dark:bg-black/20 border-b border-gray-100 dark:border-white/5">
                  <component :is="getFileIcon(group.extension)" class="w-5 h-5 text-amber-500 flex-shrink-0" />
                  <div class="flex-1 min-w-0">
                    <h4 class="font-bold text-sm text-gray-900 dark:text-white truncate">{{ group.filename }}</h4>
                    <p class="text-xs text-gray-400">{{ group.count }} copies · {{ store.formatSize(group.size) }} each · {{ store.formatSize(group.wasted_bytes) }} wasted</p>
                  </div>
                  <span class="px-2 py-0.5 bg-amber-100 dark:bg-amber-500/20 text-amber-600 dark:text-amber-400 rounded-md text-xs font-bold flex-shrink-0">×{{ group.count }}</span>
                </div>
                <!-- Files in Group -->
                <div class="divide-y divide-gray-100 dark:divide-white/5">
                  <div v-for="file in group.files" :key="file.id"
                    @click="selectDupFile(file)"
                    @dblclick="openFileInFocus(file)"
                    class="px-5 py-2.5 flex items-center gap-3 cursor-pointer transition-colors text-sm"
                    :class="selectedFile?.id === file.id ? 'bg-indigo-50 dark:bg-indigo-500/10' : 'hover:bg-gray-50 dark:hover:bg-white/5'">
                    <p class="flex-1 text-xs font-mono text-gray-500 truncate">{{ file.path }}</p>
                    <span class="text-[10px] text-gray-400 flex-shrink-0">{{ file.modified_at.split(' ')[0] }}</span>
                  </div>
                </div>
              </div>
            </div>

            <!-- Detail Panel (Duplicates) -->
            <div v-if="selectedFile" class="w-80 xl:w-96 flex-shrink-0 bg-white/70 dark:bg-white/[0.03] backdrop-blur-2xl border-l border-gray-200/50 dark:border-white/5 flex flex-col">
              <div class="h-14 px-5 flex items-center justify-between border-b border-gray-200/50 dark:border-white/5">
                <h2 class="font-bold text-sm text-gray-900 dark:text-white">{{ $t('file.preview') }}</h2>
                <button @click="selectedFile = null" class="p-1.5 hover:bg-gray-100 dark:hover:bg-white/10 rounded-full text-gray-500 cursor-pointer"><svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/></svg></button>
              </div>
              <div class="flex-1 overflow-y-auto p-5 space-y-5">
                <!-- Preview -->
                <div class="aspect-square rounded-2xl bg-gradient-to-br from-indigo-50 to-purple-50 dark:from-indigo-500/10 dark:to-purple-500/10 border border-indigo-100 dark:border-indigo-500/20 flex items-center justify-center overflow-hidden">
                  <img v-if="['jpg','jpeg','png','gif','svg','webp','bmp','heic'].includes(selectedFile.extension.toLowerCase())" :src="convertFileSrc(selectedFile.path)" class="w-full h-full object-contain" />
                  <video v-else-if="['mp4','mov','webm','mkv'].includes(selectedFile.extension.toLowerCase())" :src="convertFileSrc(selectedFile.path)" controls class="w-full h-full object-contain bg-black/5" />
                  <audio v-else-if="['mp3','wav','ogg','m4a'].includes(selectedFile.extension.toLowerCase())" :src="convertFileSrc(selectedFile.path)" controls class="w-full px-4" />
                  <component v-else :is="getFileIcon(selectedFile.extension)" class="w-20 h-20 text-indigo-500/50" />
                </div>
                <input v-if="isRenaming" ref="renameInputRef" v-model="renameInput" @blur="handleRename" @keydown.enter="handleRename" @keydown.esc="isRenaming = false" class="w-full font-extrabold text-lg break-words leading-tight text-gray-900 dark:text-white bg-transparent border-b-2 border-indigo-500 focus:outline-none" />
                <h3 v-else @click="startRename" class="font-extrabold text-lg break-words leading-tight text-gray-900 dark:text-white" :class="(isLoadingRefs || fileRefs.length > 0) ? '' : 'cursor-text hover:underline decoration-dashed decoration-gray-400 underline-offset-4'" :title="(isLoadingRefs || fileRefs.length > 0) ? $t('file.cannot_rename') : $t('file.click_to_rename')">{{ selectedFile.filename }}</h3>
                <!-- Metadata -->
                <div class="p-4 rounded-xl bg-gray-50/50 dark:bg-black/20 border border-gray-100 dark:border-white/5 space-y-2 text-sm">
                  <div class="flex justify-between"><span class="text-gray-500">Type</span><span class="font-medium uppercase text-gray-900 dark:text-white">{{ selectedFile.extension }}</span></div>
                  <div class="flex justify-between"><span class="text-gray-500">Size</span><span class="font-medium text-gray-900 dark:text-white">{{ store.formatSize(selectedFile.size) }}</span></div>
                  <div class="flex justify-between"><span class="text-gray-500">Modified</span><span class="font-medium text-gray-900 dark:text-white">{{ selectedFile.modified_at.split(' ')[0] }}</span></div>
                </div>
                <!-- Path -->
                <div>
                  <h4 class="text-xs font-bold text-gray-400 uppercase tracking-wider mb-1">{{ $t('file.location') }}</h4>
                  <p class="text-[10px] font-mono text-gray-500 break-all p-2 bg-white dark:bg-black/40 rounded-lg border border-gray-200/50 dark:border-white/5">{{ selectedFile.path }}</p>
                </div>
                <!-- Linked People -->
                <div>
                  <h4 class="text-xs font-bold text-gray-400 uppercase tracking-wider mb-2">{{ $t('file.people') }}</h4>
                  <div class="flex flex-wrap items-center gap-1.5 mb-2">
                    <span v-for="link in (selectedFile.people || [])" :key="link" class="group relative px-2.5 py-1 bg-emerald-50 dark:bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 rounded-lg text-xs font-medium border border-emerald-100 dark:border-emerald-500/20 flex items-center gap-1">
                      @{{ getPersonName(link) }}
                      <button @click="handleRemovePerson(link)" class="opacity-0 group-hover:opacity-100 hover:text-red-500 transition-opacity cursor-pointer">
                        <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/></svg>
                      </button>
                    </span>
                  </div>
                  <div class="relative">
                    <input 
                      v-if="showPeopleDropdown"
                      ref="peopleInputRef"
                      v-model="searchPeopleQuery"
                      type="text" 
                      :placeholder="$t('file.search_person_placeholder')"
                      class="w-full px-2 py-1.5 bg-white dark:bg-black/40 border border-emerald-300 dark:border-emerald-500/50 rounded-lg text-xs font-medium focus:outline-none"
                      @blur="setTimeout(() => showPeopleDropdown = false, 150)"
                    />
                    <button v-else @click="() => { showPeopleDropdown = true; nextTick(() => peopleInputRef?.focus()) }" class="px-2.5 py-1 bg-white dark:bg-white/5 border border-dashed border-gray-300 dark:border-gray-600 rounded-lg text-xs font-medium text-gray-400 hover:text-emerald-500 hover:border-emerald-300 cursor-pointer transition-colors">
                      + Link Person
                    </button>
                    
                    <!-- Dropdown -->
                    <div v-if="showPeopleDropdown && filteredPeople.length > 0" class="absolute z-10 w-full mt-1 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl shadow-lg max-h-40 overflow-y-auto">
                      <button
                        v-for="person in filteredPeople"
                        :key="person.id"
                        @click="handleSelectPerson(person)"
                        class="w-full text-left px-3 py-2 text-xs font-medium hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-700 dark:text-gray-300 cursor-pointer"
                      >
                        {{ person.title }}
                      </button>
                    </div>
                  </div>
                </div>
                <!-- Used by -->
                <div>
                  <h4 class="text-xs font-bold text-gray-400 uppercase tracking-wider mb-2">{{ $t('file.used_by') }}</h4>
                  <div v-if="isLoadingRefs" class="text-xs text-gray-400">{{ $t('file.checking_refs') }}</div>
                  <div v-else-if="fileRefs.length === 0" class="flex items-center gap-2 p-3 rounded-xl bg-green-50 dark:bg-green-500/10 border border-green-200 dark:border-green-500/20">
                    <svg class="w-4 h-4 text-green-500 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"/></svg>
                    <span class="text-xs font-medium text-green-600 dark:text-green-400">{{ $t('file.safe_to_delete') }}</span>
                  </div>
                  <div v-else class="space-y-1.5">
                    <div class="flex items-center gap-2 p-2 rounded-lg bg-red-50 dark:bg-red-500/10 border border-red-200 dark:border-red-500/20 mb-2">
                      <svg class="w-3.5 h-3.5 text-red-500 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L4.082 16.5c-.77.833.192 2.5 1.732 2.5z"/></svg>
                      <span class="text-[10px] font-bold text-red-600 dark:text-red-400">Referenced by {{ fileRefs.length }} node(s)</span>
                    </div>
                    <div v-for="ref_ in fileRefs" :key="ref_.node_id" class="flex items-center gap-2 px-3 py-2 bg-white dark:bg-black/30 rounded-lg border border-gray-200/50 dark:border-white/5">
                      <span class="px-1.5 py-0.5 bg-indigo-100 dark:bg-indigo-500/20 text-indigo-600 dark:text-indigo-400 rounded text-[9px] font-bold uppercase flex-shrink-0">{{ ref_.node_type }}</span>
                      <span class="text-xs text-gray-700 dark:text-gray-300 truncate">{{ ref_.title || 'Untitled' }}</span>
                    </div>
                  </div>
                </div>
              </div>
              <div class="p-5 border-t border-gray-200/50 dark:border-white/5 space-y-2">
                <button @click="openFileInFocus(selectedFile!)" class="w-full py-2.5 rounded-xl bg-gray-900 dark:bg-white text-white dark:text-gray-900 font-bold text-sm shadow-xl hover:scale-[1.02] active:scale-[0.98] transition-all cursor-pointer">
                  Open File
                </button>
                <button
                  @click="handleDeleteFile(selectedFile!)"
                  :disabled="isLoadingRefs || fileRefs.length > 0"
                  class="w-full py-2.5 rounded-xl font-bold text-sm transition-all cursor-pointer"
                  :class="isLoadingRefs || fileRefs.length > 0
                    ? 'bg-gray-100 dark:bg-white/5 text-gray-400 dark:text-gray-600 cursor-not-allowed'
                    : 'bg-red-50 dark:bg-red-500/10 text-red-500 hover:bg-red-100 dark:hover:bg-red-500/20 border border-red-200 dark:border-red-500/20'">
                  {{ fileRefs.length > 0 ? 'In use — cannot delete' : 'Delete File' }}
                </button>
              </div>
            </div>
          </div>
        </template>
      </div>
    </template>
  </div>
</template>

<style scoped>
.file-name { color: #1c1c1e; }
html.dark .file-name { color: #f4f4f5; }
.file-tag { color: #52525b; }
html.dark .file-tag { color: #d4d4d8; }
.file-meta { color: #6b7280; }
html.dark .file-meta { color: #9ca3af; }
.scrollbar-none::-webkit-scrollbar { display: none; }
</style>
