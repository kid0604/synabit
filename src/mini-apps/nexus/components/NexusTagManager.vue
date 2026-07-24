<script setup lang="ts">
import { ref, onMounted, computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { Search, Edit3, Trash2, Tag as TagIcon, ArrowLeft, LayoutGrid, Cloud } from 'lucide-vue-next';

import { confirm } from '@tauri-apps/plugin-dialog';

const emit = defineEmits<{
  (e: 'back'): void;
  (e: 'search-tag', tag: string): void;
}>();

const props = defineProps<{
  vaultPath: string;
}>();

interface TagItem {
  name: string;
  count: number;
}

const tags = ref<TagItem[]>([]);
const searchQuery = ref('');
const loading = ref(true);
const viewMode = ref<'grid' | 'cloud'>('grid');

const loadTags = async () => {
    loading.value = true;
    try {
        const result: [string, number][] = await invoke('get_all_tags', {});
        tags.value = result.map(([name, count]) => ({ name, count }));
    } catch (e) {
        console.error(e);
    } finally {
        loading.value = false;
    }
};

const filteredTags = computed(() => {
    if (!searchQuery.value) return tags.value;
    const q = searchQuery.value.toLowerCase();
    return tags.value.filter(t => t.name.toLowerCase().includes(q));
});

const maxTagCount = computed(() => Math.max(...tags.value.map(t => t.count), 1));

const gradients = [
    'bg-gradient-to-br from-blue-500/10 to-indigo-500/20 dark:from-blue-500/20 dark:to-indigo-500/30 border-blue-200/50 dark:border-blue-500/30',
    'bg-gradient-to-br from-emerald-500/10 to-teal-500/20 dark:from-emerald-500/20 dark:to-teal-500/30 border-emerald-200/50 dark:border-emerald-500/30',
    'bg-gradient-to-br from-orange-500/10 to-rose-500/20 dark:from-orange-500/20 dark:to-rose-500/30 border-orange-200/50 dark:border-orange-500/30',
    'bg-gradient-to-br from-purple-500/10 to-pink-500/20 dark:from-purple-500/20 dark:to-pink-500/30 border-purple-200/50 dark:border-purple-500/30',
    'bg-gradient-to-br from-cyan-500/10 to-blue-500/20 dark:from-cyan-500/20 dark:to-blue-500/30 border-cyan-200/50 dark:border-cyan-500/30',
    'bg-gradient-to-br from-amber-500/10 to-orange-500/20 dark:from-amber-500/20 dark:to-orange-500/30 border-amber-200/50 dark:border-amber-500/30',
];

const getGradientClass = (name: string) => {
    let hash = 0;
    for (let i = 0; i < name.length; i++) {
        hash = name.charCodeAt(i) + ((hash << 5) - hash);
    }
    return gradients[Math.abs(hash) % gradients.length];
};

const textColors = [
    'text-blue-600 dark:text-blue-400', 
    'text-emerald-600 dark:text-emerald-400', 
    'text-orange-600 dark:text-orange-400', 
    'text-purple-600 dark:text-purple-400', 
    'text-cyan-600 dark:text-cyan-400', 
    'text-amber-600 dark:text-amber-400',
    'text-rose-600 dark:text-rose-400',
    'text-indigo-600 dark:text-indigo-400'
];

const getTextColorClass = (name: string) => {
    let hash = 0;
    for (let i = 0; i < name.length; i++) {
        hash = name.charCodeAt(i) + ((hash << 5) - hash);
    }
    return textColors[Math.abs(hash) % textColors.length];
};

const getCloudStyle = (count: number) => {
    const minSize = 14;
    const maxSize = 72;
    const minCount = Math.min(...tags.value.map(t => t.count), 1);
    const maxCount = maxTagCount.value;
    
    let fontSize = minSize;
    if (maxCount > minCount) {
        fontSize = minSize + ((count - minCount) / (maxCount - minCount)) * (maxSize - minSize);
    }
    
    return {
        fontSize: `${fontSize}px`,
        opacity: Math.max(0.4, 0.4 + ((count - minCount) / (maxCount - minCount)) * 0.6)
    };
};

const getBoxStyle = (count: number) => {
    const ratio = count / maxTagCount.value;
    if (ratio > 0.4 && count > 5) {
        return {
            classes: 'col-span-1 sm:col-span-2 row-span-1 sm:row-span-2 p-4 sm:p-6',
            titleSize: 'text-lg sm:text-2xl',
            countSize: 'text-5xl sm:text-7xl font-black opacity-[0.08] absolute bottom-2 right-4 tracking-tighter',
            showBadge: false,
        };
    } else if (ratio > 0.15 && count > 2) {
        return {
            classes: 'col-span-1 sm:col-span-2 row-span-1 p-3 sm:p-5',
            titleSize: 'text-[15px] sm:text-lg',
            countSize: 'text-4xl sm:text-5xl font-black opacity-[0.08] absolute bottom-1 right-4 tracking-tighter',
            showBadge: false,
        };
    } else {
        return {
            classes: 'col-span-1 sm:col-span-1 row-span-1 p-3 sm:p-4',
            titleSize: 'text-[15px]',
            countSize: '',
            showBadge: true,
        };
    }
};

const editingTag = ref<string | null>(null);
const editInputValue = ref('');

const startEdit = (tag: string) => {
    editingTag.value = tag;
    editInputValue.value = tag;
};

const saveEdit = async (oldName: string) => {
    const newName = editInputValue.value.trim();
    if (!newName || newName === oldName) {
        editingTag.value = null;
        return;
    }
    
    try {
        loading.value = true;
        await invoke('rename_tag', {
            vaultPath: props.vaultPath,
            oldTag: oldName,
            newTag: newName
        });
        
        await loadTags();
    } catch (e) {
        console.error(e);
    } finally {
        editingTag.value = null;
    }
};

const deleteTag = async (tag: string) => {
    const confirmed = await confirm(
        `This will remove the tag from all files. This action cannot be undone.`, 
        { 
            title: `Delete tag "#${tag}"?`, 
            kind: 'warning',
            okLabel: 'Delete',
            cancelLabel: 'Cancel'
        }
    );
    if (!confirmed) return;
    
    try {
        loading.value = true;
        await invoke('delete_tag', {
            vaultPath: props.vaultPath,
            tag: tag
        });
        
        await loadTags();
    } catch (e) {
        console.error(e);
    }
};

onMounted(() => {
    loadTags();
});
</script>

<template>
  <div class="h-full w-full bg-[#fdfdfc] dark:bg-[#1a1a1c] flex flex-col animate-in fade-in zoom-in-95 duration-200">
      
      <!-- Header -->
      <div class="h-auto min-h-[64px] py-3 flex flex-wrap items-center justify-between px-4 sm:px-8 gap-3 border-b border-gray-200 dark:border-[#2c2c2e] bg-white/80 dark:bg-[#242426]/80 backdrop-blur-md flex-shrink-0">
          <div class="flex items-center gap-4">
              <button @click="emit('back')" class="p-2 -ml-2 text-gray-500 hover:text-black dark:hover:text-white hover:bg-gray-100 dark:hover:bg-[#2c2c2e] rounded-xl transition-all flex items-center gap-1">
                  <ArrowLeft class="w-5 h-5" />
                  <span class="text-sm font-semibold tracking-wide">Nexus</span>
              </button>
              
              <div class="h-4 w-px bg-gray-300 dark:bg-[#444]"></div>
              
              <div class="flex items-center gap-2 text-indigo-600 dark:text-indigo-400">
                  <TagIcon class="w-5 h-5" />
                  <h1 class="text-sm font-bold tracking-widest uppercase">Tag Manager</h1>
              </div>
          </div>
          
          <div class="flex items-center gap-2">
              <div class="hidden sm:flex items-center bg-gray-100 dark:bg-[#1c1c1e] rounded-xl p-1 shadow-inner mr-2">
                  <button @click="viewMode = 'grid'" :class="['p-1.5 rounded-lg transition-all', viewMode === 'grid' ? 'bg-white dark:bg-[#2c2c2e] text-indigo-600 shadow-sm' : 'text-gray-400 hover:text-gray-600 dark:hover:text-gray-300']" title="Grid View">
                      <LayoutGrid class="w-4 h-4" />
                  </button>
                  <button @click="viewMode = 'cloud'" :class="['p-1.5 rounded-lg transition-all', viewMode === 'cloud' ? 'bg-white dark:bg-[#2c2c2e] text-indigo-600 shadow-sm' : 'text-gray-400 hover:text-gray-600 dark:hover:text-gray-300']" title="Word Cloud View">
                      <Cloud class="w-4 h-4" />
                  </button>
              </div>
              
              <div class="relative w-full sm:w-64 flex-shrink-0">
                  <Search class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400" />
                  <input 
                      v-model="searchQuery"
                      type="text"
                      placeholder="Find tags..."
                      class="w-full pl-9 pr-4 py-2 bg-gray-100 dark:bg-[#1c1c1e] border border-transparent dark:border-[#3a3a3c] focus:border-indigo-500 dark:focus:border-indigo-500 rounded-xl text-sm focus:outline-none transition-all shadow-inner"
                  />
              </div>
          </div>
      </div>
      
      <!-- Content -->
      <div class="flex-1 overflow-y-auto p-4 sm:p-6 lg:p-10 overflow-x-hidden">
          <div class="w-full max-w-[1600px] mx-auto">
              
              <div v-if="loading" class="flex justify-center py-20">
                  <div class="w-8 h-8 border-2 border-indigo-500 border-t-transparent rounded-full animate-spin"></div>
              </div>
              
              <div v-else-if="filteredTags.length === 0" class="text-center py-20 text-gray-500">
                  <TagIcon class="w-12 h-12 mx-auto mb-4 opacity-20" />
                  <p class="font-medium">No tags found.</p>
              </div>
              
              <div v-else-if="viewMode === 'grid'" class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-3 sm:gap-4" style="grid-auto-flow: dense; grid-auto-rows: minmax(80px, auto);">
                  <div v-for="tag in filteredTags" :key="tag.name" 
                       class="group bg-white dark:bg-[#242426] border backdrop-blur-sm rounded-3xl transition-all flex flex-col hover:-translate-y-0.5 hover:shadow-xl hover:z-10 relative overflow-hidden"
                       :class="[getBoxStyle(tag.count).classes, getGradientClass(tag.name)]">
                      
                      <!-- Background Large Count -->
                      <div v-if="!getBoxStyle(tag.count).showBadge" 
                           class="hidden sm:block"
                           :class="getBoxStyle(tag.count).countSize" 
                           style="line-height: 0.8; user-select: none;">
                          {{ tag.count }}
                      </div>

                      <!-- Edit Mode -->
                      <div v-if="editingTag === tag.name" class="flex flex-col gap-2 h-full justify-center z-20">
                          <input 
                              v-model="editInputValue" 
                              @keyup.enter="saveEdit(tag.name)"
                              @keyup.esc="editingTag = null"
                              type="text" 
                              autoFocus
                              class="w-full px-3 py-1.5 bg-white/50 dark:bg-black/20 border-2 border-indigo-500 rounded-xl text-sm font-semibold focus:outline-none backdrop-blur-md"
                          />
                          <div class="flex gap-2">
                             <button @click="saveEdit(tag.name)" class="flex-1 py-1.5 bg-indigo-500 text-white rounded-lg text-xs font-bold hover:bg-indigo-600 transition-colors shadow-md">Save</button>
                             <button @click="editingTag = null" class="py-1.5 px-3 bg-white/50 dark:bg-white/10 text-gray-700 dark:text-gray-300 rounded-lg text-xs font-bold hover:bg-white/80 dark:hover:bg-white/20 transition-colors">Cancel</button>
                          </div>
                      </div>
                      
                      <!-- Normal Mode -->
                      <template v-else>
                          <div class="flex items-start justify-between mb-2 z-20 relative">
                              <div class="flex items-center gap-1.5 min-w-0 flex-1 cursor-pointer" @click="emit('search-tag', tag.name)" title="Search this tag">
                                  <span class="text-indigo-600/50 dark:text-indigo-400/50 font-mono text-lg font-light">#</span>
                                  <h3 class="font-bold text-gray-800 dark:text-gray-100 truncate hover:text-indigo-600 transition-colors" :class="getBoxStyle(tag.count).titleSize">{{ tag.name }}</h3>
                              </div>
                              <span :class="['ml-2 px-2 py-0.5 bg-white/60 dark:bg-black/20 text-gray-600 dark:text-gray-300 text-[11px] font-bold rounded-md flex-shrink-0 border border-white/40 dark:border-white/10 shadow-sm', !getBoxStyle(tag.count).showBadge ? 'sm:hidden' : '']">{{ tag.count }}</span>
                          </div>
                          
                          <div class="flex items-center gap-2 opacity-0 group-hover:opacity-100 transition-opacity mt-auto z-20 relative pt-2">
                              <button @click="startEdit(tag.name)" class="flex-1 py-1.5 text-gray-600 dark:text-gray-300 hover:text-indigo-700 bg-white/40 hover:bg-white/80 dark:bg-black/20 dark:hover:bg-indigo-500/30 rounded-xl transition-all flex items-center justify-center gap-1.5 border border-white/50 dark:border-white/10 hover:shadow-sm">
                                  <Edit3 class="w-3.5 h-3.5" /> <span class="text-[10px] font-bold uppercase tracking-wider">Rename</span>
                              </button>
                              <button @click="deleteTag(tag.name)" class="p-1.5 text-gray-500 dark:text-gray-400 hover:text-red-600 bg-white/40 hover:bg-red-100 dark:bg-black/20 dark:hover:bg-red-500/30 rounded-xl transition-all border border-white/50 dark:border-white/10 hover:shadow-sm" aria-label="Delete Tag">
                                  <Trash2 class="w-3.5 h-3.5" />
                              </button>
                          </div>
                      </template>
                      
                  </div>
              </div>
              
              <div v-else-if="viewMode === 'cloud'" class="flex flex-wrap justify-center items-center gap-x-6 gap-y-4 py-12 px-4 max-w-4xl mx-auto">
                  <div v-for="tag in filteredTags" :key="tag.name"
                       class="cursor-pointer transition-all hover:scale-110 group relative flex items-center justify-center"
                       :style="getCloudStyle(tag.count)"
                       @click="emit('search-tag', tag.name)"
                  >
                      <span class="font-bold tracking-tight hover:!opacity-100" :class="getTextColorClass(tag.name)" style="line-height: 1.1;">
                          {{ tag.name }}
                      </span>
                      <span class="text-[10px] font-bold absolute -top-2 -right-3 bg-gray-100 dark:bg-[#2c2c2e] text-gray-500 rounded-full w-5 h-5 flex items-center justify-center opacity-0 group-hover:opacity-100 transition-opacity border border-gray-200 dark:border-[#3c3c3e] shadow-sm pointer-events-none" style="font-size: 10px;">
                          {{ tag.count }}
                      </span>
                  </div>
              </div>
              
          </div>
      </div>
  </div>
</template>
