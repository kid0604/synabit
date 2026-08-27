<script setup lang="ts">
/**
 * The settings that `FeedConfig` has always carried and nothing could change.
 *
 * Every control here changes something observable. Two former fields —
 * "show read articles" and "mark read on scroll" — are gone rather than shown,
 * because neither was ever read by the app and a switch that does nothing is
 * worse than no switch.
 */
import { ref, watch } from 'vue';
import { useFocusTrap } from '../composables/useFocusTrap';
import { useI18n } from 'vue-i18n';
import { X, Settings, LayoutList, LayoutGrid, List, ArrowDownWideNarrow, ArrowUpWideNarrow } from 'lucide-vue-next';
import type { FeedConfig, ViewMode, SortOrder } from '../types/feed.types';

const props = defineProps<{ config: FeedConfig }>();
const emit = defineEmits<{ close: []; saved: [config: FeedConfig] }>();

const { t } = useI18n();

const dialog = ref<HTMLElement | null>(null);
useFocusTrap(dialog);

const draft = ref<FeedConfig>({ ...props.config });
watch(() => props.config, cfg => { draft.value = { ...cfg }; });

const SORTS: { id: SortOrder; icon: typeof LayoutList; labelKey: string }[] = [
  { id: 'newest', icon: ArrowDownWideNarrow, labelKey: 'feeds.sort_newest' },
  { id: 'oldest', icon: ArrowUpWideNarrow, labelKey: 'feeds.sort_oldest' },
];

const LAYOUTS: { id: ViewMode; icon: typeof LayoutList; labelKey: string }[] = [
  { id: 'magazine', icon: LayoutList, labelKey: 'feeds.view_magazine' },
  { id: 'cards', icon: LayoutGrid, labelKey: 'feeds.view_cards' },
  { id: 'titles', icon: List, labelKey: 'feeds.view_titles' },
];

// Bounds, not validation theatre: a zero-minute interval would refresh
// continuously, and a one-day retention would delete this morning's reading.
const clamp = (value: number, min: number, max: number, fallback: number) =>
  Number.isFinite(value) ? Math.min(max, Math.max(min, Math.round(value))) : fallback;

const handleSave = () => {
  emit('saved', {
    ...draft.value,
    globalUpdateInterval: clamp(draft.value.globalUpdateInterval, 5, 1440, 30),
    autoCleanupDays: clamp(draft.value.autoCleanupDays, 1, 3650, 30),
    maxArticlesPerFeed: clamp(draft.value.maxArticlesPerFeed, 50, 10000, 500),
    readingFontSize: clamp(draft.value.readingFontSize, 12, 24, 16),
    readingMaxWidth: clamp(draft.value.readingMaxWidth, 480, 1200, 720),
  });
};

const handleKeydown = (e: KeyboardEvent) => {
  if (e.key === 'Escape') emit('close');
};
</script>

<template>
  <div ref="dialog" class="fixed inset-0 z-[200] flex items-center justify-center" role="dialog" aria-modal="true" :aria-label="t('feeds.settings')" tabindex="-1" @keydown="handleKeydown">
    <div class="absolute inset-0 bg-black/50 backdrop-blur-sm" @click="emit('close')"></div>

    <div class="relative w-full max-w-lg mx-4 bg-white dark:bg-[#1a1a1a] rounded-2xl shadow-2xl border border-gray-200 dark:border-[#2c2c2c] overflow-hidden animate-in">
      <div class="flex items-center justify-between px-6 py-4 border-b border-gray-200 dark:border-[#2c2c2c]">
        <h2 class="text-lg font-bold flex items-center gap-2">
          <Settings class="w-5 h-5 text-orange-500" />
          {{ t('feeds.settings') }}
        </h2>
        <button @click="emit('close')" class="p-1.5 rounded-lg text-gray-400 hover:text-gray-600 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors" :aria-label="t('feeds.close_settings')">
          <X class="w-5 h-5" />
        </button>
      </div>

      <div class="px-6 py-5 space-y-6 max-h-[65vh] overflow-y-auto">
        <!-- Layout -->
        <section class="space-y-2">
          <h3 class="text-[11px] font-semibold uppercase tracking-wider text-gray-400 dark:text-gray-500">{{ t('feeds.settings_layout') }}</h3>
          <div class="flex items-center gap-2">
            <button
              v-for="layout in LAYOUTS"
              :key="layout.id"
              @click="draft.defaultView = layout.id"
              :class="[
                'flex-1 flex flex-col items-center gap-1.5 px-3 py-3 rounded-xl border text-xs transition-all duration-200',
                draft.defaultView === layout.id
                  ? 'border-orange-400 bg-orange-50 dark:bg-orange-900/20 text-orange-600 dark:text-orange-400 font-medium'
                  : 'border-border dark:border-border-dark text-gray-500 dark:text-gray-400 hover:border-gray-300 dark:hover:border-gray-600'
              ]"
            >
              <component :is="layout.icon" class="w-4 h-4" />
              {{ t(layout.labelKey) }}
            </button>
          </div>
        </section>

        <!-- Order -->
        <section class="space-y-2">
          <h3 class="text-[11px] font-semibold uppercase tracking-wider text-gray-400 dark:text-gray-500">{{ t('feeds.settings_sort') }}</h3>
          <div class="flex items-center gap-2">
            <button
              v-for="sort in SORTS"
              :key="sort.id"
              @click="draft.sortOrder = sort.id"
              :class="[
                'flex-1 flex items-center justify-center gap-2 px-3 py-2.5 rounded-xl border text-xs transition-all duration-200',
                draft.sortOrder === sort.id
                  ? 'border-orange-400 bg-orange-50 dark:bg-orange-900/20 text-orange-600 dark:text-orange-400 font-medium'
                  : 'border-border dark:border-border-dark text-gray-500 dark:text-gray-400 hover:border-gray-300 dark:hover:border-gray-600'
              ]"
            >
              <component :is="sort.icon" class="w-4 h-4" />
              {{ t(sort.labelKey) }}
            </button>
          </div>
        </section>

        <!-- Reading flow -->
        <section class="space-y-2">
          <label class="flex items-start justify-between gap-4 cursor-pointer">
            <span class="min-w-0">
              <span class="block text-sm text-gray-700 dark:text-gray-300">{{ t('feeds.mark_read_on_scroll_label') }}</span>
              <span class="block text-xs text-gray-400 dark:text-gray-500 mt-0.5">{{ t('feeds.mark_read_on_scroll_hint') }}</span>
            </span>
            <input
              v-model="draft.markReadOnScroll"
              type="checkbox"
              class="mt-1 w-4 h-4 shrink-0 accent-orange-500"
            />
          </label>
        </section>

        <!-- Refreshing -->
        <section class="space-y-2">
          <h3 class="text-[11px] font-semibold uppercase tracking-wider text-gray-400 dark:text-gray-500">{{ t('feeds.settings_refresh') }}</h3>
          <label class="flex items-center justify-between gap-4 text-sm text-gray-700 dark:text-gray-300">
            <span class="min-w-0">{{ t('feeds.update_interval_label') }}</span>
            <span class="flex items-center gap-2 shrink-0">
              <input
                v-model.number="draft.globalUpdateInterval"
                type="number" min="5" max="1440" step="5"
                class="w-24 px-2.5 py-1.5 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark text-sm text-right text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-orange-500/30 focus:border-orange-500"
              />
              <span class="text-xs text-gray-400 w-12">{{ t('feeds.minutes') }}</span>
            </span>
          </label>
          <p class="text-xs text-gray-400 dark:text-gray-500">{{ t('feeds.update_interval_hint') }}</p>
        </section>

        <!-- Storage -->
        <section class="space-y-2">
          <h3 class="text-[11px] font-semibold uppercase tracking-wider text-gray-400 dark:text-gray-500">{{ t('feeds.settings_storage') }}</h3>
          <label class="flex items-center justify-between gap-4 text-sm text-gray-700 dark:text-gray-300">
            <span class="min-w-0">{{ t('feeds.cleanup_days_label') }}</span>
            <span class="flex items-center gap-2 shrink-0">
              <input
                v-model.number="draft.autoCleanupDays"
                type="number" min="1" max="3650"
                class="w-24 px-2.5 py-1.5 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark text-sm text-right text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-orange-500/30 focus:border-orange-500"
              />
              <span class="text-xs text-gray-400 w-12">{{ t('feeds.days') }}</span>
            </span>
          </label>
          <label class="flex items-center justify-between gap-4 text-sm text-gray-700 dark:text-gray-300">
            <span class="min-w-0">{{ t('feeds.max_articles_label') }}</span>
            <span class="flex items-center gap-2 shrink-0">
              <input
                v-model.number="draft.maxArticlesPerFeed"
                type="number" min="50" max="10000" step="50"
                class="w-24 px-2.5 py-1.5 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark text-sm text-right text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-orange-500/30 focus:border-orange-500"
              />
              <span class="text-xs text-gray-400 w-12">{{ t('feeds.articles') }}</span>
            </span>
          </label>
          <p class="text-xs text-gray-400 dark:text-gray-500">{{ t('feeds.cleanup_hint') }}</p>
        </section>

        <!-- Reading -->
        <section class="space-y-3">
          <h3 class="text-[11px] font-semibold uppercase tracking-wider text-gray-400 dark:text-gray-500">{{ t('feeds.settings_reading') }}</h3>
          <label class="block space-y-1.5">
            <span class="flex items-center justify-between text-sm text-gray-700 dark:text-gray-300">
              {{ t('feeds.font_size_label') }}
              <span class="text-xs text-gray-400">{{ draft.readingFontSize }}px</span>
            </span>
            <input v-model.number="draft.readingFontSize" type="range" min="12" max="24" step="1" class="w-full accent-orange-500" />
          </label>
          <label class="block space-y-1.5">
            <span class="flex items-center justify-between text-sm text-gray-700 dark:text-gray-300">
              {{ t('feeds.max_width_label') }}
              <span class="text-xs text-gray-400">{{ draft.readingMaxWidth }}px</span>
            </span>
            <input v-model.number="draft.readingMaxWidth" type="range" min="480" max="1200" step="20" class="w-full accent-orange-500" />
          </label>
          <p
            class="mt-1 px-4 py-3 rounded-xl bg-surface dark:bg-surface-dark border border-border dark:border-border-dark text-text dark:text-text-dark leading-relaxed"
            :style="{ fontSize: draft.readingFontSize + 'px', maxWidth: draft.readingMaxWidth + 'px' }"
          >{{ t('feeds.reading_preview') }}</p>
        </section>
      </div>

      <div class="flex items-center justify-end gap-2 px-6 py-4 border-t border-gray-200 dark:border-[#2c2c2c]">
        <button @click="emit('close')" class="px-4 py-2 rounded-xl text-sm font-medium text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors">
          {{ t('feeds.cancel') }}
        </button>
        <button @click="handleSave" class="px-4 py-2 rounded-xl bg-orange-500 text-white text-sm font-medium hover:bg-orange-600 transition-colors shadow-sm">
          {{ t('feeds.save') }}
        </button>
      </div>
    </div>
  </div>
</template>
