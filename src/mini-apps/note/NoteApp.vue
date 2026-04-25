<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch, nextTick } from 'vue';
import { FileText, Search, PanelLeft, PanelLeftClose, PanelRight, PanelRightClose, Hash, Plus, MoreVertical, Pin, Trash2, Edit2, X, ArrowLeft, ExternalLink, Sun, CaseSensitive, Globe } from 'lucide-vue-next';
import { invoke } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';
import { confirm } from '@tauri-apps/plugin-dialog';

import TiptapEditor from './TiptapEditor.vue';
import NoteGraph from './NoteGraph.vue';

import { useAppStore } from '../../stores/useAppStore';
import { storeToRefs } from 'pinia';

import type { NoteMetadata } from '../../types/ipc';

const props = defineProps<{
  vaultPath: string;
  isFloatingView?: boolean;
  floatingNoteId?: string | null;
}>();

const appStore = useAppStore();
const { enableDailyNotes, dailyNoteFormat, dailyNoteTag } = storeToRefs(appStore);

// ─── Note State ────────────────────────────────────────────
const notes = ref<NoteMetadata[]>([]);
const currentNoteId = ref<string | null>(null);

// ─── Tab / Content Management ──────────────────────────────
const activeTabs = ref<string[]>([]);
const tabContents = ref<Record<string, string>>({});
const focusedTitles = ref<Record<string, string>>({});
const tabAccessTime = new Map<string, number>();

const currentContent = computed({
   get: () => currentNoteId.value ? tabContents.value[currentNoteId.value] || '' : '',
   set: (val) => {
       if (currentNoteId.value) tabContents.value[currentNoteId.value] = val;
   }
});

const saveTimeouts = new Map<string, ReturnType<typeof setTimeout>>();
const renamedTabs = new Map<string, string>(); // Maps old path to new path to catch delayed editor updates
let isCreatingNote = false;
let suppressWatcherUntil = 0;

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
            const rawContent = await invoke<string>('read_note', { vaultPath: props.vaultPath, path: id });
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

// ─── Size & Toggle State ───────────────────────────────────
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

// ─── Note Manager State ────────────────────────────────────
const viewMode = ref<'editor' | 'manager'>('editor');
const managerFilter = ref('');
const managerSearchQuery = ref('');

// ─── Context Menu & Search ─────────────────────────────────
const activeContextMenu = ref<string | null>(null);
const searchQuery = ref('');
const newTagInput = ref('');
const isCaseSensitiveSearch = ref(false);

// ─── Rename Modal (replaces window.prompt for mobile compat) ──
const renameModal = ref<{ show: boolean; noteId: string; value: string }>({ show: false, noteId: '', value: '' });

const toggleContext = (id: string, e: Event) => {
  e.stopPropagation();
  activeContextMenu.value = activeContextMenu.value === id ? null : id;
};

// ─── Daily Note Format Validation ──────────────────────────
const isValidDailyFormat = computed(() => {
  const fmt = dailyNoteFormat.value;
  return fmt && (fmt.includes('YYYY') || fmt.includes('YY')) && (fmt.includes('MM') || fmt.includes('M')) && (fmt.includes('DD') || fmt.includes('D'));
});

// ─── Frontmatter Utils ─────────────────────────────────────
const buildFrontmatter = (n: NoteMetadata) => {
    return `---\ntitle: "${n.title}"\npinned: ${n.pinned}\ntags: [${n.tags.map(t=>`"${t}"`).join(', ')}]\n---`;
};

// ─── Note CRUD Operations ──────────────────────────────────
const togglePin = async (id: string) => {
    const note = notes.value.find(n => n.id === id);
    if (!note) return;
    note.pinned = !note.pinned;
    try {
        const rawContent = await invoke<string>('read_note', { vaultPath: props.vaultPath, path: id });
        let body = rawContent;
        if (rawContent.startsWith('---\n') || rawContent.startsWith('---\r\n')) {
            const splitIdx = rawContent.indexOf('---', 3);
            if (splitIdx > 0) body = rawContent.substring(splitIdx + 3).replace(/^\s+/, '');
        }
        await invoke('update_note', { vaultPath: props.vaultPath, path: id, content: `${buildFrontmatter(note)}\n\n${body}` });
        scanVault();
    } catch(e) { console.error('Pin fail:', e); }
};

const deleteNote = async (id: string) => {
    const isConfirmed = await confirm('Are you sure you want to delete this note irreversibly?', { title: 'Delete Note', kind: 'warning' });
    if (!isConfirmed) return;
    try {
        if (saveTimeouts.has(id)) {
           clearTimeout(saveTimeouts.get(id)!);
           saveTimeouts.delete(id);
        }
        await invoke('delete_note', { vaultPath: props.vaultPath, path: id });
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

const handleRenamePrompt = (id: string) => {
    const note = notes.value.find(n => n.id === id);
    if (!note) return;
    renameModal.value = { show: true, noteId: id, value: note.title };
    activeContextMenu.value = null;
};

const confirmRename = async () => {
    const { noteId, value: newName } = renameModal.value;
    renameModal.value.show = false;
    const note = notes.value.find(n => n.id === noteId);
    if (!note || !newName || newName === note.title) return;
    try {
        const oldId = note.id;
        // Cancel any pending auto-save for the old path to prevent it from recreating the file after rename
        if (saveTimeouts.has(oldId)) {
            clearTimeout(saveTimeouts.get(oldId)!);
            saveTimeouts.delete(oldId);
        }
        const savedContent = tabContents.value[oldId];
        const newPath = await invoke<string>('rename_note', { vaultPath: props.vaultPath, oldPath: oldId, newName });
        
        // Secondary cancellation: if the user typed during the await rename_note, a new timeout for the old path might have been created.
        let needsSave = false;
        if (saveTimeouts.has(oldId)) {
            clearTimeout(saveTimeouts.get(oldId)!);
            saveTimeouts.delete(oldId);
            needsSave = true;
        }

        note.title = newName;
        renamedTabs.set(oldId, newPath);
        
        if (savedContent !== undefined) {
            tabContents.value[newPath] = tabContents.value[oldId] || savedContent;
            delete tabContents.value[oldId];
        }
        if (activeTabs.value.includes(oldId)) {
            activeTabs.value = activeTabs.value.map(id => id === oldId ? newPath : id);
        }
        if (tabAccessTime.has(oldId)) {
            tabAccessTime.set(newPath, tabAccessTime.get(oldId)!);
            tabAccessTime.delete(oldId);
        }

        if (currentNoteId.value === oldId) {
            currentNoteId.value = newPath;
            await invoke('update_note', { vaultPath: props.vaultPath, path: newPath, content: `${buildFrontmatter(note)}\n\n${tabContents.value[newPath] || savedContent || ''}` });
        } else {
            const rawContent = await invoke<string>('read_note', { vaultPath: props.vaultPath, path: newPath });
            let body = rawContent;
            if (rawContent.startsWith('---\n') || rawContent.startsWith('---\r\n')) {
                const splitIdx = rawContent.indexOf('---', 3);
                if (splitIdx > 0) body = rawContent.substring(splitIdx + 3).replace(/^\s+/, '');
            }
            await invoke('update_note', { vaultPath: props.vaultPath, path: newPath, content: `${buildFrontmatter(note)}\n\n${body}` });
        }
        
        if (needsSave) {
            saveNoteForTab(newPath);
        }
        delete focusedTitles.value[oldId];
        delete focusedTitles.value[newPath];
        scanVault();
    } catch(err) { alert(err); }
};

const renameTopTitle = async (e: Event) => {
    const isEnter = e.type === 'keydown' && (e as KeyboardEvent).key === 'Enter';
    const newTitle = (e.target as HTMLInputElement).value.trim();
    const note = notes.value.find(n => n.id === currentNoteId.value);
    
    const focusEditor = () => {
        if (editorRefs.value && editorRefs.value.length > 0) {
            editorRefs.value.forEach(ref => {
                if (ref && typeof ref.focus === 'function') ref.focus();
            });
        }
    };

    if (!note || note.title === newTitle || !newTitle) {
        if (isEnter) focusEditor();
        if (note) delete focusedTitles.value[note.id];
        return;
    }
    
    try {
        const oldId = note.id;
        // Cancel any pending auto-save for the old path to prevent it from recreating the file after rename
        if (saveTimeouts.has(oldId)) {
            clearTimeout(saveTimeouts.get(oldId)!);
            saveTimeouts.delete(oldId);
        }
        const savedContent = tabContents.value[oldId] || '';
        const newPath = await invoke<string>('rename_note', { vaultPath: props.vaultPath, oldPath: oldId, newName: newTitle });
        
        // Secondary cancellation: if the user typed during the await rename_note, a new timeout for the old path might have been created.
        let needsSave = false;
        if (saveTimeouts.has(oldId)) {
            clearTimeout(saveTimeouts.get(oldId)!);
            saveTimeouts.delete(oldId);
            needsSave = true;
        }

        note.title = newTitle;
        renamedTabs.set(oldId, newPath);
        
        if (tabContents.value[oldId] !== undefined) {
            tabContents.value[newPath] = tabContents.value[oldId];
            delete tabContents.value[oldId];
        }
        if (activeTabs.value.includes(oldId)) {
            activeTabs.value = activeTabs.value.map(id => id === oldId ? newPath : id);
        }
        if (tabAccessTime.has(oldId)) {
            tabAccessTime.set(newPath, tabAccessTime.get(oldId)!);
            tabAccessTime.delete(oldId);
        }

        currentNoteId.value = newPath;
        await invoke('update_note', { vaultPath: props.vaultPath, path: newPath, content: `${buildFrontmatter(note)}\n\n${tabContents.value[newPath] || savedContent || ''}` });
        scanVault();
        
        if (needsSave) {
            saveNoteForTab(newPath);
        }
        
        if (isEnter) {
            setTimeout(focusEditor, 50);
        }
    } catch(err) { alert(err); }
};

const addTag = async (e: KeyboardEvent) => {
   if (e.key === 'Enter' && newTagInput.value.trim()) {
       const note = notes.value.find(n => n.id === currentNoteId.value);
       if (note && !note.tags.includes(newTagInput.value.trim())) {
           note.tags.push(newTagInput.value.trim());
           newTagInput.value = '';
           await invoke('update_note', { vaultPath: props.vaultPath, path: note.id, content: `${buildFrontmatter(note)}\n\n${currentContent.value}` });
           scanVault();
       }
   }
};

const removeTag = async (tagToRemove: string) => {
   const note = notes.value.find(n => n.id === currentNoteId.value);
   if (note) {
       note.tags = note.tags.filter(t => t !== tagToRemove);
       await invoke('update_note', { vaultPath: props.vaultPath, path: note.id, content: `${buildFrontmatter(note)}\n\n${currentContent.value}` });
       scanVault();
   }
};

// ─── Tags Data Logic ───────────────────────────────────────
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

// ─── API Calls ─────────────────────────────────────────────
async function scanVault() {
   if (!props.vaultPath) return;
   console.trace('[NoteApp] scanVault called');
   try {
       const scannedNotes = await invoke<NoteMetadata[]>('scan_vault_path', { vaultPath: props.vaultPath });
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
    console.trace('[NoteApp] createNewNote called, isCreatingNote=', isCreatingNote);
    if (!props.vaultPath || isCreatingNote) return;
    isCreatingNote = true;
    suppressWatcherUntil = Date.now() + 3000;
    try {
        console.log('[NoteApp] Invoking create_new_note...');
        const newPath = await invoke<string>('create_new_note', { vaultPath: props.vaultPath });
        console.log('[NoteApp] Created:', newPath);
        await scanVault();
        if (newPath) {
            currentNoteId.value = newPath;
            viewMode.value = 'editor';
            await nextTick();
            const titleInput = document.querySelector('.note-title-input') as HTMLInputElement;
            if (titleInput) {
                titleInput.focus();
                titleInput.select();
            }
        }
    } catch(e) { console.error("Failed to create note:", e); }
    finally { isCreatingNote = false; }
}

async function openDailyNote() {
    if (!props.vaultPath) return;
    try {
        const finalFormat = isValidDailyFormat.value ? dailyNoteFormat.value : 'YYYY-MM-DD';
        const tag = dailyNoteTag.value.trim();
        const dailyPath = await invoke<string>('open_daily_note', { vaultPath: props.vaultPath, formatStr: finalFormat, tag });
        await scanVault();
        if (dailyPath) { currentNoteId.value = dailyPath; viewMode.value = 'editor'; }
    } catch(e) { console.error("Failed to open daily note:", e); }
}

// ─── Save & Editor ─────────────────────────────────────────
const saveNoteForTab = (rawTabId: string) => {
    let tabId = rawTabId;
    while (renamedTabs.has(tabId)) {
        tabId = renamedTabs.get(tabId)!;
    }
    const note = notes.value.find(n => n.id === tabId);
    if (!note) { console.warn('[NoteApp] saveNoteForTab: note not found for', tabId); return; }
    const existing = saveTimeouts.get(tabId);
    if (existing) clearTimeout(existing);
    saveTimeouts.set(tabId, setTimeout(async () => {
        saveTimeouts.delete(tabId);
        suppressWatcherUntil = Date.now() + 3000;
        const content = tabContents.value[tabId] || '';
        console.log('[NoteApp] saveNoteForTab executing for:', tabId, 'noteId:', note.id, 'content length:', content.length);
        let fullRaw = `${buildFrontmatter(note)}\n\n${content}`;
        try {
            await invoke('update_note', { vaultPath: props.vaultPath, path: note.id, content: fullRaw });
            note.summary = content.substring(0, 150).trim();
            emit('note-updated', { id: note.id, content });
        } catch(e) { console.error("Failed to save note:", e); }
    }, 600));
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

const onEditorUpdate = (val: string, rawTabId: string) => {
    let tabId = rawTabId;
    while (renamedTabs.has(tabId)) {
        tabId = renamedTabs.get(tabId)!;
    }
    console.log('[NoteApp] onEditorUpdate for tabId:', tabId, 'original:', rawTabId, 'val length:', val.length);
    tabContents.value[tabId] = val;
    if (currentNoteId.value === tabId) {
        emit('note-updated', { id: tabId, content: val });
    }
    saveNoteForTab(tabId);
};

watch(currentNoteId, async (newId) => {
    if (newId) {
        await loadNoteFile(newId);
        try {
            currentBacklinks.value = await invoke('get_note_backlinks', { vaultPath: props.vaultPath, targetId: newId.split(/[\\/]/).pop() || newId });
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

// ─── Derived State ─────────────────────────────────────────
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
      if (a.pinned && !b.pinned) return -1;
      if (!a.pinned && b.pinned) return 1;
      return b.date.localeCompare(a.date);
  });
});

// ─── Public API for parent (Nexus cross-navigation) ────────
const openNoteById = (id: string) => {
    handleOpenInternalNote(id);
    viewMode.value = 'editor';
};
defineExpose({ openNoteById, scanVault, notes, tabContents, loadNoteFile, currentNoteId });

// ─── Lifecycle ─────────────────────────────────────────────
onMounted(async () => {
  window.addEventListener('mousemove', onMouseMove);
  window.addEventListener('mouseup', onMouseUp);

  if (props.isFloatingView && props.floatingNoteId) {
      currentNoteId.value = props.floatingNoteId;
      viewMode.value = 'editor';
      showNoteSidebar.value = false;
      showRightSidebar.value = false;
  }

  if (props.vaultPath) {
     await scanVault();
  }

  let unlistenFns: (() => void)[] = [];

  listen('note-updated', (event: any) => {
      const data = event.payload as { id: string, content: string };
      if (currentNoteId.value === data.id) return;
      if (tabContents.value[data.id] !== undefined) {
         tabContents.value[data.id] = data.content;
      }
  }).then(fn => unlistenFns.push(fn));

  listen('vault-changed', () => { scanVault(); }).then(fn => unlistenFns.push(fn));
  
  listen('vault-file-modified', () => {
      if (Date.now() < suppressWatcherUntil) return;
      scanVault();
  }).then(fn => unlistenFns.push(fn));
  
  listen('vault-file-created-deleted', () => {
      if (Date.now() < suppressWatcherUntil) return;
      scanVault();
  }).then(fn => unlistenFns.push(fn));

  const onClickOutside = () => { activeContextMenu.value = null; };
  document.addEventListener('click', onClickOutside);

  onUnmounted(() => {
      window.removeEventListener('mousemove', onMouseMove);
      window.removeEventListener('mouseup', onMouseUp);
      document.removeEventListener('click', onClickOutside);
      unlistenFns.forEach(fn => fn());
      unlistenFns = [];
  });
});
</script>

<template>
  <div class="flex flex-1 h-full overflow-hidden"
       :class="{'cursor-col-resize': isDraggingNoteSidebar || isDraggingRightSidebar}">
    <!-- Note Sidebar -->
    <aside 
      v-show="showNoteSidebar && !isFloatingView" 
      class="border-r border-[#e6e6e6] dark:border-[#2c2c2c] bg-[#fbfbfc] dark:bg-[#191919] flex flex-col relative shrink-0"
      :style="{ width: wNoteSidebar + 'px' }"
    >
      <div class="absolute top-0 right-0 w-1.5 h-full cursor-col-resize hover:bg-black/10 dark:hover:bg-white/10 z-10 opacity-0 hover:opacity-100 transition-opacity" @mousedown.stop="startDragNoteSidebar"></div>

      <div class="h-14 flex-shrink-0 flex items-center justify-end px-4 border-b border-[#e6e6e6] dark:border-[#2c2c2c]" data-tauri-drag-region>
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
                   <div class="w-full grid grow-wrap" :data-replicated-value="(focusedTitles[tabId] !== undefined ? focusedTitles[tabId] : notes.find(n => n.id === tabId)?.title) || ''">
                     <textarea class="note-title-input w-full text-4xl font-bold bg-transparent border-none outline-none text-[#1c1c1e] dark:text-[#f4f4f5] placeholder:text-gray-300 dark:placeholder:text-gray-700 resize-none overflow-hidden col-start-1 row-start-1 h-full" 
                       rows="1"
                       :value="focusedTitles[tabId] !== undefined ? focusedTitles[tabId] : notes.find(n => n.id === tabId)?.title" 
                       @focus="focusedTitles[tabId] = ($event.target as HTMLTextAreaElement).value"
                       @input="focusedTitles[tabId] = ($event.target as HTMLTextAreaElement).value"
                       @blur="renameTopTitle" 
                       @keydown.enter.prevent="renameTopTitle" 
                       placeholder="Note Title"></textarea>
                   </div>
                </div>
                <div class="mt-4 pb-20 w-full text-text dark:text-text-dark">
                   <TiptapEditor ref="editorRefs" :model-value="tabContents[tabId]" :vault-path="vaultPath" :notes="notes" @update:model-value="(val: string) => onEditorUpdate(val, tabId)" @open-internal-note="handleOpenInternalNote" />
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
    <aside v-if="currentNoteId && !isFloatingView" v-show="showRightSidebar" class="shrink-0 relative border-l border-[#e6e6e6] dark:border-[#2c2c2c] bg-[#fbfbfc] dark:bg-[#191919] flex flex-col overflow-hidden" :style="{ width: wRightSidebar + 'px' }">
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

    <!-- Rename Modal (replaces window.prompt for mobile compat) -->
    <Teleport to="body">
      <div v-if="renameModal.show" class="fixed inset-0 z-[999] flex items-center justify-center bg-black/40 backdrop-blur-sm" @click.self="renameModal.show = false">
        <div class="bg-white dark:bg-[#2a2a2a] rounded-2xl shadow-2xl p-6 w-80 border border-[#e6e6e6] dark:border-[#3a3a3a]">
          <h3 class="text-base font-semibold text-[#1c1c1e] dark:text-[#f4f4f5] mb-4">Rename Note</h3>
          <input
            v-model="renameModal.value"
            type="text"
            class="w-full px-3 py-2 rounded-lg border border-[#e0e0e0] dark:border-[#444] bg-white dark:bg-[#1e1e1e] text-[#1c1c1e] dark:text-[#f4f4f5] text-sm focus:outline-none focus:ring-2 focus:ring-black/10 dark:focus:ring-white/20"
            @keydown.enter="confirmRename"
            autofocus
          />
          <div class="flex justify-end gap-2 mt-4">
            <button @click="renameModal.show = false" class="px-4 py-1.5 text-sm rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-[#333] transition-colors">Cancel</button>
            <button @click="confirmRename" class="px-4 py-1.5 text-sm rounded-lg bg-black dark:bg-white text-white dark:text-black font-medium hover:opacity-80 transition-opacity">Rename</button>
          </div>
        </div>
      </div>
    </Teleport>
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

/* Auto-resizing textarea for note title */
.grow-wrap {
  display: grid;
}
.grow-wrap::after {
  content: attr(data-replicated-value) " ";
  white-space: pre-wrap;
  visibility: hidden;
  grid-area: 1 / 1 / 2 / 2;
  font-size: 2.25rem; /* Matches text-4xl */
  line-height: 2.5rem; /* Matches text-4xl */
  font-weight: 700; /* Matches font-bold */
  word-break: break-word;
}
.grow-wrap > textarea {
  grid-area: 1 / 1 / 2 / 2;
}
</style>
