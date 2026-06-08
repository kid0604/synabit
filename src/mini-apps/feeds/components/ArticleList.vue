<script setup lang="ts">
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { Search, RefreshCw, CheckCheck, Rss, LayoutList, LayoutGrid, List } from 'lucide-vue-next';
import ArticleCard from './ArticleCard.vue';
import type { CachedArticle, FeedSource } from '../types/feed.types';

const props = defineProps<{
  articles: CachedArticle[];
  selectedArticle: CachedArticle | null;
  sources: FeedSource[];
  searchQuery: string;
  currentView: string;
  refreshing: boolean;
  viewMode?: 'magazine' | 'cards' | 'titles';
}>();

const emit = defineEmits<{
  'select-article': [article: CachedArticle];
  'update:search-query': [query: string];
  'update:view-mode': [mode: 'magazine' | 'cards' | 'titles'];
  'mark-all-read': [];
  'refresh': [];
}>();

const { t } = useI18n();

const getSourceName = (sourceId: string) => {
  return props.sources.find(s => s.id === sourceId)?.title || 'Unknown';
};

const hasUnread = computed(() => props.articles.some(a => !a.isRead));

const activeViewMode = computed(() => props.viewMode || 'magazine');
</script>

<template>
  <div class="flex flex-col h-full bg-base dark:bg-base-dark">
    <!-- Toolbar -->
    <div class="p-3 space-y-2 shrink-0 border-b border-border dark:border-border-dark">
      <!-- Search -->
      <div class="relative">
        <Search class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400" />
        <input
          :value="searchQuery"
          @input="emit('update:search-query', ($event.target as HTMLInputElement).value)"
          :placeholder="t('feeds.search_articles')"
          class="w-full pl-9 pr-3 py-2 rounded-xl bg-surface dark:bg-surface-dark border border-border dark:border-border-dark text-sm text-text dark:text-text-dark placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-orange-500/30 focus:border-orange-500 transition-all"
        />
      </div>

      <!-- Actions -->
      <div class="flex items-center gap-2">
        <button @click="emit('refresh')" :disabled="refreshing" class="p-1.5 rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors disabled:opacity-50" :title="t('feeds.refresh')">
          <RefreshCw class="w-4 h-4" :class="{ 'animate-spin': refreshing }" />
        </button>
        <button v-if="hasUnread" @click="emit('mark-all-read')" class="p-1.5 rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors" :title="t('feeds.mark_all_read')">
          <CheckCheck class="w-4 h-4" />
        </button>

        <!-- View mode toggles -->
        <div class="flex items-center gap-0.5 ml-1 p-0.5 rounded-lg bg-gray-100 dark:bg-gray-800">
          <button @click="emit('update:view-mode', 'magazine')" :class="['p-1 rounded-md transition-colors', activeViewMode === 'magazine' ? 'bg-white dark:bg-gray-700 shadow-sm text-orange-500' : 'text-gray-400 hover:text-gray-600 dark:hover:text-gray-300']" title="Magazine">
            <LayoutList class="w-3.5 h-3.5" />
          </button>
          <button @click="emit('update:view-mode', 'cards')" :class="['p-1 rounded-md transition-colors', activeViewMode === 'cards' ? 'bg-white dark:bg-gray-700 shadow-sm text-orange-500' : 'text-gray-400 hover:text-gray-600 dark:hover:text-gray-300']" title="Cards">
            <LayoutGrid class="w-3.5 h-3.5" />
          </button>
          <button @click="emit('update:view-mode', 'titles')" :class="['p-1 rounded-md transition-colors', activeViewMode === 'titles' ? 'bg-white dark:bg-gray-700 shadow-sm text-orange-500' : 'text-gray-400 hover:text-gray-600 dark:hover:text-gray-300']" title="Titles">
            <List class="w-3.5 h-3.5" />
          </button>
        </div>

        <span class="flex-1"></span>
        <span class="text-xs text-gray-400">{{ articles.length }} {{ t('feeds.articles') }}</span>
      </div>
    </div>

    <!-- Article list -->
    <div class="flex-1 overflow-y-auto hidden-scrollbar">
      <div v-if="articles.length > 0" :class="activeViewMode === 'cards' ? 'grid grid-cols-2 gap-3 p-3' : 'divide-y divide-border dark:divide-border-dark'">
        <ArticleCard
          v-for="article in articles"
          :key="article.id"
          :article="article"
          :is-selected="selectedArticle?.id === article.id"
          :source-name="getSourceName(article.feedSourceId)"
          :view-mode="activeViewMode"
          @select="emit('select-article', article)"
        />
      </div>

      <!-- Empty state -->
      <div v-else class="flex flex-col items-center justify-center h-full px-6 text-center">
        <div class="w-16 h-16 rounded-2xl bg-orange-50 dark:bg-orange-900/20 flex items-center justify-center mb-4">
          <Rss class="w-8 h-8 text-orange-400" />
        </div>
        <p class="text-base font-medium text-gray-600 dark:text-gray-300 mb-1">{{ t('feeds.all_caught_up') }}</p>
        <p class="text-sm text-gray-400 dark:text-gray-500">{{ t('feeds.no_articles') }}</p>
      </div>
    </div>
  </div>
</template>
