<script setup lang="ts">
import { useI18n } from 'vue-i18n';
import { Rss, MoreHorizontal, Trash2, PauseCircle, PlayCircle, CheckCheck, Pencil, AlertTriangle, AlignLeft, Crosshair, FolderInput } from 'lucide-vue-next';
import type { FeedSource, FeedCategory } from '../types/feed.types';
import { FEED_DRAG_TYPE } from '../dragType';
import { ref, nextTick } from 'vue';

const props = defineProps<{
  source: FeedSource;
  unreadCount: number;
  isSelected: boolean;
  /** Where "Move to category" can send this feed. */
  categories: FeedCategory[];
}>();

const emit = defineEmits<{
  select: [];
  remove: [];
  'pause-source': [];
  'mark-source-read': [];
  'rename-source': [newTitle: string];
  'toggle-full-text': [];
  'set-scrape-container': [selector: string];
  /** `''` means no category. */
  'move-to': [categoryId: string];
  'drag-start': [];
  'drag-end': [];
}>();

/**
 * Dragging a feed onto another category moves it (FeedsSidebar handles the
 * drop). The ⋯ menu offers the same move as a list, for the keyboard and for
 * anyone who finds dragging hard — nothing here may need a drag.
 */
const onDragStart = (e: DragEvent) => {
  if (!e.dataTransfer) return;
  e.dataTransfer.setData(FEED_DRAG_TYPE, props.source.id);
  e.dataTransfer.effectAllowed = 'move';
  showMenu.value = false;
  emit('drag-start');
};

const moveTo = (categoryId: string) => {
  showMenu.value = false;
  emit('move-to', categoryId);
};

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
    :draggable="!isRenaming && !isEditingSelector"
    :title="source.title"
    @dragstart="onDragStart"
    @dragend="emit('drag-end')"
    :class="[
      'relative group flex items-center gap-2.5 px-3 py-2 rounded-xl text-sm cursor-pointer transition-all duration-200',
      isSelected
        ? 'bg-accent/10 text-accent dark:text-accent-dark font-medium shadow-sm'
        : 'text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800/60'
    ]"
  >
    <img v-if="source.iconUrl" :src="source.iconUrl" class="w-5 h-5 rounded shrink-0" @error="($event.target as HTMLImageElement).style.display='none'" />
    <Rss v-else class="w-4 h-4 shrink-0 text-gray-500 dark:text-gray-400" />

    <!-- Rename mode -->
    <input
      v-if="isRenaming"
      ref="renameInput"
      v-model="renameValue"
      @keydown.enter.stop="confirmRename"
      @keydown.escape.stop="cancelRename"
      @blur="confirmRename"
      @click.stop
      class="flex-1 min-w-0 px-1.5 py-0.5 text-sm rounded-md bg-white dark:bg-[#111] border border-accent outline-none"
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
      class="flex-1 min-w-0 px-1.5 py-0.5 text-xs font-mono rounded-md bg-white dark:bg-[#111] border border-accent outline-none"
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

    <span v-if="unreadCount > 0 && !isRenaming" class="min-w-[20px] h-5 px-1.5 bg-accent text-white text-xs font-bold rounded-full flex items-center justify-center">{{ unreadCount > 99 ? '99+' : unreadCount }}</span>
    
    <button v-if="!isRenaming && !isEditingSelector" @click.stop="showMenu = !showMenu" class="p-1 rounded-lg opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100 pointer-coarse:opacity-100 hover:bg-gray-200 dark:hover:bg-gray-700 transition-all" :aria-label="t('feeds.a11y_open_menu')">
      <MoreHorizontal class="w-4 h-4" />
    </button>

    <div v-if="showMenu" class="absolute right-2 top-full mt-1 w-52 max-h-80 overflow-y-auto py-1.5 bg-white dark:bg-[#1a1a1a] rounded-xl shadow-xl border border-gray-200 dark:border-border-dark z-50">
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
        <Crosshair class="w-4 h-4" :class="{ 'text-accent dark:text-accent-dark': source.scrapeContainer }" />
        {{ t('feeds.scrape_selector') }}
      </button>
      <button @click.stop="emit('toggle-full-text'); showMenu = false" class="w-full flex items-center gap-2.5 px-3 py-2 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors">
        <AlignLeft class="w-4 h-4" :class="{ 'text-accent dark:text-accent-dark': source.fullTextFetch }" />
        {{ source.fullTextFetch ? t('feeds.full_text_on') : t('feeds.full_text_off') }}
      </button>
      <!-- Move to another category: the same as dragging, without the drag. -->
      <template v-if="categories.length > 0">
        <div class="my-1 border-t border-gray-200 dark:border-border-dark"></div>
        <p class="px-3 pt-1 pb-0.5 text-xs font-medium text-gray-500 dark:text-gray-400">{{ t('feeds.move_to_category') }}</p>
        <button
          v-for="cat in categories.filter(c => c.id !== source.categoryId)"
          :key="cat.id"
          @click.stop="moveTo(cat.id)"
          class="w-full flex items-center gap-2.5 px-3 py-2 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
        >
          <span class="w-2.5 h-2.5 rounded-full shrink-0 ml-[3px] mr-[3px]" :style="{ backgroundColor: cat.color || '#6b7280' }" aria-hidden="true"></span>
          <span class="truncate">{{ cat.name }}</span>
        </button>
        <button
          v-if="source.categoryId && categories.some(c => c.id === source.categoryId)"
          @click.stop="moveTo('')"
          class="w-full flex items-center gap-2.5 px-3 py-2 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
        >
          <FolderInput class="w-4 h-4" aria-hidden="true" />
          {{ t('feeds.uncategorized') }}
        </button>
      </template>
      <div class="my-1 border-t border-gray-200 dark:border-border-dark"></div>
      <button @click.stop="emit('remove'); showMenu = false" class="w-full flex items-center gap-2.5 px-3 py-2 text-sm text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors">
        <Trash2 class="w-4 h-4" />
        {{ t('feeds.remove_source') }}
      </button>
    </div>
    <div v-if="showMenu" class="fixed inset-0 z-40" @click.stop="showMenu = false"></div>
  </div>
</template>
