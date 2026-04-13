<script setup lang="ts">
import { ref, computed, onMounted, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { FolderOpen, Settings, FolderSync, X, File, Search, ChevronRight, ChevronDown, FileImage, FileText, Video, Music, FileArchive, Code, Hash } from 'lucide-vue-next';

const props = defineProps<{
    vaultPath: string;
}>();

interface FileItem {
    id: string;
    name: string;
    extension: string;
    size_mb: number;
    source_folder: string;
    date_modified: string;
    absolute_path: string;
    tags: string[];
}

const fileItems = ref<FileItem[]>([]);
const isSettingsOpen = ref(false);
const trackedSources = ref<string>('');
const isReindexing = ref(false);
const isLoading = ref(true);

const selectedFile = ref<FileItem | null>(null);

const editFileName = ref('');
const editFileExt = ref('');
const editTags = ref<string[]>([]);
const newTagInput = ref('');
const isSavingMeta = ref(false);

watch(selectedFile, (newVal) => {
    if (newVal) {
        const lastDot = newVal.name.lastIndexOf('.');
        if (lastDot > 0) { // Has extension and not just a hidden file like .gitignore
            editFileName.value = newVal.name.substring(0, lastDot);
            editFileExt.value = newVal.name.substring(lastDot);
        } else {
            editFileName.value = newVal.name;
            editFileExt.value = '';
        }
        editTags.value = [...newVal.tags];
    } else {
        editFileName.value = '';
        editFileExt.value = '';
        editTags.value = [];
    }
});

const saveFileMetadata = async () => {
    if (!selectedFile.value || isSavingMeta.value) return;
    
    // Prevent empty name
    if (!editFileName.value.trim()) {
        const lastDot = selectedFile.value.name.lastIndexOf('.');
        editFileName.value = (lastDot > 0) ? selectedFile.value.name.substring(0, lastDot) : selectedFile.value.name;
        return;
    }
    
    const fullNewName = editFileName.value.trim() + editFileExt.value;
    
    // Check if nothing changed
    const tagsUnchanged = JSON.stringify(editTags.value) === JSON.stringify(selectedFile.value.tags);
    if (fullNewName === selectedFile.value.name && tagsUnchanged) {
        return;
    }
    
    isSavingMeta.value = true;
    try {
        const newPath = await invoke<string>('update_file_metadata', {
            vaultPath: props.vaultPath,
            absolutePath: selectedFile.value.absolute_path,
            newFilename: fullNewName,
            newTags: editTags.value
        });
        
        await fetchFiles();
        
        // Re-select the updated file to keep properties panel active with correct data
        const reselected = fileItems.value.find(f => f.absolute_path === newPath);
        if (reselected) {
            selectedFile.value = reselected;
        }
    } catch(e) {
        console.error("Failed to update file metadata", e);
        // Revert on error
        const lastDot = selectedFile.value.name.lastIndexOf('.');
        if (lastDot > 0) {
            editFileName.value = selectedFile.value.name.substring(0, lastDot);
            editFileExt.value = selectedFile.value.name.substring(lastDot);
        } else {
            editFileName.value = selectedFile.value.name;
            editFileExt.value = '';
        }
    } finally {
        isSavingMeta.value = false;
    }
};

const addTag = () => {
    const val = newTagInput.value.trim().toLowerCase();
    if (val && !editTags.value.includes(val)) {
        editTags.value.push(val);
        saveFileMetadata();
    }
    newTagInput.value = '';
};

const removeTag = (tag: string) => {
    editTags.value = editTags.value.filter(t => t !== tag);
    saveFileMetadata();
};

const collapsedGroups = ref<Set<string>>(new Set());

const toggleGroup = (groupName: string) => {
    if (collapsedGroups.value.has(groupName)) {
        collapsedGroups.value.delete(groupName);
    } else {
        collapsedGroups.value.add(groupName);
    }
};

const fetchFiles = async () => {
    isLoading.value = true;
    try {
        const allItems = await invoke<FileItem[]>('get_file_items', { vaultPath: props.vaultPath });
        fileItems.value = allItems;
    } catch(e) {
        console.error("Failed to load files", e);
    } finally {
        isLoading.value = false;
    }
};

const searchQuery = ref('');

const filteredFileItems = computed(() => {
    if (!searchQuery.value.trim()) return fileItems.value;
    
    // Check if user specifically searching for tag (starts with #)
    let query = searchQuery.value.toLowerCase().trim();
    const isTagSearch = query.startsWith('#');
    if (isTagSearch) query = query.slice(1);
    
    return fileItems.value.filter(item => {
        if (isTagSearch) {
             return item.tags.some(t => t.toLowerCase().includes(query));
        }
        
        return item.name.toLowerCase().includes(query) 
            || item.extension.toLowerCase().includes(query)
            || item.absolute_path.toLowerCase().includes(query)
            || item.tags.some(t => t.toLowerCase().includes(query));
    });
});

const groupedItems = computed(() => {
    const map = new Map<string, FileItem[]>();
    for (const item of filteredFileItems.value) {
        const groupName = item.source_folder || 'Unknown Source';
        if (!map.has(groupName)) map.set(groupName, []);
        map.get(groupName)!.push(item);
    }
    return map;
});

const getFileIcon = (ext: string) => {
    switch (ext) {
        case 'jpg':
        case 'jpeg':
        case 'png':
        case 'gif':
        case 'svg':
        case 'webp': return FileImage;
        case 'pdf':
        case 'txt':
        case 'md':
        case 'doc':
        case 'docx': return FileText;
        case 'mp4':
        case 'mov':
        case 'avi': return Video;
        case 'mp3':
        case 'wav': return Music;
        case 'zip':
        case 'rar':
        case 'gz': return FileArchive;
        case 'js':
        case 'ts':
        case 'vue':
        case 'json':
        case 'html':
        case 'css':
        case 'rs': return Code;
        default: return File;
    }
};

const openSettings = async () => {
    isSettingsOpen.value = true;
    try {
        const settings = await invoke<any>('get_settings', { vaultPath: props.vaultPath });
        trackedSources.value = settings.tracked_sources.join('\n');
    } catch(e) {
        console.error("Failed to load settings", e);
    }
};

const saveAndReindex = async () => {
    isReindexing.value = true;
    try {
        const sources = trackedSources.value.split('\n').map(s => s.trim()).filter(s => s);
        await invoke('save_settings', { vaultPath: props.vaultPath, settings: { tracked_sources: sources } });
        await invoke('reindex_sources', { vaultPath: props.vaultPath });
        isSettingsOpen.value = false;
        await fetchFiles();
    } catch(e) {
        console.error(e);
    } finally {
        isReindexing.value = false;
    }
};

const openLocalFile = async (path: string) => {
    try {
        await invoke('open_local_file', { path });
    } catch(e) {
        console.error("Failed to open file", e);
    }
};

onMounted(() => {
    fetchFiles();
});

</script>

<template>
  <div class="h-full w-full flex flex-col bg-[#fdfdfc] dark:bg-[#242424] font-sans relative overflow-hidden">
    
    <!-- Header -->
    <div class="w-full flex-shrink-0 border-b border-gray-200 dark:border-[#2c2c2c] bg-white/80 dark:bg-[#1e1e1e]/80 backdrop-blur-md sticky top-0 z-10 px-8 py-5 flex items-center justify-between">
        <div class="flex items-center gap-4">
            <div class="w-10 h-10 bg-purple-100 dark:bg-purple-500/20 rounded-xl flex items-center justify-center transform -rotate-3 border border-purple-200 dark:border-purple-500/30">
                <FolderOpen class="w-5 h-5 text-purple-600 dark:text-purple-400 fill-purple-600/20" />
            </div>
            <div>
                <h1 class="text-xl font-extrabold text-[#1c1c1e] dark:text-[#f4f4f5] tracking-tight">File Manager</h1>
                <p class="text-[12px] font-medium text-gray-400 dark:text-gray-500 flex items-center gap-2">
                    {{ fileItems.length }} Indexed Files
                </p>
            </div>
        </div>
        <div>
            <button @click="openSettings" class="flex items-center gap-2 px-4 py-2 bg-gray-100 dark:bg-white/5 hover:bg-gray-200 dark:hover:bg-white/10 text-gray-700 dark:text-gray-300 rounded-lg text-sm font-semibold border border-gray-200 dark:border-transparent transition-all shadow-sm">
                <Settings class="w-4 h-4" /> Config Tracking
            </button>
        </div>
    </div>

    <!-- Main Content -->
    <div class="flex-1 overflow-y-auto px-8 py-6 scroll-smooth bg-gray-50/30 dark:bg-black/10">
        <div class="max-w-5xl mx-auto">
            
            <!-- Search Bar -->
            <div class="mb-8 relative group">
                <Search class="w-5 h-5 absolute left-4 top-1/2 transform -translate-y-1/2 text-gray-400 group-focus-within:text-purple-500 transition-colors" />
                <input 
                    v-model="searchQuery" 
                    placeholder="Search by file name, path, extension or Type # to search by tag..." 
                    class="w-full pl-11 pr-4 py-3.5 bg-white xl:bg-white dark:bg-[#1a1a1a] border border-gray-200 dark:border-[#333] rounded-2xl text-[14.5px] focus:outline-none focus:ring-4 focus:ring-purple-500/10 focus:border-purple-400 transition-all font-medium text-gray-800 dark:text-gray-200 shadow-sm" 
                />
            </div>
            
            <!-- Loading -->
            <div v-if="isLoading" class="flex justify-center py-20 opacity-50">
                <div class="w-6 h-6 rounded-full border-2 border-black dark:border-white border-t-transparent animate-spin"></div>
            </div>

            <!-- Empty State -->
            <div v-else-if="fileItems.length === 0" class="text-center py-24 px-4 bg-gray-50 dark:bg-[#1a1a1a] rounded-3xl border border-dashed border-gray-200 dark:border-[#2c2c2c]">
                <div class="w-20 h-20 bg-white dark:bg-white/5 shadow-sm rounded-full flex items-center justify-center mx-auto mb-5 border border-gray-100 dark:border-white/10">
                    <Search class="w-8 h-8 text-gray-300 dark:text-gray-600" />
                </div>
                <h3 class="text-lg font-bold text-gray-800 dark:text-gray-200 mb-2">No Files Found</h3>
                <p class="text-[#52525b] dark:text-[#a1a1aa] text-[14px]">You haven't tracked any folders yet, or your assets are empty.</p>
                <button @click="openSettings" class="mt-6 px-6 py-2 bg-black hover:bg-gray-800 text-white dark:bg-white dark:hover:bg-gray-200 dark:text-black rounded-xl font-bold shadow-md transition-all">
                    Setup Data Sources
                </button>
            </div>

            <!-- List View (Grouped) -->
            <div v-else class="space-y-6">
                <div v-for="[groupName, items] in groupedItems" :key="groupName" class="animate-in fade-in slide-in-from-bottom-2 duration-500">
                    <button @click="toggleGroup(groupName)" class="w-full text-left font-black text-gray-500 dark:text-gray-400 hover:text-black dark:hover:text-white transition-colors uppercase tracking-widest pl-2 mb-3 flex items-center justify-between group">
                        <div class="flex items-center gap-2 text-[11px]">
                            <FolderOpen class="w-3.5 h-3.5" /> {{ groupName }}
                            <span class="text-gray-300 dark:text-gray-600 px-1">{{ items.length }}</span>
                        </div>
                        <div class="text-gray-300 dark:text-gray-600 group-hover:text-black dark:group-hover:text-white pr-2">
                            <ChevronDown v-if="!collapsedGroups.has(groupName)" class="w-4 h-4" />
                            <ChevronRight v-else class="w-4 h-4" />
                        </div>
                    </button>
                    
                    <div v-show="!collapsedGroups.has(groupName)" class="bg-white dark:bg-[#1a1a1a] border border-gray-100 dark:border-[#2c2c2c] rounded-2xl overflow-hidden shadow-sm">
                        <div v-for="(item, idx) in items" :key="item.id"
                             @click="selectedFile = item"
                             class="group flex items-center gap-4 p-4 hover:bg-gray-50 dark:hover:bg-white/5 cursor-pointer transition-colors"
                             :class="{'border-b border-gray-100 dark:border-[#2c2c2c]': idx !== items.length - 1}">
                            
                            <div class="w-10 h-10 rounded-xl bg-purple-50 dark:bg-purple-500/10 flex items-center justify-center flex-shrink-0 group-hover:bg-purple-100 dark:group-hover:bg-purple-500/20 transition-colors">
                                <component :is="getFileIcon(item.extension)" class="w-5 h-5 text-purple-600 dark:text-purple-400" />
                            </div>
                            
                            <div class="flex-1 min-w-0">
                                <div class="flex items-center gap-2">
                                    <h4 class="font-semibold text-[14.5px] text-[#1c1c1e] dark:text-[#f4f4f5] truncate">{{ item.name }}</h4>
                                    <!-- Render tags if any -->
                                    <span v-for="tag in item.tags" :key="tag" class="text-[10px] px-1.5 py-0.5 rounded-md bg-gray-100 dark:bg-gray-800 text-gray-500 dark:text-gray-400 flex items-center shadow-sm">
                                        <Hash class="w-2.5 h-2.5 opacity-60 mr-0.5" /> {{ tag }}
                                    </span>
                                </div>
                                <div class="flex items-center gap-3 mt-0.5">
                                    <p class="text-[12px] text-gray-500 dark:text-gray-400 font-mono truncate max-w-[50%]">{{ item.absolute_path }}</p>
                                    <span class="text-[12px] text-gray-400 dark:text-gray-500">{{ item.size_mb.toFixed(2) }} MB</span>
                                </div>
                            </div>
                            
                            <div class="flex-shrink-0 pr-2">
                                <ChevronRight class="w-4 h-4 text-gray-300 dark:text-gray-600 group-hover:text-black dark:group-hover:text-white transition-colors" />
                            </div>
                        </div>
                    </div>
                </div>
            </div>

        </div>
    </div>

    <!-- Slide-over Preview Panel -->
    <div class="absolute top-0 right-0 h-full w-[400px] border-l border-gray-200 dark:border-[#2c2c2c] bg-[#fbfbfc] dark:bg-[#1a1a1a] shadow-2xl transition-transform duration-300 ease-out z-30"
         :class="selectedFile ? 'translate-x-0' : 'translate-x-full'">
        
        <div class="flex flex-col h-full" v-if="selectedFile">
            <!-- Header -->
            <div class="h-16 border-b border-gray-200 dark:border-[#2c2c2c] flex items-center justify-between px-6 flex-shrink-0 bg-white/80 dark:bg-[#1e1e1e]/80 backdrop-blur-md">
                <div class="flex items-center gap-3">
                    <div class="p-2 rounded-lg bg-purple-100 dark:bg-purple-500/20 text-purple-600 dark:text-purple-400">
                        <File class="w-4 h-4" />
                    </div>
                    <span class="text-xs font-bold tracking-widest text-gray-800 dark:text-gray-200 uppercase">File Properties</span>
                </div>
                <button @click="selectedFile = null" class="p-2 rounded-full hover:bg-gray-200 dark:hover:bg-white/10 text-gray-500 transition-colors">
                    <ChevronRight class="w-5 h-5" />
                </button>
            </div>

            <!-- Content Area -->
            <div class="flex-1 px-8 py-8 bg-white dark:bg-[#1a1a1a] break-words overflow-y-auto">
                <div class="flex items-center gap-0.5 mb-6">
                    <template v-if="selectedFile.source_folder === 'assets'">
                        <h2 class="text-3xl font-extrabold text-[#1c1c1e] dark:text-white leading-tight tracking-tight break-all">{{ selectedFile.name }}</h2>
                    </template>
                    <template v-else>
                        <input 
                            v-model="editFileName" 
                            @blur="saveFileMetadata"
                            @keydown.enter="($event.target as any).blur()"
                            :disabled="isSavingMeta"
                            class="bg-transparent border-none outline-none text-3xl font-extrabold text-[#1c1c1e] dark:text-white leading-tight tracking-tight focus:ring-0 p-0 min-w-0 flex-shrink"
                            placeholder="File Name"
                            :style="`width: ${Math.max(editFileName.length, 1)}ch; max-width: 100%;`"
                        />
                        <span v-if="editFileExt" class="text-3xl font-extrabold text-gray-300 dark:text-gray-600 flex-shrink-0">{{ editFileExt }}</span>
                    </template>
                </div>
                
                <div class="space-y-6">
                    <div>
                        <span class="block text-[11px] uppercase font-bold text-gray-400 mb-2">Tags</span>
                        <div class="flex flex-wrap items-center gap-2">
                            <span v-for="tag in editTags" :key="tag" class="text-xs px-2 py-1.5 rounded-md bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-300 flex items-center gap-1 shadow-sm group">
                                <Hash class="w-3 h-3 opacity-50"/> 
                                {{ tag }}
                                <button @click="removeTag(tag)" class="opacity-0 group-hover:opacity-100 hover:text-red-500 transition-opacity ml-0.5"><X class="w-3 h-3"/></button>
                            </span>
                            
                            <input 
                               v-model="newTagInput"
                               @keydown.enter="addTag"
                               placeholder="+ Add tag (Enter)"
                               class="text-xs bg-transparent border border-dashed border-gray-300 dark:border-gray-600 rounded-md py-1.5 px-2 w-28 focus:w-36 focus:outline-none focus:border-gray-400 transition-all text-[#1c1c1e] dark:text-[#f4f4f5]"
                            />
                        </div>
                    </div>

                    <div class="flex items-center justify-between">
                        <div>
                            <span class="block text-[11px] uppercase font-bold text-gray-400 mb-1">Source Size</span>
                            <div class="text-[15px] font-medium text-gray-800 dark:text-gray-200">
                                {{ selectedFile.size_mb.toFixed(2) }} MB
                            </div>
                        </div>
                        <div>
                            <span class="block text-[11px] uppercase font-bold text-gray-400 mb-1">Type</span>
                            <div class="text-[15px] p-1 px-3 rounded-md bg-gray-100 dark:bg-white/10 font-bold font-mono text-gray-800 dark:text-gray-200">
                                {{ selectedFile.extension ? `.${selectedFile.extension}` : 'Unknown' }}
                            </div>
                        </div>
                    </div>
                    
                    <div>
                        <span class="block text-[11px] uppercase font-bold text-gray-400 mb-1">Last Modified</span>
                        <div class="text-[15px] font-medium text-gray-800 dark:text-gray-200">
                            {{ selectedFile.date_modified }}
                        </div>
                    </div>
                    
                    <div class="p-4 bg-gray-50 dark:bg-[#222] rounded-xl border border-gray-200 dark:border-[#333]">
                        <span class="block text-[11px] uppercase font-bold text-gray-400 mb-2">Absolute Path</span>
                        <code class="block font-mono text-[13px] text-[#52525b] dark:text-[#a1a1aa] break-all">{{ selectedFile.absolute_path }}</code>
                    </div>
                </div>

                <div class="mt-8 pt-6 border-t border-gray-100 dark:border-[#2c2c2c]">
                    <button @click="openLocalFile(selectedFile.absolute_path)" class="w-full flex items-center justify-center gap-2 px-5 py-3.5 bg-black hover:bg-gray-800 text-white dark:bg-white dark:hover:bg-gray-200 dark:text-black rounded-xl font-bold transition-all shadow-md active:scale-[0.98]">
                        Open in System Viewer
                    </button>
                </div>
            </div>
        </div>
    </div>
    
    <!-- Backdrop for mobile/smaller screens -->
    <div v-if="selectedFile" @click="selectedFile = null" class="absolute inset-0 z-20 bg-black/5 dark:bg-black/20 backdrop-blur-[1px] transition-opacity 2xl:hidden"></div>

    <!-- Settings Modal -->
    <div v-if="isSettingsOpen" class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-md">
        <div class="bg-white dark:bg-[#1a1a1a] rounded-3xl w-full max-w-lg shadow-2xl overflow-hidden border border-gray-200 dark:border-[#2c2c2c] flex flex-col scale-100 transition-transform">
            <div class="px-6 py-5 border-b border-gray-100 dark:border-[#2c2c2c] flex justify-between items-center bg-gray-50/50 dark:bg-[#1a1a1a]">
                <h3 class="font-extrabold text-[15px] text-gray-800 dark:text-gray-200 flex items-center gap-2 uppercase tracking-wider">
                    <FolderSync class="w-4 h-4 text-purple-500" /> Tracked Sources
                </h3>
                <button @click="isSettingsOpen = false" class="text-gray-400 hover:text-black dark:hover:text-white bg-gray-200/50 dark:bg-white/10 p-1.5 rounded-full transition-colors">
                    <X class="w-4 h-4" />
                </button>
            </div>
            <div class="p-6">
                <label class="block text-[13px] font-bold text-gray-700 dark:text-gray-300 mb-3 tracking-wide">Sync External Folders</label>
                <textarea v-model="trackedSources" rows="5" class="w-full p-4 font-mono text-[13px] sm:text-sm border border-gray-200 dark:border-[#333] rounded-2xl bg-white dark:bg-[#111] text-gray-800 dark:text-gray-200 focus:ring-2 focus:ring-purple-500/20 focus:border-purple-500 dark:focus:ring-purple-500/20 dark:focus:border-purple-400 outline-none transition-all placeholder:text-gray-300 dark:placeholder:text-gray-700 leading-relaxed resize-none shadow-inner" placeholder="/Users/kid0604/Google Drive&#10;/Users/kid0604/Downloads"></textarea>
                <p class="text-[12px] text-gray-500 mt-3 flex items-start gap-1.5 font-medium">
                    <span class="text-purple-500 mt-0.5">*</span>
                    These folders and their 1st level children will be scanned and cached.
                </p>
            </div>
            <div class="px-6 py-4 border-t border-gray-100 dark:border-[#2c2c2c] flex justify-end gap-3 bg-gray-50/50 dark:bg-[#151515]">
                <button @click="isSettingsOpen = false" class="px-5 py-2.5 rounded-xl font-bold text-[13px] text-gray-600 dark:text-gray-400 hover:bg-gray-200 dark:hover:bg-[#2c2c2c] transition-colors">Cancel</button>
                <button @click="saveAndReindex" :disabled="isReindexing" class="px-5 py-2.5 rounded-xl font-bold text-[13px] text-white bg-black hover:bg-gray-800 dark:bg-white dark:text-black dark:hover:bg-gray-200 flex items-center gap-2 transition-all shadow-sm" :class="{'opacity-70 cursor-not-allowed': isReindexing}">
                    <FolderSync class="w-4 h-4" :class="{'animate-spin': isReindexing}" /> 
                    {{ isReindexing ? 'Indexing...' : 'Save & Sync' }}
                </button>
            </div>
        </div>
    </div>

  </div>
</template>
