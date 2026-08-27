<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch, nextTick, inject } from 'vue';
import { useI18n } from 'vue-i18n';
import { convertFileSrc } from '@tauri-apps/api/core';
import { useVirtualList, useWindowSize } from '@vueuse/core';
import { Search, LayoutGrid, List, FolderSync, Menu, ArrowLeft, FileText, ImageIcon, Video, Music, Code, FileArchive, FileType, Info, Copy, X, ArrowDownUp } from 'lucide-vue-next';
import { useFileStore, type FileMetadata, type FileReference } from './composables/useFileStore';
import { useFileThumbnails } from './composables/useFileThumbnails';
import { getViewer } from './composables/useViewerRegistry';
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
const thumbs = useFileThumbnails(() => props.vaultPath);

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
  return store.loadedFiles.value.find(f => f.id === activeTab.value!.id) || null;
});

const openFileInFocus = (file: FileMetadata, page?: number) => {
  if (activeTabId.value && activeTabId.value !== file.id && !skipNavPush) {
    pushNavigation?.({ app: 'file', itemId: activeTabId.value });
  }
  const existing = openTabs.value.find(t => t.id === file.id);
  if (existing) {
    existing.page = page;
    activeTabId.value = existing.id;
  } else {
    const tab: FileTab = { id: file.id, filename: file.filename, extension: file.extension, path: file.path, page };
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
      fileRefs.value = await store.getFileReferences(newFile.id);
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

const handlePeopleDropdownBlur = () => window.setTimeout(() => showPeopleDropdown.value = false, 150);

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

/**
 * A click on a file, with whatever the keyboard was doing at the time.
 *
 * Plain click selects one and opens the detail panel, which is what the app
 * did before. The modifiers are the new part, and they are the difference
 * between tagging four hundred holiday photos and tagging one.
 */
const onPick = (file: FileMetadata, event: MouseEvent) => {
  const toggle = event.metaKey || event.ctrlKey;
  const range = event.shiftKey;
  store.selectFile(file, { toggle, range });
  // The panel describes one file, so it steps aside once there are several.
  selectedFile.value = store.selection.value.size === 1 ? file : null;
};

const bulkTagInput = ref('');
const isBulkTagging = ref(false);
const bulkTagRef = ref<HTMLInputElement | null>(null);

const startBulkTag = async () => {
  isBulkTagging.value = true;
  await nextTick();
  bulkTagRef.value?.focus();
};

const applyBulkTag = async () => {
  const tag = bulkTagInput.value.trim();
  if (!tag) { isBulkTagging.value = false; return; }
  await store.tagSelection([tag]);
  bulkTagInput.value = '';
  isBulkTagging.value = false;
};

const { t } = useI18n();
const sortLabels = computed<Record<string, string>>(() => ({
  modified: t('file.sort_modified'),
  name: t('file.sort_name'),
  size: t('file.sort_size'),
  shot: t('file.sort_shot'),
  pixels: t('file.sort_pixels'),
}));

// A change of filter is a change of what is on screen, so a selection made
// against the old one no longer means anything.
watch([store.activeSourceId, store.activeType, store.activeTag, store.activeCamera, store.searchQuery], () => {
  store.clearSelection();
});

/**
 * How large a grid cell is, and therefore how many fit across.
 *
 * A photo library is browsed at different distances — small for finding a shape
 * you remember, large for judging a picture — which is why every asset manager
 * has this slider and why the fixed four-across grid felt like looking through
 * a letterbox.
 */
const cellScale = ref(1);
const CELL_BASE = 180;

// Declared above the lists on purpose: the watcher over the visible rows runs
// with `immediate: true`, so it reads `gridCols` — and through it `cellScale` —
// while the component is still being set up. Left further down, that read hits
// the temporal dead zone and the app fails to open with "Cannot access
// 'cellScale' before initialization".
// ─── Virtual Lists ───────────────────────────────────────────
// The lists are as long as the filtered set; most of their entries are `null`
// until the reader scrolls near them. See the store's list section.
const { list: virtualListItems, containerProps, wrapperProps } = useVirtualList(
  computed(() => store.rows.value), { itemHeight: 57 }
);

const { width } = useWindowSize();
const cellHeight = computed(() => Math.round(CELL_BASE * cellScale.value));
const gridCols = computed(() => {
  let base: number;
  if (width.value >= 1536) base = 5;
  else if (width.value >= 1280) base = 4;
  else if (width.value >= 768) base = 3;
  else base = 2;
  // Bigger cells mean fewer across, and never fewer than one.
  return Math.max(1, Math.round(base / cellScale.value));
});
const gridRows = computed(() => {
  const r: (FileMetadata | null)[][] = []; const c = gridCols.value;
  const all = store.rows.value;
  for (let i = 0; i < all.length; i += c) r.push(all.slice(i, i + c));
  return r;
});
const { list: virtualGridRows, containerProps: gridContainerProps, wrapperProps: gridWrapperProps } =
  useVirtualList(gridRows, { itemHeight: () => cellHeight.value });

// Build thumbnails for the cells actually on screen.
//
// Driven from the virtual list rather than the `<img>`'s load event, because
// the load event only fires once the full-size original has been decoded —
// exactly the cost the thumbnail exists to avoid. Watching the visible slice
// asks for the small copy while the cell is being laid out instead.
//
// The first look at a folder still paints originals; there is no thumbnail to
// show until one has been made, and making one means reading the file. Every
// look after that is served from `assets/.thumbs`.
watch(virtualGridRows, (visible) => {
  if (viewMode.value !== 'grid') return;
  const cols = gridCols.value;
  if (visible.length > 0) {
    // Rows of cells, so the row index has to become a file index.
    store.ensureLoaded(visible[0].index * cols, (visible[visible.length - 1].index + 1) * cols);
  }
  for (const row of visible) {
    for (const file of row.data) {
      if (file) thumbs.ensure(file);
    }
  }
}, { immediate: true });

// The list view has no thumbnails to build, but the same rows to fetch.
watch(virtualListItems, (visible) => {
  if (viewMode.value !== 'list' || visible.length === 0) return;
  store.ensureLoaded(visible[0].index, visible[visible.length - 1].index);
}, { immediate: true });

// ─── Public API ──────────────────────────────────────────────
/**
 * Open a file, landing on the page that matched when one is known.
 *
 * `query` is the search that produced this hit. A document is stored a page at
 * a time, so a four-hundred-page manual can open where the phrase actually is
 * rather than at page one — which is most of the difference between full-text
 * search being useful and being a list of filenames.
 */
const openFileById = async (id: string, _skipNavPush = false, query?: string) => {
  if (!_skipNavPush && activeTabId.value && activeTabId.value !== id && !skipNavPush) {
    pushNavigation?.({ app: 'file', itemId: activeTabId.value });
  }

  let page: number | undefined;
  if (query) {
    try {
      page = (await invoke<number | null>('find_text_page', { nodeId: id, query })) ?? undefined;
    } catch (e) {
      // A missing page number costs the reader a scroll, nothing more.
      console.error('Failed to locate the matching page', e);
    }
  }

  // Asked of the database rather than of the list: most of the library has
  // never been fetched, so searching what is loaded would usually miss.
  const file = await store.findFile(id);
  if (!file) return;
  skipNavPush = true;
  openFileInFocus(file, page);
  skipNavPush = false;
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

/**
 * Files dropped onto the window.
 *
 * Plain DOM events, not Tauri's drag-drop event. The window sets
 * `dragDropEnabled: false`, so the webview keeps the OS drop and Tauri is never
 * told about it — an earlier attempt through `onDragDropEvent` was inert for
 * exactly that reason. The same setting is what lets a file be dragged *out* of
 * this list into a note, so it is the right one to keep.
 */
const isDropTarget = ref(false);

/** A drag that carries no OS files is our own, on its way to a note. */
const carriesFiles = (event: DragEvent) =>
  Array.from(event.dataTransfer?.types ?? []).includes('Files');

const onDragOver = (event: DragEvent) => {
  if (!carriesFiles(event)) return;
  event.preventDefault();
  isDropTarget.value = true;
};

const onDragLeave = (event: DragEvent) => {
  // Only when the pointer has actually left the window, not merely crossed
  // from one cell to the next.
  if (event.relatedTarget === null) isDropTarget.value = false;
};

const onDrop = async (event: DragEvent) => {
  if (!carriesFiles(event)) return;
  event.preventDefault();
  isDropTarget.value = false;
  await store.importDroppedFiles(Array.from(event.dataTransfer?.files ?? []));
};


/** Nothing indexed at all, as opposed to nothing matching a filter. */
/** Nothing indexed at all, as opposed to nothing matching a filter. */
const hasFilters = computed(() =>
  !!(store.activeSourceId.value || store.activeType.value || store.activeTag.value
     || store.activeCamera.value || store.searchQuery.value)
);
const hasNothingIndexed = computed(() => store.total.value === 0 && !hasFilters.value);

const clearFilters = () => {
  store.activeSourceId.value = null;
  store.activeType.value = null;
  store.activeTag.value = null;
  store.activeCamera.value = null;
  store.searchQuery.value = '';
};

/**
 * The muscle memory every file browser shares.
 *
 * Arrows move, Enter opens, Escape clears, Space peeks without committing —
 * the last one being the reason Quick Look exists on macOS at all: judging a
 * picture should not cost opening and closing a window.
 *
 * Deliberately inert while a field has focus, or typing a tag would scroll the
 * list out from under the person typing it.
 */
const quickLookFile = ref<FileMetadata | null>(null);

const isTyping = (target: EventTarget | null) => {
  const el = target as HTMLElement | null;
  if (!el) return false;
  return el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.isContentEditable;
};

const step = (delta: number) => {
  const visible = store.loadedFiles.value;
  if (visible.length === 0) return;
  const at = visible.findIndex(f => f.path === selectedFile.value?.path);
  const next = visible[Math.min(Math.max(at + delta, 0), visible.length - 1)] ?? visible[0];
  store.selectFile(next);
  selectedFile.value = next;
  if (quickLookFile.value) quickLookFile.value = next;
};

const onKeydown = (event: KeyboardEvent) => {
  if (mode.value !== 'browse' || isTyping(event.target)) return;

  if (event.key === 'Escape') {
    if (menu.value) closeMenu();
    else if (quickLookFile.value) quickLookFile.value = null;
    else if (store.selection.value.size > 0) store.clearSelection();
    return;
  }
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'a') {
    event.preventDefault();
    store.selectAllMatching();
    return;
  }
  // One row in a list, one row of cells in a grid.
  const stride = viewMode.value === 'grid' ? gridCols.value : 1;
  switch (event.key) {
    case 'ArrowDown': event.preventDefault(); step(stride); break;
    case 'ArrowUp': event.preventDefault(); step(-stride); break;
    case 'ArrowRight': if (viewMode.value === 'grid') { event.preventDefault(); step(1); } break;
    case 'ArrowLeft': if (viewMode.value === 'grid') { event.preventDefault(); step(-1); } break;
    case 'Enter':
      if (selectedFile.value) { event.preventDefault(); openFileInFocus(selectedFile.value); }
      break;
    case ' ':
      if (selectedFile.value) {
        event.preventDefault();
        quickLookFile.value = quickLookFile.value ? null : selectedFile.value;
      }
      break;
  }
};

/**
 * The menu a right-click opens.
 *
 * Everything in it was already possible and none of it was reachable from the
 * file itself: revealing meant opening the info panel, labelling did not exist,
 * and deleting was a button somewhere else. A file manager is operated from the
 * thing being operated on.
 */
const menu = ref<{ x: number; y: number; file: FileMetadata } | null>(null);

const openMenu = (file: FileMetadata, event: MouseEvent) => {
  event.preventDefault();
  // Right-clicking outside the selection acts on what was clicked, which is
  // what every file manager does and what stops a stray click retagging forty
  // photos.
  if (!store.isSelected(file)) {
    store.selectFile(file);
    selectedFile.value = file;
  }
  // Kept inside the window: a menu opened near the bottom edge would otherwise
  // hang off it.
  menu.value = {
    x: Math.min(event.clientX, window.innerWidth - 220),
    y: Math.min(event.clientY, window.innerHeight - 300),
    file,
  };
};

const closeMenu = () => { menu.value = null; };

const copyPath = (path: string) => navigator.clipboard.writeText(path).catch(() => {});

const runFromMenu = async (action: () => unknown | Promise<unknown>) => {
  closeMenu();
  await action();
};

const labelColours: Record<string, string> = {
  red: '#e5484d', orange: '#f76b15', yellow: '#f0c000',
  green: '#46a758', blue: '#0091ff', purple: '#8e4ec6',
};

const isSavingCollection = ref(false);
const collectionName = ref('');
const collectionRef = ref<HTMLInputElement | null>(null);

const startSavingCollection = async () => {
  isSavingCollection.value = true;
  await nextTick();
  collectionRef.value?.focus();
};

const saveCollection = async () => {
  const name = collectionName.value.trim();
  isSavingCollection.value = false;
  collectionName.value = '';
  if (name) await store.saveCollection(name);
};

/**
 * Hand a file to whatever it is dropped on.
 *
 * A drag inside the window carries no OS file — there is nothing for the OS to
 * hand over — so what travels is a description of something already indexed.
 * The note editor reads it in `TiptapEditor.vue`'s `handleDrop` and points at
 * the file rather than copying it.
 *
 * `assetPath` is set only for files in the vault's own assets folder, because
 * that is the only form a note can carry to another device.
 */
const onDragFile = (file: FileMetadata, event: DragEvent) => {
  const marker = `${separator()}assets${separator()}`;
  const at = file.path.lastIndexOf(marker);
  const assetPath = at === -1 ? null : `assets/${file.path.slice(at + marker.length)}`;

  event.dataTransfer?.setData('application/x-synabit-file', JSON.stringify({
    filename: file.filename,
    extension: file.extension,
    assetPath,
    absPath: file.path,
  }));
  if (event.dataTransfer) event.dataTransfer.effectAllowed = 'copy';
};

const separator = () => (props.vaultPath.includes('\\') ? '\\' : '/');

// ─── Lifecycle ───────────────────────────────────────────────
onMounted(async () => {
  await store.init();
  restoreSession();
  fetchAllPeople();
  thumbs.load();
  window.addEventListener('keydown', onKeydown);
});
onUnmounted(() => {
  store.dispose();
  window.removeEventListener('keydown', onKeydown);
});
</script>

<template>
  <div class="h-full w-full flex relative bg-[#f5f5f7] dark:bg-[#0a0a0a] font-sans text-gray-900 dark:text-gray-100 overflow-hidden"
    @dragover="onDragOver" @dragleave="onDragLeave" @drop="onDrop">

    <!-- Context menu -->
    <div v-if="menu" class="fixed inset-0 z-[70]" @click="closeMenu" @contextmenu.prevent="closeMenu">
      <div class="absolute w-52 py-1.5 rounded-xl bg-white dark:bg-[#1c1c1e] border border-gray-200 dark:border-white/10 shadow-2xl text-sm"
        :style="{ left: `${menu.x}px`, top: `${menu.y}px` }" @click.stop>
        <button @click="runFromMenu(() => openFileInFocus(menu!.file))" class="w-full text-left px-3 py-1.5 hover:bg-gray-100 dark:hover:bg-white/10 cursor-pointer">{{ $t('file.open') }}</button>
        <button @click="runFromMenu(() => store.openLocalFile(menu!.file.path))" class="w-full text-left px-3 py-1.5 hover:bg-gray-100 dark:hover:bg-white/10 cursor-pointer">{{ $t('file.open_externally') }}</button>
        <button @click="runFromMenu(() => store.revealFile(menu!.file.path))" class="w-full text-left px-3 py-1.5 hover:bg-gray-100 dark:hover:bg-white/10 cursor-pointer">{{ $t('file.reveal') }}</button>
        <button @click="runFromMenu(() => copyPath(menu!.file.path))" class="w-full text-left px-3 py-1.5 hover:bg-gray-100 dark:hover:bg-white/10 cursor-pointer">{{ $t('file.copy_path') }}</button>

        <div class="my-1 border-t border-gray-100 dark:border-white/5" />
        <p class="px-3 py-1 text-[10px] font-bold uppercase tracking-wider text-gray-400">{{ $t('file.label') }}</p>
        <div class="px-3 py-1.5 flex items-center gap-2">
          <button v-for="colour in store.LABELS" :key="colour"
            @click="runFromMenu(() => store.labelSelection(colour))"
            class="w-5 h-5 rounded-full border border-black/10 hover:scale-125 transition-transform cursor-pointer"
            :style="{ background: labelColours[colour] }" :title="$t(`file.label_${colour}`)" />
          <button @click="runFromMenu(() => store.labelSelection(null))"
            class="w-5 h-5 rounded-full border border-dashed border-gray-400 hover:scale-125 transition-transform cursor-pointer"
            :title="$t('file.no_label')" />
        </div>

        <div class="my-1 border-t border-gray-100 dark:border-white/5" />
        <button @click="runFromMenu(() => handleDeleteFile(menu!.file))" class="w-full text-left px-3 py-1.5 text-red-600 dark:text-red-400 hover:bg-red-50 dark:hover:bg-red-500/10 cursor-pointer">{{ $t('file.delete') }}</button>
      </div>
    </div>

    <!-- Quick look -->
    <div v-if="quickLookFile" @click="quickLookFile = null"
      class="absolute inset-0 z-[60] bg-black/60 backdrop-blur-sm flex flex-col items-center justify-center p-8 gap-4 cursor-zoom-out">
      <div class="max-w-full max-h-[75%] flex items-center justify-center" @click.stop>
        <img v-if="['jpg','jpeg','png','gif','svg','webp','bmp','avif'].includes(quickLookFile.extension.toLowerCase())"
          :src="convertFileSrc(thumbs.pathFor(quickLookFile))" class="max-w-full max-h-full object-contain rounded-xl shadow-2xl" />
        <video v-else-if="['mp4','mov','webm','mkv'].includes(quickLookFile.extension.toLowerCase())"
          :src="convertFileSrc(quickLookFile.path)" controls autoplay class="max-w-full max-h-full rounded-xl shadow-2xl" />
        <div v-else class="w-40 h-40 rounded-3xl bg-white/10 flex items-center justify-center">
          <component :is="getFileIcon(quickLookFile.extension)" class="w-16 h-16 text-white/70" />
        </div>
      </div>
      <div class="text-center text-white/90 max-w-lg" @click.stop>
        <p class="font-semibold truncate">{{ quickLookFile.filename }}</p>
        <p class="text-xs text-white/50 mt-1">
          {{ quickLookFile.extension.toUpperCase() }} · {{ store.formatSize(quickLookFile.size) }}
          <span v-if="quickLookFile.camera"> · {{ quickLookFile.camera }}</span>
        </p>
      </div>
    </div>

    <!-- Drop target -->
    <div v-if="isDropTarget" class="absolute inset-4 z-50 rounded-3xl border-2 border-dashed border-indigo-400 bg-indigo-500/10 backdrop-blur-sm flex items-center justify-center pointer-events-none">
      <p class="text-lg font-bold text-indigo-600 dark:text-indigo-300">{{ $t('file.drop_to_import') }}</p>
    </div>


    <!-- Sidebar Overlay (mobile) -->
    <div v-if="isSidebarOpen" @click="isSidebarOpen = false" class="md:hidden absolute inset-0 bg-black/20 dark:bg-black/40 z-30" />

    <!-- Sidebar (browse + duplicates modes) -->
    <FilesSidebar v-if="mode === 'browse' || mode === 'duplicates'" :store="store" :isOpen="isSidebarOpen" @update:isOpen="isSidebarOpen = $event" @showDuplicates="showDuplicates" @saveCollection="startSavingCollection" />

    <!-- Naming a collection -->
    <div v-if="isSavingCollection" class="absolute inset-0 z-[65] bg-black/30 flex items-start justify-center pt-32" @click="isSavingCollection = false">
      <input ref="collectionRef" v-model="collectionName" @click.stop
        @keydown.enter="saveCollection" @keydown.esc="isSavingCollection = false; collectionName = ''"
        :placeholder="$t('file.collection_name')"
        class="w-80 px-4 py-3 rounded-2xl bg-white dark:bg-[#1c1c1e] border border-gray-200 dark:border-white/10 shadow-2xl text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500/50 text-gray-900 dark:text-gray-100" />
    </div>

    <!-- ═══ BROWSE MODE ═══ -->
    <template v-if="mode === 'browse'">
      <div class="flex-1 flex flex-col relative z-10 min-w-0">
        <!-- Header -->
        <div class="h-14 px-4 md:px-8 flex items-center gap-3 justify-between border-b border-gray-200/50 dark:border-white/5 bg-white/30 dark:bg-black/20 backdrop-blur-md">
          <NavButtons />
          <button @click="isSidebarOpen = true" class="md:hidden p-2 -ml-2 rounded-xl hover:bg-gray-100 dark:hover:bg-white/5 text-gray-600 dark:text-gray-300 cursor-pointer" :aria-label="$t('file.toggle_sidebar')"><Menu class="w-5 h-5" /></button>
          <div class="flex-1 max-w-xl relative group">
            <Search class="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-gray-400 group-focus-within:text-indigo-500" />
            <input v-model="store.searchQuery.value" :placeholder="$t('file.search_placeholder')" class="w-full pl-9 pr-10 py-2 bg-white/50 dark:bg-white/5 border border-gray-200/50 dark:border-white/10 rounded-xl text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500/50 text-gray-800 dark:text-gray-200 placeholder:text-gray-400" />
            <button v-if="store.searchQuery.value" @click="store.searchQuery.value = ''" class="absolute right-2 top-1/2 -translate-y-1/2 text-gray-400 hover:text-gray-600 dark:hover:text-gray-200 p-1 rounded-md hover:bg-gray-100 dark:hover:bg-white/10 cursor-pointer transition-colors" :aria-label="$t('file.clear_search')">
              <X class="w-3.5 h-3.5" />
            </button>
          </div>
          <input v-if="viewMode === 'grid'" v-model.number="cellScale" type="range" min="0.6" max="2" step="0.2"
            class="hidden lg:block w-20 flex-shrink-0 accent-indigo-500 cursor-pointer" :aria-label="$t('file.cell_size')" :title="$t('file.cell_size')" />
          <select v-model="store.sortBy.value" class="flex-shrink-0 bg-white/50 dark:bg-white/5 border border-gray-200/50 dark:border-white/10 rounded-lg text-xs px-2 py-1.5 text-gray-600 dark:text-gray-300 cursor-pointer focus:outline-none focus:ring-2 focus:ring-indigo-500/50" :aria-label="$t('file.sort_by')">
            <option v-for="(label, key) in sortLabels" :key="key" :value="key">{{ label }}</option>
          </select>
          <button @click="store.sortDescending.value = !store.sortDescending.value"
            class="flex-shrink-0 p-1.5 rounded-lg bg-white/50 dark:bg-white/5 border border-gray-200/50 dark:border-white/10 text-gray-500 hover:text-indigo-500 cursor-pointer transition-colors"
            :aria-label="$t(store.sortDescending.value ? 'file.sort_descending' : 'file.sort_ascending')"
            :title="$t(store.sortDescending.value ? 'file.sort_descending' : 'file.sort_ascending')">
            <ArrowDownUp class="w-4 h-4" />
          </button>
          <div class="flex items-center gap-1 bg-white/50 dark:bg-white/5 p-1 rounded-lg border border-gray-200/50 dark:border-white/10 flex-shrink-0">
            <button @click="viewMode = 'grid'" class="p-1.5 rounded-md transition-colors cursor-pointer" :class="viewMode === 'grid' ? 'bg-white dark:bg-white/10 shadow-sm text-indigo-500' : 'text-gray-400 hover:text-gray-600'" :aria-label="$t('file.grid_view')"><LayoutGrid class="w-4 h-4" /></button>
            <button @click="viewMode = 'list'" class="p-1.5 rounded-md transition-colors cursor-pointer" :class="viewMode === 'list' ? 'bg-white dark:bg-white/10 shadow-sm text-indigo-500' : 'text-gray-400 hover:text-gray-600'" :aria-label="$t('file.list_view')"><List class="w-4 h-4" /></button>
          </div>
        </div>

        <!-- Selection -->
        <div v-if="store.selectionSize.value > 1" class="w-full bg-indigo-500/10 text-indigo-700 dark:text-indigo-300 px-4 md:px-8 py-2.5 text-sm flex items-center gap-3 flex-wrap">
          <span class="font-semibold">{{ $t('file.selected_count', { count: store.selectionSize.value.toLocaleString() }) }}</span>

          <input v-if="isBulkTagging" ref="bulkTagRef" v-model="bulkTagInput"
            @keydown.enter="applyBulkTag" @keydown.esc="isBulkTagging = false; bulkTagInput = ''" @blur="applyBulkTag"
            :placeholder="$t('file.tag_input_placeholder')"
            class="px-2 py-1 rounded-lg bg-white dark:bg-white/10 border border-indigo-300 dark:border-indigo-500/40 text-xs focus:outline-none focus:ring-2 focus:ring-indigo-500/50 text-gray-800 dark:text-gray-100" />
          <button v-else @click="startBulkTag" class="px-3 py-1 rounded-lg bg-white dark:bg-white/10 hover:bg-indigo-50 dark:hover:bg-white/20 text-xs font-semibold cursor-pointer transition-colors">
            {{ $t('file.tag_selection') }}
          </button>

          <button @click="store.selectAllMatching" class="text-xs font-medium hover:underline cursor-pointer">{{ $t('file.select_all') }}</button>
          <button @click="store.clearSelection" class="ml-auto text-xs font-medium hover:underline cursor-pointer">{{ $t('file.deselect') }}</button>
        </div>

        <!-- Scanning -->
        <div v-if="store.isScanning.value" class="w-full bg-indigo-500/10 text-indigo-600 dark:text-indigo-400 px-8 py-2.5 text-sm font-medium flex items-center gap-3">
          <FolderSync class="w-4 h-4 animate-spin" /> {{ $t('file.scanning') }}
        </div>

        <!-- File List -->
        <div class="flex-1 overflow-y-auto p-4 md:p-6">
          <div v-if="store.isLoading.value" class="flex justify-center py-20"><div class="w-8 h-8 border-2 border-indigo-500 border-t-transparent rounded-full animate-spin" /></div>
          <div v-else-if="store.total.value === 0" class="flex flex-col items-center justify-center h-full text-center px-6">
            <FileArchive class="w-16 h-16 mb-4 opacity-20 text-gray-400" />
            <!-- An empty library and an empty filter are different problems,
                 and telling somebody "no files" when they have thousands is
                 not an answer to either. -->
            <template v-if="hasNothingIndexed">
              <p class="text-lg font-semibold text-gray-600 dark:text-gray-300 mb-1">{{ $t('file.empty_title') }}</p>
              <p class="text-sm text-gray-400 max-w-sm mb-5">{{ $t('file.empty_body') }}</p>
              <div class="flex items-center gap-2 flex-wrap justify-center">
                <button @click="store.addNewSource" class="px-4 py-2 rounded-xl bg-indigo-500 text-white text-sm font-semibold hover:bg-indigo-600 cursor-pointer transition-colors">{{ $t('file.add_folder') }}</button>
                <button @click="store.importFiles" class="px-4 py-2 rounded-xl bg-gray-100 dark:bg-white/10 text-gray-700 dark:text-gray-200 text-sm font-semibold hover:bg-gray-200 dark:hover:bg-white/20 cursor-pointer transition-colors">{{ $t('file.import_files') }}</button>
              </div>
            </template>
            <template v-else>
              <p class="text-lg font-medium text-gray-500">{{ $t('file.no_files') }}</p>
              <button @click="clearFilters" class="mt-3 text-sm text-indigo-500 hover:underline cursor-pointer">{{ $t('file.all_files') }}</button>
            </template>
          </div>

          <!-- Grid View -->
          <div v-else-if="viewMode === 'grid'" v-bind="gridContainerProps" class="h-full overflow-y-auto">
            <div v-bind="gridWrapperProps">
              <div v-for="{ index, data: row } in virtualGridRows" :key="index" class="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 gap-3 mb-3">
                <template v-for="(file, cell) in row" :key="file?.path ?? `pending-${cell}`">
                <div v-if="!file" class="rounded-2xl border border-gray-200/40 dark:border-white/5 bg-white/30 dark:bg-white/[0.02] animate-pulse" />
                <div v-else
                  @click="onPick(file, $event)" @dblclick="openFileInFocus(file)" @contextmenu="openMenu(file, $event)"
                  draggable="true" @dragstart="onDragFile(file, $event)"
                  class="group bg-white/60 dark:bg-white/[0.03] border border-gray-200/50 dark:border-white/5 rounded-2xl p-4 cursor-pointer hover:bg-white dark:hover:bg-white/10 transition-all hover:shadow-xl hover:-translate-y-1"
                  :class="{'ring-2 ring-indigo-500 border-transparent': store.isSelected(file)}">
                  <div class="rounded-xl bg-gray-100/50 dark:bg-black/20 mb-3 flex items-center justify-center overflow-hidden"
                    :style="{ height: `${Math.round(cellHeight * 0.62)}px` }">
                    <img v-if="isPreviewable(file.extension) && ['jpg','jpeg','png','gif','webp','svg','bmp'].includes(file.extension.toLowerCase())" :src="convertFileSrc(thumbs.pathFor(file))" class="w-full h-full object-cover" loading="lazy" />
                    <component v-else :is="getFileIcon(file.extension)" class="w-10 h-10 text-gray-400 dark:text-gray-500 group-hover:text-indigo-500 transition-colors" />
                  </div>
                  <div class="flex items-center gap-1.5 mb-1">
                    <span v-if="file.label" class="w-2 h-2 rounded-full flex-shrink-0" :style="{ background: labelColours[file.label] }" />
                    <h4 class="file-name text-sm font-bold truncate">{{ file.filename }}</h4>
                  </div>
                  <div class="flex items-center justify-between text-xs text-gray-500"><span>{{ file.extension.toUpperCase() }}</span><span>{{ store.formatSize(file.size) }}</span></div>
                </div>
                </template>
              </div>
            </div>
          </div>

          <!-- List View -->
          <div v-else class="bg-white/60 dark:bg-white/[0.03] border border-gray-200/50 dark:border-white/5 rounded-2xl overflow-hidden shadow-sm flex flex-col h-full">
            <div class="hidden md:grid grid-cols-[2fr_1fr_1fr_2fr] gap-4 px-6 py-3 bg-gray-50/50 dark:bg-black/20 file-meta font-medium border-b border-gray-200/50 dark:border-white/5 text-sm">
              <div>{{ $t('file.name_col') }}</div><div>{{ $t('file.size_col') }}</div><div>{{ $t('file.modified_col') }}</div><div>{{ $t('file.tags') }}</div>
            </div>
            <div v-bind="containerProps" class="flex-1 overflow-y-auto">
              <div v-bind="wrapperProps">
                <template v-for="{ data: file, index } in virtualListItems" :key="file?.path ?? `pending-${index}`">
                <div v-if="!file" class="h-[57px] border-b border-gray-100/50 dark:border-white/5 flex items-center px-6">
                  <div class="h-3 w-1/3 rounded bg-gray-200/60 dark:bg-white/5 animate-pulse" />
                </div>
                <div v-else
                  @click="onPick(file, $event)" @dblclick="openFileInFocus(file)" @contextmenu="openMenu(file, $event)"
                  draggable="true" @dragstart="onDragFile(file, $event)"
                  class="flex flex-col md:grid md:grid-cols-[2fr_1fr_1fr_2fr] gap-1 md:gap-4 px-4 md:px-6 py-3 hover:bg-white dark:hover:bg-white/5 cursor-pointer transition-colors border-b border-gray-100/50 dark:border-white/5 text-sm"
                  :class="{'bg-indigo-50/50 dark:bg-indigo-500/10': store.isSelected(file)}">
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
                </template>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- Detail Panel (Browse) -->
      <div v-if="selectedFile && store.selection.value.size <= 1" class="absolute md:relative inset-0 md:inset-auto z-40 md:z-20 w-full md:w-80 xl:w-96 flex-shrink-0 bg-white md:bg-white/70 dark:bg-[#0a0a0a] md:dark:bg-white/[0.03] backdrop-blur-2xl md:border-l border-gray-200/50 dark:border-white/5 flex flex-col">
        <div class="h-14 px-5 flex items-center justify-between border-b border-gray-200/50 dark:border-white/5">
          <h2 class="font-bold text-sm text-gray-900 dark:text-white">{{ $t('file.details') }}</h2>
          <button @click="selectedFile = null" class="p-1.5 hover:bg-gray-100 dark:hover:bg-white/10 rounded-full text-gray-500 cursor-pointer" :aria-label="$t('file.close_panel')"><svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/></svg></button>
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
            <div class="flex justify-between"><span class="text-gray-500">{{ $t('file.type') }}</span><span class="font-medium uppercase text-gray-900 dark:text-white">{{ selectedFile.extension }}</span></div>
            <div class="flex justify-between"><span class="text-gray-500">{{ $t('file.size_col') }}</span><span class="font-medium text-gray-900 dark:text-white">{{ store.formatSize(selectedFile.size) }}</span></div>
            <div class="flex justify-between"><span class="text-gray-500">{{ $t('file.modified_col') }}</span><span class="font-medium text-gray-900 dark:text-white">{{ selectedFile.modified_at.split(' ')[0] }}</span></div>
          </div>
          <!-- Tags -->
          <div>
            <h4 class="text-xs font-bold text-gray-400 uppercase tracking-wider mb-2">{{ $t('file.tags') }}</h4>
            <div class="flex flex-wrap items-center gap-1.5">
              <span v-for="tag in selectedFile.tags" :key="tag" class="group relative px-2.5 py-1 bg-indigo-50 dark:bg-indigo-500/10 text-indigo-600 dark:text-indigo-400 rounded-lg text-xs font-medium border border-indigo-100 dark:border-indigo-500/20 flex items-center gap-1">
                #{{ tag }}
                <button @click="handleRemoveTag(tag)" class="opacity-0 group-hover:opacity-100 hover:text-red-500 transition-opacity cursor-pointer" :aria-label="$t('file.remove_tag')">
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
                <button @click="handleRemovePerson(link)" class="opacity-0 group-hover:opacity-100 hover:text-red-500 transition-opacity cursor-pointer" :aria-label="$t('file.remove_person')">
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
                @blur="handlePeopleDropdownBlur"
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
            {{ $t('file.open_file') }}
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
            v-bind="activeTab.extension.toLowerCase() === 'pdf' ? { initialPage: activeTab.page } : {}"
            :key="activeTab.id"
            class="flex-1 min-w-0"
          />
          <FilesInfoPanel
            v-if="showInfoPanel && activeFileMetadata"
            :file="activeFileMetadata"
            :store="store"
            :vault-path="vaultPath"
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
            {{ $t('file.rescan') }}
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
                  <div v-for="file in group.files" :key="file.path"
                    @click="selectDupFile(file)"
                    @dblclick="openFileInFocus(file)"
                    class="px-5 py-2.5 flex items-center gap-3 cursor-pointer transition-colors text-sm"
                    :class="selectedFile?.path === file.path ? 'bg-indigo-50 dark:bg-indigo-500/10' : 'hover:bg-gray-50 dark:hover:bg-white/5'">
                    <p class="flex-1 text-xs font-mono text-gray-500 truncate">{{ file.path }}</p>
                    <span class="text-[10px] text-gray-400 flex-shrink-0">{{ file.modified_at.split(' ')[0] }}</span>
                  </div>
                </div>
              </div>
            </div>

            <!-- Detail Panel (Duplicates) -->
            <div v-if="selectedFile" class="absolute md:relative inset-0 md:inset-auto z-40 w-full md:w-80 xl:w-96 flex-shrink-0 bg-white md:bg-white/70 dark:bg-[#0a0a0a] md:dark:bg-white/[0.03] backdrop-blur-2xl md:border-l border-gray-200/50 dark:border-white/5 flex flex-col">
              <div class="h-14 px-5 flex items-center justify-between border-b border-gray-200/50 dark:border-white/5">
                <h2 class="font-bold text-sm text-gray-900 dark:text-white">{{ $t('file.preview') }}</h2>
                <button @click="selectedFile = null" class="p-1.5 hover:bg-gray-100 dark:hover:bg-white/10 rounded-full text-gray-500 cursor-pointer" :aria-label="$t('file.close_panel')"><svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"/></svg></button>
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
                  <div class="flex justify-between"><span class="text-gray-500">{{ $t('file.type') }}</span><span class="font-medium uppercase text-gray-900 dark:text-white">{{ selectedFile.extension }}</span></div>
                  <div class="flex justify-between"><span class="text-gray-500">{{ $t('file.size_col') }}</span><span class="font-medium text-gray-900 dark:text-white">{{ store.formatSize(selectedFile.size) }}</span></div>
                  <div class="flex justify-between"><span class="text-gray-500">{{ $t('file.modified_col') }}</span><span class="font-medium text-gray-900 dark:text-white">{{ selectedFile.modified_at.split(' ')[0] }}</span></div>
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
                      <button @click="handleRemovePerson(link)" class="opacity-0 group-hover:opacity-100 hover:text-red-500 transition-opacity cursor-pointer" :aria-label="$t('file.remove_person')">
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
                      @blur="handlePeopleDropdownBlur"
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
                  {{ $t('file.open_file') }}
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
