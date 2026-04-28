<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open, confirm, message } from '@tauri-apps/plugin-dialog';
import { marked } from 'marked';
import DOMPurify from 'dompurify';
import { useVirtualList, useWindowSize } from '@vueuse/core';
import { 
    FolderOpen, FolderSync, X, Search, FileText,
    Video, Music, FileArchive, Code, Plus, Trash2,
    ImageIcon, LayoutGrid, List, HardDrive, FileType, Unlink, Menu
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
const isSidebarOpen = ref(false);

const selectedFile = ref<FileMetadata | null>(null);
const activeSourceId = ref<string | null>(null);
const activeType = ref<string | null>(null);

const searchQuery = ref('');
const viewMode = ref<'grid' | 'list'>('list');

const editFileName = ref('');
const newTagInput = ref('');
const isAddingTag = ref(false);
const isSavingTags = ref(false);

const previewContent = ref<string | null>(null);
const isLoadingPreview = ref(false);

const isImageFile = computed(() => {
    if (!selectedFile.value) return false;
    return ['jpg', 'jpeg', 'png', 'gif', 'svg', 'webp', 'bmp', 'ico', 'tiff', 'heic'].includes(selectedFile.value.extension.toLowerCase());
});

const isVideoFile = computed(() => {
    if (!selectedFile.value) return false;
    return ['mp4', 'mov', 'avi', 'webm', 'mkv', 'flv', 'wmv', 'm4v'].includes(selectedFile.value.extension.toLowerCase());
});

const isAudioFile = computed(() => {
    if (!selectedFile.value) return false;
    return ['mp3', 'wav', 'ogg', 'm4a', 'flac', 'aac', 'wma', 'alac'].includes(selectedFile.value.extension.toLowerCase());
});

const isPdfFile = computed(() => {
    if (!selectedFile.value) return false;
    return selectedFile.value.extension.toLowerCase() === 'pdf';
});

const isTextFile = computed(() => {
    if (!selectedFile.value) return false;
    return ['js', 'ts', 'vue', 'json', 'html', 'css', 'rs', 'py', 'txt', 'md', 'csv', 'yaml', 'toml', 'xml'].includes(selectedFile.value.extension.toLowerCase());
});

import { watch } from 'vue';

watch(selectedFile, async (newVal) => {
    if (newVal) {
        // Strip extension for edit field
        if (newVal.extension && newVal.filename.endsWith(`.${newVal.extension}`)) {
            editFileName.value = newVal.filename.substring(0, newVal.filename.lastIndexOf('.'));
        } else {
            editFileName.value = newVal.filename;
        }
        isAddingTag.value = false;
        newTagInput.value = '';
        
        // Load text preview if necessary
        previewContent.value = null;
        if (isTextFile.value) {
            isLoadingPreview.value = true;
            try {
                previewContent.value = await invoke<string>('read_local_file_content', { path: newVal.path });
            } catch (e) {
                console.error("Failed to read text preview", e);
                previewContent.value = "Unable to load preview or file is too large.";
            } finally {
                isLoadingPreview.value = false;
            }
        }
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

const syncAllSources = async () => {
    if (isScanning.value) return;
    isScanning.value = true;
    try {
        await invoke('reindex_sources', { vaultPath: props.vaultPath });
        if (isGDriveConnected.value) {
            await invoke('get_gdrive_files', { vaultPath: props.vaultPath });
        }
        await fetchFiles();
    } catch(e) {
        console.error("Failed to sync sources", e);
    } finally {
        isScanning.value = false;
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
    const extLower = ext.toLowerCase();
    if (['jpg', 'jpeg', 'png', 'gif', 'webp', 'bmp', 'svg', 'heic'].includes(extLower)) return ImageIcon;
    if (['mp4', 'mkv', 'avi', 'mov', 'webm'].includes(extLower)) return Video;
    if (['mp3', 'wav', 'flac', 'ogg', 'm4a'].includes(extLower)) return Music;
    if (['pdf', 'doc', 'docx', 'txt', 'md'].includes(extLower)) return FileText;
    if (['zip', 'rar', '7z', 'tar', 'gz'].includes(extLower)) return FileArchive;
    if (['js', 'ts', 'vue', 'rs', 'py', 'json', 'html', 'css'].includes(extLower)) return Code;
    return FileType;
};

// --- Cloud Connect ---
const isGDriveConnected = ref(false);
const gdriveEmail = ref('');
const isConnectingGDrive = ref(false);

const checkGDriveStatus = async () => {
    try {
        isGDriveConnected.value = await invoke<boolean>('is_gdrive_connected', { vaultPath: props.vaultPath });
        if (isGDriveConnected.value) {
            gdriveEmail.value = await invoke<string>('get_gdrive_user_info', { vaultPath: props.vaultPath });
        }
    } catch (e) {
        console.error("Failed to check GDrive status", e);
    }
};

const connectGDrive = async () => {
    if (isConnectingGDrive.value) return;
    isConnectingGDrive.value = true;
    try {
        const resp = await invoke<string>('connect_gdrive', { vaultPath: props.vaultPath });
        if (resp === 'WAITING_DEEP_LINK') {
            // Keep spinning until the deep link event listener handles it
            return;
        }
        
        if (resp === 'SUCCESS') {
            isGDriveConnected.value = true;
            gdriveEmail.value = await invoke<string>('get_gdrive_user_info', { vaultPath: props.vaultPath });
            // Fetch the files, which will auto-insert into DB
            await invoke('get_gdrive_files', { vaultPath: props.vaultPath });
            // Refresh local state
            await fetchFiles();
            activeSourceId.value = 'gdrive';
        }
    } catch (e: any) {
        console.error("Failed to connect GDrive", JSON.stringify(e));
        const errStr = typeof e === 'object' ? JSON.stringify(e) : String(e);
        await message(`Failed to connect Google Drive: ${errStr}`, { title: 'Error', kind: 'error' });
        isConnectingGDrive.value = false;
    }
    
    // Only reset loading if not waiting for deep link
    isConnectingGDrive.value = false;
};

const syncGDrive = async () => {
    isScanning.value = true;
    try {
        await invoke('get_gdrive_files', { vaultPath: props.vaultPath });
        await fetchFiles();
    } catch (e: any) {
        console.error("GDrive sync failed", e);
    } finally {
        isScanning.value = false;
    }
};

const disconnectGDrive = async () => {
    const isConfirmed = await confirm('Are you sure you want to disconnect Google Drive? This will remove all cloud files from your view.', { title: 'Disconnect Google Drive', kind: 'warning' });
    if (!isConfirmed) return;
    try {
        await invoke('disconnect_gdrive', { vaultPath: props.vaultPath });
        isGDriveConnected.value = false;
        gdriveEmail.value = '';
        if (activeSourceId.value === 'gdrive') activeSourceId.value = null;
        await fetchFiles();
    } catch (e: any) {
        console.error("Failed to disconnect", e);
    }
};

const getFileTypeGroup = (ext: string) => {
    const e = ext.toLowerCase();
    if (['jpg', 'jpeg', 'png', 'gif', 'svg', 'webp', 'bmp', 'ico', 'tiff', 'heic'].includes(e)) return 'Images';
    if (['pdf', 'txt', 'md', 'doc', 'docx'].includes(e)) return 'Documents';
    if (['mp4', 'mov', 'avi', 'webm', 'mkv', 'flv', 'wmv', 'm4v'].includes(e)) return 'Videos';
    if (['mp3', 'wav', 'ogg', 'm4a', 'flac', 'aac', 'wma', 'alac'].includes(e)) return 'Audio';
    if (['zip', 'rar', 'gz'].includes(e)) return 'Archives';
    if (['js', 'ts', 'vue', 'json', 'html', 'css', 'rs', 'py'].includes(e)) return 'Code';
    return 'Other';
};

const filteredFiles = computed(() => {
    let result = files.value;
    
    if (activeSourceId.value) {
        if (activeSourceId.value === 'gdrive') {
            result = result.filter(f => f.source_type === 'gdrive');
        } else {
            const source = sources.value.find(s => s.id === activeSourceId.value);
            if (source) {
                result = result.filter(f => f.path.startsWith(source.path));
            }
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

const { width } = useWindowSize();
const gridCols = computed(() => {
    if (width.value >= 1536) return 5;
    if (width.value >= 1280) return 4;
    if (width.value >= 768) return 3;
    return 2;
});

const gridRows = computed(() => {
    const result = [];
    const cols = gridCols.value;
    for (let i = 0; i < filteredFiles.value.length; i += cols) {
        result.push(filteredFiles.value.slice(i, i + cols));
    }
    return result;
});

const { list: virtualListItems, containerProps, wrapperProps } = useVirtualList(filteredFiles, {
  itemHeight: 57,
});

const { list: virtualGridRows, containerProps: gridContainerProps, wrapperProps: gridWrapperProps } = useVirtualList(gridRows, {
  itemHeight: 180,
});

const formatSize = (bytes: number) => {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
};

let unlistenAuthEvent: (() => void) | null = null;

onMounted(async () => {
    await fetchSources();
    await checkGDriveStatus();
    await fetchFiles();
    // Silently sync in background on load
    syncAllSources();

    listen('omnidrive-auth-code', async (e: any) => {
        const code = e.payload.code;
        try {
            const success = await invoke('connect_gdrive_complete', { authCode: code, vaultPath: props.vaultPath });
            if (success) {
                isGDriveConnected.value = true;
                gdriveEmail.value = await invoke<string>('get_gdrive_user_info', { vaultPath: props.vaultPath });
                await invoke('get_gdrive_files', { vaultPath: props.vaultPath });
                await fetchFiles();
                activeSourceId.value = 'gdrive';
            }
        } catch (err: any) {
             console.error("OmniDrive auth complete failed", err);
             const errStr = typeof err === 'object' ? JSON.stringify(err) : String(err);
             await message(`Failed to connect Google Drive: ${errStr}`, { title: 'Error', kind: 'error' });
        } finally {
            isConnectingGDrive.value = false;
        }
    }).then(fn => unlistenAuthEvent = fn);
});

onUnmounted(() => {
    if (unlistenAuthEvent) unlistenAuthEvent();
});

</script>

<template>
  <div class="h-full w-full flex relative bg-[#f5f5f7] dark:bg-[#0a0a0a] font-sans text-gray-900 dark:text-gray-100 overflow-hidden">
    
    <!-- Mobile Sidebar Overlay -->
    <div v-if="isSidebarOpen" @click="isSidebarOpen = false" class="md:hidden absolute inset-0 bg-black/20 dark:bg-black/40 z-30 transition-opacity"></div>

    <!-- Sidebar -->
    <div class="absolute md:relative inset-y-0 left-0 w-64 flex-shrink-0 bg-white/95 md:bg-white/40 dark:bg-[#1a1a1a]/95 md:dark:bg-white/[0.02] backdrop-blur-xl border-r border-gray-200/50 dark:border-white/5 flex flex-col z-40 transition-transform duration-300 md:translate-x-0"
         :class="isSidebarOpen ? 'translate-x-0 shadow-2xl' : '-translate-x-full'">
        <div class="p-4 md:p-6 flex items-center justify-between">
            <div class="flex items-center gap-3">
                <button @click="syncAllSources" :class="{'animate-spin text-white': isScanning, 'shadow-lg shadow-indigo-500/20 text-white hover:scale-105 active:scale-95': !isScanning}" class="w-8 h-8 rounded-lg bg-gradient-to-br from-indigo-500 to-purple-600 flex items-center justify-center transition-all cursor-pointer group" title="Sync Files">
                    <FolderSync class="w-4 h-4" />
                </button>
                <h1 class="font-bold text-lg tracking-tight">OmniDrive</h1>
            </div>
            <button @click="isSidebarOpen = false" class="md:hidden p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#333] text-gray-500 transition-colors">
                <X class="w-5 h-5" />
            </button>
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
                        <button @click="removeSource(source.id)" class="absolute right-2 top-1/2 -translate-y-1/2 md:opacity-0 opacity-100 group-hover:opacity-100 p-1.5 hover:bg-red-100 dark:hover:bg-red-500/20 text-red-500 rounded-md transition-all">
                            <Trash2 class="w-3.5 h-3.5" />
                        </button>
                    </div>

                    <!-- Cloud Drives -->
                    <div class="flex items-center justify-between px-2 pt-4 mb-2">
                        <h3 class="text-xs font-bold text-gray-400 dark:text-gray-500 uppercase tracking-wider">Cloud Drives</h3>
                        <button @click="syncGDrive" title="Sync Google Drive" class="text-gray-400 hover:text-indigo-500 transition-colors">
                            <FolderSync class="w-4 h-4" />
                        </button>
                    </div>
                    <button @click="connectGDrive" v-if="!isGDriveConnected" class="w-full flex items-center gap-3 px-3 py-2 rounded-xl text-sm font-medium transition-all hover:bg-blue-50 dark:hover:bg-blue-500/10 text-blue-600 dark:text-blue-400">
                        <svg v-if="!isConnectingGDrive" class="w-4 h-4" viewBox="0 0 24 24" fill="currentColor">
                            <path d="M12 2L2 19h7.5l5.5-9.5h7L12 2zm1.5 12.5L8 22h14l-5.5-9.5h-3zM2 19l4.5 7.5h7.5L9.5 19H2z"/>
                        </svg>
                        <FolderSync v-else class="w-4 h-4 animate-spin" />
                        Connect Google Drive
                    </button>
                    
                    <div v-else class="relative group">
                        <button @click="activeSourceId = 'gdrive'; activeType = null" 
                                class="w-full flex items-center gap-3 px-3 py-2 rounded-xl text-sm transition-all"
                                :class="activeSourceId === 'gdrive' ? 'bg-blue-50 dark:bg-blue-500/10 text-blue-600 dark:text-blue-400' : 'hover:bg-gray-100 dark:hover:bg-white/5 text-gray-600 dark:text-gray-400'">
                            <svg class="w-4 h-4 shrink-0" viewBox="0 0 24 24" fill="currentColor">
                                <path d="M12 2L2 19h7.5l5.5-9.5h7L12 2zm1.5 12.5L8 22h14l-5.5-9.5h-3zM2 19l4.5 7.5h7.5L9.5 19H2z"/>
                            </svg>
                            <div class="flex flex-col items-start truncate pr-6">
                                <span class="font-medium truncate">Google Drive</span>
                                <span v-if="gdriveEmail" class="text-[10px] opacity-70 truncate">{{ gdriveEmail }}</span>
                            </div>
                        </button>
                        <button @click.stop="disconnectGDrive" title="Disconnect" class="absolute right-2 top-1/2 -translate-y-1/2 md:opacity-0 opacity-100 group-hover:opacity-100 p-1.5 hover:bg-red-100 dark:hover:bg-red-500/20 text-red-500 rounded-md transition-all">
                            <Unlink class="w-3.5 h-3.5" />
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
        <div class="h-16 md:h-20 px-4 md:px-8 flex items-center gap-3 md:gap-4 justify-between border-b border-gray-200/50 dark:border-white/5 bg-white/30 dark:bg-black/20 backdrop-blur-md">
            <button @click="isSidebarOpen = true" class="md:hidden p-2 -ml-2 rounded-xl hover:bg-gray-100 dark:hover:bg-white/5 text-gray-600 dark:text-gray-300 transition-colors">
                <Menu class="w-5 h-5" />
            </button>
            <div class="flex-1 max-w-xl relative group">
                <Search class="w-4 h-4 absolute left-3 md:left-4 top-1/2 -translate-y-1/2 text-gray-400 group-focus-within:text-indigo-500 transition-colors" />
                <input v-model="searchQuery" placeholder="Search files, tags..." class="w-full pl-9 md:pl-10 pr-4 py-2 md:py-2.5 bg-white/50 dark:bg-white/5 border border-gray-200/50 dark:border-white/10 rounded-xl text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500/50 transition-all text-gray-800 dark:text-gray-200 placeholder:text-gray-400" />
            </div>
            
            <div class="flex items-center gap-1 md:gap-2 bg-white/50 dark:bg-white/5 p-1 rounded-lg border border-gray-200/50 dark:border-white/10 flex-shrink-0">
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
        <div class="flex-1 overflow-y-auto p-4 md:p-8 custom-scrollbar">
            <div v-if="isLoading" class="flex justify-center py-20">
                <div class="w-8 h-8 border-2 border-indigo-500 border-t-transparent rounded-full animate-spin"></div>
            </div>
            
            <div v-else-if="filteredFiles.length === 0" class="flex flex-col items-center justify-center h-full text-gray-400">
                <FileArchive class="w-12 h-12 md:w-16 md:h-16 mb-4 opacity-20" />
                <p class="text-base md:text-lg font-medium text-gray-500">No files found</p>
                <p class="text-xs md:text-sm text-center">Try adjusting your filters or adding a new source.</p>
            </div>

            <!-- Grid View (Virtual) -->
            <div v-else-if="viewMode === 'grid'" v-bind="gridContainerProps" class="h-full overflow-y-auto custom-scrollbar">
                <div v-bind="gridWrapperProps">
                    <div v-for="{ index, data: row } in virtualGridRows" :key="index" class="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 2xl:grid-cols-6 gap-3 md:gap-4 mb-4">
                        <div v-for="file in row" :key="file.id" 
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
                </div>
            </div>

            <!-- List View -->
            <div v-else class="bg-white/60 dark:bg-white/[0.03] border border-gray-200/50 dark:border-white/5 backdrop-blur-md rounded-2xl overflow-hidden shadow-sm flex flex-col h-full">
                <!-- List Header (Desktop Only) -->
                <div class="hidden md:grid grid-cols-[2fr_1fr_1fr_2fr] gap-4 px-6 py-4 bg-gray-50/50 dark:bg-black/20 text-gray-500 font-medium border-b border-gray-200/50 dark:border-white/5 sticky top-0 z-10 text-sm">
                    <div>Name</div>
                    <div>Size</div>
                    <div>Modified</div>
                    <div>Tags</div>
                </div>
                
                <!-- List Body (Virtual) -->
                <div v-bind="containerProps" class="flex-1 overflow-y-auto custom-scrollbar">
                    <div v-bind="wrapperProps">
                        <div v-for="{ data: file } in virtualListItems" :key="file.id" 
                            @click="selectedFile = file"
                            class="flex flex-col md:grid md:grid-cols-[2fr_1fr_1fr_2fr] gap-1 md:gap-4 px-4 md:px-6 py-3 hover:bg-white dark:hover:bg-white/5 cursor-pointer transition-colors border-b border-gray-100/50 dark:border-white/5 text-sm"
                            :class="{'bg-indigo-50/50 dark:bg-indigo-500/10': selectedFile?.id === file.id}">
                            
                            <!-- Name & Icon -->
                            <div class="flex items-center gap-3 overflow-hidden">
                                <component :is="getFileIcon(file.extension)" class="w-6 h-6 md:w-5 md:h-5 flex-shrink-0 text-indigo-500" />
                                <span class="font-medium truncate text-base md:text-sm">{{ file.filename }}</span>
                            </div>
                            
                            <!-- Metadata (Second line on mobile, Columns on desktop) -->
                            <div class="flex items-center gap-3 md:contents text-xs md:text-sm pl-9 md:pl-0">
                                <div class="text-gray-500 truncate font-mono md:font-sans">{{ formatSize(file.size) }}</div>
                                <div class="text-gray-400 md:text-gray-500 truncate">{{ file.modified_at.split(' ')[0] }}</div>
                                
                                <div class="flex gap-1 overflow-hidden ml-auto md:ml-0">
                                    <span v-for="t in file.tags.slice(0,2)" :key="t" class="px-1.5 md:px-2 py-0.5 bg-gray-100 dark:bg-white/10 rounded text-[10px] md:text-xs text-gray-600 dark:text-gray-300 truncate">#{{ t }}</span>
                                    <span v-if="file.tags.length > 2" class="px-1.5 md:px-2 py-0.5 bg-gray-100 dark:bg-white/10 rounded text-[10px] md:text-xs text-gray-600 dark:text-gray-300 flex-shrink-0">+{{file.tags.length - 2}}</span>
                                </div>
                            </div>
                            
                        </div>
                    </div>
                </div>
            </div>
        </div>
    </div>

    <!-- Detail Panel -->
    <div v-if="selectedFile" class="w-96 xl:w-[450px] flex-shrink-0 bg-white/70 dark:bg-white/[0.03] backdrop-blur-2xl border-l border-gray-200/50 dark:border-white/5 flex flex-col z-20 shadow-[-10px_0_30px_rgba(0,0,0,0.02)] dark:shadow-[-10px_0_30px_rgba(0,0,0,0.2)] animate-in slide-in-from-right-4 duration-300">
        <div class="h-20 px-6 flex items-center justify-between border-b border-gray-200/50 dark:border-white/5">
            <h2 class="font-bold text-sm">File Details</h2>
            <button @click="selectedFile = null" class="p-2 hover:bg-gray-100 dark:hover:bg-white/10 rounded-full transition-colors text-gray-500">
                <X class="w-4 h-4" />
            </button>
        </div>
        
        <div class="flex-1 overflow-y-auto p-6">
            <div class="aspect-square w-full rounded-2xl bg-gradient-to-br from-indigo-50 to-purple-50 dark:from-indigo-500/10 dark:to-purple-500/10 border border-indigo-100 dark:border-indigo-500/20 flex items-center justify-center mb-6 shadow-inner overflow-hidden relative">
                
                <img v-if="isImageFile" :src="convertFileSrc(selectedFile.path)" class="w-full h-full object-contain" />
                
                <video v-else-if="isVideoFile" :src="convertFileSrc(selectedFile.path)" controls class="w-full h-full object-contain bg-black/5" />
                
                <audio v-else-if="isAudioFile" :src="convertFileSrc(selectedFile.path)" controls class="w-full px-4" />
                
                <iframe v-else-if="isPdfFile" :src="convertFileSrc(selectedFile.path)" class="w-full h-full border-none bg-white"></iframe>
                
                <div v-else-if="isTextFile" class="w-full h-full overflow-y-auto bg-white/50 dark:bg-black/20 p-4 text-xs font-mono text-gray-700 dark:text-gray-300 custom-scrollbar text-left">
                    <div v-if="isLoadingPreview" class="flex items-center justify-center h-full">
                        <div class="w-5 h-5 border-2 border-indigo-500 border-t-transparent rounded-full animate-spin"></div>
                    </div>
                    <div v-else-if="selectedFile.extension.toLowerCase() === 'md'" class="prose prose-sm dark:prose-invert max-w-none" v-html="DOMPurify.sanitize(marked.parse(previewContent || '') as string)"></div>
                    <pre v-else class="whitespace-pre-wrap break-words m-0">{{ previewContent }}</pre>
                </div>
                
                <component v-else :is="getFileIcon(selectedFile.extension)" class="w-20 h-20 text-indigo-500/50 dark:text-indigo-400/50" />
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
                            <button @click="removeTag(tag)" class="md:opacity-0 opacity-100 group-hover:opacity-100 hover:text-red-500 transition-opacity ml-0.5" :disabled="isSavingTags">
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
