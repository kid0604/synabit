<script setup lang="ts">
import { useI18n } from 'vue-i18n';
import { Rss, MoreHorizontal, Trash2, PauseCircle, PlayCircle, CheckCheck, Pencil, AlertTriangle, AlignLeft, Crosshair } from 'lucide-vue-next';
import type { FeedSource } from '../types/feed.types';
import { ref, nextTick } from 'vue';

const props = defineProps<{
  source: FeedSource;
  unreadCount: number;
  isSelected: boolean;
}>();

const emit = defineEmits<{
  select: [];
  remove: [];
  'pause-source': [];
  'mark-source-read': [];
  'rename-source': [newTitle: string];
  'toggle-full-text': [];
  'set-scrape-container': [selector: string];
}>();

const { t } = useI18n();
const showMenu = ref(false);
const isRenaming = ref(false);
const renameValue = ref('');
const renameInput = ref<HTMLInputElement | null>(null);

const startRename = () => {
  showMenu.value = false;
  renameValue.value = props.source.title;
  isRenaming.value = true;
  nextTick(() => {
    renameInput.value?.focus();
    renameInput.value?.select();
  });
};

const confirmRename = () => {
  const trimmed = renameValue.value.trim();
  if (trimmed && trimmed !== props.source.title) {
    emit('rename-source', trimmed);
  }
  isRenaming.value = false;
};

const cancelRename = () => {
  isRenaming.value = false;
};

// The selector that names article cards on a scraped page, for a site the
// built-in guesses do not fit. Edited in place, like the title, rather than
// behind a dialog — it is one line of text about one feed.
const isEditingSelector = ref(false);
const selectorValue = ref('');
const selectorInput = ref<HTMLInputElement | null>(null);

const startSelectorEdit = () => {
  showMenu.value = false;
  selectorValue.value = props.source.scrapeContainer;
  isEditingSelector.value = true;
  nextTick(() => {
    selectorInput.value?.focus();
    selectorInput.value?.select();
  });
};

const confirmSelector = () => {
  const trimmed = selectorValue.value.trim();
  if (trimmed !== props.source.scrapeContainer) emit('set-scrape-container', trimmed);
  isEditingSelector.value = false;
};
</script>

<template>
  <div
    @click="!isRenaming && !isEditingSelector && emit('select')"
    :class="[
      'relative group flex items-center gap-2.5 px-3 py-2 rounded-xl text-sm cursor-pointer transition-all duration-200',
      isSelected
        ? 'bg-orange-50 dark:bg-orange-900/20 text-orange-600 dark:text-orange-400 font-medium shadow-sm'
        : 'text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800/60'
    ]"
  >
    <img v-if="source.iconUrl" :src="source.iconUrl" class="w-5 h-5 rounded shrink-0" @error="($event.target as HTMLImageElement).style.display='none'" />
    <Rss v-else class="w-4 h-4 shrink-0 text-gray-400" />

    <!-- Rename mode -->
    <input
      v-if="isRenaming"
      ref="renameInput"
      v-model="renameValue"
      @keydown.enter.stop="confirmRename"
      @keydown.escape.stop="cancelRename"
      @blur="confirmRename"
      @click.stop
      class="flex-1 min-w-0 px-1.5 py-0.5 text-sm rounded-md bg-white dark:bg-[#111] border border-orange-400 outline-none"
    />
    <!-- Scrape selector -->
    <input
      v-else-if="isEditingSelector"
      ref="selectorInput"
      v-model="selectorValue"
      :placeholder="t('feeds.scrape_selector_placeholder')"
      @keydown.enter.stop="confirmSelector"
      @keydown.escape.stop="isEditingSelector = false"
      @blur="confirmSelector"
      @click.stop
      class="flex-1 min-w-0 px-1.5 py-0.5 text-xs font-mono rounded-md bg-white dark:bg-[#111] border border-orange-400 outline-none"
    />
    <!-- Normal display -->
    <span v-else class="flex-1 truncate" :class="{ 'opacity-50': source.isPaused }">{{ source.title }}</span>

    <!--
      A feed that has been failing for weeks used to look exactly like one
      that simply had nothing new; the error was recorded and never shown.
    -->
    <AlertTriangle
      v-if="source.lastError && !isRenaming"
      class="w-3.5 h-3.5 text-amber-500 shrink-0"
      :title="`${t('feeds.feed_error')}: ${source.lastError}`"
    />

    <span v-if="unreadCount > 0 && !isRenaming" class="min-w-[20px] h-5 px-1.5 bg-orange-500 text-white text-[11px] font-bold rounded-full flex items-center justify-center">{{ unreadCount > 99 ? '99+' : unreadCount }}</span>
    
    <button v-if="!isRenaming && !isEditingSelector" @click.stop="showMenu = !showMenu" class="p-1 rounded-lg opacity-0 group-hover:opacity-100 hover:bg-gray-200 dark:hover:bg-gray-700 transition-all" :aria-label="t('feeds.a11y_open_menu')">
      <MoreHorizontal class="w-4 h-4" />
    </button>

    <div v-if="showMenu" class="absolute right-2 top-full mt-1 w-48 py-1.5 bg-white dark:bg-[#1a1a1a] rounded-xl shadow-xl border border-gray-200 dark:border-[#2c2c2c] z-50">
      <button @click.stop="startRename" class="w-full flex items-center gap-2.5 px-3 py-2 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors">
        <Pencil class="w-4 h-4" />
        {{ t('feeds.rename_source') }}
      </button>
      <button @click.stop="emit('pause-source'); showMenu = false" class="w-full flex items-center gap-2.5 px-3 py-2 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors">
        <PauseCircle v-if="!source.isPaused" class="w-4 h-4" />
        <PlayCircle v-else class="w-4 h-4" />
        {{ source.isPaused ? t('feeds.resume_feed') : t('feeds.pause_feed') }}
      </button>
      <button @click.stop="emit('mark-source-read'); showMenu = false" class="w-full flex items-center gap-2.5 px-3 py-2 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors">
        <CheckCheck class="w-4 h-4" />
        {{ t('feeds.mark_feed_read') }}
      </button>
      <button
        v-if="source.feedType === 'scrape'"
        @click.stop="startSelectorEdit"
        class="w-full flex items-center gap-2.5 px-3 py-2 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
      >
        <Crosshair class="w-4 h-4" :class="{ 'text-orange-500': source.scrapeContainer }" />
        {{ t('feeds.scrape_selector') }}
      </button>
      <button @click.stop="emit('toggle-full-text'); showMenu = false" class="w-full flex items-center gap-2.5 px-3 py-2 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors">
        <AlignLeft class="w-4 h-4" :class="{ 'text-orange-500': source.fullTextFetch }" />
        {{ source.fullTextFetch ? t('feeds.full_text_on') : t('feeds.full_text_off') }}
      </button>
      <div class="my-1 border-t border-gray-200 dark:border-[#2c2c2c]"></div>
      <button @click.stop="emit('remove'); showMenu = false" class="w-full flex items-center gap-2.5 px-3 py-2 text-sm text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors">
        <Trash2 class="w-4 h-4" />
        {{ t('feeds.remove_source') }}
      </button>
    </div>
    <div v-if="showMenu" class="fixed inset-0 z-40" @click.stop="showMenu = false"></div>
  </div>
</template>
