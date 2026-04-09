<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue';
import { Tag, FileText, Search, Settings, PanelLeft, PanelLeftClose, PanelRight, PanelRightClose, ChevronDown, ChevronRight, Hash, FolderOpen, Plus, MoreVertical, Pin, Trash2, Edit2, X } from 'lucide-vue-next';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import TiptapEditor from './components/TiptapEditor.vue';

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

// --- Size & Toggle State ---
const wSidebar1 = ref(240);
const wSidebar2 = ref(280);
const showSidebar1 = ref(true);
const showSidebar2 = ref(true);

// --- Drag Logic ---
const isDragging1 = ref(false);
const isDragging2 = ref(false);
const startDrag1 = () => { isDragging1.value = true; };
const startDrag2 = () => { isDragging2.value = true; };

const onMouseMove = (e: MouseEvent) => {
  if (isDragging1.value) {
    wSidebar1.value = Math.max(150, Math.min(e.clientX, 400));
  } else if (isDragging2.value) {
    const leftOffset = showSidebar1.value ? wSidebar1.value : 0;
    wSidebar2.value = Math.max(200, Math.min(e.clientX - leftOffset, 600));
  }
};

const onMouseUp = () => {
  if (isDragging1.value || isDragging2.value) {
    isDragging1.value = false;
    isDragging2.value = false;
  }
};

onMounted(() => {
  window.addEventListener('mousemove', onMouseMove);
  window.addEventListener('mouseup', onMouseUp);
  document.addEventListener('click', () => { activeContextMenu.value = null; });
  if (vaultPath.value) {
     scanVault();
  }
});
onUnmounted(() => {
  window.removeEventListener('mousemove', onMouseMove);
  window.removeEventListener('mouseup', onMouseUp);
});

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
        }
    } catch(e) {
        console.error("Failed to create note:", e);
    }
}

// --- Editor Read/Write Logic ---
const currentContent = ref('');
let saveTimeout: ReturnType<typeof setTimeout>;

const loadNoteFile = async (id: string) => {
    if (!id) return;
    try {
        const rawContent = await invoke<string>('read_note', { path: id });
        let body = rawContent;
        if (rawContent.startsWith('---\n') || rawContent.startsWith('---\r\n')) {
            const splitIdx = rawContent.indexOf('---', 3);
            if (splitIdx > 0) {
                body = rawContent.substring(splitIdx + 3).replace(/^\s+/, '');
            }
        }
        currentContent.value = body;
    } catch(e) {
        console.error("Failed to read note:", e);
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
        } catch(e) {
             console.error("Failed to save note:", e);
        }
    }, 600);
}

const onEditorUpdate = (newMd: string) => {
    currentContent.value = newMd;
    saveNoteFile();
};

watch(currentNoteId, (newId) => {
    if (newId) {
        loadNoteFile(newId);
    } else {
        currentContent.value = '';
    }
});

// --- Derived State ---
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

const toggleSidebar1 = () => showSidebar1.value = !showSidebar1.value;
const toggleSidebar2 = () => showSidebar2.value = !showSidebar2.value;
</script>

<template>
  <div class="flex h-screen w-full bg-[#fdfdfc] text-[#1c1c1e] dark:bg-[#121212] dark:text-[#f4f4f5] font-sans overflow-hidden select-none"
       :class="{'cursor-col-resize': isDragging1 || isDragging2}">
       
    <!-- Application State 1: No Vault Selected -->
    <div v-if="!vaultPath" class="flex-1 flex flex-col items-center justify-center p-8 bg-[#fdfdfc] dark:bg-[#121212]" data-tauri-drag-region>
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
      <!-- Sidebar 1: Tags -->
      <aside 
        v-show="showSidebar1" 
        class="border-r border-[#e6e6e6] dark:border-[#2c2c2c] bg-white dark:bg-[#1e1e1e] flex flex-col relative shrink-0"
        :style="{ width: wSidebar1 + 'px' }"
        data-tauri-drag-region>
        
        <!-- Drag Handle 1 -->
        <div 
          class="absolute top-0 right-0 w-1.5 h-full cursor-col-resize hover:bg-black/10 dark:hover:bg-white/10 z-10 opacity-0 hover:opacity-100 transition-opacity"
          @mousedown.stop="startDrag1"
        ></div>

        <!-- Header -->
        <div class="h-10 flex flex-shrink-0 items-center px-4 pointer-events-none" data-tauri-drag-region>
        </div>
        
        <!-- Tags List -->
        <div class="flex-1 overflow-y-auto p-3" @mousedown.stop>
          <div class="mb-4">
            <p class="text-xs font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-2 px-3">Tags & Library</p>
            <div class="space-y-1">
              <template v-for="tag in tagTree" :key="tag.name">
                <div 
                  class="w-full flex items-center px-3 py-1.5 rounded-lg text-sm transition-colors cursor-pointer group"
                  :class="selectedTags.has(tag.name) ? 'bg-black/5 dark:bg-white/10' : 'hover:bg-gray-100 dark:hover:bg-[#2a2a2a] text-[#52525b] dark:text-[#a1a1aa]'"
                  @click="toggleTagSelection(tag.name)"
                >
                  <button 
                    v-if="tag.children && tag.children.length > 0"
                    class="p-0.5 hover:bg-gray-200 dark:hover:bg-gray-700 rounded mr-1 opacity-50 group-hover:opacity-100 transition-opacity"
                    @click.stop="tag.expanded = !tag.expanded"
                  >
                    <ChevronDown v-if="tag.expanded" class="w-3.5 h-3.5" />
                    <ChevronRight v-else class="w-3.5 h-3.5" />
                  </button>
                  <div v-else class="w-5 mr-1" />
                  
                  <Hash class="w-4 h-4 opacity-70 mr-2" />
                  <span class="flex-1 truncate select-none">{{ tag.basename }}</span>
                </div>
                
                <template v-if="tag.expanded && tag.children">
                  <div 
                    v-for="child in tag.children" 
                    :key="child.name"
                    class="w-full flex items-center pl-10 pr-3 py-1.5 rounded-lg text-sm transition-colors cursor-pointer"
                    :class="selectedTags.has(child.name) ? 'bg-black/5 dark:bg-white/10' : 'hover:bg-gray-100 dark:hover:bg-[#2a2a2a] text-[#52525b] dark:text-[#a1a1aa]'"
                    @click="toggleTagSelection(child.name)"
                  >
                    <Tag class="w-3.5 h-3.5 opacity-50 mr-2" />
                    <span class="flex-1 truncate select-none">{{ child.basename }}</span>
                  </div>
                </template>
              </template>

              <!-- Empty State Tags -->
              <div v-if="tagTree.length === 0" class="text-center p-4 text-xs text-gray-400">
                  No tags found in vault.
              </div>
            </div>
          </div>
        </div>

        <!-- Footer/Settings -->
        <div class="flex-shrink-0 p-3 border-t border-[#e6e6e6] dark:border-[#2c2c2c]" @mousedown.stop>
          <button @click="scanVault" class="w-full flex items-center gap-2 px-3 py-2 rounded-lg text-sm hover:bg-gray-100 dark:hover:bg-[#2a2a2a] text-[#52525b] dark:text-[#a1a1aa] transition-colors mb-1">
            <Search class="w-4 h-4 opacity-70" />
            <span>Rescan Vault</span>
          </button>
          <button @click="vaultPath = ''; localStorage.removeItem('synabitVaultPath')" class="w-full flex items-center gap-2 px-3 py-2 rounded-lg text-sm hover:bg-gray-100 dark:hover:bg-[#2a2a2a] text-[#52525b] dark:text-[#a1a1aa] transition-colors">
            <Settings class="w-4 h-4 opacity-70" />
            <span>Switch Vault</span>
          </button>
        </div>
      </aside>

      <!-- Sidebar 2: Notes List -->
      <aside 
        v-show="showSidebar2" 
        class="border-r border-[#e6e6e6] dark:border-[#2c2c2c] bg-[#fbfbfc] dark:bg-[#191919] flex flex-col relative shrink-0"
        :style="{ width: wSidebar2 + 'px' }">
        
        <!-- Drag Handle 2 -->
        <div 
          class="absolute top-0 right-0 w-1.5 h-full cursor-col-resize hover:bg-black/10 dark:hover:bg-white/10 z-10 opacity-0 hover:opacity-100 transition-opacity"
          @mousedown.stop="startDrag2"
        ></div>

        <!-- Header / Search -->
        <div class="h-14 flex-shrink-0 flex items-center px-4 border-b border-[#e6e6e6] dark:border-[#2c2c2c]" data-tauri-drag-region>
          <div class="relative w-full" @mousedown.stop>
            <Search class="absolute left-2.5 top-1/2 -translate-y-1/2 w-4 h-4 text-[#8b8b8b] dark:text-[#71717a]" />
            <input 
              v-model="searchQuery"
              type="text" 
              placeholder="Search notes..." 
              class="w-full pl-8 pr-3 py-1.5 bg-white dark:bg-[#2c2c2c] border border-[#e6e6e6] dark:border-transparent mx-auto block rounded-md text-sm focus:outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-shadow text-[#1c1c1e] dark:text-[#f4f4f5] placeholder:text-gray-400 dark:placeholder:text-gray-500"
            >
          </div>
        </div>

        <!-- Notes List -->
        <div class="flex-1 overflow-y-auto" @mousedown.stop>
          <div 
            v-for="note in filteredNotes" 
            :key="note.id"
            @click="currentNoteId = note.id"
            class="p-4 border-b border-[#e6e6e6]/50 dark:border-[#2c2c2c]/50 cursor-pointer transition-colors relative group"
            :class="currentNoteId === note.id ? 'bg-white dark:bg-[#2a2a2a] shadow-sm' : 'hover:bg-white/50 dark:hover:bg-[#252525]'"
          >
            <!-- 3 Dot Menu -->
            <div class="absolute right-3 top-3 opacity-0 group-hover:opacity-100 transition-opacity z-10" :class="{'opacity-100': activeContextMenu === note.id}">
               <button @click="(e) => toggleContext(note.id, e)" class="p-1 rounded bg-white dark:bg-[#2a2a2a] shadow-sm hover:bg-gray-100 dark:hover:bg-gray-600 border border-gray-200 dark:border-gray-600">
                  <MoreVertical class="w-3.5 h-3.5 text-gray-500 dark:text-gray-300"/>
               </button>
               <div v-if="activeContextMenu === note.id" class="absolute right-0 top-8 w-32 bg-white dark:bg-[#2c2c2c] shadow-lg rounded border border-gray-200 dark:border-gray-700 z-50 py-1 overflow-hidden">
                  <button @click.stop="togglePin(note.id)" class="w-full text-left px-3 py-2 text-xs hover:bg-gray-100 dark:hover:bg-gray-600 flex items-center gap-2">
                     <Pin class="w-3 h-3" /> {{ note.pinned ? 'Unpin' : 'Pin' }}
                  </button>
                  <button @click.stop="handleRenamePrompt(note.id)" class="w-full text-left px-3 py-2 text-xs hover:bg-gray-100 dark:hover:bg-gray-600 flex items-center gap-2">
                     <Edit2 class="w-3 h-3" /> Rename
                  </button>
                  <button @click.stop="deleteNote(note.id)" class="w-full text-left px-3 py-2 text-xs hover:bg-red-50 dark:hover:bg-red-900/30 text-red-600 dark:text-red-400 flex items-center gap-2">
                     <Trash2 class="w-3 h-3" /> Delete
                  </button>
               </div>
            </div>

            <h3 class="font-medium text-sm mb-1 pr-6 flex items-center gap-1.5" :class="{'text-[#1c1c1e] dark:text-[#f4f4f5]': true}">
              <Pin v-if="note.pinned" class="w-3.5 h-3.5 text-orange-500 fill-orange-500/20 shrink-0" />
              <span class="truncate">{{ note.title || 'Untitled Note' }}</span>
            </h3>
            <p class="text-xs text-[#52525b] dark:text-[#a1a1aa] line-clamp-2 mb-2 leading-relaxed opacity-70 pr-4">
              {{ note.summary || 'No content...' }}
            </p>
            <div class="flex items-center justify-between mt-2">
              <div class="flex gap-1 overflow-hidden" v-if="note.tags.length">
                 <span v-for="tag in note.tags" :key="tag" class="text-[9px] px-1.5 py-0.5 rounded bg-gray-200 dark:bg-gray-700 text-gray-700 dark:text-gray-300 whitespace-nowrap">
                    {{ tag.split('/').pop() }}
                 </span>
              </div>
              <div class="text-[10px] text-[#8b8b8b] dark:text-[#71717a] font-medium shrink-0 ml-2">{{ note.date }}</div>
            </div>
          </div>
          
          <!-- Empty State -->
          <div v-if="filteredNotes.length === 0" class="p-8 text-center text-sm text-[#52525b] dark:text-[#a1a1aa]">
            No notes match.
          </div>
        </div>
      </aside>

      <!-- Main Area: Editor -->
      <main class="flex-1 flex flex-col bg-[#fdfdfc] dark:bg-[#121212] min-w-[300px]" @mousedown.stop>
        <!-- Controls Header Area -->
        <div class="h-10 flex-shrink-0 w-full flex items-center justify-between px-4" data-tauri-drag-region>
          <div class="flex gap-2">
            <button @click="toggleSidebar1" class="p-1 rounded-md hover:bg-gray-200 dark:hover:bg-gray-800 text-gray-500 transition-colors" title="Toggle Tags">
              <PanelLeftClose v-if="showSidebar1" class="w-4 h-4" />
              <PanelLeft v-else class="w-4 h-4" />
            </button>
            <button @click="toggleSidebar2" class="p-1 rounded-md hover:bg-gray-200 dark:hover:bg-gray-800 text-gray-500 transition-colors" title="Toggle Notes">
              <PanelRightClose v-if="showSidebar2" class="w-4 h-4" />
              <PanelRight v-else class="w-4 h-4" />
            </button>
          </div>
          
          <div>
            <button @click="createNewNote" class="p-1 rounded-md hover:bg-gray-200 dark:hover:bg-gray-800 text-gray-500 transition-colors" title="New Note">
               <Plus class="w-5 h-5 text-gray-700 dark:text-gray-300" />
            </button>
          </div>
        </div>

        <div v-if="currentNoteId" class="flex-1 px-12 pb-12 max-w-4xl mx-auto w-full overflow-y-auto cursor-text">
          <div class="mb-4 pt-4">
             <div class="flex gap-2 mb-4 flex-wrap items-center">
                <span v-for="tag in notes.find(n => n.id === currentNoteId)?.tags" :key="tag" class="text-xs px-2 py-1 rounded-md bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-300 flex items-center gap-1 group/tag">
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
              :value="notes.find(n => n.id === currentNoteId)?.title"
              @blur="renameTopTitle"
              @keydown.enter="renameTopTitle"
              placeholder="Note Title"
            >
          </div>
          <div class="mt-4 pb-20 w-full text-text dark:text-text-dark">
             <TiptapEditor 
                :model-value="currentContent" 
                :vault-path="vaultPath"
                @update:model-value="onEditorUpdate" 
             />
          </div>
        </div>
        <div v-else class="flex-1 flex items-center justify-center text-[#52525b] dark:text-[#a1a1aa]">
          <div class="text-center">
            <FileText class="w-12 h-12 mx-auto mb-4 opacity-20" />
            <p>Select a note to start editing</p>
          </div>
        </div>
      </main>
    </template>
  </div>
</template>

<style scoped>
[data-tauri-drag-region] {
  -webkit-app-region: drag;
}
</style>