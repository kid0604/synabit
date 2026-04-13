<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue';
import { FileText, Search, Settings, PanelLeft, PanelLeftClose, PanelRight, PanelRightClose, Hash, FolderOpen, Plus, MoreVertical, Pin, Trash2, Edit2, X, Calendar, CheckSquare, Zap, Globe, ArrowLeft, ExternalLink } from 'lucide-vue-next';
import { invoke } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { open } from '@tauri-apps/plugin-dialog';
import TiptapEditor from './components/TiptapEditor.vue';
import QuickCap from './components/QuickCap.vue';
import Tasks from './components/Tasks.vue';
import CalendarApp from './components/CalendarApp.vue';
import Nexus from './components/Nexus.vue';
import FileManager from './components/FileManager.vue';

// --- Vault & Data State ---
const vaultPath = ref<string>(localStorage.getItem('synabitVaultPath') || '');

interface NoteMetadata {
  id: string;
  title: string;
  summary: string;
  date: string;
  tags: string[];
  path: string;
  pinned: boolean;
}

const notes = ref<NoteMetadata[]>([]);
const currentNoteId = ref<string | null>(null);

// --- App View State ---
const activeTool = ref<'nexus' | 'quickcap' | 'note' | 'task' | 'calendar' | 'file' | 'settings'>('nexus');

const themeMode = ref<'light' | 'dark' | 'system'>(localStorage.getItem('synabitThemeMode') as 'light' | 'dark' | 'system' || 'system');

const applyTheme = () => {
  const isDark = themeMode.value === 'dark' || (themeMode.value === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches);
  if (isDark) {
    document.documentElement.classList.add('dark');
  } else {
    document.documentElement.classList.remove('dark');
  }
};

watch(themeMode, (newMode) => {
  localStorage.setItem('synabitThemeMode', newMode);
  applyTheme();
});

// --- Size & Toggle State ---
const wNoteSidebar = ref(300);
const showNoteSidebar = ref(true);

const wRightSidebar = ref(288);
const showRightSidebar = ref(true);

// --- Drag Logic ---
const isDraggingNoteSidebar = ref(false);
const startDragNoteSidebar = () => { isDraggingNoteSidebar.value = true; };

const isDraggingRightSidebar = ref(false);
const startDragRightSidebar = () => { isDraggingRightSidebar.value = true; };

const onMouseMove = (e: MouseEvent) => {
  if (isDraggingNoteSidebar.value) {
    // 64 is the width of the fixed global navigation sidebar
    wNoteSidebar.value = Math.max(220, Math.min(e.clientX - 64, 600));
  } else if (isDraggingRightSidebar.value) {
    wRightSidebar.value = Math.max(200, Math.min(window.innerWidth - e.clientX, 600));
  }
};

const onMouseUp = () => {
  if (isDraggingNoteSidebar.value) {
    isDraggingNoteSidebar.value = false;
  }
  if (isDraggingRightSidebar.value) {
    isDraggingRightSidebar.value = false;
  }
};

const isFloatingView = ref(false);

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
  }

  // Listen to cross-window note updates
  listen('note-updated', (event: any) => {
      const data = event.payload as { id: string, content: string };
      // If we have it in the cache, update it instantly
      if (tabContents.value[data.id] !== undefined) {
         tabContents.value[data.id] = data.content;
      }
      // If it's the current note, update current content so it flows to Tiptap
      if (currentNoteId.value === data.id) {
         currentContent.value = data.content;
      }
  });
  
  getCurrentWindow().onCloseRequested(async () => {
      if (currentNoteId.value) {
          const note = notes.value.find(n => n.id === currentNoteId.value);
          if (note && currentContent.value) {
              let fullRaw = `${buildFrontmatter(note)}\n\n${currentContent.value}`;
              try {
                  await invoke('update_note', { path: note.id, content: fullRaw });
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

// --- Note Manager State ---
const viewMode = ref<'editor' | 'manager'>('editor');
const managerFilter = ref('');
const managerSearchQuery = ref('');

// --- Context Menu & Action Logic ---
const activeContextMenu = ref<string | null>(null);
const searchQuery = ref('');
const newTagInput = ref('');

const toggleContext = (id: string, e: Event) => {
  e.stopPropagation();
  activeContextMenu.value = activeContextMenu.value === id ? null : id;
};

const buildFrontmatter = (n: NoteMetadata) => {
    return `---\ntitle: "${n.title}"\npinned: ${n.pinned}\ntags: [${n.tags.map(t=>`"${t}"`).join(', ')}]\n---`;
};

const togglePin = async (id: string) => {
    const note = notes.value.find(n => n.id === id);
    if (!note) return;
    note.pinned = !note.pinned;
    try {
        const rawContent = await invoke<string>('read_note', { path: id });
        let body = rawContent;
        if (rawContent.startsWith('---\n') || rawContent.startsWith('---\r\n')) {
            const splitIdx = rawContent.indexOf('---', 3);
            if (splitIdx > 0) body = rawContent.substring(splitIdx + 3).replace(/^\s+/, '');
        }
        await invoke('update_note', { path: id, content: `${buildFrontmatter(note)}\n\n${body}` });
        scanVault();
    } catch(e) { console.error('Pin fail:', e); }
};

const deleteNote = async (id: string) => {
    if (!confirm('Are you sure you want to delete this note irreversibly?')) return;
    try {
        await invoke('delete_note', { path: id });
        if (currentNoteId.value === id) {
           currentNoteId.value = null;
           currentContent.value = '';
        }
        scanVault();
    } catch(e) { console.error('Delete fail:', e); }
};

const openInNewWindow = async (id: string) => {
    try {
        await invoke('spawn_note_window', { noteId: id });
    } catch(e) {
        console.error("Failed to open note in new window", e);
    }
    activeContextMenu.value = null;
};

const handleRenamePrompt = async (id: string) => {
    const note = notes.value.find(n => n.id === id);
    if (!note) return;
    const newName = prompt("Enter new note name:", note.title);
    if (newName && newName !== note.title) {
       try {
           const newPath = await invoke<string>('rename_note', { 
               vaultPath: vaultPath.value, oldPath: note.id, newName 
           });
           note.title = newName;
           // If editing the active note, sync id properly
           if (currentNoteId.value === note.id) {
               currentNoteId.value = newPath;
               await invoke('update_note', { path: newPath, content: `${buildFrontmatter(note)}\n\n${currentContent.value}` });
           } else {
               // Update frontmatter for inactive note
               const rawContent = await invoke<string>('read_note', { path: newPath });
               let body = rawContent;
               if (rawContent.startsWith('---\n') || rawContent.startsWith('---\r\n')) {
                   const splitIdx = rawContent.indexOf('---', 3);
                   if (splitIdx > 0) body = rawContent.substring(splitIdx + 3).replace(/^\s+/, '');
               }
               await invoke('update_note', { path: newPath, content: `${buildFrontmatter(note)}\n\n${body}` });
           }
           scanVault();
       } catch(err) {
           alert(err);
       }
    }
};

const renameTopTitle = async (e: Event) => {
    const newTitle = (e.target as HTMLInputElement).value.trim();
    const note = notes.value.find(n => n.id === currentNoteId.value);
    if (!note || note.title === newTitle || !newTitle) return;
    try {
        const newPath = await invoke<string>('rename_note', { 
            vaultPath: vaultPath.value, oldPath: note.id, newName: newTitle 
        });
        note.title = newTitle;
        currentNoteId.value = newPath;
        // The path changed, so we must trigger a save properly with new frontmatter
        await invoke('update_note', { path: newPath, content: `${buildFrontmatter(note)}\n\n${currentContent.value}` });
        scanVault();
    } catch(err) {
        alert(err);
    }
};

const addTag = async (e: KeyboardEvent) => {
   if (e.key === 'Enter' && newTagInput.value.trim()) {
       const note = notes.value.find(n => n.id === currentNoteId.value);
       if (note && !note.tags.includes(newTagInput.value.trim())) {
           note.tags.push(newTagInput.value.trim());
           newTagInput.value = '';
           await invoke('update_note', { path: note.id, content: `${buildFrontmatter(note)}\n\n${currentContent.value}` });
           scanVault();
       }
   }
};

const removeTag = async (tagToRemove: string) => {
   const note = notes.value.find(n => n.id === currentNoteId.value);
   if (note) {
       note.tags = note.tags.filter(t => t !== tagToRemove);
       await invoke('update_note', { path: note.id, content: `${buildFrontmatter(note)}\n\n${currentContent.value}` });
       scanVault();
   }
};

// --- Tags Data Logic ---
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
  
  // Count tags and their children
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
    // Only process top-level tags here
    if (!name.includes('/')) {
      const children: TagNode[] = Array.from(data.children).map(childName => ({
        name: childName,
        basename: childName.split('/')[1],
        count: map.get(childName)?.count || 0,
        expanded: false,
        children: []
      }));
      
      tree.push({
        name,
        basename: name,
        count: data.count,
        expanded: true,
        children
      });
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

// --- API Calls ---
const selectVault = async () => {
    try {
        const selected = await open({
            title: 'Select Note Vault Directory',
            directory: true,
            multiple: false,
        });
        if (selected) {
            vaultPath.value = selected as string;
            localStorage.setItem('synabitVaultPath', vaultPath.value);
            scanVault();
        }
    } catch(err) {
        console.error(err);
    }
};

const scanVault = async () => {
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
   } catch(e) {
       console.error("Failed to scan vault:", e);
   }
}

const createNewNote = async () => {
    if (!vaultPath.value) return;
    try {
        const newPath = await invoke<string>('create_new_note', { vaultPath: vaultPath.value });
        await scanVault();
        // focus the new note
        if (newPath) {
           currentNoteId.value = newPath;
           viewMode.value = 'editor';
        }
    } catch(e) {
        console.error("Failed to create note:", e);
    }
}

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
    
    // Update access time for LRU
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
            const rawContent = await invoke<string>('read_note', { path: id });
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

const saveNoteFile = () => {
    if (!currentNoteId.value) return;
    const note = notes.value.find(n => n.id === currentNoteId.value);
    if (!note) return;
    
    clearTimeout(saveTimeout);
    saveTimeout = setTimeout(async () => {
        let fullRaw = `${buildFrontmatter(note)}\n\n${currentContent.value}`;
        try {
            await invoke('update_note', { path: note.id, content: fullRaw });
            note.summary = currentContent.value.substring(0, 150).trim();
            // Broadcast the update to all windows
            emit('note-updated', { id: note.id, content: currentContent.value });
        } catch(e) {
             console.error("Failed to save note:", e);
        }
    }, 600);
}

const currentBacklinks = ref<NoteMetadata[]>([]);
const editorRefs = ref<any[]>([]);

const onEditorUpdate = (val: string) => {
    currentContent.value = val;
    if (currentNoteId.value) {
        emit('note-updated', { id: currentNoteId.value, content: val });
    }
    saveNoteFile();
};

watch(currentNoteId, async (newId) => {
    if (newId) {
        await loadNoteFile(newId);
        try {
            currentBacklinks.value = await invoke('get_note_backlinks', { vaultPath: vaultPath.value, targetId: newId.split(/[\\/]/).pop() || newId });
        } catch (e) {
            console.error(e);
            currentBacklinks.value = [];
        }
    } else {
        currentBacklinks.value = [];
    }
});

const handleOpenInternalNote = (noteId: string) => {
    const exists = notes.value.find(n => n.id === noteId);
    if (exists) {
        currentNoteId.value = noteId;
    } else {
        const existsByName = notes.value.find(n => n.id.endsWith(noteId));
        if (existsByName) currentNoteId.value = existsByName.id;
    }
};

// --- Derived State ---
const allTags = computed(() => {
    const counts = new Map<string, number>();
    notes.value.forEach(note => {
        note.tags.forEach(tag => {
            counts.set(tag, (counts.get(tag) || 0) + 1);
        });
    });
    return Array.from(counts.entries())
        .map(([name, count]) => ({ name, count }))
        .sort((a,b) => b.count - a.count);
});

const topTags = computed(() => allTags.value.slice(0, 10));

const topPinnedNotes = computed(() => {
    return filteredNotes.value.filter(n => n.pinned).slice(0, 5);
});

const recentNotes = computed(() => {
    return filteredNotes.value.filter(n => !n.pinned).slice(0, 10);
});

const openNoteManager = (filterType: string) => {
    managerFilter.value = filterType;
    viewMode.value = 'manager';
};

const managerFilteredNotes = computed(() => {
   let result = notes.value;
   if (managerSearchQuery.value.trim()) {
      const q = managerSearchQuery.value.toLowerCase();
      result = result.filter(n => n.title.toLowerCase().includes(q) || n.tags.some(t => t.toLowerCase().includes(q)));
   }
   
   if (managerFilter.value === 'notes' || !managerFilter.value || managerFilter.value === 'tags') {
      return result;
   } else if (managerFilter.value === 'pinned') {
      return result.filter(n => n.pinned);
   } else {
      return result.filter(n => n.tags.includes(managerFilter.value));
   }
});

const filteredNotes = computed(() => {
  let result = notes.value;
  
  if (searchQuery.value.trim()) {
      const q = searchQuery.value.toLowerCase();
      result = result.filter(n => 
          n.title.toLowerCase().includes(q) || 
          n.tags.some(t => t.toLowerCase().includes(q))
      );
  }
  
  if (selectedTags.value.size > 0) {
      result = result.filter(n => n.tags.some(t => selectedTags.value.has(t)));
  }
  
  return result.sort((a,b) => {
      // Pinned dồn lên trên
      if (a.pinned && !b.pinned) return -1;
      if (!a.pinned && b.pinned) return 1;
      // Trong cùng nhóm Pinned/Unpinned thì sort date
      return b.date.localeCompare(a.date);
  });
});

const handleEditFromNexus = (id: string, type: string) => {
    if (type === 'note') {
        activeTool.value = 'note';
        handleOpenInternalNote(id);
    } else if (type === 'quickcap') {
        activeTool.value = 'quickcap';
        localStorage.setItem('synabit_edit_target_id', id);
    } else if (type === 'task') {
        activeTool.value = 'task';
        localStorage.setItem('synabit_edit_target_id', id);
    }
};

const clearVault = () => {
    localStorage.removeItem('synabitVaultPath');
    vaultPath.value = '';
    activeTool.value = 'note';
};

</script>

<template>
  <div class="flex h-screen w-full bg-[#fdfdfc] text-[#1c1c1e] dark:bg-[#242424] dark:text-[#f4f4f5] font-sans overflow-hidden select-none"
       :class="{'cursor-col-resize': isDraggingNoteSidebar || isDraggingRightSidebar}">
       
    <!-- Application State 1: No Vault Selected -->
    <div v-if="!vaultPath" class="flex-1 flex flex-col items-center justify-center p-8 bg-[#fdfdfc] dark:bg-[#242424]" data-tauri-drag-region>
        <div class="max-w-md w-full text-center space-y-6">
            <div class="w-20 h-20 bg-gray-100 dark:bg-gray-800 rounded-full flex items-center justify-center mx-auto shadow-inner">
               <FileText class="w-10 h-10 text-gray-400" />
            </div>
            <div>
               <h1 class="text-2xl font-bold mb-2">Welcome to Synabit</h1>
               <p class="text-[#52525b] dark:text-[#a1a1aa] text-sm">A minimalist, tag-based Local-First note taking app. Please select a folder on your computer to store your Markdown notes.</p>
            </div>
            <button @click="selectVault" class="px-6 py-3 bg-black hover:bg-gray-800 text-white dark:bg-white dark:hover:bg-gray-200 dark:text-black rounded-lg font-medium shadow-md transition-all active:scale-95 flex items-center justify-center mx-auto gap-2">
               <FolderOpen class="w-5 h-5" />
               Open Vault
            </button>
        </div>
    </div>

    <!-- Application State 2: Vault Selected -->
    <template v-else>
      <!-- GLOBAL NAVIGATION SIDEBAR 1 -->
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
            <button @click="activeTool = 'settings'" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', activeTool === 'settings' ? 'bg-[#e6e6e6] text-black dark:bg-[#333] dark:text-white shadow-sm' : 'text-gray-500 hover:bg-gray-200 dark:hover:bg-gray-800']">
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
          <!-- Drag Handle -->
          <div 
            class="absolute top-0 right-0 w-1.5 h-full cursor-col-resize hover:bg-black/10 dark:hover:bg-white/10 z-10 opacity-0 hover:opacity-100 transition-opacity"
            @mousedown.stop="startDragNoteSidebar"
          ></div>

          <!-- Tool Header -->
          <div class="h-14 flex-shrink-0 flex items-center justify-between px-4 border-b border-[#e6e6e6] dark:border-[#2c2c2c]" data-tauri-drag-region>
             <span class="font-semibold text-sm">Notes Quick View</span>
             <div class="flex gap-1" @mousedown.stop>
               <button @click="createNewNote" class="px-2 py-1.5 flex items-center gap-1.5 rounded-md bg-[#e6e6e6] text-[#1c1c1e] dark:bg-[#333] dark:text-white hover:opacity-80 transition-opacity" title="New Note">
                 <Plus class="w-3.5 h-3.5" />
                 <span class="text-xs font-medium">New</span>
               </button>
             </div>
          </div>
          
          <div class="px-3 pt-3 pb-2 sticky top-0 bg-[#fbfbfc] dark:bg-[#191919] z-10" @mousedown.stop>
              <div class="relative w-full">
                <Search class="absolute left-2.5 top-1/2 -translate-y-1/2 w-4 h-4 text-[#8b8b8b] dark:text-[#71717a]" />
                <input 
                  v-model="searchQuery"
                  type="text" 
                  placeholder="Search notes..." 
                  class="w-full pl-8 pr-3 py-1.5 bg-white dark:bg-[#2c2c2c] border border-[#e6e6e6] dark:border-transparent mx-auto block rounded-md text-sm focus:outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-shadow text-[#1c1c1e] dark:text-[#f4f4f5] placeholder:text-gray-400 dark:placeholder:text-gray-500"
                >
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
                     <div 
                        v-for="note in topPinnedNotes" 
                        :key="note.id"
                        @click="currentNoteId = note.id; viewMode = 'editor'"
                        class="px-3 py-2 border border-transparent rounded-lg cursor-pointer transition-colors relative group"
                        :class="currentNoteId === note.id ? 'bg-white dark:bg-[#2a2a2a] shadow-sm border-[#e6e6e6] dark:border-[#3a3a3a]' : 'hover:bg-white/50 dark:hover:bg-[#252525] hover:border-[#e6e6e6] dark:hover:border-[#2f2f2f]'"
                     >
                        <div class="absolute right-2 top-2 opacity-0 group-hover:opacity-100 transition-opacity z-10" :class="{'opacity-100': activeContextMenu === note.id}">
                           <button @click.stop="(e) => toggleContext(note.id, e)" class="p-1 rounded bg-white dark:bg-[#2a2a2a] shadow-sm hover:bg-gray-100 border border-gray-200 dark:border-gray-600">
                              <MoreVertical class="w-3.5 h-3.5 text-gray-500"/>
                           </button>
                           <div v-if="activeContextMenu === note.id" class="absolute right-0 top-6 w-32 bg-white dark:bg-[#2c2c2c] shadow-lg rounded border border-gray-200 dark:border-gray-700 z-50 py-1 overflow-hidden">
                              <button @click.stop="togglePin(note.id)" class="w-full text-left px-3 py-2 text-xs hover:bg-gray-100 dark:hover:bg-gray-600 flex items-center gap-2">
                                 <Pin class="w-3 h-3" /> {{ note.pinned ? 'Unpin' : 'Pin' }}
                              </button>
                              <button @click.stop="openInNewWindow(note.id)" class="w-full text-left px-3 py-2 text-xs hover:bg-gray-100 dark:hover:bg-gray-600 flex items-center gap-2">
                                 <ExternalLink class="w-3 h-3" /> Open in New Window
                              </button>
                              <button @click.stop="handleRenamePrompt(note.id)" class="w-full text-left px-3 py-2 text-xs hover:bg-gray-100 dark:hover:bg-gray-600 flex items-center gap-2">
                                 <Edit2 class="w-3 h-3" /> Rename
                              </button>
                              <button @click.stop="deleteNote(note.id)" class="w-full text-left px-3 py-2 text-xs hover:bg-red-50 dark:hover:bg-red-900/30 text-red-600 dark:text-red-400 flex items-center gap-2">
                                 <Trash2 class="w-3 h-3" /> Delete
                              </button>
                           </div>
                        </div>
                        <div class="flex items-center gap-2 mb-1.5 pr-6">
                            <Pin class="w-3 h-3 text-orange-500 shrink-0 fill-orange-500/20" />
                            <span class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] truncate">{{ note.title || 'Untitled Note' }}</span>
                        </div>
                        <div class="flex flex-wrap gap-1" v-if="note.tags.length">
                            <span v-for="tag in note.tags" :key="tag" class="text-[9px] px-1.5 py-0.5 rounded bg-gray-200/60 dark:bg-[#333] text-gray-600 dark:text-gray-300">
                                {{ tag.split('/').pop() }}
                            </span>
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
                     <div 
                        v-for="note in recentNotes" 
                        :key="note.id"
                        @click="currentNoteId = note.id; viewMode = 'editor'"
                        class="px-3 py-2 border border-transparent rounded-lg cursor-pointer transition-colors relative group"
                        :class="currentNoteId === note.id ? 'bg-white dark:bg-[#2a2a2a] shadow-sm border-[#e6e6e6] dark:border-[#3a3a3a]' : 'hover:bg-white/50 dark:hover:bg-[#252525] hover:border-[#e6e6e6] dark:hover:border-[#2f2f2f]'"
                     >
                        <div class="absolute right-2 top-2 opacity-0 group-hover:opacity-100 transition-opacity z-10" :class="{'opacity-100': activeContextMenu === note.id}">
                           <button @click.stop="(e) => toggleContext(note.id, e)" class="p-1 rounded bg-white dark:bg-[#2a2a2a] shadow-sm hover:bg-gray-100 border border-gray-200 dark:border-gray-600">
                              <MoreVertical class="w-3.5 h-3.5 text-gray-500"/>
                           </button>
                           <div v-if="activeContextMenu === note.id" class="absolute right-0 top-6 w-32 bg-white dark:bg-[#2c2c2c] shadow-lg rounded border border-gray-200 dark:border-gray-700 z-50 py-1 overflow-hidden">
                              <button @click.stop="togglePin(note.id)" class="w-full text-left px-3 py-2 text-xs hover:bg-gray-100 dark:hover:bg-gray-600 flex items-center gap-2">
                                 <Pin class="w-3 h-3" /> {{ note.pinned ? 'Unpin' : 'Pin' }}
                              </button>
                              <button @click.stop="openInNewWindow(note.id)" class="w-full text-left px-3 py-2 text-xs hover:bg-gray-100 dark:hover:bg-gray-600 flex items-center gap-2">
                                 <ExternalLink class="w-3 h-3" /> Open in New Window
                              </button>
                              <button @click.stop="handleRenamePrompt(note.id)" class="w-full text-left px-3 py-2 text-xs hover:bg-gray-100 dark:hover:bg-gray-600 flex items-center gap-2">
                                 <Edit2 class="w-3 h-3" /> Rename
                              </button>
                              <button @click.stop="deleteNote(note.id)" class="w-full text-left px-3 py-2 text-xs hover:bg-red-50 dark:hover:bg-red-900/30 text-red-600 dark:text-red-400 flex items-center gap-2">
                                 <Trash2 class="w-3 h-3" /> Delete
                              </button>
                           </div>
                        </div>
                        <div class="flex items-center gap-2 mb-1.5 pr-6">
                            <FileText class="w-3.5 h-3.5 text-gray-400 shrink-0 opacity-80" />
                            <span class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] truncate">{{ note.title || 'Untitled Note' }}</span>
                        </div>
                        <div class="flex flex-wrap gap-1" v-if="note.tags.length">
                            <span v-for="tag in note.tags" :key="tag" class="text-[9px] px-1.5 py-0.5 rounded bg-gray-200/60 dark:bg-[#333] text-gray-600 dark:text-gray-300">
                                {{ tag.split('/').pop() }}
                            </span>
                        </div>
                     </div>
                 </div>
                 <!-- Empty State -->
                 <div v-if="recentNotes.length === 0" class="p-8 text-center text-sm text-[#52525b] dark:text-[#a1a1aa]">
                   No notes match.
                 </div>
             </div>
    </div>
  </aside>

      <!-- Main Area: Editor / Manager -->
      <main class="flex-1 flex flex-col bg-[#fdfdfc] dark:bg-[#242424] min-w-[300px]" @mousedown.stop>
        <template v-if="viewMode === 'editor'">
            <!-- Controls Header Area -->
            <div v-if="!isFloatingView" class="h-10 flex-shrink-0 w-full flex items-center justify-between px-4" data-tauri-drag-region>
              <div class="flex gap-2">
                <button @click="showNoteSidebar = !showNoteSidebar" class="p-1 rounded-md hover:bg-gray-200 dark:hover:bg-gray-800 text-gray-500 transition-colors" title="Toggle Sidebar">
                  <PanelLeftClose v-if="showNoteSidebar" class="w-4 h-4" />
                  <PanelLeft v-else class="w-4 h-4" />
                </button>
              </div>
              
              <div class="flex gap-2">
                <button v-if="currentBacklinks.length > 0" @click="showRightSidebar = !showRightSidebar" class="p-1 relative ml-2 rounded-md hover:bg-gray-200 dark:hover:bg-gray-800 text-gray-500 transition-colors" title="Toggle Backlinks">
                  <PanelRightClose v-if="showRightSidebar" class="w-4 h-4" />
                  <PanelRight v-else class="w-4 h-4" />
                </button>
              </div>
            </div>
            
            <div v-else class="h-8 flex-shrink-0 w-full z-50 bg-[#fdfdfc] dark:bg-[#242424]" data-tauri-drag-region></div>

            <template v-if="activeTabs.length > 0">
              <template v-for="tabId in activeTabs" :key="tabId">
                <div v-show="currentNoteId === tabId" class="flex-1 overflow-y-auto w-full relative">
                  <!-- Loading State -->
                  <div v-if="tabContents[tabId] === undefined" class="absolute inset-0 flex items-center justify-center bg-[#fdfdfc] dark:bg-[#242424]">
                      <div class="w-8 h-8 rounded-full border-2 border-gray-200 border-t-gray-400 animate-spin"></div>
                  </div>
                  
                  <!-- Editor Content -->
                  <div v-else class="px-12 pb-12 max-w-4xl mx-auto w-full cursor-text">
                  <div class="mb-4 pt-4">
                     <div class="flex gap-2 mb-4 flex-wrap items-center">
                        <span v-for="tag in notes.find(n => n.id === tabId)?.tags" :key="tag" class="text-xs px-2 py-1 rounded-md bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-300 flex items-center gap-1 group/tag">
                            <Hash class="w-3 h-3 opacity-50"/>
                            {{ tag }}
                            <button @click="removeTag(tag)" class="opacity-0 group-hover/tag:opacity-100 hover:text-red-500 transition-opacity ml-1 p-0.5"><X class="w-3 h-3"/></button>
                         </span>
                         
                         <!-- Add tag Input -->
                         <div class="relative flex items-center">
                            <Plus class="w-3 h-3 absolute left-1.5 text-gray-400" />
                            <input 
                               v-model="newTagInput"
                               @keydown="addTag"
                               placeholder="Add tag..."
                               class="text-xs bg-transparent border border-dashed border-gray-300 dark:border-gray-600 rounded-md py-1 pl-5 pr-2 w-24 focus:w-32 focus:outline-none focus:border-gray-400 transition-all text-[#1c1c1e] dark:text-[#f4f4f5]"
                            />
                         </div>
                     </div>
                    <input 
                      type="text" 
                      class="w-full text-4xl font-bold bg-transparent border-none outline-none text-[#1c1c1e] dark:text-[#f4f4f5] placeholder:text-gray-300 dark:placeholder:text-gray-700"
                      :value="notes.find(n => n.id === tabId)?.title"
                      @blur="renameTopTitle"
                      @keydown.enter="renameTopTitle"
                      placeholder="Note Title"
                    >
                  </div>
                  <div class="mt-4 pb-20 w-full text-text dark:text-text-dark">
                     <TiptapEditor 
                        ref="editorRefs"
                        :model-value="tabContents[tabId]" 
                        :vault-path="vaultPath"
                        :notes="notes"
                        @update:model-value="onEditorUpdate" 
                        @open-internal-note="handleOpenInternalNote"
                     />
                     <!-- Backlinks UI moved to Right Sidebar -->
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
               <!-- Header -->
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
                   <!-- Search Bar -->
                   <div class="relative w-full mb-8">
                     <Search class="absolute left-4 top-1/2 -translate-y-1/2 w-5 h-5 text-[#8b8b8b] dark:text-[#71717a]" />
                     <input 
                       v-model="managerSearchQuery"
                       type="text" 
                       placeholder="Search notes or tags..." 
                       class="w-full pl-12 pr-4 py-3 bg-white dark:bg-[#1a1a1a] border border-[#e6e6e6] dark:border-[#2c2c2c] rounded-xl text-base shadow-sm focus:outline-none focus:ring-2 focus:ring-purple-500/50 transition-shadow text-[#1c1c1e] dark:text-[#f4f4f5]"
                     >
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
                              <tr v-for="note in managerFilteredNotes" :key="note.id" 
                                  @click="currentNoteId = note.id; viewMode = 'editor'"
                                  class="hover:bg-gray-50 dark:hover:bg-[#2a2a2a] cursor-pointer transition-colors group">
                                 <td class="py-3 px-4 w-8">
                                    <Pin v-if="note.pinned" class="w-3.5 h-3.5 text-orange-500 fill-orange-500/20" />
                                    <FileText v-else class="w-3.5 h-3.5 text-gray-400 opacity-50" />
                                 </td>
                                 <td class="py-3 px-4 font-medium text-[#1c1c1e] dark:text-[#f4f4f5] max-w-[250px] truncate">
                                    {{ note.title || 'Untitled Note' }}
                                 </td>
                                 <td class="py-3 px-4">
                                    <div class="flex flex-wrap gap-1" v-if="note.tags.length">
                                       <span v-for="tag in note.tags.slice(0, 3)" :key="tag" class="text-[10px] px-1.5 py-0.5 rounded bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-300">
                                          {{ tag.split('/').pop() }}
                                       </span>
                                       <span v-if="note.tags.length > 3" class="text-[10px] px-1.5 py-0.5 rounded bg-gray-100 dark:bg-gray-800 text-gray-500">+{{ note.tags.length - 3 }}</span>
                                    </div>
                                    <span v-else class="text-xs text-gray-400 italic">No tags</span>
                                 </td>
                                 <td class="py-3 px-4 text-xs text-gray-500 dark:text-gray-400 whitespace-nowrap text-right">
                                    {{ note.date }}
                                 </td>
                                 <td class="py-3 px-4 w-12 text-center" @click.stop>
                                    <div class="relative flex justify-center">
                                       <button @click="(e) => toggleContext('manager_'+note.id, e)" class="p-1 rounded opacity-0 group-hover:opacity-100 hover:bg-gray-200 dark:hover:bg-[#444] transition">
                                          <MoreVertical class="w-4 h-4 text-gray-500" />
                                       </button>
                                       <div v-if="activeContextMenu === 'manager_'+note.id" class="absolute right-6 top-0 w-32 bg-white dark:bg-[#2c2c2c] shadow-lg rounded border border-gray-200 dark:border-gray-700 z-50 py-1 overflow-hidden">
                                          <button @click.stop="togglePin(note.id); activeContextMenu = null;" class="w-full text-left px-3 py-2 text-xs hover:bg-gray-100 dark:hover:bg-gray-600 flex items-center gap-2">
                                             <Pin class="w-3 h-3" /> {{ note.pinned ? 'Unpin' : 'Pin' }}
                                          </button>
                                          <button @click.stop="deleteNote(note.id); activeContextMenu = null;" class="w-full text-left px-3 py-2 text-xs hover:bg-red-50 dark:hover:bg-red-900/30 text-red-600 dark:text-red-400 flex items-center gap-2">
                                             <Trash2 class="w-3 h-3" /> Delete
                                          </button>
                                       </div>
                                    </div>
                                 </td>
                              </tr>
                              <tr v-if="managerFilteredNotes.length === 0">
                                 <td colspan="5" class="py-12 text-center text-gray-500">
                                    No notes found matching current filters.
                                 </td>
                              </tr>
                           </tbody>
                        </table>
                     </div>
                   </div>
               </div>
            </div>
        </template>
      </main>

      <!-- Right Sidebar: Backlinks -->
      <aside v-if="currentNoteId && currentBacklinks.length > 0" v-show="showRightSidebar" class="shrink-0 relative border-l border-[#e6e6e6] dark:border-[#2c2c2c] bg-[#fbfbfc] dark:bg-[#191919] flex flex-col overflow-hidden" :style="{ width: wRightSidebar + 'px' }">
        <!-- Drag Handle -->
        <div 
          class="absolute top-0 left-0 w-1.5 h-full cursor-col-resize hover:bg-black/10 dark:hover:bg-white/10 z-10 opacity-0 hover:opacity-100 transition-opacity"
          @mousedown.stop="startDragRightSidebar"
        ></div>
        <div class="h-10 flex-shrink-0 flex items-center px-4 border-b border-[#e6e6e6] dark:border-[#2c2c2c]" data-tauri-drag-region>
            <Globe class="w-4 h-4 text-gray-500 mr-2" />
            <span class="font-bold text-[11px] tracking-wider text-gray-500 uppercase mt-0.5">Linked Mentions ({{ currentBacklinks.length }})</span>
            <button @click="showRightSidebar = false" class="p-1 ml-auto rounded-md hover:bg-gray-200 dark:hover:bg-gray-800 text-gray-400 transition-colors">
               <X class="w-3.5 h-3.5" />
            </button>
        </div>
        <div class="flex-1 overflow-y-auto p-4 space-y-3 bg-[#fdfdfc] dark:bg-[#242424]">
            <div 
              v-for="bl in currentBacklinks" 
              :key="bl.id" 
              @click="handleOpenInternalNote(bl.id)"
              class="p-3.5 rounded-xl border border-gray-100 dark:border-[#2c2c2c] bg-white dark:bg-[#1a1a1a] cursor-pointer hover:border-purple-500/50 dark:hover:border-purple-500/50 hover:shadow-md transition-all group"
            >
              <h5 class="text-sm font-semibold text-[#1c1c1e] dark:text-[#f4f4f5] mb-1.5 flex items-center gap-2">
                  <FileText class="w-3.5 h-3.5 opacity-40 group-hover:text-purple-500 group-hover:opacity-100 transition-colors"/> 
                  <span class="truncate">{{ bl.title }}</span>
              </h5>
              <p class="text-[11px] text-[#52525b] dark:text-[#a1a1aa] line-clamp-3 leading-relaxed opacity-80 group-hover:opacity-100 transition-opacity">{{ bl.summary || 'No text content available.' }}</p>
            </div>
        </div>
      </aside>
    </template>

    <!-- QUICKCAP TOOL VIEW -->
    <template v-else-if="activeTool === 'quickcap'">
       <main class="flex-1 overflow-hidden relative">
          <QuickCap :vaultPath="vaultPath" />
       </main>
    </template>

    <!-- NEXUS TOOL -->
    <template v-else-if="activeTool === 'nexus'">
       <main class="flex-1 overflow-hidden relative">
          <Nexus :vaultPath="vaultPath" @edit-item="handleEditFromNexus" />
       </main>
    </template>

    <!-- TASK TOOL -->
    <template v-else-if="activeTool === 'task'">
       <main class="flex-1 overflow-hidden relative">
          <Tasks :vaultPath="vaultPath" />
       </main>
    </template>
    
    <!-- CALENDAR TOOL -->
    <template v-else-if="activeTool === 'calendar'">
      <main class="flex-1 overflow-hidden relative">
         <CalendarApp :vaultPath="vaultPath" />
      </main>
    </template>
    
    <template v-else-if="activeTool === 'file'">
       <main class="flex-1 overflow-hidden relative">
          <FileManager :vaultPath="vaultPath" />
       </main>
    </template>

    <!-- SETTINGS VIEW -->
    <template v-else-if="activeTool === 'settings'">
      <main class="flex-1 overflow-y-auto bg-[#fdfdfc] dark:bg-[#242424] flex justify-center text-[#1c1c1e] dark:text-[#f4f4f5]">
         <div class="max-w-2xl w-full py-16 px-8">
            <h1 class="text-3xl font-bold mb-8">Settings</h1>
            
            <div class="space-y-8">
               <!-- Vault Management -->
               <section>
                  <h2 class="text-xl font-semibold mb-4 border-b border-[#e6e6e6] dark:border-[#2c2c2c] pb-2">Vault Management</h2>
                  <div class="bg-gray-50 dark:bg-[#1e1e1e] p-6 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c]">
                     <div class="mb-4">
                       <p class="text-sm text-gray-500 dark:text-gray-400 font-medium mb-1">Current Vault Location</p>
                       <p class="font-mono text-sm break-all text-black dark:text-white bg-white dark:bg-[#2a2a2a] p-2 rounded-md border border-gray-200 dark:border-transparent">{{ vaultPath }}</p>
                     </div>
                     <button @click="clearVault" class="px-5 py-2.5 bg-black hover:bg-gray-800 text-white dark:bg-white dark:hover:bg-gray-200 dark:text-black rounded-lg text-sm font-medium transition-all shadow-md mt-2 flex items-center gap-2">
                        <FolderOpen class="w-4 h-4" /> Switch Vault Folder
                     </button>
                  </div>
               </section>
               
               <!-- Appearance -->
               <section>
                  <h2 class="text-xl font-semibold mb-4 border-b border-[#e6e6e6] dark:border-[#2c2c2c] pb-2">Appearance</h2>
                  <div class="p-6 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c] bg-gray-50 dark:bg-[#1e1e1e]">
                      <h3 class="text-sm font-medium mb-3">Theme Preference</h3>
                      <div class="flex items-center gap-6">
                         <label class="flex items-center gap-2 cursor-pointer group">
                            <input type="radio" value="light" v-model="themeMode" class="accent-black dark:accent-white w-4 h-4 cursor-pointer">
                            <span class="text-sm text-gray-700 dark:text-gray-300 group-hover:text-black dark:group-hover:text-white transition-colors">Light</span>
                         </label>
                         <label class="flex items-center gap-2 cursor-pointer group">
                            <input type="radio" value="dark" v-model="themeMode" class="accent-black dark:accent-white w-4 h-4 cursor-pointer">
                            <span class="text-sm text-gray-700 dark:text-gray-300 group-hover:text-black dark:group-hover:text-white transition-colors">Dark</span>
                         </label>
                         <label class="flex items-center gap-2 cursor-pointer group">
                            <input type="radio" value="system" v-model="themeMode" class="accent-black dark:accent-white w-4 h-4 cursor-pointer">
                            <span class="text-sm text-gray-700 dark:text-gray-300 group-hover:text-black dark:group-hover:text-white transition-colors">System</span>
                         </label>
                      </div>
                  </div>
               </section>
               
               <!-- About -->
               <section>
                  <h2 class="text-xl font-semibold mb-4 border-b border-[#e6e6e6] dark:border-[#2c2c2c] pb-2">About Synabit</h2>
                  <div class="p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c] bg-gray-50 dark:bg-[#1e1e1e]">
                      <p class="text-sm font-medium">Synabit v1.0.0-alpha</p>
                      <p class="text-xs text-gray-500 mt-1">A unified productivity workspace.</p>
                  </div>
               </section>
            </div>
         </div>
      </main>
    </template>
    
    </template>
  </div>
</template>

<style scoped>
[data-tauri-drag-region] {
  -webkit-app-region: drag;
}
</style>