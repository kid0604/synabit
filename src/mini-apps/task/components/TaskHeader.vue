<script setup lang="ts">
import { Plus, List, Trello, Table2, Grid2x2, Search, X, Menu as MenuIcon } from 'lucide-vue-next';
import NavButtons from '../../../shared/components/NavButtons.vue';

defineProps<{
  activeProject: any;
  activeCategory: string;
  viewMode: 'list' | 'board' | 'table' | 'matrix';
  searchQuery: string;
}>();

const emit = defineEmits<{
  (e: 'update:viewMode', mode: 'list' | 'board' | 'table' | 'matrix'): void;
  (e: 'update:searchQuery', query: string): void;
  (e: 'create-task'): void;
  (e: 'open-mobile-sidebar'): void;
}>();
</script>

<template>
  <div class="px-4 md:px-8 pt-12 md:pt-10 pb-2 md:pb-4 shrink-0 border-b border-transparent">
      <div class="flex items-center justify-between mb-4 md:mb-6">
          <div class="flex items-center gap-3">
              <NavButtons />
              <button @click="emit('open-mobile-sidebar')" class="md:hidden p-1 -ml-1 text-gray-500 hover:text-gray-800 dark:hover:text-gray-200 cursor-pointer">
                  <MenuIcon class="w-6 h-6" />
              </button>
              <h1 class="text-2xl md:text-3xl font-semibold text-[#1c1c1e] dark:text-[#f4f4f5] tracking-tight capitalize truncate max-w-[200px] sm:max-w-md lg:max-w-xl">
                  {{ activeProject ? activeProject.title : (activeCategory === 'all' ? $t('task.all_tasks') : activeCategory === 'today' ? $t('task.today') : activeCategory === 'upcoming' ? $t('task.upcoming') : activeCategory === 'someday' ? $t('task.someday') : activeCategory === 'transferred' ? $t('task.transferred') : activeCategory) }}
              </h1>
          </div>
          <div class="flex items-center gap-3">
              <!-- New Task Button -->
              <button 
                  @click="emit('create-task')"
                  class="hidden md:flex items-center px-3 py-1.5 bg-blue-500 hover:bg-blue-600 text-white rounded-lg shadow-[0_2px_10px_rgba(59,130,246,0.3)] hover:shadow-[0_4px_14px_rgba(59,130,246,0.4)] transition-all cursor-pointer text-sm font-medium"
              >
                  <Plus class="w-4 h-4 mr-1.5"/>
                  {{ $t('task.new_btn') }}
              </button>

              <div class="flex bg-gray-100 dark:bg-[#1a1a1a] p-1 rounded-xl">
                  <button @click="emit('update:viewMode', 'list')" class="p-1.5 rounded-lg transition-colors cursor-pointer" :class="viewMode === 'list' ? 'bg-white dark:bg-[#2c2c2c] shadow-sm text-black dark:text-white' : 'text-gray-500 hover:text-black dark:hover:text-white'" :title="$t('task.list_view')">
                      <List class="w-4 h-4"/>
                  </button>
                  <button @click="emit('update:viewMode', 'board')" class="p-1.5 rounded-lg transition-colors cursor-pointer" :class="viewMode === 'board' ? 'bg-white dark:bg-[#2c2c2c] shadow-sm text-black dark:text-white' : 'text-gray-500 hover:text-black dark:hover:text-white'" :title="$t('task.board_view')">
                      <Trello class="w-4 h-4"/>
                  </button>
                  <button @click="emit('update:viewMode', 'table')" class="p-1.5 rounded-lg transition-colors cursor-pointer" :class="viewMode === 'table' ? 'bg-white dark:bg-[#2c2c2c] shadow-sm text-black dark:text-white' : 'text-gray-500 hover:text-black dark:hover:text-white'" :title="$t('task.table_view')">
                      <Table2 class="w-4 h-4"/>
                  </button>
                  <button @click="emit('update:viewMode', 'matrix')" class="p-1.5 rounded-lg transition-colors cursor-pointer" :class="viewMode === 'matrix' ? 'bg-white dark:bg-[#2c2c2c] shadow-sm text-black dark:text-white' : 'text-gray-500 hover:text-black dark:hover:text-white'" :title="$t('task.matrix_view')">
                      <Grid2x2 class="w-4 h-4"/>
                  </button>
              </div>
          </div>
      </div>

      <!-- Bar (Search & Properties) -->
      <div class="mt-4 flex flex-row items-center gap-3">
          <div class="relative w-full sm:max-w-xs group">
              <div class="absolute inset-y-0 left-0 pl-3.5 flex items-center pointer-events-none">
                  <Search class="h-4 w-4 text-gray-400 group-focus-within:text-blue-500 transition-colors" />
              </div>
              <input 
                  :value="searchQuery"
                  @input="emit('update:searchQuery', ($event.target as HTMLInputElement).value)"
                  type="text" 
                  class="block w-full pl-10 pr-3 py-2 border border-gray-200 dark:border-[#2c2c2c] rounded-full leading-5 bg-white dark:bg-[#1e1e1e] text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-black/5 dark:focus:ring-white/10 sm:text-sm transition-all shadow-[0_2px_8px_rgba(0,0,0,0.02)]" 
                  :placeholder="$t('task.search_placeholder')" 
              />
              <button v-if="searchQuery" @click="emit('update:searchQuery', '')" class="absolute inset-y-0 right-0 pr-3 flex items-center cursor-pointer z-10">
                  <X class="h-4 w-4 text-gray-400 hover:text-gray-600 transition-colors" />
              </button>
              
              <!-- Advanced Search Tooltip/Hints -->
              <div class="absolute top-full left-0 mt-2 p-3 bg-white dark:bg-[#1e1e1e] border border-gray-100 dark:border-[#2c2c2c] rounded-xl shadow-[0_10px_30px_rgba(0,0,0,0.1)] dark:shadow-[0_10px_30px_rgba(0,0,0,0.5)] z-20 w-72 opacity-0 invisible group-focus-within:opacity-100 group-focus-within:visible transition-all">
                  <div class="flex items-center text-[10px] font-semibold text-gray-400 dark:text-gray-500 mb-2.5 uppercase tracking-wider">
                      <Search class="w-3.5 h-3.5 mr-1" /> {{ $t('task.quick_syntax') }}
                  </div>
                  <div class="space-y-2 text-[11px] text-gray-600 dark:text-gray-400">
                      <div class="flex items-center gap-2"><span class="font-mono bg-blue-50/80 dark:bg-blue-900/30 px-1 border border-blue-100 dark:border-blue-900/50 rounded text-blue-600 dark:text-blue-400 font-medium whitespace-nowrap">is:transferred</span>, <span class="font-mono bg-blue-50/80 dark:bg-blue-900/30 px-1 border border-blue-100 dark:border-blue-900/50 rounded text-blue-600 dark:text-blue-400 font-medium whitespace-nowrap">is:tracked</span></div>
                      <div class="flex items-center gap-2"><span class="font-mono bg-purple-50/80 dark:bg-purple-900/30 px-1 border border-purple-100 dark:border-purple-900/50 rounded text-purple-600 dark:text-purple-400 font-medium whitespace-nowrap">p:3</span> {{ $t('task.syntax_or') }} <span class="font-mono bg-indigo-50/80 dark:bg-indigo-900/30 px-1 border border-indigo-100 dark:border-indigo-900/50 rounded text-indigo-600 dark:text-indigo-400 font-medium whitespace-nowrap">status:todo</span></div>
                      <div class="flex items-center gap-2"><span class="font-mono bg-emerald-50/80 dark:bg-emerald-900/30 px-1 border border-emerald-100 dark:border-emerald-900/50 rounded text-emerald-600 dark:text-emerald-400 font-medium whitespace-nowrap">@name</span> <span class="text-gray-400">{{ $t('task.syntax_assign') }}</span></div>
                      <div class="flex items-center gap-2"><span class="font-mono bg-amber-50/80 dark:bg-amber-900/30 px-1 border border-amber-100 dark:border-amber-900/50 rounded text-amber-600 dark:text-amber-400 font-medium whitespace-nowrap">#tag</span> {{ $t('task.syntax_or') }} <span class="font-mono bg-amber-50/80 dark:bg-amber-900/30 px-1 border border-amber-100 dark:border-amber-900/50 rounded text-amber-600 dark:text-amber-400 font-medium whitespace-nowrap">tag:urgent</span></div>
                      <div class="flex items-center gap-2"><span class="font-mono bg-slate-100 dark:bg-slate-800/50 px-1 border border-slate-200 dark:border-[#333] rounded text-slate-600 dark:text-slate-300 font-medium whitespace-nowrap">prop:cost=100</span> <span class="text-gray-400 px-1">(Custom Prop)</span></div>
                  </div>
              </div>
          </div>
      </div>
  </div>
</template>
