<script setup lang="ts">
import { ref, computed, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { 
    FolderOpen, FolderSync, X, File, Search, FileText,
    Video, Music, FileArchive, Code, Plus, Trash2,
    HardDrive, Image as ImageIcon, FileType, LayoutGrid, List
} from 'lucide-vue-next';

const props = defineProps<{
    vaultPath: string;
}>();

interface FileMetadata {
    id: string;
    path: string;
    filename: string;
    extension: string;
    size: number;
    created_at: string;
    modified_at: string;
    tags: string[];
    source_type: string;
}

interface FileSource {
    id: string;
    path: string;
    name: string;
}

const files = ref<FileMetadata[]>([]);
const sources = ref<FileSource[]>([]);
const isLoading = ref(true);
const isScanning = ref(false);

const selectedFile = ref<FileMetadata | null>(null);
const activeSourceId = ref<string | null>(null);
const activeType = ref<string | null>(null);

const searchQuery = ref('');
const viewMode = ref<'grid' | 'list'>('grid');

const editFileName = ref('');
const newTagInput = ref('');
const isAddingTag = ref(false);
const isSavingTags = ref(false);

import { watch } from 'vue';

watch(selectedFile, (newVal) => {
    if (newVal) {
        // Strip extension for edit field
        if (newVal.extension && newVal.filename.endsWith(`.${newVal.extension}`)) {
            editFileName.value = newVal.filename.substring(0, newVal.filename.lastIndexOf('.'));
        } else {
            editFileName.value = newVal.filename;
        }
        isAddingTag.value = false;
        newTagInput.value = '';
    }
});

const isAssetsFile = computed(() => {
    if (!selectedFile.value) return false;
    return selectedFile.value.path.includes('/assets/');
});

const saveFileName = async () => {
    if (!selectedFile.value || isAssetsFile.value || isSavingTags.value) return;
    let newName = editFileName.value.trim();
    if (!newName) {
        // Revert
        if (selectedFile.value.extension && selectedFile.value.filename.endsWith(`.${selectedFile.value.extension}`)) {
            editFileName.value = selectedFile.value.filename.substring(0, selectedFile.value.filename.lastIndexOf('.'));
        } else {
            editFileName.value = selectedFile.value.filename;
        }
        return;
    }
    
    // Append extension back
    if (selectedFile.value.extension && !newName.endsWith(`.${selectedFile.value.extension}`)) {
        newName = `${newName}.${selectedFile.value.extension}`;
    }
    
    if (newName === selectedFile.value.filename) {
        return;
    }
    
    isSavingTags.value = true;
    try {
        const newPath = await invoke<string>('update_file_metadata', {
            vaultPath: props.vaultPath,
            path: selectedFile.value.path,
            newFilename: newName,
            newTags: selectedFile.value.tags
        });
        
        selectedFile.value.filename = newName;
        selectedFile.value.path = newPath;
        
        const idx = files.value.findIndex(f => f.id === selectedFile.value!.id);
        if (idx !== -1) {
            files.value[idx].filename = newName;
            files.value[idx].path = newPath;
        }
    } catch(e) {
        console.error("Failed to rename file", e);
        editFileName.value = selectedFile.value.filename;
    } finally {
        isSavingTags.value = false;
    }
};

const addTag = async () => {
    if (!selectedFile.value || !newTagInput.value.trim() || isSavingTags.value) return;
    const tag = newTagInput.value.trim().toLowerCase();
    
    if (selectedFile.value.tags.includes(tag)) {
        newTagInput.value = '';
        isAddingTag.value = false;
        return;
    }
    
    isSavingTags.value = true;
    const updatedTags = [...selectedFile.value.tags, tag];
    
    try {
        await invoke('update_file_metadata', {
            vaultPath: props.vaultPath,
            path: selectedFile.value.path,
            newFilename: selectedFile.value.filename,
            newTags: updatedTags
        });
        selectedFile.value.tags = updatedTags;
        newTagInput.value = '';
        isAddingTag.value = false;
        
        // Update in main list
        const idx = files.value.findIndex(f => f.id === selectedFile.value!.id);
        if (idx !== -1) files.value[idx].tags = updatedTags;
    } catch(e) {
        console.error("Failed to add tag", e);
    } finally {
        isSavingTags.value = false;
    }
};

const removeTag = async (tag: string) => {
    if (!selectedFile.value || isSavingTags.value) return;
    isSavingTags.value = true;
    
    const updatedTags = selectedFile.value.tags.filter(t => t !== tag);
    try {
        await invoke('update_file_metadata', {
            vaultPath: props.vaultPath,
            path: selectedFile.value.path,
            newFilename: selectedFile.value.filename,
            newTags: updatedTags
        });
        selectedFile.value.tags = updatedTags;
        
        const idx = files.value.findIndex(f => f.id === selectedFile.value!.id);
        if (idx !== -1) files.value[idx].tags = updatedTags;
    } catch(e) {
        console.error("Failed to remove tag", e);
    } finally {
        isSavingTags.value = false;
    }
};

const fetchSources = async () => {
    try {
        sources.value = await invoke<FileSource[]>('get_file_sources', { vaultPath: props.vaultPath });
    } catch (e) {
        console.error("Failed to load sources", e);
    }
};

const fetchFiles = async () => {
    isLoading.value = true;
    try {
        files.value = await invoke<FileMetadata[]>('query_files', { vaultPath: props.vaultPath });
    } catch (e) {
        console.error("Failed to load files", e);
    } finally {
        isLoading.value = false;
    }
};

const addNewSource = async () => {
    try {
        const selectedPath = await open({
            directory: true,
            multiple: false,
            title: "Select a folder to sync"
        });
        
        if (selectedPath && typeof selectedPath === 'string') {
            const folderName = selectedPath.split('/').pop() || selectedPath.split('\\').pop() || "Unknown Folder";
            await invoke('add_file_source', { 
                vaultPath: props.vaultPath, 
                path: selectedPath, 
                name: folderName 
            });
            await fetchSources();
            // Start scan visually
            isScanning.value = true;
            // Await actual scan so UI refreshes after it's done
            await invoke('scan_directory', { vaultPath: props.vaultPath, sourcePath: selectedPath });
            await fetchFiles();
            isScanning.value = false;
        }
    } catch (e) {
        console.error("Failed to add source", e);
        isScanning.value = false;
    }
};

const removeSource = async (id: string) => {
    try {
        await invoke('remove_file_source', { vaultPath: props.vaultPath, sourceId: id });
        if (activeSourceId.value === id) activeSourceId.value = null;
        await fetchSources();
        await fetchFiles(); // Refresh to remove files from that source (if backend supports it later)
    } catch (e) {
        console.error("Failed to remove source", e);
    }
};

const openLocalFile = async (path: string) => {
    try {
        await invoke('open_local_file', { vaultPath: props.vaultPath, path });
    } catch(e) {
        console.error("Failed to open file", e);
    }
};

const getFileIcon = (ext: string) => {
    const e = ext.toLowerCase();
    if (['jpg', 'jpeg', 'png', 'gif', 'svg', 'webp'].includes(e)) return ImageIcon;
    if (['pdf', 'txt', 'md', 'doc', 'docx'].includes(e)) return FileText;
    if (['mp4', 'mov', 'avi'].includes(e)) return Video;
    if (['mp3', 'wav'].includes(e)) return Music;
    if (['zip', 'rar', 'gz'].includes(e)) return FileArchive;
    if (['js', 'ts', 'vue', 'json', 'html', 'css', 'rs', 'py'].includes(e)) return Code;
    return File;
};

const getFileTypeGroup = (ext: string) => {
    const e = ext.toLowerCase();
    if (['jpg', 'jpeg', 'png', 'gif', 'svg', 'webp'].includes(e)) return 'Images';
    if (['pdf', 'txt', 'md', 'doc', 'docx'].includes(e)) return 'Documents';
    if (['mp4', 'mov', 'avi'].includes(e)) return 'Videos';
    if (['mp3', 'wav'].includes(e)) return 'Audio';
    if (['zip', 'rar', 'gz'].includes(e)) return 'Archives';
    if (['js', 'ts', 'vue', 'json', 'html', 'css', 'rs', 'py'].includes(e)) return 'Code';
    return 'Other';
};

const filteredFiles = computed(() => {
    let result = files.value;
    
    if (activeSourceId.value) {
        const source = sources.value.find(s => s.id === activeSourceId.value);
        if (source) {
            result = result.filter(f => f.path.startsWith(source.path));
        }
    }
    
    if (activeType.value) {
        result = result.filter(f => getFileTypeGroup(f.extension) === activeType.value);
    }
    
    if (searchQuery.value) {
        let q = searchQuery.value.toLowerCase().trim();
        const isTagSearch = q.startsWith('#');
        
        if (isTagSearch) {
            q = q.slice(1);
            result = result.filter(f => f.tags.some(t => t.toLowerCase().includes(q)));
        } else {
            result = result.filter(f => 
                f.filename.toLowerCase().includes(q) || 
                f.tags.some(t => t.toLowerCase().includes(q)) ||
                f.extension.toLowerCase().includes(q)
            );
        }
    }
    
    return result;
});

const formatSize = (bytes: number) => {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
};

onMounted(async () => {
    await fetchSources();
    await fetchFiles();
});

</script>

<template>
  <div class="h-full w-full flex bg-[#f5f5f7] dark:bg-[#0a0a0a] font-sans text-gray-900 dark:text-gray-100 overflow-hidden">
    
    <!-- Sidebar -->
    <div class="w-64 flex-shrink-0 bg-white/40 dark:bg-white/[0.02] backdrop-blur-xl border-r border-gray-200/50 dark:border-white/5 flex flex-col z-20">
        <div class="p-6 flex items-center gap-3">
            <div class="w-8 h-8 rounded-lg bg-gradient-to-br from-indigo-500 to-purple-600 flex items-center justify-center shadow-lg shadow-indigo-500/20">
                <FolderSync class="w-4 h-4 text-white" />
            </div>
            <h1 class="font-bold text-lg tracking-tight">OmniDrive</h1>
        </div>
        
        <div class="flex-1 overflow-y-auto px-4 pb-6 space-y-8">
            <!-- Sources -->
            <div>
                <div class="flex items-center justify-between px-2 mb-2">
                    <h3 class="text-xs font-bold text-gray-400 dark:text-gray-500 uppercase tracking-wider">Locations</h3>
                    <button @click="addNewSource" class="text-gray-400 hover:text-indigo-500 transition-colors">
                        <Plus class="w-4 h-4" />
                    </button>
                </div>
                <div class="space-y-1">
                    <button @click="activeSourceId = null; activeType = null" 
                            class="w-full flex items-center gap-3 px-3 py-2 rounded-xl text-sm font-medium transition-all"
                            :class="!activeSourceId && !activeType ? 'bg-indigo-50 dark:bg-indigo-500/10 text-indigo-600 dark:text-indigo-400' : 'hover:bg-gray-100 dark:hover:bg-white/5 text-gray-600 dark:text-gray-400'">
                        <HardDrive class="w-4 h-4" /> All Files
                    </button>
                    
                    <div v-for="source in sources" :key="source.id" class="group relative">
                        <button @click="activeSourceId = source.id; activeType = null" 
                                class="w-full flex items-center gap-3 px-3 py-2 rounded-xl text-sm font-medium transition-all"
                                :class="activeSourceId === source.id ? 'bg-indigo-50 dark:bg-indigo-500/10 text-indigo-600 dark:text-indigo-400' : 'hover:bg-gray-100 dark:hover:bg-white/5 text-gray-600 dark:text-gray-400'">
                            <FolderOpen class="w-4 h-4" /> <span class="truncate">{{ source.name }}</span>
                        </button>
                        <button @click="removeSource(source.id)" class="absolute right-2 top-1/2 -translate-y-1/2 opacity-0 group-hover:opacity-100 p-1.5 hover:bg-red-100 dark:hover:bg-red-500/20 text-red-500 rounded-md transition-all">
                            <Trash2 class="w-3.5 h-3.5" />
                        </button>
                    </div>
                </div>
            </div>

            <!-- Types -->
            <div>
                <h3 class="text-xs font-bold text-gray-400 dark:text-gray-500 uppercase tracking-wider px-2 mb-2">Categories</h3>
                <div class="space-y-1">
                    <button v-for="t in ['Images', 'Documents', 'Videos', 'Audio', 'Code', 'Archives']" :key="t"
                            @click="activeType = t; activeSourceId = null"
                            class="w-full flex items-center gap-3 px-3 py-2 rounded-xl text-sm font-medium transition-all"
                            :class="activeType === t ? 'bg-purple-50 dark:bg-purple-500/10 text-purple-600 dark:text-purple-400' : 'hover:bg-gray-100 dark:hover:bg-white/5 text-gray-600 dark:text-gray-400'">
                        <component :is="t === 'Images' ? ImageIcon : t === 'Videos' ? Video : t === 'Audio' ? Music : t === 'Code' ? Code : FileType" class="w-4 h-4" />
                        {{ t }}
                    </button>
                </div>
            </div>
        </div>
    </div>

    <!-- Main Content -->
    <div class="flex-1 flex flex-col relative z-10 min-w-0">
        <!-- Header -->
        <div class="h-20 px-8 flex items-center justify-between border-b border-gray-200/50 dark:border-white/5 bg-white/30 dark:bg-black/20 backdrop-blur-md">
            <div class="flex-1 max-w-xl relative group">
                <Search class="w-4 h-4 absolute left-4 top-1/2 -translate-y-1/2 text-gray-400 group-focus-within:text-indigo-500 transition-colors" />
                <input v-model="searchQuery" placeholder="Search files, tags, extensions..." class="w-full pl-10 pr-4 py-2.5 bg-white/50 dark:bg-white/5 border border-gray-200/50 dark:border-white/10 rounded-xl text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500/50 transition-all text-gray-800 dark:text-gray-200 placeholder:text-gray-400" />
            </div>
            
            <div class="flex items-center gap-2 ml-4 bg-white/50 dark:bg-white/5 p-1 rounded-lg border border-gray-200/50 dark:border-white/10">
                <button @click="viewMode = 'grid'" class="p-1.5 rounded-md transition-colors" :class="viewMode === 'grid' ? 'bg-white dark:bg-white/10 shadow-sm text-indigo-500' : 'text-gray-400 hover:text-gray-600 dark:hover:text-gray-300'">
                    <LayoutGrid class="w-4 h-4" />
                </button>
                <button @click="viewMode = 'list'" class="p-1.5 rounded-md transition-colors" :class="viewMode === 'list' ? 'bg-white dark:bg-white/10 shadow-sm text-indigo-500' : 'text-gray-400 hover:text-gray-600 dark:hover:text-gray-300'">
                    <List class="w-4 h-4" />
                </button>
            </div>
        </div>

        <!-- Scanning Indicator -->
        <div v-if="isScanning" class="w-full bg-indigo-500/10 text-indigo-600 dark:text-indigo-400 px-8 py-3 text-sm font-medium flex items-center gap-3">
            <FolderSync class="w-4 h-4 animate-spin" /> Scanning directory and indexing files...
        </div>

        <!-- File List/Grid -->
        <div class="flex-1 overflow-y-auto p-8 custom-scrollbar">
            <div v-if="isLoading" class="flex justify-center py-20">
                <div class="w-8 h-8 border-2 border-indigo-500 border-t-transparent rounded-full animate-spin"></div>
            </div>
            
            <div v-else-if="filteredFiles.length === 0" class="flex flex-col items-center justify-center h-full text-gray-400">
                <FileArchive class="w-16 h-16 mb-4 opacity-20" />
                <p class="text-lg font-medium text-gray-500">No files found</p>
                <p class="text-sm">Try adjusting your filters or adding a new source.</p>
            </div>

            <!-- Grid View -->
            <div v-else-if="viewMode === 'grid'" class="grid grid-cols-2 md:grid-cols-3 xl:grid-cols-4 2xl:grid-cols-5 gap-4">
                <div v-for="file in filteredFiles" :key="file.id" 
                     @click="selectedFile = file"
                     class="group bg-white/60 dark:bg-white/[0.03] border border-gray-200/50 dark:border-white/5 backdrop-blur-md rounded-2xl p-4 cursor-pointer hover:bg-white dark:hover:bg-white/10 transition-all hover:shadow-xl hover:shadow-indigo-500/5 hover:-translate-y-1"
                     :class="{'ring-2 ring-indigo-500 border-transparent': selectedFile?.id === file.id}">
                    <div class="aspect-square rounded-xl bg-gray-100/50 dark:bg-black/20 mb-4 flex items-center justify-center border border-gray-200/30 dark:border-white/5">
                        <component :is="getFileIcon(file.extension)" class="w-12 h-12 text-gray-400 dark:text-gray-500 group-hover:text-indigo-500 transition-colors" />
                    </div>
                    <h4 class="text-sm font-bold truncate mb-1" :title="file.filename">{{ file.filename }}</h4>
                    <div class="flex items-center justify-between text-xs text-gray-500">
                        <span>{{ file.extension.toUpperCase() || 'FILE' }}</span>
                        <span>{{ formatSize(file.size) }}</span>
                    </div>
                </div>
            </div>

            <!-- List View -->
            <div v-else class="bg-white/60 dark:bg-white/[0.03] border border-gray-200/50 dark:border-white/5 backdrop-blur-md rounded-2xl overflow-hidden shadow-sm">
                <table class="w-full text-left text-sm">
                    <thead class="bg-gray-50/50 dark:bg-black/20 text-gray-500 font-medium border-b border-gray-200/50 dark:border-white/5">
                        <tr>
                            <th class="px-6 py-4 font-medium">Name</th>
                            <th class="px-6 py-4 font-medium">Size</th>
                            <th class="px-6 py-4 font-medium">Modified</th>
                            <th class="px-6 py-4 font-medium">Tags</th>
                        </tr>
                    </thead>
                    <tbody class="divide-y divide-gray-200/50 dark:divide-white/5">
                        <tr v-for="file in filteredFiles" :key="file.id" 
                            @click="selectedFile = file"
                            class="hover:bg-white dark:hover:bg-white/5 cursor-pointer transition-colors"
                            :class="{'bg-indigo-50/50 dark:bg-indigo-500/10': selectedFile?.id === file.id}">
                            <td class="px-6 py-3">
                                <div class="flex items-center gap-3">
                                    <component :is="getFileIcon(file.extension)" class="w-5 h-5 text-indigo-500" />
                                    <span class="font-medium truncate max-w-[200px] xl:max-w-md">{{ file.filename }}</span>
                                </div>
                            </td>
                            <td class="px-6 py-3 text-gray-500">{{ formatSize(file.size) }}</td>
                            <td class="px-6 py-3 text-gray-500">{{ file.modified_at.split(' ')[0] }}</td>
                            <td class="px-6 py-3">
                                <div class="flex gap-1">
                                    <span v-for="t in file.tags.slice(0,2)" :key="t" class="px-2 py-0.5 bg-gray-100 dark:bg-white/10 rounded text-xs text-gray-600 dark:text-gray-300">#{{ t }}</span>
                                    <span v-if="file.tags.length > 2" class="px-2 py-0.5 bg-gray-100 dark:bg-white/10 rounded text-xs text-gray-600 dark:text-gray-300">+{{file.tags.length - 2}}</span>
                                </div>
                            </td>
                        </tr>
                    </tbody>
                </table>
            </div>
        </div>
    </div>

    <!-- Detail Panel -->
    <div v-if="selectedFile" class="w-80 flex-shrink-0 bg-white/70 dark:bg-white/[0.03] backdrop-blur-2xl border-l border-gray-200/50 dark:border-white/5 flex flex-col z-20 shadow-[-10px_0_30px_rgba(0,0,0,0.02)] dark:shadow-[-10px_0_30px_rgba(0,0,0,0.2)] animate-in slide-in-from-right-4 duration-300">
        <div class="h-20 px-6 flex items-center justify-between border-b border-gray-200/50 dark:border-white/5">
            <h2 class="font-bold text-sm">File Details</h2>
            <button @click="selectedFile = null" class="p-2 hover:bg-gray-100 dark:hover:bg-white/10 rounded-full transition-colors text-gray-500">
                <X class="w-4 h-4" />
            </button>
        </div>
        
        <div class="flex-1 overflow-y-auto p-6">
            <div class="aspect-square w-full rounded-2xl bg-gradient-to-br from-indigo-50 to-purple-50 dark:from-indigo-500/10 dark:to-purple-500/10 border border-indigo-100 dark:border-indigo-500/20 flex items-center justify-center mb-6 shadow-inner">
                <component :is="getFileIcon(selectedFile.extension)" class="w-20 h-20 text-indigo-500/50 dark:text-indigo-400/50" />
            </div>
            
            <template v-if="isAssetsFile">
                <h3 class="font-extrabold text-lg break-words leading-tight mb-2">{{ selectedFile.filename }}</h3>
            </template>
            <template v-else>
                <div class="flex items-center gap-0.5 mb-2">
                    <input 
                        v-model="editFileName"
                        @blur="saveFileName"
                        @keydown.enter="($event.target as HTMLInputElement).blur()"
                        :disabled="isSavingTags"
                        class="bg-transparent border-none outline-none font-extrabold text-lg break-all p-0 focus:ring-0 text-gray-900 dark:text-white min-w-0"
                        placeholder="File Name"
                        :style="{ width: `${Math.max(editFileName.length, 1)}ch`, maxWidth: '100%' }"
                    />
                    <span v-if="selectedFile.extension" class="font-extrabold text-lg text-gray-400 dark:text-gray-500 flex-shrink-0">.{{ selectedFile.extension }}</span>
                </div>
            </template>
            
            <div class="space-y-4 mt-6">
                <div class="p-4 rounded-xl bg-gray-50/50 dark:bg-black/20 border border-gray-100 dark:border-white/5 space-y-3">
                    <div class="flex justify-between text-sm">
                        <span class="text-gray-500">Type</span>
                        <span class="font-medium uppercase">{{ selectedFile.extension || 'Unknown' }}</span>
                    </div>
                    <div class="flex justify-between text-sm">
                        <span class="text-gray-500">Size</span>
                        <span class="font-medium">{{ formatSize(selectedFile.size) }}</span>
                    </div>
                    <div class="flex justify-between text-sm">
                        <span class="text-gray-500">Modified</span>
                        <span class="font-medium">{{ selectedFile.modified_at.split(' ')[0] }}</span>
                    </div>
                </div>

                <div>
                    <h4 class="text-xs font-bold text-gray-400 uppercase tracking-wider mb-2">Location</h4>
                    <p class="text-xs font-mono text-gray-500 break-all p-3 bg-white dark:bg-black/40 rounded-lg border border-gray-200/50 dark:border-white/5">{{ selectedFile.path }}</p>
                </div>
                
                <div>
                    <h4 class="text-xs font-bold text-gray-400 uppercase tracking-wider mb-2">Tags</h4>
                    <div class="flex flex-wrap items-center gap-2">
                        <span v-for="tag in selectedFile.tags" :key="tag" class="group relative px-2.5 py-1 bg-indigo-50 dark:bg-indigo-500/10 text-indigo-600 dark:text-indigo-400 rounded-lg text-xs font-medium border border-indigo-100 dark:border-indigo-500/20 shadow-sm flex items-center gap-1">
                            #{{ tag }}
                            <button @click="removeTag(tag)" class="opacity-0 group-hover:opacity-100 hover:text-red-500 transition-opacity ml-0.5" :disabled="isSavingTags">
                                <X class="w-3 h-3" />
                            </button>
                        </span>
                        
                        <input 
                            v-if="isAddingTag"
                            v-model="newTagInput"
                            @keydown.enter="addTag"
                            @blur="isAddingTag = false; newTagInput = ''"
                            ref="tagInputRef"
                            type="text"
                            placeholder="Tag name..."
                            class="px-2.5 py-1 bg-white dark:bg-black/40 border border-indigo-300 dark:border-indigo-500/50 rounded-lg text-xs font-medium focus:outline-none w-24 shadow-sm"
                            autofocus
                        />
                        <button v-else @click="isAddingTag = true" class="px-2.5 py-1 bg-white dark:bg-white/5 border border-dashed border-gray-300 dark:border-gray-600 rounded-lg text-xs font-medium text-gray-500 hover:text-indigo-500 transition-colors">
                            + Add Tag
                        </button>
                    </div>
                </div>
            </div>
        </div>
        
        <div class="p-6 border-t border-gray-200/50 dark:border-white/5 bg-white/50 dark:bg-transparent">
            <button @click="openLocalFile(selectedFile.path)" class="w-full py-3 rounded-xl bg-gray-900 dark:bg-white text-white dark:text-gray-900 font-bold text-sm shadow-xl shadow-gray-900/20 dark:shadow-white/10 hover:scale-[1.02] active:scale-[0.98] transition-all">
                Open in Native App
            </button>
        </div>
    </div>
  </div>
</template>

<style scoped>
.custom-scrollbar::-webkit-scrollbar {
    width: 6px;
}
.custom-scrollbar::-webkit-scrollbar-track {
    background: transparent;
}
.custom-scrollbar::-webkit-scrollbar-thumb {
    background: rgba(156, 163, 175, 0.3);
    border-radius: 10px;
}
.dark .custom-scrollbar::-webkit-scrollbar-thumb {
    background: rgba(255, 255, 255, 0.1);
}
</style>
