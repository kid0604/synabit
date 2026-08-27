<script setup lang="ts">
import { FolderOpen, FolderSync, X, Plus, Trash2, HardDrive, Camera, Cloud, Unlink, Bookmark,
  ImageIcon, Video, Music, Code, FileType, Copy, FilePlus2 } from 'lucide-vue-next';
import type { useFileStore } from '../composables/useFileStore';

const props = defineProps<{
  store: ReturnType<typeof useFileStore>;
  isOpen: boolean;
}>();
const emit = defineEmits<{
  (e: 'update:isOpen', v: boolean): void;
  (e: 'showDuplicates'): void;
  (e: 'saveCollection'): void;
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
        <button @click="store.syncAllSources" :class="{'animate-spin text-white': store.isScanning.value}" class="w-8 h-8 rounded-lg bg-gradient-to-br from-indigo-500 to-purple-600 flex items-center justify-center transition-all cursor-pointer text-white hover:scale-105 active:scale-95 shadow-lg shadow-indigo-500/20" :aria-label="$t('file.sync_all')">
          <FolderSync class="w-4 h-4" />
        </button>
        <h1 class="font-bold text-lg tracking-tight text-gray-900 dark:text-white">{{ $t('file.title') }}</h1>
      </div>
      <button @click="emit('update:isOpen', false)" class="md:hidden p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#333] text-gray-500 transition-colors" :aria-label="$t('file.close_panel')">
        <X class="w-5 h-5" />
      </button>
    </div>

    <div v-if="store.textProgress.value" class="mx-4 mb-3 px-3 py-2 rounded-xl bg-emerald-50 dark:bg-emerald-500/10 text-xs">
      <span class="font-medium text-emerald-700 dark:text-emerald-300">
        {{ $t('file.reading_text') }} · {{ $t('file.reading_remaining', { count: store.textProgress.value.remaining.toLocaleString() }) }}
      </span>
    </div>

    <div v-if="store.scanProgress.value" class="mx-4 mb-3 px-3 py-2 rounded-xl bg-indigo-50 dark:bg-indigo-500/10 text-xs">
      <div class="flex items-center justify-between gap-2">
        <span class="font-medium text-indigo-700 dark:text-indigo-300 truncate">
          {{ $t('file.scan_files', { count: store.scanProgress.value.indexed.toLocaleString() }) }}
          <span v-if="store.scanProgress.value.hashed" class="opacity-70">· {{ $t('file.scan_read', { count: store.scanProgress.value.hashed.toLocaleString() }) }}</span>
        </span>
        <button @click="store.stopScanning" class="text-indigo-500 hover:text-indigo-700 dark:hover:text-indigo-200 font-semibold cursor-pointer flex-shrink-0">{{ $t('file.scan_stop') }}</button>
      </div>
      <p class="text-[10px] text-indigo-500/70 dark:text-indigo-300/60 truncate mt-0.5">{{ store.scanProgress.value.source }}</p>
    </div>

    <div class="flex-1 overflow-y-auto px-4 pb-6 space-y-8">
      <!-- Sources -->
      <div>
        <div class="flex items-center justify-between px-2 mb-2">
          <h3 class="text-xs font-bold text-gray-400 dark:text-gray-500 uppercase tracking-wider">{{ $t('file.locations') }}</h3>
          <div class="flex items-center gap-1">
            <button @click="store.importFiles" class="text-gray-400 hover:text-emerald-500 transition-colors cursor-pointer" :title="$t('file.import_files')"><FilePlus2 class="w-4 h-4" /></button>
            <button @click="store.addNewSource" class="text-gray-400 hover:text-indigo-500 transition-colors cursor-pointer" :title="$t('file.add_folder')"><Plus class="w-4 h-4" /></button>
          </div>
        </div>
        <div class="space-y-1">
          <button @click="store.activeSourceId.value = null; store.activeType.value = null; store.activeTag.value = null"
            class="w-full flex items-center gap-3 px-3 py-2 rounded-xl text-sm font-medium transition-all cursor-pointer"
            :class="!store.activeSourceId.value && !store.activeType.value ? 'bg-indigo-50 dark:bg-indigo-500/10 text-indigo-600 dark:text-indigo-400' : 'hover:bg-gray-100 dark:hover:bg-white/5 text-gray-600 dark:text-gray-400'">
            <HardDrive class="w-4 h-4" /> {{ $t('file.all_files') }}
          </button>
          <div v-for="source in store.sources.value" :key="source.id" class="group relative">
            <button @click="store.activeSourceId.value = source.id; store.activeType.value = null; store.activeTag.value = null"
              class="w-full flex items-center gap-3 px-3 py-2 rounded-xl text-sm font-medium transition-all cursor-pointer"
              :class="store.activeSourceId.value === source.id ? 'bg-indigo-50 dark:bg-indigo-500/10 text-indigo-600 dark:text-indigo-400' : 'hover:bg-gray-100 dark:hover:bg-white/5 text-gray-600 dark:text-gray-400'">
              <FolderOpen class="w-4 h-4" /> <span class="truncate">{{ source.name }}</span>
            </button>
            <button @click="store.removeSource(source.id)" class="absolute right-2 top-1/2 -translate-y-1/2 md:opacity-0 opacity-100 group-hover:opacity-100 p-1.5 hover:bg-red-100 dark:hover:bg-red-500/20 text-red-500 rounded-md transition-all cursor-pointer" :aria-label="$t('file.remove_source')">
              <Trash2 class="w-3.5 h-3.5" />
            </button>
          </div>

        </div>
      </div>

      <!-- Categories -->
      <div>
        <h3 class="text-xs font-bold text-gray-400 dark:text-gray-500 uppercase tracking-wider px-2 mb-2">{{ $t('file.categories') }}</h3>
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
        <h3 class="text-xs font-bold text-gray-400 dark:text-gray-500 uppercase tracking-wider px-2 mb-2">{{ $t('file.tools') }}</h3>
        <button @click="$emit('showDuplicates')"
          class="w-full flex items-center gap-3 px-3 py-2 rounded-xl text-sm font-medium transition-all cursor-pointer hover:bg-gray-100 dark:hover:bg-white/5 text-gray-600 dark:text-gray-400">
          <Copy class="w-4 h-4" />
          <span>{{ $t('file.duplicates') }}</span>
          <span v-if="store.duplicateReport.value?.total_groups" class="ml-auto px-1.5 py-0.5 bg-amber-100 dark:bg-amber-500/20 text-amber-600 dark:text-amber-400 rounded text-[10px] font-bold">
            {{ store.duplicateReport.value.total_groups }}
          </span>
        </button>
      </div>

      <!-- Cloud -->
      <div>
        <h3 class="text-xs font-bold text-gray-400 dark:text-gray-500 uppercase tracking-wider px-2 mb-2">{{ $t('file.cloud') }}</h3>
        <button v-if="!store.isGDriveConnected.value" @click="store.connectGDrive"
          class="w-full flex items-center gap-3 px-3 py-2 rounded-xl text-sm font-medium transition-all hover:bg-blue-50 dark:hover:bg-blue-500/10 text-blue-600 dark:text-blue-400 cursor-pointer">
          <FolderSync v-if="store.isConnectingGDrive.value" class="w-4 h-4 animate-spin" />
          <Cloud v-else class="w-4 h-4" />
          {{ $t('file.gdrive_connect') }}
        </button>
        <div v-else class="group relative">
          <button @click="store.activeSourceId.value = 'gdrive'; store.activeType.value = null; store.activeTag.value = null"
            class="w-full flex items-center gap-3 px-3 py-2 rounded-xl text-sm transition-all cursor-pointer"
            :class="store.activeSourceId.value === 'gdrive' ? 'bg-blue-50 dark:bg-blue-500/10 text-blue-600 dark:text-blue-400' : 'hover:bg-gray-100 dark:hover:bg-white/5 text-gray-600 dark:text-gray-400'">
            <Cloud class="w-4 h-4 shrink-0" />
            <div class="flex flex-col items-start truncate pr-12">
              <span class="font-medium truncate">Google Drive</span>
              <span v-if="store.gdriveEmail.value" class="text-[10px] opacity-70 truncate">{{ store.gdriveEmail.value }}</span>
            </div>
          </button>
          <div class="absolute right-2 top-1/2 -translate-y-1/2 flex items-center gap-1 md:opacity-0 opacity-100 group-hover:opacity-100 transition-opacity">
            <button @click.stop="store.syncGDrive" class="p-1.5 hover:bg-blue-100 dark:hover:bg-blue-500/20 text-blue-500 rounded-md cursor-pointer" :title="$t('file.gdrive_refresh')">
              <FolderSync class="w-3.5 h-3.5" />
            </button>
            <button @click.stop="store.disconnectGDrive" class="p-1.5 hover:bg-red-100 dark:hover:bg-red-500/20 text-red-500 rounded-md cursor-pointer" :title="$t('file.gdrive_disconnect')">
              <Unlink class="w-3.5 h-3.5" />
            </button>
          </div>
        </div>
      </div>

      <!-- Collections -->
      <div>
        <div class="flex items-center justify-between px-2 mb-2">
          <h3 class="text-xs font-bold text-gray-400 dark:text-gray-500 uppercase tracking-wider">{{ $t('file.collections') }}</h3>
          <button @click="emit('saveCollection')" class="text-gray-400 hover:text-indigo-500 transition-colors cursor-pointer" :title="$t('file.save_collection')">
            <Bookmark class="w-4 h-4" />
          </button>
        </div>
        <p v-if="store.collections.value.length === 0" class="px-3 text-xs text-gray-400">{{ $t('file.save_collection') }}</p>
        <div v-else class="space-y-1">
          <div v-for="saved in store.collections.value" :key="saved.id" class="group relative">
            <button @click="store.applyCollection(saved)"
              class="w-full flex items-center gap-3 px-3 py-2 rounded-xl text-sm font-medium transition-all cursor-pointer text-left hover:bg-gray-100 dark:hover:bg-white/5 text-gray-600 dark:text-gray-400">
              <Bookmark class="w-4 h-4 flex-shrink-0" /> <span class="truncate pr-6">{{ saved.name }}</span>
            </button>
            <button @click.stop="store.deleteCollection(saved.id)"
              class="absolute right-2 top-1/2 -translate-y-1/2 md:opacity-0 opacity-100 group-hover:opacity-100 p-1.5 hover:bg-red-100 dark:hover:bg-red-500/20 text-red-500 rounded-md transition-all cursor-pointer"
              :title="$t('file.delete_collection')">
              <Trash2 class="w-3.5 h-3.5" />
            </button>
          </div>
        </div>
      </div>

      <!-- Cameras -->
      <div v-if="store.cameras.value.length > 0">
        <h3 class="text-xs font-bold text-gray-400 dark:text-gray-500 uppercase tracking-wider px-2 mb-2">{{ $t('file.cameras') }}</h3>
        <div class="space-y-1">
          <button v-for="camera in store.cameras.value" :key="camera"
            @click="store.activeCamera.value = store.activeCamera.value === camera ? null : camera"
            class="w-full flex items-center gap-3 px-3 py-2 rounded-xl text-sm font-medium transition-all cursor-pointer text-left"
            :class="store.activeCamera.value === camera ? 'bg-sky-50 dark:bg-sky-500/10 text-sky-600 dark:text-sky-400' : 'hover:bg-gray-100 dark:hover:bg-white/5 text-gray-600 dark:text-gray-400'">
            <Camera class="w-4 h-4 flex-shrink-0" /> <span class="truncate">{{ camera }}</span>
          </button>
        </div>
      </div>

      <!-- Tags -->
      <div>
        <h3 class="text-xs font-bold text-gray-400 dark:text-gray-500 uppercase tracking-wider px-2 mb-2">{{ $t('file.tags') }}</h3>
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
        <p v-else class="px-3 text-[11px] text-gray-400 dark:text-gray-500 italic">{{ $t('file.no_tags') }}</p>
      </div>
    </div>
  </div>
</template>
