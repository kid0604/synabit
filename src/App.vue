<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue';
import { FileText, Search, PanelLeft, PanelLeftClose, PanelRight, PanelRightClose, Hash, FolderOpen, Plus, MoreVertical, Pin, Trash2, Edit2, X, Calendar, CheckSquare, Zap, Globe, ArrowLeft, ExternalLink, Sun, Cloud, RefreshCw, CloudOff, CaseSensitive, Settings } from 'lucide-vue-next';
import { invoke } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { open, confirm } from '@tauri-apps/plugin-dialog';

// Components
import TiptapEditor from './components/TiptapEditor.vue';
import QuickCap from './components/QuickCap.vue';
import Tasks from './components/Tasks.vue';
import CalendarApp from './components/CalendarApp.vue';
import Nexus from './components/Nexus.vue';
import FileManager from './components/FileManager.vue';
import NoteGraph from './components/NoteGraph.vue';
import SettingsModal from './components/SettingsModal.vue';

// Composables
import { useSettings } from './composables/useSettings';
import { useGDrive } from './composables/useGDrive';

// Types
import type { NoteMetadata } from './types/ipc';

// ─── Settings ─────────────────────────────────────────────
const {
  showSettingsModal, openSettings, applyTheme,
  enableDailyNotes, dailyNoteFormat, dailyNoteTag, isValidDailyFormat,
} = useSettings();

// ─── Vault & Data State ───────────────────────────────────
const vaultPath = ref<string>(localStorage.getItem('synabitVaultPath') || '');
const vaultType = ref<'local' | 'gdrive'>(localStorage.getItem('synabitVaultType') as any || 'local');
const notes = ref<NoteMetadata[]>([]);
const currentNoteId = ref<string | null>(null);

// ─── App View State ───────────────────────────────────────
const activeTool = ref<'nexus' | 'quickcap' | 'note' | 'task' | 'calendar' | 'file'>('nexus');

// ─── Tab / Content Management ─────────────────────────────
const activeTabs = ref<string[]>([]);
const tabContents = ref<Record<string, string>>({});
const tabAccessTime = new Map<string, number>();

const currentContent = computed({
   get: () => currentNoteId.value ? tabContents.value[currentNoteId.value] || '' : '',
   set: (val) => {
       if (currentNoteId.value) tabContents.value[currentNoteId.value] = val;
   }
});

let saveTimeout: ReturnType<typeof setTimeout>;

const loadNoteFile = async (id: string) => {
    if (!id) return;
    tabAccessTime.set(id, Date.now());
    
    if (!activeTabs.value.includes(id)) {
        if (activeTabs.value.length >= 10) {
            let oldestId = activeTabs.value[0];
            let oldestTime = tabAccessTime.get(oldestId) || Infinity;
            for (const t of activeTabs.value) {
                const time = tabAccessTime.get(t) || 0;
                if (time < oldestTime) {
                    oldestTime = time;
                    oldestId = t;
                }
            }
            activeTabs.value = activeTabs.value.filter(t => t !== oldestId);
            delete tabContents.value[oldestId];
            tabAccessTime.delete(oldestId);
        }
        activeTabs.value.push(id);
    }
    
    if (tabContents.value[id] === undefined) {
        try {
            const rawContent = await invoke<string>('read_note', { vaultPath: vaultPath.value, path: id });
            let body = rawContent;
            if (rawContent.startsWith('---\n') || rawContent.startsWith('---\r\n')) {
                const splitIdx = rawContent.indexOf('---', 3);
                if (splitIdx > 0) {
                    body = rawContent.substring(splitIdx + 3).replace(/^\s+/, '');
                }
            }
            tabContents.value[id] = body;
        } catch(e) {
            console.error("Failed to read note:", e);
        }
    }
};

// ─── Google Drive ─────────────────────────────────────────
const gdrive = useGDrive(vaultPath, vaultType, scanVault, tabContents, loadNoteFile, currentNoteId);

// ─── Size & Toggle State ─────────────────────────────────
const wNoteSidebar = ref(300);
const showNoteSidebar = ref(true);
const wRightSidebar = ref(288);
const showRightSidebar = ref(true);

const isDraggingNoteSidebar = ref(false);
const startDragNoteSidebar = () => { isDraggingNoteSidebar.value = true; };
const isDraggingRightSidebar = ref(false);
const startDragRightSidebar = () => { isDraggingRightSidebar.value = true; };

const onMouseMove = (e: MouseEvent) => {
  if (isDraggingNoteSidebar.value) {
    wNoteSidebar.value = Math.max(220, Math.min(e.clientX - 64, 600));
  } else if (isDraggingRightSidebar.value) {
    wRightSidebar.value = Math.max(200, Math.min(window.innerWidth - e.clientX, 600));
  }
};
const onMouseUp = () => {
  isDraggingNoteSidebar.value = false;
  isDraggingRightSidebar.value = false;
};

const isFloatingView = ref(false);

// ─── Note Manager State ───────────────────────────────────
const viewMode = ref<'editor' | 'manager'>('editor');
const managerFilter = ref('');
const managerSearchQuery = ref('');

// ─── Context Menu & Search ────────────────────────────────
const activeContextMenu = ref<string | null>(null);
const searchQuery = ref('');
const newTagInput = ref('');
const isCaseSensitiveSearch = ref(false);

const toggleContext = (id: string, e: Event) => {
  e.stopPropagation();
  activeContextMenu.value = activeContextMenu.value === id ? null : id;
};

// ─── Frontmatter Utils ────────────────────────────────────
const buildFrontmatter = (n: NoteMetadata) => {
    return `---\ntitle: "${n.title}"\npinned: ${n.pinned}\ntags: [${n.tags.map(t=>`"${t}"`).join(', ')}]\n---`;
};

// ─── Note CRUD Operations ─────────────────────────────────
const togglePin = async (id: string) => {
    const note = notes.value.find(n => n.id === id);
    if (!note) return;
    note.pinned = !note.pinned;
    try {
        const rawContent = await invoke<string>('read_note', { vaultPath: vaultPath.value, path: id });
        let body = rawContent;
        if (rawContent.startsWith('---\n') || rawContent.startsWith('---\r\n')) {
            const splitIdx = rawContent.indexOf('---', 3);
            if (splitIdx > 0) body = rawContent.substring(splitIdx + 3).replace(/^\s+/, '');
        }
        await invoke('update_note', { vaultPath: vaultPath.value, path: id, content: `${buildFrontmatter(note)}\n\n${body}` });
        scanVault();
    } catch(e) { console.error('Pin fail:', e); }
};

const deleteNote = async (id: string) => {
    const isConfirmed = await confirm('Are you sure you want to delete this note irreversibly?', { title: 'Delete Note', kind: 'warning' });
    if (!isConfirmed) return;
    try {
        if (currentNoteId.value === id) {
           clearTimeout(saveTimeout);
        }
        await invoke('delete_note', { vaultPath: vaultPath.value, path: id });
        delete tabContents.value[id];
        activeTabs.value = activeTabs.value.filter(t => t !== id);
        tabAccessTime.delete(id);
        if (currentNoteId.value === id) {
           currentNoteId.value = null;
        }
        scanVault();
    } catch(e) { console.error('Delete fail:', e); }
};

const openInNewWindow = async (id: string) => {
    try { await invoke('spawn_note_window', { noteId: id }); } catch(e) { console.error("Failed to open note in new window", e); }
    activeContextMenu.value = null;
};

const handleRenamePrompt = async (id: string) => {
    const note = notes.value.find(n => n.id === id);
    if (!note) return;
    const newName = prompt("Enter new note name:", note.title);
    if (newName && newName !== note.title) {
       try {
           const newPath = await invoke<string>('rename_note', { vaultPath: vaultPath.value, oldPath: note.id, newName });
           note.title = newName;
           if (currentNoteId.value === note.id) {
               currentNoteId.value = newPath;
               await invoke('update_note', { vaultPath: vaultPath.value, path: newPath, content: `${buildFrontmatter(note)}\n\n${currentContent.value}` });
           } else {
               const rawContent = await invoke<string>('read_note', { vaultPath: vaultPath.value, path: newPath });
               let body = rawContent;
               if (rawContent.startsWith('---\n') || rawContent.startsWith('---\r\n')) {
                   const splitIdx = rawContent.indexOf('---', 3);
                   if (splitIdx > 0) body = rawContent.substring(splitIdx + 3).replace(/^\s+/, '');
               }
               await invoke('update_note', { vaultPath: vaultPath.value, path: newPath, content: `${buildFrontmatter(note)}\n\n${body}` });
           }
           scanVault();
       } catch(err) { alert(err); }
    }
};

const renameTopTitle = async (e: Event) => {
    const newTitle = (e.target as HTMLInputElement).value.trim();
    const note = notes.value.find(n => n.id === currentNoteId.value);
    if (!note || note.title === newTitle || !newTitle) return;
    try {
        const newPath = await invoke<string>('rename_note', { vaultPath: vaultPath.value, oldPath: note.id, newName: newTitle });
        note.title = newTitle;
        currentNoteId.value = newPath;
        await invoke('update_note', { vaultPath: vaultPath.value, path: newPath, content: `${buildFrontmatter(note)}\n\n${currentContent.value}` });
        scanVault();
    } catch(err) { alert(err); }
};

const addTag = async (e: KeyboardEvent) => {
   if (e.key === 'Enter' && newTagInput.value.trim()) {
       const note = notes.value.find(n => n.id === currentNoteId.value);
       if (note && !note.tags.includes(newTagInput.value.trim())) {
           note.tags.push(newTagInput.value.trim());
           newTagInput.value = '';
           await invoke('update_note', { vaultPath: vaultPath.value, path: note.id, content: `${buildFrontmatter(note)}\n\n${currentContent.value}` });
           scanVault();
       }
   }
};

const removeTag = async (tagToRemove: string) => {
   const note = notes.value.find(n => n.id === currentNoteId.value);
   if (note) {
       note.tags = note.tags.filter(t => t !== tagToRemove);
       await invoke('update_note', { vaultPath: vaultPath.value, path: note.id, content: `${buildFrontmatter(note)}\n\n${currentContent.value}` });
       scanVault();
   }
};

// ─── Tags Data Logic ──────────────────────────────────────
interface TagNode {
  name: string;
  basename: string;
  count: number;
  expanded: boolean;
  children: TagNode[];
}

const tagTree = ref<TagNode[]>([]);
const selectedTags = ref<Set<string>>(new Set());

const buildTagTree = (allNotes: NoteMetadata[]) => {
  const map = new Map<string, { count: number, children: Set<string> }>();
  allNotes.forEach(n => {
    n.tags.forEach(tagPath => {
       const parts = tagPath.split('/');
       const parent = parts[0];
       if (!map.has(parent)) map.set(parent, { count: 0, children: new Set() });
       map.get(parent)!.count++;
       if (parts.length > 1) {
          const childName = `${parent}/${parts[1]}`;
          map.get(parent)!.children.add(childName);
          if (!map.has(childName)) map.set(childName, { count: 0, children: new Set() });
          map.get(childName)!.count++;
       }
    })
  });
  const tree: TagNode[] = [];
  map.forEach((data, name) => {
    if (!name.includes('/')) {
      const children: TagNode[] = Array.from(data.children).map(childName => ({
        name: childName, basename: childName.split('/')[1], count: map.get(childName)?.count || 0, expanded: false, children: []
      }));
      tree.push({ name, basename: name, count: data.count, expanded: true, children });
    }
  });
  tagTree.value = tree.sort((a,b) => a.name.localeCompare(b.name));
};

const toggleTagSelection = (tagName: string) => {
  const newSet = new Set(selectedTags.value);
  if (newSet.has(tagName)) newSet.delete(tagName);
  else newSet.add(tagName);
  selectedTags.value = newSet;
};

// ─── API Calls ────────────────────────────────────────────
const selectVault = async () => {
    try {
        const selected = await open({ title: 'Select Note Vault Directory', directory: true, multiple: false });
        if (selected) {
            vaultPath.value = selected as string;
            localStorage.setItem('synabitVaultPath', vaultPath.value);
            invoke('start_vault_watcher', { vaultPath: vaultPath.value }).catch(console.error);
            scanVault();
        }
    } catch(err) { console.error(err); }
};

async function scanVault() {
   if (!vaultPath.value) return;
   try {
       const scannedNotes = await invoke<NoteMetadata[]>('scan_vault_path', { vaultPath: vaultPath.value });
       notes.value = scannedNotes;
       buildTagTree(scannedNotes);
       if (scannedNotes.length > 0 && !currentNoteId.value) {
           currentNoteId.value = scannedNotes[0].id;
       } else if (scannedNotes.length === 0) {
           currentNoteId.value = null;
       }
   } catch(e) { console.error("Failed to scan vault:", e); }
}

const createNewNote = async () => {
    if (!vaultPath.value) return;
    try {
        const newPath = await invoke<string>('create_new_note', { vaultPath: vaultPath.value });
        await scanVault();
        if (newPath) { currentNoteId.value = newPath; viewMode.value = 'editor'; }
    } catch(e) { console.error("Failed to create note:", e); }
}

async function openDailyNote() {
    if (!vaultPath.value) return;
    try {
        const finalFormat = isValidDailyFormat.value ? dailyNoteFormat.value : 'YYYY-MM-DD';
        const tag = dailyNoteTag.value.trim();
        const dailyPath = await invoke<string>('open_daily_note', { vaultPath: vaultPath.value, formatStr: finalFormat, tag });
        await scanVault();
        if (dailyPath) { currentNoteId.value = dailyPath; viewMode.value = 'editor'; }
    } catch(e) { console.error("Failed to open daily note:", e); }
}

// ─── Save & Editor ────────────────────────────────────────
const saveNoteFile = () => {
    if (!currentNoteId.value) return;
    const note = notes.value.find(n => n.id === currentNoteId.value);
    if (!note) return;
    clearTimeout(saveTimeout);
    saveTimeout = setTimeout(async () => {
        let fullRaw = `${buildFrontmatter(note)}\n\n${currentContent.value}`;
        try {
            await invoke('update_note', { vaultPath: vaultPath.value, path: note.id, content: fullRaw });
            note.summary = currentContent.value.substring(0, 150).trim();
            emit('note-updated', { id: note.id, content: currentContent.value });
        } catch(e) { console.error("Failed to save note:", e); }
    }, 600);
}

const currentBacklinks = ref<NoteMetadata[]>([]);
const activeNote = computed(() => notes.value.find(n => n.id === currentNoteId.value) || null);

const currentOutgoingLinks = computed(() => {
    if (!currentContent.value) return [];
    const regex = /synabit:\/\/note\/([^\s\)"']+)/g;
    const links = new Set<string>();
    let m;
    while ((m = regex.exec(currentContent.value)) !== null) {
        const targetFilename = decodeURIComponent(m[1]);
        const targetNote = notes.value.find(n => n.path.endsWith(targetFilename));
        if (targetNote) links.add(targetNote.id);
        else links.add(targetFilename);
    }
    return Array.from(links);
});

const editorRefs = ref<any[]>([]);

const onEditorUpdate = (val: string) => {
    currentContent.value = val;
    if (currentNoteId.value) {
        tabContents.value[currentNoteId.value] = val;
        emit('note-updated', { id: currentNoteId.value, content: val });
    }
    saveNoteFile();
};

watch(currentNoteId, async (newId) => {
    if (newId) {
        await loadNoteFile(newId);
        try {
            currentBacklinks.value = await invoke('get_note_backlinks', { vaultPath: vaultPath.value, targetId: newId.split(/[\\/]/).pop() || newId });
        } catch (e) { console.error(e); currentBacklinks.value = []; }
    } else { currentBacklinks.value = []; }
});

const handleOpenInternalNote = (noteId: string) => {
    const exists = notes.value.find(n => n.id === noteId);
    if (exists) { currentNoteId.value = noteId; }
    else {
        const existsByName = notes.value.find(n => n.id.endsWith(noteId));
        if (existsByName) currentNoteId.value = existsByName.id;
    }
};

// ─── Derived State ────────────────────────────────────────
const allTags = computed(() => {
    const counts = new Map<string, number>();
    notes.value.forEach(note => { note.tags.forEach(tag => { counts.set(tag, (counts.get(tag) || 0) + 1); }); });
    return Array.from(counts.entries()).map(([name, count]) => ({ name, count })).sort((a,b) => b.count - a.count);
});

const topTags = computed(() => allTags.value.slice(0, 10));
const topPinnedNotes = computed(() => filteredNotes.value.filter(n => n.pinned).slice(0, 5));
const recentNotes = computed(() => filteredNotes.value.filter(n => !n.pinned).slice(0, 10));

const openNoteManager = (filterType: string) => {
    managerFilter.value = filterType;
    viewMode.value = 'manager';
};

const managerFilteredNotes = computed(() => {
   let result = notes.value;
   if (managerSearchQuery.value.trim()) {
      const q = managerSearchQuery.value.trim();
      const isTagSearch = q.startsWith('#');
      const searchTerm = isTagSearch ? q.slice(1) : q;
      const searchStr = isCaseSensitiveSearch.value ? searchTerm : searchTerm.toLowerCase();
      const match = (text: string) => {
         if (!text) return false;
         return isCaseSensitiveSearch.value ? text.includes(searchStr) : text.toLowerCase().includes(searchStr);
      };
      result = result.filter(n => {
         if (isTagSearch) return n.tags.some(t => match(t));
         return match(n.title) || n.tags.some(t => match(t)) || match(n.content);
      });
   }
   if (managerFilter.value === 'notes' || !managerFilter.value || managerFilter.value === 'tags') return result;
   else if (managerFilter.value === 'pinned') return result.filter(n => n.pinned);
   else return result.filter(n => n.tags.includes(managerFilter.value));
});

const filteredNotes = computed(() => {
  let result = notes.value;
  if (searchQuery.value.trim()) {
      const q = searchQuery.value.trim();
      const isTagSearch = q.startsWith('#');
      const searchTerm = isTagSearch ? q.slice(1) : q;
      const searchStr = isCaseSensitiveSearch.value ? searchTerm : searchTerm.toLowerCase();
      const match = (text: string) => {
         if (!text) return false;
         return isCaseSensitiveSearch.value ? text.includes(searchStr) : text.toLowerCase().includes(searchStr);
      };
      result = result.filter(n => {
          if (isTagSearch) return n.tags.some(t => match(t));
          return match(n.title) || n.tags.some(t => match(t)) || match(n.content);
      });
  }
  if (selectedTags.value.size > 0) {
      result = result.filter(n => n.tags.some(t => selectedTags.value.has(t)));
  }
  return result.sort((a,b) => {
      // Pinned notes float to top
      if (a.pinned && !b.pinned) return -1;
      if (!a.pinned && b.pinned) return 1;
      // Within same pinned/unpinned group, sort by date descending
      return b.date.localeCompare(a.date);
  });
});

const handleEditFromNexus = (id: string, type: string) => {
    if (type === 'note') { activeTool.value = 'note'; handleOpenInternalNote(id); }
    else if (type === 'quickcap') { activeTool.value = 'quickcap'; localStorage.setItem('synabit_edit_target_id', id); }
    else if (type === 'task') { activeTool.value = 'task'; localStorage.setItem('synabit_edit_target_id', id); }
};

const clearVault = () => {
    localStorage.removeItem('synabitVaultPath');
    localStorage.removeItem('synabitVaultType');
    vaultPath.value = '';
    vaultType.value = 'local';
    activeTool.value = 'note';
    gdrive.setupAutoSync();
};

// ─── Lifecycle ────────────────────────────────────────────
onMounted(() => {
  window.addEventListener('mousemove', onMouseMove);
  window.addEventListener('mouseup', onMouseUp);
  document.addEventListener('click', () => { activeContextMenu.value = null; });
  applyTheme();
  window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', applyTheme);
  
  const params = new URLSearchParams(window.location.search);
  const floatingId = params.get('floatingNote');
  if (floatingId) {
      isFloatingView.value = true;
      currentNoteId.value = floatingId;
      activeTool.value = 'note';
      viewMode.value = 'editor';
      showNoteSidebar.value = false;
      showRightSidebar.value = false;
  }
  
  if (vaultPath.value) {
     scanVault();
     invoke('start_vault_watcher', { vaultPath: vaultPath.value }).catch(console.error);
  }
  
  gdrive.checkGDriveAuth().then(() => { gdrive.setupAutoSync(); });

  listen('note-updated', (event: any) => {
      const data = event.payload as { id: string, content: string };
      if (currentNoteId.value === data.id) return;
      if (tabContents.value[data.id] !== undefined) {
         tabContents.value[data.id] = data.content;
      }
  });
  
  listen('vault-changed', () => { scanVault(); });
  
  listen('vault-file-created-deleted', () => {
      scanVault();
      if (vaultType.value === 'gdrive' && gdrive.gdriveConnected.value && !gdrive.gdriveSyncing.value) {
          gdrive.syncGDrive();
      }
  });

  listen('vault-file-modified', () => { scanVault(); });
  
  getCurrentWindow().onCloseRequested(async () => {
      if (currentNoteId.value) {
          const note = notes.value.find(n => n.id === currentNoteId.value);
          if (note && currentContent.value) {
              let fullRaw = `${buildFrontmatter(note)}\n\n${currentContent.value}`;
              try {
                  await invoke('update_note', { vaultPath: vaultPath.value, path: note.id, content: fullRaw });
                  emit('note-updated', { id: note.id, content: currentContent.value });
              } catch(e) { console.error('Save before close failed', e); }
          }
      }
  });
});

onUnmounted(() => {
  window.removeEventListener('mousemove', onMouseMove);
  window.removeEventListener('mouseup', onMouseUp);
  window.matchMedia('(prefers-color-scheme: dark)').removeEventListener('change', applyTheme);
});
</script>

<template>
  <div class="flex h-screen w-full bg-[#fdfdfc] text-[#1c1c1e] dark:bg-[#242424] dark:text-[#f4f4f5] font-sans overflow-hidden select-none"
       :class="{'cursor-col-resize': isDraggingNoteSidebar || isDraggingRightSidebar}">
       
    <!-- Application State 1: No Vault Selected -->
    <div v-if="!vaultPath" class="flex-1 flex flex-col items-center justify-center p-8 bg-[#fdfdfc] dark:bg-[#242424]" data-tauri-drag-region>
        <div class="max-w-lg w-full text-center space-y-8">
            <div class="w-20 h-20 bg-gray-100 dark:bg-gray-800 rounded-full flex items-center justify-center mx-auto shadow-inner">
               <FileText class="w-10 h-10 text-gray-400" />
            </div>
            <div>
               <h1 class="text-2xl font-bold mb-2">Welcome to Synabit</h1>
               <p class="text-[#52525b] dark:text-[#a1a1aa] text-sm">Choose how you want to store your vault.</p>
            </div>
            
            <div class="flex gap-4 justify-center" @mousedown.stop>
              <button @click="selectVault" class="group flex flex-col items-center gap-3 p-6 w-48 rounded-2xl border-2 border-[#e6e6e6] dark:border-[#333] hover:border-black dark:hover:border-white bg-white dark:bg-[#1e1e1e] transition-all hover:shadow-lg active:scale-[0.98] cursor-pointer">
                <div class="w-12 h-12 rounded-xl bg-gray-100 dark:bg-gray-800 flex items-center justify-center group-hover:bg-gray-200 dark:group-hover:bg-gray-700 transition-colors">
                  <FolderOpen class="w-6 h-6 text-gray-600 dark:text-gray-300" />
                </div>
                <div>
                  <p class="font-semibold text-sm">Local Folder</p>
                  <p class="text-[11px] text-gray-400 mt-1">Store on this computer</p>
                </div>
              </button>
              
              <button @click="gdrive.connectGDrive()" :disabled="gdrive.gdriveAuthLoading.value" class="group flex flex-col items-center gap-3 p-6 w-48 rounded-2xl border-2 border-[#e6e6e6] dark:border-[#333] hover:border-blue-500 dark:hover:border-blue-400 bg-white dark:bg-[#1e1e1e] transition-all hover:shadow-lg active:scale-[0.98] cursor-pointer disabled:opacity-60 disabled:pointer-events-none">
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
      <!-- GLOBAL NAVIGATION SIDEBAR -->
      <nav v-if="!isFloatingView" class="w-16 flex-shrink-0 bg-[#fbfbfc] dark:bg-[#191919] border-r border-[#e6e6e6] dark:border-[#2c2c2c] flex flex-col items-center py-4 z-20" data-tauri-drag-region>
         <div class="flex-1 flex flex-col items-center gap-3 mt-4 w-full" @mousedown.stop>
            <button @click="activeTool = 'nexus'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'nexus' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
               <Globe class="w-5 h-5" />
               <span class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Nexus</span>
            </button>
            <div class="w-8 h-px bg-[#e6e6e6] dark:bg-[#2c2c2c] my-1 rounded"></div>
            <button @click="activeTool = 'quickcap'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'quickcap' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
               <Zap class="w-5 h-5" />
               <span class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">QuickCap</span>
            </button>
            <button @click="activeTool = 'note'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'note' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
               <FileText class="w-5 h-5" />
               <span class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Notes</span>
            </button>
            <button @click="activeTool = 'task'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'task' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
               <CheckSquare class="w-5 h-5" />
               <span class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Tasks</span>
            </button>
            <button @click="activeTool = 'calendar'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'calendar' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
               <Calendar class="w-5 h-5" />
               <span class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Calendar</span>
            </button>
            <button @click="activeTool = 'file'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'file' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
               <FolderOpen class="w-5 h-5" />
               <span class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">Files</span>
            </button>
         </div>
         <div class="flex-shrink-0 w-full flex flex-col items-center gap-3 mb-2" @mousedown.stop>
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

      <!-- NOTE TOOL -->
      <template v-if="activeTool === 'note'">
        <!-- Note Sidebar -->
        <aside 
          v-show="showNoteSidebar" 
          class="border-r border-[#e6e6e6] dark:border-[#2c2c2c] bg-[#fbfbfc] dark:bg-[#191919] flex flex-col relative shrink-0"
          :style="{ width: wNoteSidebar + 'px' }"
        >
          <div class="absolute top-0 right-0 w-1.5 h-full cursor-col-resize hover:bg-black/10 dark:hover:bg-white/10 z-10 opacity-0 hover:opacity-100 transition-opacity" @mousedown.stop="startDragNoteSidebar"></div>

          <div class="h-14 flex-shrink-0 flex items-center justify-between px-4 border-b border-[#e6e6e6] dark:border-[#2c2c2c]" data-tauri-drag-region>
             <span class="font-semibold text-sm">Notes Quick View</span>
             <div class="flex gap-1" @mousedown.stop>
               <button v-if="enableDailyNotes" @click="openDailyNote" class="px-2 py-1.5 flex items-center gap-1.5 rounded-md hover:bg-[#e6e6e6] dark:hover:bg-[#333] text-[#52525b] dark:text-[#a1a1aa] hover:text-[#1c1c1e] dark:hover:text-white transition-colors" title="Today's Daily Note">
                 <Sun class="w-3.5 h-3.5" />
                 <span class="text-xs font-medium">Today</span>
               </button>
               <button @click="createNewNote" class="px-2 py-1.5 flex items-center gap-1.5 rounded-md bg-[#e6e6e6] text-[#1c1c1e] dark:bg-[#333] dark:text-white hover:opacity-80 transition-opacity" title="New Note">
                 <Plus class="w-3.5 h-3.5" />
                 <span class="text-xs font-medium">New</span>
               </button>
             </div>
          </div>
          
          <div class="px-3 pt-3 pb-2 sticky top-0 bg-[#fbfbfc] dark:bg-[#191919] z-10" @mousedown.stop>
              <div class="relative w-full">
                <Search class="absolute left-2.5 top-1/2 -translate-y-1/2 w-4 h-4 text-[#8b8b8b] dark:text-[#71717a]" />
                <input v-model="searchQuery" type="text" placeholder="Search notes..." class="w-full pl-8 pr-14 py-1.5 bg-white dark:bg-[#2c2c2c] border border-[#e6e6e6] dark:border-transparent mx-auto block rounded-md text-sm focus:outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-shadow text-[#1c1c1e] dark:text-[#f4f4f5] placeholder:text-gray-400 dark:placeholder:text-gray-500">
                <button v-if="searchQuery" @click="searchQuery = ''" class="absolute right-7 top-1/2 -translate-y-1/2 p-0.5 rounded-full hover:bg-gray-100 dark:hover:bg-[#3f3f46] text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors">
                  <X class="w-3.5 h-3.5" />
                </button>
                <button @click="isCaseSensitiveSearch = !isCaseSensitiveSearch" :class="['absolute right-2 top-1/2 -translate-y-1/2 p-0.5 rounded-sm transition-colors', isCaseSensitiveSearch ? 'bg-purple-100 text-purple-600 dark:bg-purple-500/20 dark:text-purple-400' : 'text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 hover:bg-gray-100 dark:hover:bg-[#3f3f46]']" title="Match Case">
                  <CaseSensitive class="w-3.5 h-3.5" />
                </button>
              </div>
          </div>

          <div class="flex-1 overflow-y-auto" @mousedown.stop>
             <!-- Tags Section -->
             <div class="mb-4">
                 <div class="flex justify-between items-center px-4 mb-2 mt-3">
                     <span class="text-[11px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider">Top Tags</span>
                     <button @click="openNoteManager('tags')" class="text-[10px] text-purple-500 hover:text-purple-600 font-medium">Show all</button>
                 </div>
                 <div class="px-2 space-y-0.5" v-if="topTags.length > 0">
                     <div v-for="tag in topTags" :key="tag.name"
                          @click="toggleTagSelection(tag.name)"
                          class="w-full flex items-center justify-between px-3 py-1.5 rounded-lg text-sm transition-colors cursor-pointer group"
                          :class="selectedTags.has(tag.name) ? 'bg-black/5 dark:bg-white/10' : 'hover:bg-gray-100 dark:hover:bg-[#2a2a2a] text-[#52525b] dark:text-[#a1a1aa]'">
                          <div class="flex items-center gap-2 truncate">
                              <Hash class="w-3.5 h-3.5 opacity-70 group-hover:text-black dark:group-hover:text-white transition-colors" />
                              <span class="truncate select-none group-hover:text-black dark:group-hover:text-white transition-colors">{{ tag.name.split('/').pop() }}</span>
                          </div>
                          <span class="text-[10px] opacity-50 bg-black/5 dark:bg-white/10 px-1.5 py-0.5 rounded-full min-w-[20px] text-center">{{ tag.count }}</span>
                     </div>
                 </div>
                 <div v-else class="text-center p-4 text-xs text-gray-400">No tags found</div>
             </div>

             <!-- Pinned Section -->
             <div class="mb-4" v-if="topPinnedNotes.length > 0">
                 <div class="flex justify-between items-center px-4 mb-2">
                     <span class="text-[11px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider">Pinned Notes</span>
                     <button @click="openNoteManager('pinned')" class="text-[10px] text-purple-500 hover:text-purple-600 font-medium">Show all</button>
                 </div>
                 <div class="px-2 space-y-0.5">
                     <div v-for="note in topPinnedNotes" :key="note.id"
                        @click="currentNoteId = note.id; viewMode = 'editor'"
                        class="px-3 py-2 border border-transparent rounded-lg cursor-pointer transition-colors relative group"
                        :class="currentNoteId === note.id ? 'bg-white dark:bg-[#2a2a2a] shadow-sm border-[#e6e6e6] dark:border-[#3a3a3a]' : 'hover:bg-white/50 dark:hover:bg-[#252525] hover:border-[#e6e6e6] dark:hover:border-[#2f2f2f]'">
                        <div class="absolute right-2 top-2 opacity-0 group-hover:opacity-100 transition-opacity z-10" :class="{'opacity-100': activeContextMenu === note.id}">
                           <button @click.stop="(e) => toggleContext(note.id, e)" class="p-1 rounded bg-white dark:bg-[#2a2a2a] shadow-sm hover:bg-gray-100 border border-gray-200 dark:border-gray-600">
                              <MoreVertical class="w-3.5 h-3.5 text-gray-500"/>
                           </button>
                           <div v-if="activeContextMenu === note.id" class="absolute right-0 top-6 w-32 bg-white dark:bg-[#2c2c2c] shadow-lg rounded border border-gray-200 dark:border-gray-700 z-50 py-1 overflow-hidden">
                              <button @click.stop="togglePin(note.id)" class="w-full text-left px-3 py-2 text-xs hover:bg-gray-100 dark:hover:bg-gray-600 flex items-center gap-2"><Pin class="w-3 h-3" /> {{ note.pinned ? 'Unpin' : 'Pin' }}</button>
                              <button @click.stop="openInNewWindow(note.id)" class="w-full text-left px-3 py-2 text-xs hover:bg-gray-100 dark:hover:bg-gray-600 flex items-center gap-2"><ExternalLink class="w-3 h-3" /> Open in New Window</button>
                              <button @click.stop="handleRenamePrompt(note.id)" class="w-full text-left px-3 py-2 text-xs hover:bg-gray-100 dark:hover:bg-gray-600 flex items-center gap-2"><Edit2 class="w-3 h-3" /> Rename</button>
                              <button @click.stop="deleteNote(note.id)" class="w-full text-left px-3 py-2 text-xs hover:bg-red-50 dark:hover:bg-red-900/30 text-red-600 dark:text-red-400 flex items-center gap-2"><Trash2 class="w-3 h-3" /> Delete</button>
                           </div>
                        </div>
                        <div class="flex items-center gap-2 mb-1.5 pr-6">
                            <Pin class="w-3 h-3 text-orange-500 shrink-0 fill-orange-500/20" />
                            <span class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] truncate">{{ note.title || 'Untitled Note' }}</span>
                        </div>
                        <div class="flex flex-wrap gap-1" v-if="note.tags.length">
                            <span v-for="tag in note.tags" :key="tag" class="text-[9px] px-1.5 py-0.5 rounded bg-gray-200/60 dark:bg-[#333] text-gray-600 dark:text-gray-300">{{ tag.split('/').pop() }}</span>
                        </div>
                     </div>
                 </div>
             </div>

             <!-- Recent Notes -->
             <div class="mb-4">
                 <div class="flex justify-between items-center px-4 mb-2 mt-2">
                     <span class="text-[11px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider">Recent Notes</span>
                     <button @click="openNoteManager('notes')" class="text-[10px] text-purple-500 hover:text-purple-600 font-medium">Show all</button>
                 </div>
                 <div class="px-2 space-y-0.5">
                     <div v-for="note in recentNotes" :key="note.id"
                        @click="currentNoteId = note.id; viewMode = 'editor'"
                        class="px-3 py-2 border border-transparent rounded-lg cursor-pointer transition-colors relative group"
                        :class="currentNoteId === note.id ? 'bg-white dark:bg-[#2a2a2a] shadow-sm border-[#e6e6e6] dark:border-[#3a3a3a]' : 'hover:bg-white/50 dark:hover:bg-[#252525] hover:border-[#e6e6e6] dark:hover:border-[#2f2f2f]'">
                        <div class="absolute right-2 top-2 opacity-0 group-hover:opacity-100 transition-opacity z-10" :class="{'opacity-100': activeContextMenu === note.id}">
                           <button @click.stop="(e) => toggleContext(note.id, e)" class="p-1 rounded bg-white dark:bg-[#2a2a2a] shadow-sm hover:bg-gray-100 border border-gray-200 dark:border-gray-600">
                              <MoreVertical class="w-3.5 h-3.5 text-gray-500"/>
                           </button>
                           <div v-if="activeContextMenu === note.id" class="absolute right-0 top-6 w-32 bg-white dark:bg-[#2c2c2c] shadow-lg rounded border border-gray-200 dark:border-gray-700 z-50 py-1 overflow-hidden">
                              <button @click.stop="togglePin(note.id)" class="w-full text-left px-3 py-2 text-xs hover:bg-gray-100 dark:hover:bg-gray-600 flex items-center gap-2"><Pin class="w-3 h-3" /> {{ note.pinned ? 'Unpin' : 'Pin' }}</button>
                              <button @click.stop="openInNewWindow(note.id)" class="w-full text-left px-3 py-2 text-xs hover:bg-gray-100 dark:hover:bg-gray-600 flex items-center gap-2"><ExternalLink class="w-3 h-3" /> Open in New Window</button>
                              <button @click.stop="handleRenamePrompt(note.id)" class="w-full text-left px-3 py-2 text-xs hover:bg-gray-100 dark:hover:bg-gray-600 flex items-center gap-2"><Edit2 class="w-3 h-3" /> Rename</button>
                              <button @click.stop="deleteNote(note.id)" class="w-full text-left px-3 py-2 text-xs hover:bg-red-50 dark:hover:bg-red-900/30 text-red-600 dark:text-red-400 flex items-center gap-2"><Trash2 class="w-3 h-3" /> Delete</button>
                           </div>
                        </div>
                        <div class="flex items-center gap-2 mb-1.5 pr-6">
                            <FileText class="w-3.5 h-3.5 text-gray-400 shrink-0 opacity-80" />
                            <span class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] truncate">{{ note.title || 'Untitled Note' }}</span>
                        </div>
                        <div class="flex flex-wrap gap-1" v-if="note.tags.length">
                            <span v-for="tag in note.tags" :key="tag" class="text-[9px] px-1.5 py-0.5 rounded bg-gray-200/60 dark:bg-[#333] text-gray-600 dark:text-gray-300">{{ tag.split('/').pop() }}</span>
                        </div>
                     </div>
                 </div>
                 <div v-if="recentNotes.length === 0" class="p-8 text-center text-sm text-[#52525b] dark:text-[#a1a1aa]">
                   No notes match.
                 </div>
             </div>
    </div>
  </aside>

      <!-- Main Area: Editor / Manager -->
      <main class="flex-1 flex flex-col bg-[#fdfdfc] dark:bg-[#242424] min-w-[300px]" @mousedown.stop>
        <template v-if="viewMode === 'editor'">
            <div v-if="!isFloatingView" class="h-10 flex-shrink-0 w-full flex items-center justify-between px-4" data-tauri-drag-region>
              <div class="flex gap-2">
                <button @click="showNoteSidebar = !showNoteSidebar" class="p-1 rounded-md hover:bg-gray-200 dark:hover:bg-gray-800 text-gray-500 transition-colors" title="Toggle Sidebar">
                  <PanelLeftClose v-if="showNoteSidebar" class="w-4 h-4" />
                  <PanelLeft v-else class="w-4 h-4" />
                </button>
              </div>
              <div class="flex gap-2">
                <button v-if="currentNoteId" @click="showRightSidebar = !showRightSidebar" class="p-1 relative ml-2 rounded-md hover:bg-gray-200 dark:hover:bg-gray-800 text-gray-500 transition-colors" title="Toggle Right Sidebar">
                  <PanelRightClose v-if="showRightSidebar" class="w-4 h-4" />
                  <PanelRight v-else class="w-4 h-4" />
                </button>
              </div>
            </div>
            <div v-else class="h-8 flex-shrink-0 w-full z-50 bg-[#fdfdfc] dark:bg-[#242424]" data-tauri-drag-region></div>

            <template v-if="activeTabs.length > 0">
              <template v-for="tabId in activeTabs" :key="tabId">
                <div v-show="currentNoteId === tabId" class="flex-1 overflow-y-auto w-full relative">
                  <div v-if="tabContents[tabId] === undefined" class="absolute inset-0 flex items-center justify-center bg-[#fdfdfc] dark:bg-[#242424]">
                      <div class="w-8 h-8 rounded-full border-2 border-gray-200 border-t-gray-400 animate-spin"></div>
                  </div>
                  <div v-else class="px-12 pb-12 max-w-4xl mx-auto w-full cursor-text">
                  <div class="mb-4 pt-4">
                     <div class="flex gap-2 mb-4 flex-wrap items-center">
                        <span v-for="tag in notes.find(n => n.id === tabId)?.tags" :key="tag" class="text-xs px-2 py-1 rounded-md bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-300 flex items-center gap-1 group/tag">
                            <Hash class="w-3 h-3 opacity-50"/>
                            {{ tag }}
                            <button @click="removeTag(tag)" class="opacity-0 group-hover/tag:opacity-100 hover:text-red-500 transition-opacity ml-1 p-0.5"><X class="w-3 h-3"/></button>
                         </span>
                         <div class="relative flex items-center">
                            <Plus class="w-3 h-3 absolute left-1.5 text-gray-400" />
                            <input v-model="newTagInput" @keydown="addTag" placeholder="Add tag..." class="text-xs bg-transparent border border-dashed border-gray-300 dark:border-gray-600 rounded-md py-1 pl-5 pr-2 w-24 focus:w-32 focus:outline-none focus:border-gray-400 transition-all text-[#1c1c1e] dark:text-[#f4f4f5]" />
                         </div>
                     </div>
                    <input type="text" class="w-full text-4xl font-bold bg-transparent border-none outline-none text-[#1c1c1e] dark:text-[#f4f4f5] placeholder:text-gray-300 dark:placeholder:text-gray-700" :value="notes.find(n => n.id === tabId)?.title" @blur="renameTopTitle" @keydown.enter="renameTopTitle" placeholder="Note Title">
                  </div>
                  <div class="mt-4 pb-20 w-full text-text dark:text-text-dark">
                     <TiptapEditor ref="editorRefs" :model-value="tabContents[tabId]" :vault-path="vaultPath" :notes="notes" @update:model-value="onEditorUpdate" @open-internal-note="handleOpenInternalNote" />
                  </div>
                  </div>
                </div>
              </template>
            </template>
            <div v-else class="flex-1 flex items-center justify-center text-[#52525b] dark:text-[#a1a1aa]">
              <div class="text-center">
                <FileText class="w-12 h-12 mx-auto mb-4 opacity-20" />
                <p>Select a note to start editing</p>
              </div>
            </div>
        </template>
        <template v-else-if="viewMode === 'manager'">
            <div class="flex-1 flex flex-col bg-[#fdfdfc] dark:bg-[#242424] h-full relative z-0 overflow-y-auto">
               <div class="flex items-center justify-between px-6 h-14 border-b border-[#e6e6e6] dark:border-[#2c2c2c] shrink-0 sticky top-0 bg-[#fdfdfc] dark:bg-[#242424] z-10" data-tauri-drag-region>
                  <div class="flex items-center gap-3">
                     <button @click="viewMode = 'editor'" class="p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] transition-colors text-gray-500">
                        <ArrowLeft class="w-5 h-5" />
                     </button>
                     <h1 class="text-xl font-bold text-[#1c1c1e] dark:text-[#f4f4f5]">
                        {{ managerFilter === 'tags' && !managerSearchQuery ? 'All Tags' : managerSearchQuery ? 'Search Results' : managerFilter === 'notes' || !managerFilter ? 'All Notes' : managerFilter === 'pinned' ? 'Pinned Notes' : 'Tag: ' + managerFilter.split('/').pop() }}
                     </h1>
                  </div>
               </div>
               
               <div class="flex-1 flex flex-col p-8 w-full max-w-4xl mx-auto">
                   <div class="relative w-full mb-8">
                     <Search class="absolute left-4 top-1/2 -translate-y-1/2 w-5 h-5 text-[#8b8b8b] dark:text-[#71717a]" />
                     <input v-model="managerSearchQuery" type="text" placeholder="Search notes or tags..." class="w-full pl-12 pr-20 py-3 bg-white dark:bg-[#1a1a1a] border border-[#e6e6e6] dark:border-[#2c2c2c] rounded-xl text-base shadow-sm focus:outline-none focus:ring-2 focus:ring-purple-500/50 transition-shadow placeholder:text-gray-400 manager-search-input">
                     <button v-if="managerSearchQuery" @click="managerSearchQuery = ''" class="absolute right-12 top-1/2 -translate-y-1/2 p-1.5 rounded-full hover:bg-gray-100 dark:hover:bg-[#2c2c2c] text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors">
                       <X class="w-4 h-4" />
                     </button>
                     <button @click="isCaseSensitiveSearch = !isCaseSensitiveSearch" :class="['absolute right-3 top-1/2 -translate-y-1/2 p-1.5 rounded-md transition-colors', isCaseSensitiveSearch ? 'bg-purple-100 text-purple-600 dark:bg-purple-500/20 dark:text-purple-400' : 'text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 hover:bg-gray-100 dark:hover:bg-[#2c2c2c]']" title="Match Case">
                       <CaseSensitive class="w-4 h-4" />
                     </button>
                   </div>
                   
                   <!-- Tags View -->
                   <div v-if="managerFilter === 'tags' && !managerSearchQuery" class="w-full">
                      <div class="flex flex-wrap gap-3">
                         <div v-for="tag in allTags" :key="tag.name" @click="managerFilter = tag.name" class="px-4 py-2 bg-white dark:bg-[#1f1f1f] border border-[#e6e6e6] dark:border-[#2c2c2c] rounded-lg cursor-pointer hover:border-[#d4d4d8] dark:hover:border-[#444] transition-all flex items-center gap-2 group">
                            <Hash class="w-4 h-4 text-gray-400 group-hover:text-[#1c1c1e] dark:group-hover:text-white transition-colors" />
                            <span class="font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">{{ tag.name.split('/').pop() }}</span>
                            <span class="text-xs bg-gray-100 dark:bg-[#2c2c2c] px-2 py-0.5 rounded text-gray-500">{{ tag.count }}</span>
                         </div>
                      </div>
                   </div>
                   
                   <!-- Notes Table View -->
                   <div v-else class="w-full">
                     <div class="bg-white dark:bg-[#252525] border border-[#e6e6e6] dark:border-[#333] rounded-xl overflow-hidden shadow-sm">
                        <table class="w-full text-left border-collapse">
                           <thead>
                              <tr class="bg-gray-50 dark:bg-[#1a1a1a] border-b border-[#e6e6e6] dark:border-[#333]">
                                 <th class="py-2.5 px-4 text-xs font-semibold text-gray-500 uppercase w-8"></th>
                                 <th class="py-2.5 px-4 text-xs font-semibold text-gray-500 uppercase w-5/12">Title</th>
                                 <th class="py-2.5 px-4 text-xs font-semibold text-gray-500 uppercase">Tags</th>
                                 <th class="py-2.5 px-4 text-xs font-semibold text-gray-500 uppercase whitespace-nowrap text-right">Modified</th>
                                 <th class="py-2.5 px-4 text-xs font-semibold text-gray-500 uppercase w-12 text-center">Action</th>
                              </tr>
                           </thead>
                           <tbody class="divide-y divide-[#e6e6e6] dark:divide-[#333] text-sm">
                              <tr v-for="note in managerFilteredNotes" :key="note.id" @click="currentNoteId = note.id; viewMode = 'editor'" class="hover:bg-gray-50 dark:hover:bg-[#2a2a2a] cursor-pointer transition-colors group">
                                 <td class="py-3 px-4 w-8">
                                    <Pin v-if="note.pinned" class="w-3.5 h-3.5 text-orange-500 fill-orange-500/20" />
                                    <FileText v-else class="w-3.5 h-3.5 text-gray-400 opacity-50" />
                                 </td>
                                 <td class="py-3 px-4 font-medium text-[#1c1c1e] dark:text-[#f4f4f5] max-w-[250px] truncate">{{ note.title || 'Untitled Note' }}</td>
                                 <td class="py-3 px-4">
                                    <div class="flex flex-wrap gap-1" v-if="note.tags.length">
                                       <span v-for="tag in note.tags.slice(0, 3)" :key="tag" class="text-[10px] px-1.5 py-0.5 rounded bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-300">{{ tag.split('/').pop() }}</span>
                                       <span v-if="note.tags.length > 3" class="text-[10px] px-1.5 py-0.5 rounded bg-gray-100 dark:bg-gray-800 text-gray-500">+{{ note.tags.length - 3 }}</span>
                                    </div>
                                    <span v-else class="text-xs text-gray-400 italic">No tags</span>
                                 </td>
                                 <td class="py-3 px-4 text-xs text-gray-500 dark:text-gray-400 whitespace-nowrap text-right">{{ note.date }}</td>
                                 <td class="py-3 px-4 w-12 text-center" @click.stop>
                                    <div class="relative flex justify-center">
                                       <button @click="(e) => toggleContext('manager_'+note.id, e)" class="p-1 rounded opacity-0 group-hover:opacity-100 hover:bg-gray-200 dark:hover:bg-[#444] transition">
                                          <MoreVertical class="w-4 h-4 text-gray-500" />
                                       </button>
                                       <div v-if="activeContextMenu === 'manager_'+note.id" class="absolute right-6 top-0 w-32 bg-white dark:bg-[#2c2c2c] shadow-lg rounded border border-gray-200 dark:border-gray-700 z-50 py-1 overflow-hidden">
                                          <button @click.stop="togglePin(note.id); activeContextMenu = null;" class="w-full text-left px-3 py-2 text-xs hover:bg-gray-100 dark:hover:bg-gray-600 flex items-center gap-2"><Pin class="w-3 h-3" /> {{ note.pinned ? 'Unpin' : 'Pin' }}</button>
                                          <button @click.stop="deleteNote(note.id); activeContextMenu = null;" class="w-full text-left px-3 py-2 text-xs hover:bg-red-50 dark:hover:bg-red-900/30 text-red-600 dark:text-red-400 flex items-center gap-2"><Trash2 class="w-3 h-3" /> Delete</button>
                                       </div>
                                    </div>
                                 </td>
                              </tr>
                              <tr v-if="managerFilteredNotes.length === 0">
                                 <td colspan="5" class="py-12 text-center text-gray-500">No notes found matching current filters.</td>
                              </tr>
                           </tbody>
                        </table>
                     </div>
                   </div>
               </div>
            </div>
        </template>
      </main>

      <!-- Right Sidebar: Graph & Backlinks -->
      <aside v-if="currentNoteId" v-show="showRightSidebar" class="shrink-0 relative border-l border-[#e6e6e6] dark:border-[#2c2c2c] bg-[#fbfbfc] dark:bg-[#191919] flex flex-col overflow-hidden" :style="{ width: wRightSidebar + 'px' }">
        <div class="absolute top-0 left-0 w-1.5 h-full cursor-col-resize hover:bg-black/10 dark:hover:bg-white/10 z-10 opacity-0 hover:opacity-100 transition-opacity" @mousedown.stop="startDragRightSidebar"></div>
        <div class="h-10 flex-shrink-0 flex items-center px-4 border-b border-[#e6e6e6] dark:border-[#2c2c2c]" data-tauri-drag-region>
            <Globe class="w-4 h-4 text-gray-500 mr-2" />
            <span class="font-bold text-[11px] tracking-wider text-gray-500 uppercase mt-0.5">Graph View</span>
            <button @click="showRightSidebar = false" class="p-1 ml-auto rounded-md hover:bg-gray-200 dark:hover:bg-gray-800 text-gray-400 transition-colors">
               <X class="w-3.5 h-3.5" />
            </button>
        </div>
        <div class="h-1/2 border-b border-[#e6e6e6] dark:border-[#2c2c2c] overflow-hidden">
            <NoteGraph v-if="activeNote" :current-note-id="currentNoteId || ''" :current-note-title="activeNote.title || 'Untitled Node'" :tags="activeNote.tags || []" :outgoing-links="currentOutgoingLinks" :backlinks="currentBacklinks" :all-notes="notes" @open-note="handleOpenInternalNote" />
        </div>
        <div class="h-10 flex-shrink-0 flex items-center px-4 border-b border-[#e6e6e6] dark:border-[#2c2c2c]">
            <span class="font-bold text-[11px] tracking-wider text-[#8b8b8b] dark:text-[#71717a] uppercase mt-0.5">Linked Mentions ({{ currentBacklinks.length }})</span>
        </div>
        <div class="flex-1 overflow-y-auto p-2 space-y-1">
            <div v-if="currentBacklinks.length === 0" class="text-[13px] text-gray-400 text-center py-4">No linked mentions.</div>
            <div v-for="bl in currentBacklinks" :key="bl.id" @click="handleOpenInternalNote(bl.id)" class="p-3 border border-transparent rounded-lg cursor-pointer hover:bg-white/50 dark:hover:bg-[#252525] hover:border-[#e6e6e6] dark:hover:border-[#2f2f2f] transition-all group">
              <h5 class="flex items-center gap-2 mb-1.5 pr-2">
                  <FileText class="w-3.5 h-3.5 text-gray-400 shrink-0 opacity-80 group-hover:text-purple-500 group-hover:opacity-100 transition-colors"/>
                  <span class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] truncate">{{ bl.title }}</span>
              </h5>
              <p class="text-[11px] text-[#52525b] dark:text-[#a1a1aa] line-clamp-3 leading-relaxed pl-5">{{ bl.summary || 'No text content available.' }}</p>
            </div>
        </div>
      </aside>
    </template>

    <!-- QUICKCAP TOOL -->
    <template v-else-if="activeTool === 'quickcap'">
       <main class="flex-1 overflow-hidden relative"><QuickCap :vaultPath="vaultPath" /></main>
    </template>

    <!-- NEXUS TOOL -->
    <template v-else-if="activeTool === 'nexus'">
       <main class="flex-1 overflow-hidden relative"><Nexus :vaultPath="vaultPath" @edit-item="handleEditFromNexus" /></main>
    </template>

    <!-- TASK TOOL -->
    <template v-else-if="activeTool === 'task'">
       <main class="flex-1 overflow-hidden relative"><Tasks :vaultPath="vaultPath" /></main>
    </template>
    
    <!-- CALENDAR TOOL -->
    <template v-else-if="activeTool === 'calendar'">
      <main class="flex-1 overflow-hidden relative"><CalendarApp :vaultPath="vaultPath" /></main>
    </template>
    
    <template v-else-if="activeTool === 'file'">
       <main class="flex-1 overflow-hidden relative"><FileManager :vaultPath="vaultPath" /></main>
    </template>

    <!-- SETTINGS MODAL -->
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
      @disconnect-gdrive="gdrive.disconnectGDrive().then(clearVault)"
      @update:gdrive-auto-sync-enabled="gdrive.gdriveAutoSyncEnabled.value = $event"
      @update:gdrive-auto-sync-interval="gdrive.gdriveAutoSyncInterval.value = $event"
    />
    
    </template>
  </div>
</template>

<style scoped>
[data-tauri-drag-region] {
  -webkit-app-region: drag;
}

.manager-search-input {
  color: #1c1c1e !important;
}
html.dark .manager-search-input {
  color: #f4f4f5 !important;
}
</style>