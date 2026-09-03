<script setup lang="ts">
/**
 * Choosing the mark a kind is drawn with.
 *
 * The code table has an icon for the dozen kinds this app ships screens for,
 * and `Box` for everything else — which is every kind somebody invents, and
 * Things exists for exactly those. Two invented kinds were two identical
 * squares in every list, table and graph.
 *
 * Two screens in one. Before anybody types it shows the few dozen kinds of
 * thing people actually keep, in the order they scan for them — a person who
 * does not yet know what they want needs a page to look at, not a search box.
 * Typing opens the whole library, around 1,900 names.
 *
 * Results are capped. Rendering nineteen hundred buttons to answer `a` is a
 * slow frame for a list nobody reads past the first row of.
 */
import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { Check, RotateCcw } from 'lucide-vue-next';

import { ICON_NAMES, SUGGESTED_ICONS, iconNamed, iconForNodeType } from './nodeTypeIcon';

const props = defineProps<{
  nodeType: string;
  /** The name currently chosen, or `null` for whatever the app defaults to. */
  chosen: string | null;
  /** Where the button that opened this sits, in screen coordinates. */
  at: { x: number; y: number };
}>();

const emit = defineEmits<{
  /** `null` means "go back to the default", which is not the same as `box`. */
  pick: [icon: string | null];
  close: [];
}>();

const { t } = useI18n();

const search = ref('');

/** Enough to choose from, few enough to draw in one frame. */
const MOST = 96;

const searching = computed(() => !!search.value.trim());

const matches = computed(() => {
  const needle = search.value.trim().toLowerCase().replace(/\s+/g, '-');
  if (!needle) return SUGGESTED_ICONS;
  // A name that starts with what was typed before one that merely contains it:
  // typing `car` should not put `shopping-cart` above `car`.
  const starts = ICON_NAMES.filter(n => n.startsWith(needle));
  const rest = ICON_NAMES.filter(n => !n.startsWith(needle) && n.includes(needle));
  return [...starts, ...rest];
});

const names = computed(() => matches.value.slice(0, MOST));
const hidden = computed(() => Math.max(0, matches.value.length - MOST));

const WIDTH = 288;
const HEIGHT = 348;

/** Worked out rather than measured, like every other menu in this app. */
const position = computed(() => {
  const below = props.at.y + 6;
  const fits = below + HEIGHT <= window.innerHeight - 8;
  return {
    left: `${Math.max(8, Math.min(props.at.x, window.innerWidth - WIDTH - 8))}px`,
    top: fits ? `${below}px` : `${Math.max(8, props.at.y - HEIGHT)}px`,
  };
});
</script>

<template>
  <Teleport to="body">
    <div class="fixed inset-0 z-[90]" @click="emit('close')" />
    <div
      :style="position"
      class="fixed w-72 z-[91] p-2 rounded-lg
             bg-white dark:bg-[#2c2c2c] shadow-lg border border-gray-200 dark:border-gray-700"
    >
      <input
        v-model="search"
        type="text"
        spellcheck="false"
        :placeholder="t('things.icon_search')"
        class="w-full px-2.5 py-1.5 mb-2 rounded-md text-xs outline-none
               bg-gray-50 dark:bg-white/5 border border-gray-200 dark:border-gray-700
               text-text dark:text-text-dark placeholder-gray-400"
      />

      <div class="grid grid-cols-8 gap-1 max-h-56 overflow-y-auto">
        <button
          v-for="name in names"
          :key="name"
          type="button"
          @click="emit('pick', name)"
          :title="name"
          class="relative aspect-square flex items-center justify-center rounded-md
                 cursor-pointer transition-colors"
          :class="name === chosen
            ? 'bg-blue-100 dark:bg-blue-900/40 text-blue-700 dark:text-blue-300'
            : 'text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-white/10'"
        >
          <component :is="iconNamed(name)" class="w-4 h-4" />
        </button>
      </div>

      <p v-if="!names.length" class="py-6 text-center text-xs text-gray-400">
        {{ t('things.icon_none_match') }}
      </p>
      <!-- Said rather than silently cut: a grid that stops looks complete. -->
      <p v-else-if="hidden" class="pt-1.5 text-center text-[11px] text-gray-400">
        {{ t('things.icon_more', { n: hidden }) }}
      </p>
      <p v-else-if="!searching" class="pt-1.5 text-center text-[11px] text-gray-400">
        {{ t('things.icon_search_all', { n: ICON_NAMES.length }) }}
      </p>

      <!--
        Clearing is its own row, not a slot in the grid. "No choice" is not one
        of the icons — picking `box` deliberately and never having picked are
        different states, and only the second follows the app if it ever ships
        a real icon for this kind.
      -->
      <div class="mt-2 pt-2 border-t border-gray-100 dark:border-gray-700">
        <button
          type="button"
          @click="emit('pick', null)"
          class="w-full flex items-center gap-2 px-2 py-1.5 rounded-md text-xs text-left
                 cursor-pointer hover:bg-gray-100 dark:hover:bg-white/10"
          :class="chosen ? 'text-gray-600 dark:text-gray-300' : 'text-gray-400'"
        >
          <RotateCcw class="w-3.5 h-3.5" />
          {{ t('things.icon_default') }}
          <component :is="iconForNodeType(nodeType)" v-if="!chosen" class="w-3.5 h-3.5 ml-auto" />
          <Check v-if="!chosen" class="w-3.5 h-3.5 text-blue-500" />
        </button>
      </div>
    </div>
  </Teleport>
</template>
