<script setup lang="ts">
import { FolderOpen, FolderSync, X, Plus, Trash2, HardDrive, Unlink,
  ImageIcon, Video, Music, Code, FileType, Menu, Copy } from 'lucide-vue-next';
import type { useFileStore } from '../composables/useFileStore';

const props = defineProps<{
  store: ReturnType<typeof useFileStore>;
  isOpen: boolean;
}>();
const emit = defineEmits<{
  (e: 'update:isOpen', v: boolean): void;
  (e: 'showDuplicates'): void;
}>();

const categories = ['Images', 'Documents', 'Videos', 'Audio', 'Code', 'Archives'] as const;
const catIcon = (t: string) => {
  if (t === 'Images') return ImageIcon;
  if (t === 'Videos') return Video;
  if (t === 'Audio') return Music;
  if (t === 'Code') return Code;
  return FileType;
};
</script>

<template>
  <div class="absolute md:relative inset-y-0 left-0 w-64 flex-shrink-0 bg-[#fbfbfc] dark:bg-[#191919] border-r border-[#e6e6e6] dark:border-[#2c2c2c] flex flex-col z-40 transition-transform duration-300 md:translate-x-0"
       :class="isOpen ? 'translate-x-0 shadow-2xl' : '-translate-x-full'">
    <div class="p-4 md:p-6 flex items-center justify-between">
      <div class="flex items-center gap-3">
        <button @click="store.syncAllSources" :class="{'animate-spin text-white': store.isScanning.value}" class="w-8 h-8 rounded-lg bg-gradient-to-br from-indigo-500 to-purple-600 flex items-center justify-center transition-all cursor-pointer text-white hover:scale-105 active:scale-95 shadow-lg shadow-indigo-500/20">
          <FolderSync class="w-4 h-4" />
        </button>
        <h1 class="font-bold text-lg tracking-tight text-gray-900 dark:text-white">Files</h1>
      </div>
      <button @click="emit('update:isOpen', false)" class="md:hidden p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#333] text-gray-500 transition-colors">
        <X class="w-5 h-5" />
      </button>
    </div>

    <div class="flex-1 overflow-y-auto px-4 pb-6 space-y-8">
      <!-- Sources -->
      <div>
        <div class="flex items-center justify-between px-2 mb-2">
          <h3 class="text-xs font-bold text-gray-400 dark:text-gray-500 uppercase tracking-wider">Locations</h3>
          <button @click="store.addNewSource" class="text-gray-400 hover:text-indigo-500 transition-colors cursor-pointer"><Plus class="w-4 h-4" /></button>
        </div>
        <div class="space-y-1">
          <button @click="store.activeSourceId.value = null; store.activeType.value = null; store.activeTag.value = null"
            class="w-full flex items-center gap-3 px-3 py-2 rounded-xl text-sm font-medium transition-all cursor-pointer"
            :class="!store.activeSourceId.value && !store.activeType.value ? 'bg-indigo-50 dark:bg-indigo-500/10 text-indigo-600 dark:text-indigo-400' : 'hover:bg-gray-100 dark:hover:bg-white/5 text-gray-600 dark:text-gray-400'">
            <HardDrive class="w-4 h-4" /> All Files
          </button>
          <div v-for="source in store.sources.value" :key="source.id" class="group relative">
            <button @click="store.activeSourceId.value = source.id; store.activeType.value = null; store.activeTag.value = null"
              class="w-full flex items-center gap-3 px-3 py-2 rounded-xl text-sm font-medium transition-all cursor-pointer"
              :class="store.activeSourceId.value === source.id ? 'bg-indigo-50 dark:bg-indigo-500/10 text-indigo-600 dark:text-indigo-400' : 'hover:bg-gray-100 dark:hover:bg-white/5 text-gray-600 dark:text-gray-400'">
              <FolderOpen class="w-4 h-4" /> <span class="truncate">{{ source.name }}</span>
            </button>
            <button @click="store.removeSource(source.id)" class="absolute right-2 top-1/2 -translate-y-1/2 md:opacity-0 opacity-100 group-hover:opacity-100 p-1.5 hover:bg-red-100 dark:hover:bg-red-500/20 text-red-500 rounded-md transition-all cursor-pointer">
              <Trash2 class="w-3.5 h-3.5" />
            </button>
          </div>

          <!-- Cloud -->
          <div class="flex items-center justify-between px-2 pt-4 mb-2">
            <h3 class="text-xs font-bold text-gray-400 dark:text-gray-500 uppercase tracking-wider">Cloud Drives</h3>
          </div>
          <button v-if="!store.isGDriveConnected.value" @click="store.connectGDrive"
            class="w-full flex items-center gap-3 px-3 py-2 rounded-xl text-sm font-medium transition-all hover:bg-blue-50 dark:hover:bg-blue-500/10 text-blue-600 dark:text-blue-400 cursor-pointer">
            <FolderSync v-if="store.isConnectingGDrive.value" class="w-4 h-4 animate-spin" />
            <svg v-else class="w-4 h-4" viewBox="0 0 24 24" fill="currentColor"><path d="M12 2L2 19h7.5l5.5-9.5h7L12 2zm1.5 12.5L8 22h14l-5.5-9.5h-3zM2 19l4.5 7.5h7.5L9.5 19H2z"/></svg>
            Connect Google Drive
          </button>
          <div v-else class="relative group">
            <button @click="store.activeSourceId.value = 'gdrive'; store.activeType.value = null; store.activeTag.value = null"
              class="w-full flex items-center gap-3 px-3 py-2 rounded-xl text-sm transition-all cursor-pointer"
              :class="store.activeSourceId.value === 'gdrive' ? 'bg-blue-50 dark:bg-blue-500/10 text-blue-600 dark:text-blue-400' : 'hover:bg-gray-100 dark:hover:bg-white/5 text-gray-600 dark:text-gray-400'">
              <svg class="w-4 h-4 shrink-0" viewBox="0 0 24 24" fill="currentColor"><path d="M12 2L2 19h7.5l5.5-9.5h7L12 2zm1.5 12.5L8 22h14l-5.5-9.5h-3zM2 19l4.5 7.5h7.5L9.5 19H2z"/></svg>
              <div class="flex flex-col items-start truncate pr-6">
                <span class="font-medium truncate">Google Drive</span>
                <span v-if="store.gdriveEmail.value" class="text-[10px] opacity-70 truncate">{{ store.gdriveEmail.value }}</span>
              </div>
            </button>
            <button @click.stop="store.disconnectGDrive" class="absolute right-2 top-1/2 -translate-y-1/2 md:opacity-0 opacity-100 group-hover:opacity-100 p-1.5 hover:bg-red-100 dark:hover:bg-red-500/20 text-red-500 rounded-md transition-all cursor-pointer">
              <Unlink class="w-3.5 h-3.5" />
            </button>
          </div>
        </div>
      </div>

      <!-- Categories -->
      <div>
        <h3 class="text-xs font-bold text-gray-400 dark:text-gray-500 uppercase tracking-wider px-2 mb-2">Categories</h3>
        <div class="space-y-1">
          <button v-for="t in categories" :key="t"
            @click="store.activeType.value = t; store.activeSourceId.value = null; store.activeTag.value = null"
            class="w-full flex items-center gap-3 px-3 py-2 rounded-xl text-sm font-medium transition-all cursor-pointer"
            :class="store.activeType.value === t ? 'bg-purple-50 dark:bg-purple-500/10 text-purple-600 dark:text-purple-400' : 'hover:bg-gray-100 dark:hover:bg-white/5 text-gray-600 dark:text-gray-400'">
            <component :is="catIcon(t)" class="w-4 h-4" /> {{ t }}
          </button>
        </div>
      </div>

      <!-- Duplicates -->
      <div>
        <h3 class="text-xs font-bold text-gray-400 dark:text-gray-500 uppercase tracking-wider px-2 mb-2">Tools</h3>
        <button @click="$emit('showDuplicates')"
          class="w-full flex items-center gap-3 px-3 py-2 rounded-xl text-sm font-medium transition-all cursor-pointer hover:bg-gray-100 dark:hover:bg-white/5 text-gray-600 dark:text-gray-400">
          <Copy class="w-4 h-4" />
          <span>Duplicates</span>
          <span v-if="store.duplicateReport.value?.total_groups" class="ml-auto px-1.5 py-0.5 bg-amber-100 dark:bg-amber-500/20 text-amber-600 dark:text-amber-400 rounded text-[10px] font-bold">
            {{ store.duplicateReport.value.total_groups }}
          </span>
        </button>
      </div>

      <!-- Tags -->
      <div>
        <h3 class="text-xs font-bold text-gray-400 dark:text-gray-500 uppercase tracking-wider px-2 mb-2">Tags</h3>
        <div v-if="store.allTags.value.length > 0" class="flex flex-wrap gap-1.5 px-2">
          <button v-for="tag in store.allTags.value" :key="tag"
            @click="store.activeTag.value = store.activeTag.value === tag ? null : tag; store.activeSourceId.value = null; store.activeType.value = null"
            class="px-2.5 py-1 rounded-lg text-[11px] font-medium transition-all cursor-pointer border"
            :class="store.activeTag.value === tag
              ? 'bg-emerald-50 dark:bg-emerald-500/15 text-emerald-600 dark:text-emerald-400 border-emerald-200 dark:border-emerald-500/30'
              : 'bg-white/50 dark:bg-white/5 text-gray-500 dark:text-gray-400 border-gray-200/50 dark:border-white/10 hover:border-emerald-300 dark:hover:border-emerald-500/30 hover:text-emerald-600 dark:hover:text-emerald-400'">
            #{{ tag }}
          </button>
        </div>
        <p v-else class="px-3 text-[11px] text-gray-400 dark:text-gray-500 italic">No tags yet</p>
      </div>
    </div>
  </div>
</template>
