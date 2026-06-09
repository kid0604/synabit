<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { useEventBus } from '../../composables/useEventBus';
import { usePlatform } from '../../composables/usePlatform';
import { Rss, RefreshCw, Plus } from 'lucide-vue-next';
import { logger } from '../../utils/logger';
import { ask } from '@tauri-apps/plugin-dialog';
import NavButtons from '../../shared/components/NavButtons.vue';

import FeedsSidebar from './components/FeedsSidebar.vue';
import ArticleList from './components/ArticleList.vue';
import ArticleReader from './components/ArticleReader.vue';
import AddFeedModal from './components/AddFeedModal.vue';
import ImportExportModal from './components/ImportExportModal.vue';

import { useArticleService } from './composables/useArticleService';
import { useFeedActions } from './composables/useFeedActions';
import type { FeedSource, FeedCategory, FeedConfig, CachedArticle, ArticleFilter } from './types/feed.types';
import { DEFAULT_CONFIG } from './types/feed.types';

const props = defineProps<{ vaultPath: string }>();

const { t } = useI18n();
const bus = useEventBus();
const { useMobileLayout } = usePlatform();
const feedService = useArticleService();
const feedActions = useFeedActions();

// State
const loading = ref(true);
const refreshing = ref(false);
const sources = ref<FeedSource[]>([]);
const categories = ref<FeedCategory[]>([]);
const config = ref<FeedConfig>({ ...DEFAULT_CONFIG });
const articles = ref<CachedArticle[]>([]);
const unreadCounts = ref<Record<string, number>>({});
const totalUnread = ref(0);

const selectedSourceId = ref<string | null>(null);
const selectedCategoryId = ref<string | null>(null);
const selectedArticle = ref<CachedArticle | null>(null);
const currentView = ref<'today' | 'all' | 'starred' | 'read-later' | 'unread'>('all');
const searchQuery = ref('');
const showAddFeedModal = ref(false);
const showImportExportModal = ref(false);
const viewMode = ref<'magazine' | 'cards' | 'titles'>(config.value.defaultView || 'magazine');

// Mobile state
const mobilePanel = ref<'list' | 'reader'>('list');

// Computed filter
const currentFilter = computed<ArticleFilter>(() => ({
  sourceId: selectedSourceId.value || undefined,
  categoryId: selectedCategoryId.value || undefined,
  view: currentView.value,
  search: searchQuery.value || undefined,
  limit: 50,
  offset: 0,
}));

// Load data
const loadData = async () => {
  if (!props.vaultPath) return;
  loading.value = true;
  try {
    const [s, c, cfg, counts, total] = await Promise.all([
      feedService.getSources(),
      feedService.getCategories(),
      feedService.getConfig(),
      feedService.getUnreadCounts(),
      feedService.getTotalUnread(),
    ]);
    sources.value = s;
    categories.value = c;
    config.value = cfg;
    unreadCounts.value = counts;
    totalUnread.value = total;
    await loadArticles();
  } catch (e) {
    logger.error('Failed to load feeds data', e);
  } finally {
    loading.value = false;
  }
};

const loadArticles = async () => {
  try {
    if (searchQuery.value) {
      articles.value = await feedService.searchArticles(searchQuery.value);
    } else {
      articles.value = await feedService.getArticles(currentFilter.value);
    }
  } catch (e) {
    logger.error('Failed to load articles', e);
  }
};

const handleRefresh = async () => {
  refreshing.value = true;
  try {
    await feedService.refreshFeeds();
    await loadData();
  } catch (e) {
    logger.error('Failed to refresh feeds', e);
  } finally {
    refreshing.value = false;
  }
};

const handleSelectSource = (sourceId: string | null) => {
  selectedSourceId.value = sourceId;
  selectedCategoryId.value = null;
  selectedArticle.value = null;
  loadArticles();
};

const handleSelectCategory = (categoryId: string | null) => {
  selectedCategoryId.value = categoryId;
  selectedSourceId.value = null;
  selectedArticle.value = null;
  loadArticles();
};

const handleSelectView = (view: typeof currentView.value) => {
  currentView.value = view;
  selectedSourceId.value = null;
  selectedCategoryId.value = null;
  selectedArticle.value = null;
  loadArticles();
};

const handleSelectArticle = async (article: CachedArticle) => {
  selectedArticle.value = article;
  if (!article.isRead) {
    await feedService.markRead(article.id, true);
    article.isRead = true;
    const counts = await feedService.getUnreadCounts();
    unreadCounts.value = counts;
    totalUnread.value = await feedService.getTotalUnread();
  }
  if (useMobileLayout.value) {
    mobilePanel.value = 'reader';
  }
};

const handleMarkAllRead = async () => {
  await feedService.markAllRead(selectedSourceId.value || undefined, selectedCategoryId.value || undefined);
  await loadData();
};

const handleToggleStar = async (articleId: string) => {
  await feedService.toggleStar(articleId);
  const article = articles.value.find(a => a.id === articleId);
  if (article) article.isStarred = !article.isStarred;
  if (selectedArticle.value?.id === articleId) {
    selectedArticle.value = { ...selectedArticle.value, isStarred: !selectedArticle.value.isStarred };
  }
};

const handleToggleReadLater = async (articleId: string) => {
  await feedService.toggleReadLater(articleId);
  const article = articles.value.find(a => a.id === articleId);
  if (article) article.isReadLater = !article.isReadLater;
  if (selectedArticle.value?.id === articleId) {
    selectedArticle.value = { ...selectedArticle.value, isReadLater: !selectedArticle.value.isReadLater };
  }
};

const handleClipToNote = (article: CachedArticle) => feedActions.clipToNote(article);
const handleQuickCapture = (article: CachedArticle) => feedActions.quickCapture(article);
const handleCreateTask = (article: CachedArticle) => feedActions.createTask(article);

const handleFeedAdded = async () => {
  showAddFeedModal.value = false;
  await loadData();
};

const handleRemoveSource = async (sourceId: string) => {
  const source = sources.value.find(s => s.id === sourceId);
  const name = source?.title || sourceId;
  const yes = await ask(`${t('feeds.confirm_remove_source')}\n\n${name}`, { title: t('feeds.remove_source'), kind: 'warning' });
  if (!yes) return;
  await feedService.removeSource(sourceId);
  if (selectedSourceId.value === sourceId) selectedSourceId.value = null;
  await loadData();
};

const handlePauseSource = async (sourceId: string) => {
  const source = sources.value.find(s => s.id === sourceId);
  if (source) {
    source.isPaused = !source.isPaused;
    await feedService.updateSource(source);
    await loadData();
  }
};

const handleMarkSourceRead = async (sourceId: string) => {
  await feedService.markAllRead(sourceId);
  await loadData();
};

const handleImported = async () => {
  showImportExportModal.value = false;
  await loadData();
};

const handleMobileBack = () => {
  mobilePanel.value = 'list';
  selectedArticle.value = null;
};

const handleArticleUpdated = (updated: CachedArticle) => {
  selectedArticle.value = updated;
  // Also update in the articles list
  const idx = articles.value.findIndex(a => a.id === updated.id);
  if (idx >= 0) {
    articles.value[idx] = updated;
  }
};

// Debounce
let _debounceTimer: ReturnType<typeof setTimeout> | null = null;
const debouncedLoad = (fn: () => void, ms = 300) => {
  if (_debounceTimer) clearTimeout(_debounceTimer);
  _debounceTimer = setTimeout(fn, ms);
};

// Search debounce
let searchTimer: ReturnType<typeof setTimeout> | null = null;
const handleSearchUpdate = (query: string) => {
  searchQuery.value = query;
  if (searchTimer) clearTimeout(searchTimer);
  searchTimer = setTimeout(() => loadArticles(), 300);
};

// Keyboard shortcuts
const handleKeyboard = (e: KeyboardEvent) => {
  // Don't trigger if typing in input
  if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
  
  switch(e.key) {
    case 'j': // Next article
      navigateArticle(1);
      break;
    case 'k': // Previous article
      navigateArticle(-1);
      break;
    case 's': // Toggle star
      if (selectedArticle.value) handleToggleStar(selectedArticle.value.id);
      break;
    case 'm': // Toggle read
      if (selectedArticle.value) {
        feedService.markRead(selectedArticle.value.id, !selectedArticle.value.isRead);
        selectedArticle.value.isRead = !selectedArticle.value.isRead;
        loadData();
      }
      break;
    case 'b': // Toggle read later
      if (selectedArticle.value) handleToggleReadLater(selectedArticle.value.id);
      break;
    case 'o': // Open original
      if (selectedArticle.value?.url) window.open(selectedArticle.value.url, '_blank');
      break;
    case 'r': // Refresh
      if (!e.ctrlKey && !e.metaKey) handleRefresh();
      break;
    case 'Escape':
      selectedArticle.value = null;
      if (useMobileLayout.value) mobilePanel.value = 'list';
      break;
  }
};

const navigateArticle = (direction: number) => {
  if (articles.value.length === 0) return;
  const currentIdx = selectedArticle.value 
    ? articles.value.findIndex(a => a.id === selectedArticle.value!.id)
    : -1;
  const nextIdx = Math.max(0, Math.min(articles.value.length - 1, currentIdx + direction));
  handleSelectArticle(articles.value[nextIdx]);
};

// Lifecycle
onMounted(async () => {
  await loadData();
  bus.on('vault:file-modified', () => debouncedLoad(() => loadData()));
  bus.on('vault:file-created-deleted', () => debouncedLoad(() => loadData()));
  bus.on('vault:sync-completed', () => debouncedLoad(() => loadData()));
  bus.on('node:updated', ({ nodeType }) => {
    if (nodeType === 'feed_source' || nodeType === 'feed_article') debouncedLoad(() => loadData());
  });

  // Keyboard shortcuts
  window.addEventListener('keydown', handleKeyboard);

  // Auto-cleanup on mount
  feedService.runCleanup().catch(() => {});
  // Cleanup every 6 hours
  const cleanupInterval = setInterval(() => {
    feedService.runCleanup().catch(() => {});
  }, 6 * 60 * 60 * 1000);
  onUnmounted(() => {
    window.removeEventListener('keydown', handleKeyboard);
    clearInterval(cleanupInterval);
  });
});

const openFeedById = (feedId: string) => handleSelectSource(feedId);
const openArticleById = async (articleId: string) => {
  let article = articles.value.find(a => a.id === articleId);
  if (!article) {
    currentView.value = 'all';
    await loadArticles();
    article = articles.value.find(a => a.id === articleId);
  }
  if (article) handleSelectArticle(article);
};

defineExpose({ openFeedById, openArticleById });
</script>

<template>
  <div class="flex-1 flex flex-col h-full bg-base dark:bg-base-dark overflow-hidden relative">
    <!-- Loading -->
    <div v-if="loading && sources.length === 0" class="absolute inset-0 flex items-center justify-center z-[100] bg-base/50 dark:bg-base-dark/50">
      <div class="animate-spin rounded-full h-8 w-8 border-b-2 border-orange-500"></div>
    </div>

    <!-- Topbar -->
    <div class="flex items-center justify-between p-6 shrink-0">
      <div>
        <h1 class="text-2xl font-bold flex items-center gap-2">
          <NavButtons />
          <Rss class="w-6 h-6 text-orange-500" />
          {{ t('feeds.title') }}
        </h1>
        <p class="text-sm text-gray-500 dark:text-gray-400">{{ t('feeds.subtitle') }}</p>
      </div>
      <div class="flex items-center gap-3">
        <button @click="handleRefresh" :disabled="refreshing" class="p-2.5 rounded-xl bg-surface dark:bg-surface-dark border border-border dark:border-border-dark text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors shadow-sm disabled:opacity-50" :title="t('feeds.refresh_all')">
          <RefreshCw class="w-5 h-5" :class="{ 'animate-spin': refreshing }" />
        </button>
        <button @click="showAddFeedModal = true" class="flex items-center gap-2 px-4 py-2.5 rounded-xl bg-orange-500 text-white hover:bg-orange-600 transition-colors shadow-sm font-medium">
          <Plus class="w-5 h-5" />
          <span>{{ t('feeds.add_feed') }}</span>
        </button>
      </div>
    </div>

    <!-- Main Content -->
    <div class="flex-1 flex gap-0 overflow-hidden">
      <template v-if="!useMobileLayout">
        <FeedsSidebar :sources="sources" :categories="categories" :unread-counts="unreadCounts" :total-unread="totalUnread" :selected-source-id="selectedSourceId" :selected-category-id="selectedCategoryId" :current-view="currentView" @select-source="handleSelectSource" @select-category="handleSelectCategory" @select-view="handleSelectView" @remove-source="handleRemoveSource" @open-opml="showImportExportModal = true" @pause-source="handlePauseSource" @mark-source-read="handleMarkSourceRead" class="w-[260px] shrink-0 border-r border-border dark:border-border-dark" />
        <ArticleList :articles="articles" :selected-article="selectedArticle" :sources="sources" :search-query="searchQuery" :current-view="currentView" :refreshing="refreshing" :view-mode="viewMode" @select-article="handleSelectArticle" @update:search-query="handleSearchUpdate" @update:view-mode="viewMode = $event" @mark-all-read="handleMarkAllRead" @refresh="handleRefresh" class="w-[380px] shrink-0 border-r border-border dark:border-border-dark" />
        <ArticleReader :article="selectedArticle" :config="config" :sources="sources" @toggle-star="handleToggleStar" @toggle-read-later="handleToggleReadLater" @clip-to-note="handleClipToNote" @quick-capture="handleQuickCapture" @create-task="handleCreateTask" @article-updated="handleArticleUpdated" class="flex-1 min-w-0" />
      </template>
      <template v-else>
        <ArticleList v-if="mobilePanel === 'list'" :articles="articles" :selected-article="selectedArticle" :sources="sources" :search-query="searchQuery" :current-view="currentView" :refreshing="refreshing" :view-mode="viewMode" @select-article="handleSelectArticle" @update:search-query="handleSearchUpdate" @update:view-mode="viewMode = $event" @mark-all-read="handleMarkAllRead" @refresh="handleRefresh" class="flex-1" />
        <ArticleReader v-else :article="selectedArticle" :config="config" :sources="sources" :show-back-button="true" @back="handleMobileBack" @toggle-star="handleToggleStar" @toggle-read-later="handleToggleReadLater" @clip-to-note="handleClipToNote" @quick-capture="handleQuickCapture" @create-task="handleCreateTask" @article-updated="handleArticleUpdated" class="flex-1" />
      </template>
    </div>

    <AddFeedModal v-if="showAddFeedModal" :categories="categories" @close="showAddFeedModal = false" @added="handleFeedAdded" />
    <ImportExportModal v-if="showImportExportModal" @close="showImportExportModal = false" @imported="handleImported" />
  </div>
</template>
