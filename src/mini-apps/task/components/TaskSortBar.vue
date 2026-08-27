<script setup lang="ts">
/**
 * Choosing how the list is arranged.
 *
 * Two plain selects rather than a menu: there are eleven options between them
 * and no state to hide, so anything cleverer would only be more clicks.
 *
 * The styling is not decoration. The first version put each icon in a `<label>`
 * beside its select, left the native chevron on, and sized the controls
 * differently from the search field they sit next to — so the row read as four
 * loose glyphs and two form widgets rather than as one toolbar. Each control is
 * now a single pill built from the same classes as the search input, with the
 * icon inside its border and one chevron drawn by us.
 */
import { ArrowUpDown, Rows3, Keyboard, ChevronDown } from 'lucide-vue-next';
import { SORT_MODES, GROUP_MODES, type SortMode, type GroupMode } from '../sorting';

defineProps<{ sort: SortMode; group: GroupMode }>();

const emit = defineEmits<{
  (e: 'update:sort', mode: SortMode): void;
  (e: 'update:group', mode: GroupMode): void;
  (e: 'show-shortcuts'): void;
}>();

/**
 * The search input's own classes, so the two line up by construction rather
 * than by two numbers that agree today.
 */
const PILL = 'appearance-none w-full pl-9 pr-8 py-2 border border-gray-200 dark:border-[#2c2c2c] '
  + 'rounded-full leading-5 bg-white dark:bg-[#1e1e1e] text-[#1c1c1e] dark:text-[#f4f4f5] '
  + 'focus:outline-none focus:ring-2 focus:ring-black/5 dark:focus:ring-white/10 sm:text-sm '
  + 'transition-all shadow-[0_2px_8px_rgba(0,0,0,0.02)] cursor-pointer';
</script>

<template>
  <div class="flex items-center gap-2">
    <!-- Sort -->
    <div class="relative group">
      <ArrowUpDown class="absolute inset-y-0 left-3 my-auto h-3.5 w-3.5 text-gray-400 group-focus-within:text-blue-500 transition-colors pointer-events-none" />
      <select
        :value="sort"
        @change="emit('update:sort', ($event.target as HTMLSelectElement).value as SortMode)"
        :class="PILL"
        :aria-label="$t('task.sort_by')"
      >
        <option v-for="mode in SORT_MODES" :key="mode" :value="mode">{{ $t('task.sort_' + mode) }}</option>
      </select>
      <ChevronDown class="absolute inset-y-0 right-3 my-auto h-3.5 w-3.5 text-gray-400 pointer-events-none" />
    </div>

    <!-- Group -->
    <div class="relative group">
      <Rows3 class="absolute inset-y-0 left-3 my-auto h-3.5 w-3.5 text-gray-400 group-focus-within:text-blue-500 transition-colors pointer-events-none" />
      <select
        :value="group"
        @change="emit('update:group', ($event.target as HTMLSelectElement).value as GroupMode)"
        :class="PILL"
        :aria-label="$t('task.group_by')"
      >
        <option v-for="mode in GROUP_MODES" :key="mode" :value="mode">{{ $t('task.group_' + mode) }}</option>
      </select>
      <ChevronDown class="absolute inset-y-0 right-3 my-auto h-3.5 w-3.5 text-gray-400 pointer-events-none" />
    </div>

    <!--
      A round button of the same height, so the row ends on the same line it
      started on. Hidden on a phone, where there is no keyboard to shortcut.
    -->
    <button
      @click="emit('show-shortcuts')"
      class="hidden md:flex shrink-0 items-center justify-center w-9 h-9 rounded-full border border-gray-200 dark:border-[#2c2c2c] bg-white dark:bg-[#1e1e1e] text-gray-400 hover:text-black dark:hover:text-white transition-colors cursor-pointer shadow-[0_2px_8px_rgba(0,0,0,0.02)]"
      :aria-label="$t('task.a11y_open_shortcuts')"
      :title="$t('task.shortcuts') + ' (?)'"
    >
      <Keyboard class="w-4 h-4" />
    </button>
  </div>
</template>
