<script setup lang="ts">
import { Inbox, Sun, Calendar, Coffee, Send, Plus, X } from 'lucide-vue-next';

const props = defineProps<{
  activeCategory: string;
  categoryCounts: { all: number; today: number; upcoming: number; someday: number; transferred: number };
  projects: any[];
  isMobileOpen?: boolean;
  variant: 'desktop' | 'mobile';
}>();

const emit = defineEmits<{
  (e: 'update:activeCategory', value: string): void;
  (e: 'create-project'): void;
  (e: 'close-mobile'): void;
}>();

const selectCategory = (cat: string) => {
  emit('update:activeCategory', cat);
  if (props.variant === 'mobile') emit('close-mobile');
};
</script>

<template>
  <!-- DESKTOP SIDEBAR -->
  <div v-if="variant === 'desktop'" class="w-64 border-r border-[#e6e6e6] dark:border-[#2c2c2c] bg-gray-50/50 dark:bg-[#1a1a1a]/50 flex flex-col pt-10 shrink-0 hidden md:flex">
      <div class="flex flex-col px-3 space-y-1">
          <button @click="selectCategory('all')" class="flex items-center justify-between px-3 py-2 rounded-lg transition-colors cursor-pointer" :class="activeCategory === 'all' ? 'bg-white dark:bg-[#2c2c2c] text-black dark:text-white shadow-sm font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#242424]'">
              <div class="flex items-center"><Inbox class="w-4 h-4 mr-3" />{{ $t('task.all_tasks') }}</div>
              <span class="text-xs bg-gray-200 dark:bg-[#333] px-1.5 py-0.5 rounded-full text-gray-600 dark:text-gray-400" v-if="categoryCounts.all">{{ categoryCounts.all }}</span>
          </button>
          <button @click="selectCategory('today')" class="flex items-center justify-between px-3 py-2 rounded-lg transition-colors cursor-pointer" :class="activeCategory === 'today' ? 'bg-white dark:bg-[#2c2c2c] text-blue-600 dark:text-blue-400 shadow-sm font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#242424]'">
              <div class="flex items-center"><Sun class="w-4 h-4 mr-3" />{{ $t('task.today') }}</div>
              <span class="text-xs bg-blue-100 dark:bg-blue-900/30 px-1.5 py-0.5 rounded-full text-blue-600 dark:text-blue-400" v-if="categoryCounts.today">{{ categoryCounts.today }}</span>
          </button>
          <button @click="selectCategory('upcoming')" class="flex items-center justify-between px-3 py-2 rounded-lg transition-colors cursor-pointer" :class="activeCategory === 'upcoming' ? 'bg-white dark:bg-[#2c2c2c] text-red-600 dark:text-red-400 shadow-sm font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#242424]'">
              <div class="flex items-center"><Calendar class="w-4 h-4 mr-3" />{{ $t('task.upcoming') }}</div>
              <span class="text-xs bg-red-100 dark:bg-red-900/30 px-1.5 py-0.5 rounded-full text-red-600 dark:text-red-400" v-if="categoryCounts.upcoming">{{ categoryCounts.upcoming }}</span>
          </button>
          <button @click="selectCategory('someday')" class="flex items-center justify-between px-3 py-2 rounded-lg transition-colors cursor-pointer" :class="activeCategory === 'someday' ? 'bg-white dark:bg-[#2c2c2c] text-yellow-600 dark:text-yellow-400 shadow-sm font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#242424]'">
              <div class="flex items-center"><Coffee class="w-4 h-4 mr-3" />{{ $t('task.someday') }}</div>
              <span class="text-xs bg-yellow-100 dark:bg-yellow-900/30 px-1.5 py-0.5 rounded-full text-yellow-600 dark:text-yellow-400" v-if="categoryCounts.someday">{{ categoryCounts.someday }}</span>
          </button>
          <button @click="selectCategory('transferred')" class="flex items-center justify-between px-3 py-2 rounded-lg transition-colors cursor-pointer" :class="activeCategory === 'transferred' ? 'bg-white dark:bg-[#2c2c2c] text-slate-600 dark:text-slate-400 shadow-sm font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#242424]'">
              <div class="flex items-center"><Send class="w-4 h-4 mr-3" />{{ $t('task.transferred') }}</div>
              <span class="text-xs bg-slate-200 dark:bg-slate-700 px-1.5 py-0.5 rounded-full text-slate-600 dark:text-slate-400" v-if="categoryCounts.transferred">{{ categoryCounts.transferred }}</span>
          </button>
          
          <div class="pt-4 pb-1 px-3 flex items-center justify-between group">
              <span class="text-[10px] font-bold text-gray-400 uppercase tracking-wider">{{ $t('task.projects') }}</span>
              <button @click="emit('create-project')" class="text-gray-400 hover:text-indigo-500 opacity-0 group-hover:opacity-100 transition-opacity" :title="$t('task.new_project')">
                  <Plus class="w-3.5 h-3.5"/>
              </button>
          </div>
          <button v-for="proj in projects" :key="proj.id" @click="selectCategory('project:' + proj.id)" class="flex items-center justify-between px-3 py-2 rounded-lg transition-colors cursor-pointer group" :class="activeCategory === 'project:' + proj.id ? 'bg-white dark:bg-[#2c2c2c] text-indigo-600 dark:text-indigo-400 shadow-sm font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#242424]'">
              <div class="flex items-center truncate">
                  <svg class="w-4 h-4 mr-3 shrink-0" :class="activeCategory === 'project:' + proj.id ? 'text-indigo-500' : 'text-gray-400 group-hover:text-indigo-400'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg>
                  <span class="truncate">{{ proj.title }}</span>
              </div>
          </button>
      </div>
  </div>

  <!-- MOBILE SIDEBAR OVERLAY -->
  <div v-if="variant === 'mobile' && isMobileOpen" class="fixed inset-0 z-[120] md:hidden flex">
      <!-- Backdrop -->
      <div class="absolute inset-0 bg-black/20 dark:bg-black/60 backdrop-blur-sm transition-opacity" @click="emit('close-mobile')"></div>
      
      <!-- Sidebar Panel -->
      <div class="relative w-[75%] max-w-sm h-full bg-[#fdfdfc] dark:bg-[#1e1e1e] shadow-2xl flex flex-col transform transition-transform duration-300" style="padding-top: max(env(safe-area-inset-top), 20px);">
          <!-- Header with Close Button -->
          <div class="flex items-center justify-between px-5 pb-4 border-b border-gray-100 dark:border-[#2c2c2c] shrink-0">
              <h2 class="text-xl font-semibold text-[#1c1c1e] dark:text-[#f4f4f5]">{{ $t('task.views') }}</h2>
              <button @click="emit('close-mobile')" class="p-2 -mr-2 rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-[#2a2a2a] transition-colors cursor-pointer" aria-label="More Options">
                  <X class="w-5 h-5" />
              </button>
          </div>
          
          <!-- Menu Items -->
          <div class="flex-1 overflow-y-auto px-3 py-6 flex flex-col space-y-1.5">
              <button @click="selectCategory('all')" class="flex items-center justify-between px-3 py-3 rounded-xl transition-colors cursor-pointer" :class="activeCategory === 'all' ? 'bg-black/5 dark:bg-white/10 text-black dark:text-white font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-[#242424]'">
                  <div class="flex items-center"><Inbox class="w-5 h-5 mr-3" />{{ $t('task.all_tasks') }}</div>
                  <span class="text-xs bg-gray-200 dark:bg-[#333] px-2 py-0.5 rounded-full text-gray-600 dark:text-gray-400" v-if="categoryCounts.all">{{ categoryCounts.all }}</span>
              </button>
              <button @click="selectCategory('today')" class="flex items-center justify-between px-3 py-3 rounded-xl transition-colors cursor-pointer" :class="activeCategory === 'today' ? 'bg-blue-50 dark:bg-blue-900/20 text-blue-600 dark:text-blue-400 font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-[#242424]'">
                  <div class="flex items-center"><Sun class="w-5 h-5 mr-3" />{{ $t('task.today') }}</div>
                  <span class="text-xs bg-blue-100 dark:bg-blue-900/30 px-2 py-0.5 rounded-full text-blue-600 dark:text-blue-400" v-if="categoryCounts.today">{{ categoryCounts.today }}</span>
              </button>
              <button @click="selectCategory('upcoming')" class="flex items-center justify-between px-3 py-3 rounded-xl transition-colors cursor-pointer" :class="activeCategory === 'upcoming' ? 'bg-red-50 dark:bg-red-900/20 text-red-600 dark:text-red-400 font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-[#242424]'">
                  <div class="flex items-center"><Calendar class="w-5 h-5 mr-3" />{{ $t('task.upcoming') }}</div>
                  <span class="text-xs bg-red-100 dark:bg-red-900/30 px-2 py-0.5 rounded-full text-red-600 dark:text-red-400" v-if="categoryCounts.upcoming">{{ categoryCounts.upcoming }}</span>
              </button>
              <button @click="selectCategory('someday')" class="flex items-center justify-between px-3 py-3 rounded-xl transition-colors cursor-pointer" :class="activeCategory === 'someday' ? 'bg-yellow-50 dark:bg-yellow-900/20 text-yellow-600 dark:text-yellow-400 font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-[#242424]'">
                  <div class="flex items-center"><Coffee class="w-5 h-5 mr-3" />{{ $t('task.someday') }}</div>
                  <span class="text-xs bg-yellow-100 dark:bg-yellow-900/30 px-2 py-0.5 rounded-full text-yellow-600 dark:text-yellow-400" v-if="categoryCounts.someday">{{ categoryCounts.someday }}</span>
              </button>
              <button @click="selectCategory('transferred')" class="flex items-center justify-between px-3 py-3 rounded-xl transition-colors cursor-pointer" :class="activeCategory === 'transferred' ? 'bg-slate-100 dark:bg-slate-800 text-slate-700 dark:text-slate-300 font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-[#242424]'">
                  <div class="flex items-center"><Send class="w-5 h-5 mr-3" />{{ $t('task.transferred') }}</div>
                  <span class="text-xs bg-slate-200 dark:bg-slate-700 px-2 py-0.5 rounded-full text-slate-600 dark:text-slate-400" v-if="categoryCounts.transferred">{{ categoryCounts.transferred }}</span>
              </button>
              
              <div class="pt-4 pb-1 px-3 flex items-center justify-between">
                  <span class="text-[10px] font-bold text-gray-400 uppercase tracking-wider">{{ $t('task.projects') }}</span>
                  <button @click="emit('create-project')" class="text-gray-400 hover:text-indigo-500" :title="$t('task.new_project')">
                      <Plus class="w-4 h-4"/>
                  </button>
              </div>
              <button v-for="proj in projects" :key="proj.id" @click="selectCategory('project:' + proj.id)" class="flex items-center justify-between px-3 py-3 rounded-xl transition-colors cursor-pointer" :class="activeCategory === 'project:' + proj.id ? 'bg-indigo-50 dark:bg-indigo-900/20 text-indigo-600 dark:text-indigo-400 font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-[#242424]'">
                  <div class="flex items-center truncate">
                      <svg class="w-5 h-5 mr-3 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg>
                      <span class="truncate">{{ proj.title }}</span>
                  </div>
              </button>
          </div>
      </div>
  </div>
</template>
