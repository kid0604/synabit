<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue';
import { FileText, Search, Settings, PanelLeft, PanelLeftClose, PanelRight, PanelRightClose, Hash, FolderOpen, Plus, MoreVertical, Pin, Trash2, Edit2, X, Calendar, CheckSquare, Zap, Globe, ArrowLeft, ExternalLink, Sun, Cloud, RefreshCw, CloudOff } from 'lucide-vue-next';
import { invoke } from '@tauri-apps/api/core';
import { emit, listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { open, confirm } from '@tauri-apps/plugin-dialog';
import TiptapEditor from './components/TiptapEditor.vue';
import QuickCap from './components/QuickCap.vue';
import Tasks from './components/Tasks.vue';
import CalendarApp from './components/CalendarApp.vue';
import Nexus from './components/Nexus.vue';
import FileManager from './components/FileManager.vue';
import NoteGraph from './components/NoteGraph.vue';

// --- Vault & Data State ---
const vaultPath = ref<string>(localStorage.getItem('synabitVaultPath') || '');

// --- Google Drive State ---
const vaultType = ref<'local' | 'gdrive'>(localStorage.getItem('synabitVaultType') as any || 'local');
const gdriveConnected = ref(false);
const gdriveSyncing = ref(false);
const gdriveSyncError = ref('');
const lastSyncTime = ref(localStorage.getItem('synabitLastSyncTime') || '');
const gdriveAuthLoading = ref(false);

const gdriveAutoSyncEnabled = ref(localStorage.getItem('synabitGDriveAutoSync') === 'true');
const gdriveAutoSyncInterval = ref(Number(localStorage.getItem('synabitGDriveInterval') || '15'));
let autoSyncTimer: number | null = null;

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
const activeTool = ref<'nexus' | 'quickcap' | 'note' | 'task' | 'calendar' | 'file'>('nexus');

// --- Settings Modal State ---
const showSettingsModal = ref(false);
const settingsTab = ref<'general' | 'notes' | 'tasks' | 'about'>('general');
const taskArchiveDays = ref(Number(localStorage.getItem('synabitTaskArchiveDays') || '30'));

const enableDailyNotes = ref(localStorage.getItem('synabitConfig_enableDailyNotes') !== 'false');
watch(enableDailyNotes, (val) => localStorage.setItem('synabitConfig_enableDailyNotes', String(val)));

const dailyNoteFormat = ref(localStorage.getItem('synabitConfig_dailyNoteFormat') || 'YYYY-MM-DD');
watch(dailyNoteFormat, (val) => localStorage.setItem('synabitConfig_dailyNoteFormat', val));

const dailyNoteTag = ref(localStorage.getItem('synabitConfig_dailyNoteTag') ?? 'daily');
watch(dailyNoteTag, (val) => localStorage.setItem('synabitConfig_dailyNoteTag', val));

const isValidDailyFormat = computed(() => {
    const val = dailyNoteFormat.value.toUpperCase();
    return val.includes('YY') && val.includes('MM') && (val.includes('DD') || val.includes('D'));
});

watch(taskArchiveDays, (v) => {
    localStorage.setItem('synabitTaskArchiveDays', String(v));
});

const openSettings = () => {
    showSettingsModal.value = true;
    settingsTab.value = 'general';
};

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
     invoke('start_vault_watcher', { vaultPath: vaultPath.value }).catch(console.error);
  }
  
  // Check Google Drive auth status
  checkGDriveAuth().then(() => {
     setupAutoSync();
  });

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
  
  listen('vault-changed', () => {
      scanVault();
  });
  
  listen('vault-filesystem-changed', () => {
      scanVault();
      if (vaultType.value === 'gdrive' && gdriveConnected.value && !gdriveSyncing.value) {
          syncGDrive();
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
    const isConfirmed = await confirm('Are you sure you want to delete this note irreversibly?', { title: 'Delete Note', kind: 'warning' });
    if (!isConfirmed) return;
    try {
        if (currentNoteId.value === id) {
           clearTimeout(saveTimeout);
        }
        await invoke('delete_note', { path: id });
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
            invoke('start_vault_watcher', { vaultPath: vaultPath.value }).catch(console.error);
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

async function openDailyNote() {
    if (!vaultPath.value) return;
    try {
        const finalFormat = isValidDailyFormat.value ? dailyNoteFormat.value : 'YYYY-MM-DD';
        const tag = dailyNoteTag.value.trim();
        const dailyPath = await invoke<string>('open_daily_note', { vaultPath: vaultPath.value, formatStr: finalFormat, tag });
        await scanVault();
        if (dailyPath) {
            currentNoteId.value = dailyPath;
            viewMode.value = 'editor';
        }
    } catch(e) {
        console.error("Failed to open daily note:", e);
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
const activeNote = computed(() => notes.value.find(n => n.id === currentNoteId.value) || null);

const currentOutgoingLinks = computed(() => {
    if (!currentContent.value) return [];
    // Regex matches synabit://note/([^)]+)
    const regex = /synabit:\/\/note\/([^\s\)"']+)/g;
    const links = new Set<string>();
    let m;
    while ((m = regex.exec(currentContent.value)) !== null) {
        // m[1] contains the basename/path. 
        // Our notes use full paths as ID, but links often just use basename.
        // We'll try to find the full note object via basename or raw.
        const targetFilename = decodeURIComponent(m[1]);
        const targetNote = notes.value.find(n => n.path.endsWith(targetFilename));
        if (targetNote) {
           links.add(targetNote.id);
        } else {
           links.add(targetFilename); // Fallback
        }
    }
    return Array.from(links);
});

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
      const q = managerSearchQuery.value.toLowerCase().trim();
      const isTagSearch = q.startsWith('#');
      const searchTerm = isTagSearch ? q.slice(1) : q;
      
      result = result.filter(n => {
         if (isTagSearch) {
             return n.tags.some(t => t.toLowerCase().includes(searchTerm));
         }
         return n.title.toLowerCase().includes(searchTerm) || n.tags.some(t => t.toLowerCase().includes(searchTerm));
      });
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
      const q = searchQuery.value.toLowerCase().trim();
      const isTagSearch = q.startsWith('#');
      const searchTerm = isTagSearch ? q.slice(1) : q;
      
      result = result.filter(n => {
          if (isTagSearch) {
              return n.tags.some(t => t.toLowerCase().includes(searchTerm));
          }
          return n.title.toLowerCase().includes(searchTerm) || 
                 n.tags.some(t => t.toLowerCase().includes(searchTerm));
      });
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
    localStorage.removeItem('synabitVaultType');
    vaultPath.value = '';
    vaultType.value = 'local';
    activeTool.value = 'note';
    setupAutoSync();
};

// --- Google Drive Functions ---
const setupAutoSync = () => {
    if (autoSyncTimer !== null) { 
        window.clearInterval(autoSyncTimer); 
        autoSyncTimer = null; 
    }
    if (gdriveAutoSyncEnabled.value && vaultType.value === 'gdrive' && gdriveConnected.value) {
        let mins = Math.max(1, Math.min(60, gdriveAutoSyncInterval.value));
        autoSyncTimer = window.setInterval(() => {
            if (!gdriveSyncing.value && gdriveConnected.value && vaultType.value === 'gdrive') {
                syncGDrive();
            }
        }, mins * 60 * 1000);
    }
};

watch(gdriveAutoSyncEnabled, (val) => {
    localStorage.setItem('synabitGDriveAutoSync', String(val));
    setupAutoSync();
});

watch(gdriveAutoSyncInterval, (val) => {
    let safeVal = Math.max(1, Math.min(60, val || 1));
    if (safeVal !== val) { 
        gdriveAutoSyncInterval.value = safeVal; 
        return; 
    }
    localStorage.setItem('synabitGDriveInterval', String(safeVal));
    setupAutoSync();
});

const checkGDriveAuth = async () => {
    try {
        gdriveConnected.value = await invoke<boolean>('gdrive_auth_status');
    } catch { gdriveConnected.value = false; }
};

const connectGDrive = async () => {
    gdriveAuthLoading.value = true;
    gdriveSyncError.value = '';
    try {
        await invoke<string>('gdrive_auth_start');
        gdriveConnected.value = true;
        // Get cache path and use it as vault
        const cachePath = await invoke<string>('gdrive_get_cache_path');
        vaultPath.value = cachePath;
        vaultType.value = 'gdrive';
        localStorage.setItem('synabitVaultPath', cachePath);
        localStorage.setItem('synabitVaultType', 'gdrive');
        // Initial sync
        await syncGDrive();
        scanVault();
        setupAutoSync();
    } catch (e: any) {
        gdriveSyncError.value = e?.toString() || 'Connection failed';
    } finally {
        gdriveAuthLoading.value = false;
    }
};

const disconnectGDrive = async () => {
    try {
        await invoke('gdrive_disconnect');
        gdriveConnected.value = false;
        clearVault();
        setupAutoSync();
    } catch (e) { console.error('Disconnect failed:', e); }
};

const syncGDrive = async () => {
    if (gdriveSyncing.value || !vaultPath.value) return;
    gdriveSyncing.value = true;
    gdriveSyncError.value = '';
    try {
        const result = await invoke<{ pulled: number; pushed: number; deleted: number; errors: string[] }>('gdrive_sync_full', { vaultPath: vaultPath.value });
        const now = new Date().toLocaleTimeString();
        lastSyncTime.value = now;
        localStorage.setItem('synabitLastSyncTime', now);
        if (result.errors.length > 0) {
            gdriveSyncError.value = `${result.errors.length} error(s)`;
            console.warn('Sync errors:', result.errors);
        }
        // Re-scan vault after sync to pick up pulled changes
        if (result.pulled > 0) {
            await scanVault();
        }
    } catch (e: any) {
        gdriveSyncError.value = e?.toString() || 'Sync failed';
        console.error('Sync failed:', e);
    } finally {
        gdriveSyncing.value = false;
    }
};

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
              <!-- Option 1: Local Vault -->
              <button @click="selectVault" class="group flex flex-col items-center gap-3 p-6 w-48 rounded-2xl border-2 border-[#e6e6e6] dark:border-[#333] hover:border-black dark:hover:border-white bg-white dark:bg-[#1e1e1e] transition-all hover:shadow-lg active:scale-[0.98] cursor-pointer">
                <div class="w-12 h-12 rounded-xl bg-gray-100 dark:bg-gray-800 flex items-center justify-center group-hover:bg-gray-200 dark:group-hover:bg-gray-700 transition-colors">
                  <FolderOpen class="w-6 h-6 text-gray-600 dark:text-gray-300" />
                </div>
                <div>
                  <p class="font-semibold text-sm">Local Folder</p>
                  <p class="text-[11px] text-gray-400 mt-1">Store on this computer</p>
                </div>
              </button>
              
              <!-- Option 2: Google Drive -->
              <button @click="connectGDrive" :disabled="gdriveAuthLoading" class="group flex flex-col items-center gap-3 p-6 w-48 rounded-2xl border-2 border-[#e6e6e6] dark:border-[#333] hover:border-blue-500 dark:hover:border-blue-400 bg-white dark:bg-[#1e1e1e] transition-all hover:shadow-lg active:scale-[0.98] cursor-pointer disabled:opacity-60 disabled:pointer-events-none">
                <div class="w-12 h-12 rounded-xl bg-blue-50 dark:bg-blue-900/30 flex items-center justify-center group-hover:bg-blue-100 dark:group-hover:bg-blue-900/50 transition-colors">
                  <Cloud v-if="!gdriveAuthLoading" class="w-6 h-6 text-blue-500" />
                  <RefreshCw v-else class="w-6 h-6 text-blue-500 animate-spin" />
                </div>
                <div>
                  <p class="font-semibold text-sm">Google Drive</p>
                  <p class="text-[11px] text-gray-400 mt-1">Sync across devices</p>
                </div>
              </button>
            </div>
            
            <p v-if="gdriveSyncError" class="text-red-500 text-xs px-4">{{ gdriveSyncError }}</p>
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
            <!-- Google Drive Sync Indicator -->
            <button v-if="vaultType === 'gdrive'" @click="syncGDrive" :disabled="gdriveSyncing" :class="['relative group w-10 h-10 rounded-xl flex items-center justify-center transition-all cursor-pointer', gdriveSyncError ? 'text-red-500 hover:bg-red-100 dark:hover:bg-red-900/30' : gdriveConnected ? 'text-blue-500 hover:bg-blue-100 dark:hover:bg-blue-900/30' : 'text-gray-400 hover:bg-gray-200 dark:hover:bg-gray-800']" :title="gdriveSyncing ? 'Syncing...' : lastSyncTime ? `Last sync: ${lastSyncTime}` : 'Sync with Google Drive'">
               <RefreshCw v-if="gdriveSyncing" class="w-5 h-5 animate-spin" />
               <CloudOff v-else-if="gdriveSyncError" class="w-5 h-5" />
               <Cloud v-else class="w-5 h-5" />
               <span class="absolute left-full ml-3 px-2.5 py-1 whitespace-nowrap bg-black dark:bg-white text-white dark:text-black text-xs font-semibold rounded-md opacity-0 group-hover:opacity-100 pointer-events-none transition-all z-50 shadow-lg">{{ gdriveSyncing ? 'Syncing…' : gdriveSyncError ? 'Sync Error' : lastSyncTime ? `Synced ${lastSyncTime}` : 'Sync Now' }}</span>
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
          <!-- Drag Handle -->
          <div 
            class="absolute top-0 right-0 w-1.5 h-full cursor-col-resize hover:bg-black/10 dark:hover:bg-white/10 z-10 opacity-0 hover:opacity-100 transition-opacity"
            @mousedown.stop="startDragNoteSidebar"
          ></div>

          <!-- Tool Header -->
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
                       class="w-full pl-12 pr-4 py-3 bg-white dark:bg-[#1a1a1a] border border-[#e6e6e6] dark:border-[#2c2c2c] rounded-xl text-base shadow-sm focus:outline-none focus:ring-2 focus:ring-purple-500/50 transition-shadow placeholder:text-gray-400 manager-search-input"
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

      <!-- Right Sidebar: Graph & Backlinks -->
      <aside v-if="currentNoteId" v-show="showRightSidebar" class="shrink-0 relative border-l border-[#e6e6e6] dark:border-[#2c2c2c] bg-[#fbfbfc] dark:bg-[#191919] flex flex-col overflow-hidden" :style="{ width: wRightSidebar + 'px' }">
        <!-- Drag Handle -->
        <div 
          class="absolute top-0 left-0 w-1.5 h-full cursor-col-resize hover:bg-black/10 dark:hover:bg-white/10 z-10 opacity-0 hover:opacity-100 transition-opacity"
          @mousedown.stop="startDragRightSidebar"
        ></div>
        
        <!-- Graph View Section -->
        <div class="h-10 flex-shrink-0 flex items-center px-4 border-b border-[#e6e6e6] dark:border-[#2c2c2c]" data-tauri-drag-region>
            <Globe class="w-4 h-4 text-gray-500 mr-2" />
            <span class="font-bold text-[11px] tracking-wider text-gray-500 uppercase mt-0.5">Graph View</span>
            <button @click="showRightSidebar = false" class="p-1 ml-auto rounded-md hover:bg-gray-200 dark:hover:bg-gray-800 text-gray-400 transition-colors">
               <X class="w-3.5 h-3.5" />
            </button>
        </div>
        <div class="h-1/2 border-b border-[#e6e6e6] dark:border-[#2c2c2c] overflow-hidden">
            <NoteGraph 
                v-if="activeNote"
                :current-note-id="currentNoteId || ''"
                :current-note-title="activeNote.title || 'Untitled Node'"
                :tags="activeNote.tags || []"
                :outgoing-links="currentOutgoingLinks"
                :backlinks="currentBacklinks"
                :all-notes="notes"
                @open-note="handleOpenInternalNote"
            />
        </div>

        <!-- Backlinks Section -->
        <div class="h-10 flex-shrink-0 flex items-center px-4 border-b border-[#e6e6e6] dark:border-[#2c2c2c]">
            <span class="font-bold text-[11px] tracking-wider text-[#8b8b8b] dark:text-[#71717a] uppercase mt-0.5">Linked Mentions ({{ currentBacklinks.length }})</span>
        </div>
        <div class="flex-1 overflow-y-auto p-2 space-y-1">
            <div v-if="currentBacklinks.length === 0" class="text-[13px] text-gray-400 text-center py-4">No linked mentions.</div>
            <div 
              v-for="bl in currentBacklinks" 
              :key="bl.id" 
              @click="handleOpenInternalNote(bl.id)"
              class="p-3 border border-transparent rounded-lg cursor-pointer hover:bg-white/50 dark:hover:bg-[#252525] hover:border-[#e6e6e6] dark:hover:border-[#2f2f2f] transition-all group"
            >
              <h5 class="flex items-center gap-2 mb-1.5 pr-2">
                  <FileText class="w-3.5 h-3.5 text-gray-400 shrink-0 opacity-80 group-hover:text-purple-500 group-hover:opacity-100 transition-colors"/> 
                  <span class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] truncate">{{ bl.title }}</span>
              </h5>
              <p class="text-[11px] text-[#52525b] dark:text-[#a1a1aa] line-clamp-3 leading-relaxed pl-5">{{ bl.summary || 'No text content available.' }}</p>
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

    <!-- SETTINGS MODAL (Global Overlay) -->
    <Teleport to="body">
      <Transition name="settings-modal">
        <div v-if="showSettingsModal" class="fixed inset-0 z-[200] flex items-center justify-center">
          <!-- Backdrop -->
          <div class="absolute inset-0 bg-black/40 dark:bg-black/60 backdrop-blur-sm" @mousedown="showSettingsModal = false"></div>
          
          <!-- Modal Container -->
          <div class="relative w-[720px] max-w-[90vw] h-[520px] max-h-[85vh] bg-[#fdfdfc] dark:bg-[#242424] rounded-2xl shadow-2xl border border-[#e0e0e0] dark:border-[#333] flex overflow-hidden" @mousedown.stop>
            
            <!-- Left Tab Navigation -->
            <nav class="w-[200px] shrink-0 bg-[#f5f5f5] dark:bg-[#1a1a1a] border-r border-[#e6e6e6] dark:border-[#2c2c2c] flex flex-col py-5 px-3">
              <h2 class="text-[13px] font-bold text-[#1c1c1e] dark:text-[#f4f4f5] mb-5 px-2">Settings</h2>
              
              <div class="space-y-0.5">
                <button @click="settingsTab = 'general'" 
                  :class="['w-full text-left px-3 py-2 rounded-lg text-[13px] font-medium transition-all flex items-center gap-2.5', settingsTab === 'general' ? 'bg-white dark:bg-[#2a2a2a] text-[#1c1c1e] dark:text-white shadow-sm' : 'text-[#52525b] dark:text-[#a1a1aa] hover:bg-white/60 dark:hover:bg-[#252525] hover:text-[#1c1c1e] dark:hover:text-white']">
                  <Settings class="w-4 h-4 opacity-70" />
                  General
                </button>
                <button @click="settingsTab = 'notes'" 
                  :class="['w-full text-left px-3 py-2 rounded-lg text-[13px] font-medium transition-all flex items-center gap-2.5', settingsTab === 'notes' ? 'bg-white dark:bg-[#2a2a2a] text-[#1c1c1e] dark:text-white shadow-sm' : 'text-[#52525b] dark:text-[#a1a1aa] hover:bg-white/60 dark:hover:bg-[#252525] hover:text-[#1c1c1e] dark:hover:text-white']">
                  <FileText class="w-4 h-4 opacity-70" />
                  Notes
                </button>
                <button @click="settingsTab = 'tasks'" 
                  :class="['w-full text-left px-3 py-2 rounded-lg text-[13px] font-medium transition-all flex items-center gap-2.5', settingsTab === 'tasks' ? 'bg-white dark:bg-[#2a2a2a] text-[#1c1c1e] dark:text-white shadow-sm' : 'text-[#52525b] dark:text-[#a1a1aa] hover:bg-white/60 dark:hover:bg-[#252525] hover:text-[#1c1c1e] dark:hover:text-white']">
                  <CheckSquare class="w-4 h-4 opacity-70" />
                  Tasks
                </button>
                <button @click="settingsTab = 'about'" 
                  :class="['w-full text-left px-3 py-2 rounded-lg text-[13px] font-medium transition-all flex items-center gap-2.5', settingsTab === 'about' ? 'bg-white dark:bg-[#2a2a2a] text-[#1c1c1e] dark:text-white shadow-sm' : 'text-[#52525b] dark:text-[#a1a1aa] hover:bg-white/60 dark:hover:bg-[#252525] hover:text-[#1c1c1e] dark:hover:text-white']">
                  <Globe class="w-4 h-4 opacity-70" />
                  About
                </button>
              </div>
            </nav>
            
            <!-- Right Content Area -->
            <div class="flex-1 flex flex-col overflow-hidden">
              <!-- Header -->
              <div class="h-12 shrink-0 flex items-center justify-between px-6 border-b border-[#e6e6e6] dark:border-[#2c2c2c]">
                <h3 class="text-[15px] font-semibold text-[#1c1c1e] dark:text-[#f4f4f5] capitalize">{{ settingsTab }}</h3>
                <button @click="showSettingsModal = false" class="p-1.5 rounded-lg hover:bg-gray-100 dark:hover:bg-[#333] text-gray-400 hover:text-gray-600 dark:hover:text-gray-200 transition-colors">
                  <X class="w-4 h-4" />
                </button>
              </div>
              
              <!-- Scrollable Content -->
              <div class="flex-1 overflow-y-auto p-6">
                
                <!-- === GENERAL TAB === -->
                <div v-if="settingsTab === 'general'" class="space-y-6">
                  <!-- Vault Management -->
                  <section>
                    <h4 class="text-[13px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-3">Vault</h4>
                    <div class="bg-[#f8f8f8] dark:bg-[#1e1e1e] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c]">
                      <div class="flex items-center gap-2 mb-2">
                        <p class="text-[11px] font-medium text-gray-400 dark:text-gray-500">Storage Type</p>
                        <span v-if="vaultType === 'gdrive'" class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-blue-100 dark:bg-blue-900/40 text-blue-600 dark:text-blue-400 text-[10px] font-semibold">
                          <Cloud class="w-3 h-3" /> Google Drive
                        </span>
                        <span v-else class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-300 text-[10px] font-semibold">
                          <FolderOpen class="w-3 h-3" /> Local
                        </span>
                      </div>
                      <p class="font-mono text-[12px] break-all text-[#1c1c1e] dark:text-[#f4f4f5] bg-white dark:bg-[#2a2a2a] px-3 py-2 rounded-lg border border-gray-200 dark:border-transparent">{{ vaultPath }}</p>
                      <button @click="clearVault" class="mt-3 px-4 py-2 bg-black hover:bg-gray-800 text-white dark:bg-white dark:hover:bg-gray-200 dark:text-black rounded-lg text-[12px] font-medium transition-all shadow-sm flex items-center gap-2">
                        <FolderOpen class="w-3.5 h-3.5" /> Switch Vault
                      </button>
                    </div>
                  </section>

                  <!-- Google Drive Sync (only shown when gdrive mode) -->
                  <section v-if="vaultType === 'gdrive'">
                    <h4 class="text-[13px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-3">Google Drive Sync</h4>
                    <div class="bg-[#f8f8f8] dark:bg-[#1e1e1e] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c] space-y-4">
                      <!-- Connection Status -->
                      <div class="flex items-center justify-between">
                        <div class="flex items-center gap-2">
                          <div :class="['w-2 h-2 rounded-full', gdriveConnected ? 'bg-green-500' : 'bg-red-500']"></div>
                          <p class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">{{ gdriveConnected ? 'Connected' : 'Disconnected' }}</p>
                        </div>
                        <button @click="syncGDrive" :disabled="gdriveSyncing" class="px-3 py-1.5 rounded-lg text-[12px] font-medium transition-all flex items-center gap-1.5 bg-blue-500 hover:bg-blue-600 text-white disabled:opacity-60">
                          <RefreshCw class="w-3.5 h-3.5" :class="gdriveSyncing ? 'animate-spin' : ''" />
                          {{ gdriveSyncing ? 'Syncing…' : 'Sync Now' }}
                        </button>
                      </div>
                      
                      <!-- Last Sync Time -->
                      <div v-if="lastSyncTime" class="flex items-center gap-2 text-[11px] text-gray-400">
                        <span>Last synced: {{ lastSyncTime }}</span>
                      </div>
                      
                      <!-- Sync Error -->
                      <div v-if="gdriveSyncError" class="text-[11px] text-red-500 bg-red-50 dark:bg-red-900/20 px-3 py-2 rounded-lg">
                        ⚠️ {{ gdriveSyncError }}
                      </div>
                      
                      <!-- Auto Sync Settings -->
                      <div class="border-t border-[#e6e6e6] dark:border-[#2c2c2c] pt-4">
                        <div class="flex items-center justify-between mb-3">
                          <p class="text-[12px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">Periodic Auto Sync</p>
                          <label class="relative inline-flex items-center cursor-pointer">
                            <input type="checkbox" v-model="gdriveAutoSyncEnabled" class="sr-only peer">
                            <div class="w-9 h-5 bg-gray-200 peer-focus:outline-none rounded-full peer dark:bg-gray-700 peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-4 after:w-4 after:transition-all dark:border-gray-600 peer-checked:bg-blue-500"></div>
                          </label>
                        </div>
                        <div v-if="gdriveAutoSyncEnabled" class="flex items-center justify-between">
                           <p class="text-[11px] text-gray-500 dark:text-gray-400">Sync interval (minutes)</p>
                           <input type="number" v-model.number="gdriveAutoSyncInterval" min="1" max="60" class="w-16 px-2 py-1 bg-white dark:bg-[#2a2a2a] border border-[#e6e6e6] dark:border-[#3a3a3a] rounded text-[12px] text-center text-[#1c1c1e] dark:text-[#f4f4f5] focus:outline-none focus:border-blue-500" />
                        </div>
                      </div>
                      
                      <!-- Disconnect -->
                      <div class="border-t border-[#e6e6e6] dark:border-[#2c2c2c] pt-4">
                        <button @click="disconnectGDrive" class="px-4 py-2 rounded-lg text-[12px] font-medium border border-red-300 dark:border-red-800 text-red-600 dark:text-red-400 hover:bg-red-50 dark:hover:bg-red-900/20 transition-all flex items-center gap-2">
                          <CloudOff class="w-3.5 h-3.5" /> Disconnect Google Drive
                        </button>
                      </div>
                    </div>
                  </section>

                  <!-- Theme -->
                  <section>
                    <h4 class="text-[13px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-3">Appearance</h4>
                    <div class="bg-[#f8f8f8] dark:bg-[#1e1e1e] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c]">
                      <p class="text-[12px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] mb-3">Theme</p>
                      <div class="flex gap-2">
                        <button v-for="mode in (['light', 'dark', 'system'] as const)" :key="mode"
                          @click="themeMode = mode"
                          :class="['px-4 py-2 rounded-lg text-[12px] font-medium transition-all border capitalize', themeMode === mode ? 'bg-black text-white dark:bg-white dark:text-black border-transparent shadow-sm' : 'bg-white dark:bg-[#2a2a2a] border-[#e0e0e0] dark:border-[#3a3a3a] text-gray-600 dark:text-gray-300 hover:border-gray-400 dark:hover:border-gray-500']">
                          {{ mode }}
                        </button>
                      </div>
                    </div>
                  </section>

                </div>
                
                <!-- === NOTES TAB === -->
                <div v-else-if="settingsTab === 'notes'" class="space-y-6">
                  <!-- Features -->
                  <section>
                    <h4 class="text-[13px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-3">Features</h4>
                    <div class="bg-[#f8f8f8] dark:bg-[#1e1e1e] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c] flex flex-col gap-4">
                        <div class="flex items-center justify-between">
                          <div>
                            <p class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">Enable Daily Notes</p>
                            <p class="text-[11px] text-gray-400 dark:text-gray-500 mt-0.5">Show the "Today" button to quickly create and access daily notes.</p>
                          </div>
                          <button @click="enableDailyNotes = !enableDailyNotes" class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center justify-center rounded-full focus:outline-none transition-colors duration-200 ease-in-out" :class="enableDailyNotes ? 'bg-purple-600' : 'bg-gray-300 dark:bg-gray-600'">
                            <span class="pointer-events-none inline-block h-4 w-4 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out" :class="enableDailyNotes ? 'translate-x-2' : '-translate-x-2'"/>
                          </button>
                        </div>
                        
                        <!-- Format settings, disabled if feature is off -->
                        <div class="border-t border-[#e6e6e6] dark:border-[#2c2c2c] pt-4 flex items-center justify-between" :class="!enableDailyNotes ? 'opacity-50 pointer-events-none' : ''">
                          <div>
                            <p class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">Date Format</p>
                            <p class="text-[11px] text-gray-400 dark:text-gray-500 mt-0.5">Format of the daily note filename (e.g. YYYY-MM-DD or DD-MM-YYYY).</p>
                          </div>
                          <div class="flex flex-col items-end gap-1">
                             <input type="text" v-model="dailyNoteFormat" class="w-28 px-3 py-1.5 rounded-lg bg-white dark:bg-[#2a2a2a] border text-[13px] text-center text-[#1c1c1e] dark:text-[#f4f4f5] focus:outline-none focus:ring-1 transition-colors" :class="isValidDailyFormat ? 'border-[#e0e0e0] dark:border-[#3a3a3a] focus:ring-black dark:focus:ring-white' : 'border-red-400 focus:ring-red-500'" />
                             <span v-if="!isValidDailyFormat" class="text-[10px] text-red-500 font-medium">Requires YY, MM, DD</span>
                          </div>
                        </div>
                        
                        <!-- Default Tag -->
                        <div class="border-t border-[#e6e6e6] dark:border-[#2c2c2c] pt-4 flex items-center justify-between" :class="!enableDailyNotes ? 'opacity-50 pointer-events-none' : ''">
                          <div>
                            <p class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">Default Tag</p>
                            <p class="text-[11px] text-gray-400 dark:text-gray-500 mt-0.5">Tag automatically assigned to new daily notes.</p>
                          </div>
                          <input type="text" v-model="dailyNoteTag" placeholder="daily" class="w-28 px-3 py-1.5 rounded-lg bg-white dark:bg-[#2a2a2a] border border-[#e0e0e0] dark:border-[#3a3a3a] text-[13px] text-center text-[#1c1c1e] dark:text-[#f4f4f5] focus:outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-colors" />
                        </div>
                    </div>
                  </section>
                </div>
                
                <!-- === TASKS TAB === -->
                <div v-else-if="settingsTab === 'tasks'" class="space-y-6">
                  <section>
                    <h4 class="text-[13px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-3">Auto Archive</h4>
                    <div class="bg-[#f8f8f8] dark:bg-[#1e1e1e] p-4 rounded-xl border border-[#e6e6e6] dark:border-[#2c2c2c]">
                      <div class="flex items-center justify-between mb-2">
                        <div>
                          <p class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">Archive completed tasks</p>
                          <p class="text-[11px] text-gray-400 dark:text-gray-500 mt-0.5">Tasks marked as "done" for longer than this period will be moved to the <code class="px-1 py-0.5 bg-gray-200 dark:bg-[#333] rounded text-[10px]">Tasks/archived</code> folder.</p>
                        </div>
                      </div>
                      <div class="flex items-center gap-3 mt-3">
                        <label class="text-[12px] text-gray-500 dark:text-gray-400">After</label>
                        <input type="number" v-model.number="taskArchiveDays" min="1" max="365" class="w-20 px-3 py-1.5 rounded-lg bg-white dark:bg-[#2a2a2a] border border-[#e0e0e0] dark:border-[#3a3a3a] text-[13px] text-center text-[#1c1c1e] dark:text-[#f4f4f5] focus:outline-none focus:ring-1 focus:ring-black dark:focus:ring-white" />
                        <span class="text-[12px] text-gray-500 dark:text-gray-400">days</span>
                      </div>
                    </div>
                  </section>
                </div>
                
                <!-- === ABOUT TAB === -->
                <div v-else-if="settingsTab === 'about'" class="space-y-6">
                  <section>
                    <div class="text-center pt-8">
                      <div class="w-16 h-16 bg-gradient-to-br from-gray-100 to-gray-200 dark:from-[#2a2a2a] dark:to-[#333] rounded-2xl flex items-center justify-center mx-auto mb-4 shadow-inner">
                        <Globe class="w-8 h-8 text-gray-400" />
                      </div>
                      <h3 class="text-[18px] font-bold text-[#1c1c1e] dark:text-[#f4f4f5]">Synabit</h3>
                      <p class="text-[12px] text-gray-400 dark:text-gray-500 mt-1">Version 1.0.0-alpha</p>
                      <p class="text-[12px] text-gray-500 dark:text-gray-400 mt-4 max-w-xs mx-auto leading-relaxed">A unified, local-first productivity workspace for notes, tasks, quick captures, and more.</p>
                    </div>
                  </section>
                </div>
              </div>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>
    
    </template>
  </div>
</template>

<style scoped>
[data-tauri-drag-region] {
  -webkit-app-region: drag;
}

.settings-modal-enter-active,
.settings-modal-leave-active {
  transition: opacity 0.2s ease;
}
.settings-modal-enter-active > div:last-child,
.settings-modal-leave-active > div:last-child {
  transition: transform 0.2s ease, opacity 0.2s ease;
}
.settings-modal-enter-from,
.settings-modal-leave-to {
  opacity: 0;
}
.settings-modal-enter-from > div:last-child {
  transform: scale(0.95) translateY(10px);
  opacity: 0;
}
.settings-modal-leave-to > div:last-child {
  transform: scale(0.95) translateY(10px);
  opacity: 0;
}

.manager-search-input {
  color: #1c1c1e !important;
}
html.dark .manager-search-input {
  color: #f4f4f5 !important;
}
</style>