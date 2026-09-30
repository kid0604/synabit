<script setup lang="ts">
import { computed, ref } from 'vue';
import { storeToRefs } from 'pinia';
import { Plus, List, Trello, Table2, Grid2x2, Search, X, Lightbulb } from 'lucide-vue-next';
import { useAppStore } from '../../../stores/useAppStore';
import AppHeader from '../../../shared/components/AppHeader.vue';
import { taskViewInPlatformScope } from '../../../shared/platformScope';

defineProps<{
  /** Only the linear views can be sorted and grouped; see `TaskSortBar`. */
  showSortBar?: boolean;
  /**
   * What to call whatever is on screen.
   *
   * Worked out by the caller rather than by a chain of ternaries here. The
   * chain ended in `activeCategory` itself, so any bucket it had not been
   * taught about printed its own internal id: selecting a saved search put
   * `Filter:Filters/ac40aa52-…` across the top of the page.
   */
  title: string;
  viewMode: 'list' | 'board' | 'table' | 'matrix';
  searchQuery: string;
}>();

/**
 * Board and Matrix move cards with the HTML5 drag-and-drop API, which never
 * fires from a touch event — on a phone they draw correctly and simply cannot
 * be operated. Table is a wide grid. So on mobile only List is offered, and
 * with one view left the switcher itself is noise.
 */
const availableViewCount = computed(
  () => (['list', 'board', 'table', 'matrix'] as const).filter(taskViewInPlatformScope).length,
);

/**
 * The search words (`is:tracked`, `prop:cost=100`) are there for whoever asks
 * for them, and only then. They used to open on every focus of the box, which
 * taught syntax to somebody who only wanted to type a word and, on a phone,
 * covered the results being searched. Simple mode does not offer them at all.
 */
const { simpleMode } = storeToRefs(useAppStore());
const showTips = ref(false);

const emit = defineEmits<{
  (e: 'update:viewMode', mode: 'list' | 'board' | 'table' | 'matrix'): void;
  (e: 'update:searchQuery', query: string): void;
  (e: 'create-task'): void;
  (e: 'open-mobile-sidebar'): void;
}>();
</script>

<template>
  <div class="shrink-0">
      <AppHeader
          :title="title"
          :primaryLabel="$t('task.new_task')"
          :primaryIcon="Plus"
          :sidebarLabel="$t('task.a11y_open_sidebar')"
          @primary="emit('create-task')"
          @open-sidebar="emit('open-mobile-sidebar')"
      >
          <!--
            The title keeps no `capitalize`. The bucket names come from
            translations and are already cased properly, while a project or a
            saved search is named by the user — and CSS capitalising it turns
            their `iOS rollout` into `IOS Rollout`.
          -->
          <template #search>
              <div class="relative w-full min-w-0 max-w-xs flex items-center gap-1" @keydown.esc="showTips = false">
                  <div class="relative flex-1 min-w-0 group">
                      <div class="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none">
                          <Search class="h-4 w-4 text-gray-500 dark:text-gray-400 group-focus-within:text-accent transition-colors" aria-hidden="true" />
                      </div>
                      <input
                          id="task-search-input"
                          :value="searchQuery"
                          @input="emit('update:searchQuery', ($event.target as HTMLInputElement).value)"
                          type="text"
                          class="block w-full h-9 pl-9 pr-8 border border-gray-200 dark:border-border-dark rounded-full bg-white dark:bg-surface-dark text-text dark:text-text-dark placeholder-gray-500 dark:placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-accent/30 text-sm transition-all"
                          :placeholder="$t('task.search_placeholder')"
                          :aria-label="$t('common.search')"
                      />
                      <button v-if="searchQuery" @click="emit('update:searchQuery', '')" class="absolute inset-y-0 right-0 pr-3 flex items-center cursor-pointer z-10" :aria-label="$t('task.a11y_clear_search')" :title="$t('task.a11y_clear_search')">
                          <X class="h-4 w-4 text-gray-500 dark:text-gray-400 hover:text-gray-600 transition-colors" aria-hidden="true" />
                      </button>
                  </div>

                  <!-- Search words, on request only (see `showTips`). -->
                  <button
                      v-if="!simpleMode"
                      type="button"
                      class="btn-icon flex-shrink-0"
                      :class="showTips ? 'text-accent dark:text-accent-dark' : ''"
                      :aria-expanded="showTips"
                      aria-controls="task-search-tips"
                      :aria-label="$t('task.search_tips')"
                      :title="$t('task.search_tips')"
                      @click="showTips = !showTips"
                  >
                      <Lightbulb class="w-4 h-4" aria-hidden="true" />
                  </button>
                  <div v-if="showTips && !simpleMode" id="task-search-tips" class="absolute top-full right-0 mt-2 p-3 bg-white dark:bg-surface-dark border border-gray-100 dark:border-border-dark rounded-xl shadow-[0_10px_30px_rgba(0,0,0,0.1)] dark:shadow-[0_10px_30px_rgba(0,0,0,0.5)] z-20 w-72 max-w-[calc(100vw-2rem)]">
                      <div class="flex items-center justify-between mb-2">
                          <p class="text-xs font-semibold text-gray-600 dark:text-gray-300">{{ $t('task.quick_syntax') }}</p>
                          <button type="button" class="btn-icon" :aria-label="$t('common.close')" :title="$t('common.close')" @click="showTips = false">
                              <X class="w-4 h-4" aria-hidden="true" />
                          </button>
                      </div>
                      <p class="mb-2.5 text-xs text-gray-500 dark:text-gray-400">{{ $t('task.search_tips_intro') }}</p>
                      <div class="space-y-2 text-xs text-gray-600 dark:text-gray-400">
                          <div class="flex items-center flex-wrap gap-x-2 gap-y-1"><span class="font-mono bg-blue-50/80 dark:bg-blue-900/30 px-1 border border-blue-100 dark:border-blue-900/50 rounded text-blue-600 dark:text-blue-400 font-medium whitespace-nowrap">is:tracked</span> <span class="text-gray-500 dark:text-gray-400">{{ $t('task.syntax_tracked') }}</span></div>
                          <div class="flex items-center flex-wrap gap-x-2 gap-y-1"><span class="font-mono bg-blue-50/80 dark:bg-blue-900/30 px-1 border border-blue-100 dark:border-blue-900/50 rounded text-blue-600 dark:text-blue-400 font-medium whitespace-nowrap">is:transferred</span> <span class="text-gray-500 dark:text-gray-400">{{ $t('task.syntax_transferred') }}</span></div>
                          <div class="flex items-center flex-wrap gap-x-2 gap-y-1"><span class="font-mono bg-purple-50/80 dark:bg-purple-900/30 px-1 border border-purple-100 dark:border-purple-900/50 rounded text-purple-600 dark:text-purple-400 font-medium whitespace-nowrap">p:3</span> {{ $t('task.syntax_or') }} <span class="font-mono bg-indigo-50/80 dark:bg-indigo-900/30 px-1 border border-indigo-100 dark:border-indigo-900/50 rounded text-indigo-600 dark:text-indigo-400 font-medium whitespace-nowrap">status:todo</span></div>
                          <div class="flex items-center flex-wrap gap-x-2 gap-y-1"><span class="font-mono bg-emerald-50/80 dark:bg-emerald-900/30 px-1 border border-emerald-100 dark:border-emerald-900/50 rounded text-emerald-600 dark:text-emerald-400 font-medium whitespace-nowrap">@name</span> <span class="text-gray-500 dark:text-gray-400">{{ $t('task.syntax_assign') }}</span></div>
                          <div class="flex items-center flex-wrap gap-x-2 gap-y-1"><span class="font-mono bg-amber-50/80 dark:bg-amber-900/30 px-1 border border-amber-100 dark:border-amber-900/50 rounded text-amber-600 dark:text-amber-400 font-medium whitespace-nowrap">#tag</span> {{ $t('task.syntax_or') }} <span class="font-mono bg-amber-50/80 dark:bg-amber-900/30 px-1 border border-amber-100 dark:border-amber-900/50 rounded text-amber-600 dark:text-amber-400 font-medium whitespace-nowrap">tag:urgent</span></div>
                          <div class="flex items-center flex-wrap gap-x-2 gap-y-1"><span class="font-mono bg-slate-100 dark:bg-slate-800/50 px-1 border border-slate-200 dark:border-[#333] rounded text-slate-600 dark:text-slate-300 font-medium whitespace-nowrap">prop:cost=100</span> <span class="text-gray-500 dark:text-gray-400">{{ $t('task.syntax_custom_prop') }}</span></div>
                      </div>
                  </div>
              </div>
          </template>

          <template #actions>
              <div v-if="availableViewCount > 1" class="flex bg-gray-100 dark:bg-[#1a1a1a] p-1 rounded-xl" role="group" :aria-label="$t('task.views')">
                  <button @click="emit('update:viewMode', 'list')" class="p-1.5 rounded-lg transition-colors cursor-pointer" :class="viewMode === 'list' ? 'bg-white dark:bg-[#2c2c2c] shadow-sm text-accent dark:text-accent-dark' : 'text-gray-500 hover:text-black dark:hover:text-white'" :title="$t('task.list_view')" :aria-label="$t('task.list_view')" :aria-pressed="viewMode === 'list'">
                      <List class="w-4 h-4"/>
                  </button>
                  <button v-if="taskViewInPlatformScope('board')" @click="emit('update:viewMode', 'board')" class="p-1.5 rounded-lg transition-colors cursor-pointer" :class="viewMode === 'board' ? 'bg-white dark:bg-[#2c2c2c] shadow-sm text-accent dark:text-accent-dark' : 'text-gray-500 hover:text-black dark:hover:text-white'" :title="$t('task.board_view')" :aria-label="$t('task.board_view')" :aria-pressed="viewMode === 'board'">
                      <Trello class="w-4 h-4"/>
                  </button>
                  <button v-if="taskViewInPlatformScope('table')" @click="emit('update:viewMode', 'table')" class="p-1.5 rounded-lg transition-colors cursor-pointer" :class="viewMode === 'table' ? 'bg-white dark:bg-[#2c2c2c] shadow-sm text-accent dark:text-accent-dark' : 'text-gray-500 hover:text-black dark:hover:text-white'" :title="$t('task.table_view')" :aria-label="$t('task.table_view')" :aria-pressed="viewMode === 'table'">
                      <Table2 class="w-4 h-4"/>
                  </button>
                  <button v-if="taskViewInPlatformScope('matrix')" @click="emit('update:viewMode', 'matrix')" class="p-1.5 rounded-lg transition-colors cursor-pointer" :class="viewMode === 'matrix' ? 'bg-white dark:bg-[#2c2c2c] shadow-sm text-accent dark:text-accent-dark' : 'text-gray-500 hover:text-black dark:hover:text-white'" :title="$t('task.matrix_view')" :aria-label="$t('task.matrix_view')" :aria-pressed="viewMode === 'matrix'">
                      <Grid2x2 class="w-4 h-4"/>
                  </button>
              </div>
          </template>
      </AppHeader>

      <!-- Saving the search and sorting the list, under the bar they belong to -->
      <div v-if="$slots['save-search'] || $slots.sort" class="px-4 md:px-6 pt-3 flex flex-row items-center gap-3 flex-wrap empty:hidden">
          <slot name="save-search" />
          <slot name="sort" />
      </div>
  </div>
</template>
