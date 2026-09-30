<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { useEventBus } from '../../composables/useEventBus';
import { usePlatform } from '../../composables/usePlatform';
import { Rss, RefreshCw, Plus, Settings, Filter, X } from 'lucide-vue-next';
import { logger } from '../../utils/logger';
import { listen } from '@tauri-apps/api/event';
import AppHeader from '../../shared/components/AppHeader.vue';
import UndoToast from '../../shared/components/UndoToast.vue';
import { useUndoableAction } from '../../composables/useUndoableAction';
import { showAppNotice } from '../../composables/useAppNotice';

import FeedsSidebar from './components/FeedsSidebar.vue';
import ArticleList from './components/ArticleList.vue';
import ArticleReader from './components/ArticleReader.vue';
import AddFeedModal from './components/AddFeedModal.vue';
import ImportExportModal from './components/ImportExportModal.vue';
import FeedErrorToast from './components/FeedErrorToast.vue';
import FeedsSettingsModal from './components/FeedsSettingsModal.vue';
import FeedsShortcutsHelp from './components/FeedsShortcutsHelp.vue';
import FeedsRulesModal from './components/FeedsRulesModal.vue';

import { useArticleService, localMidnight } from './composables/useArticleService';
import { useFeedActions } from './composables/useFeedActions';
import type { FeedSource, FeedCategory, FeedConfig, CachedArticle, ArticleFilter, ViewCounts, FeedView, ViewMode, SortOrder, Highlight } from './types/feed.types';
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
const configLoaded = ref(false);
const articles = ref<CachedArticle[]>([]);
const unreadCounts = ref<Record<string, number>>({});
const viewCounts = ref<ViewCounts>({ today: 0, unread: 0, starred: 0, readLater: 0 });

const selectedSourceId = ref<string | null>(null);
const selectedCategoryId = ref<string | null>(null);
const selectedArticle = ref<CachedArticle | null>(null);
const currentView = ref<FeedView>('all');
const searchQuery = ref('');
const showAddFeedModal = ref(false);
const showImportExportModal = ref(false);
const showSettingsModal = ref(false);
const showShortcuts = ref(false);
const showRulesModal = ref(false);

// One screenful at a time. The list used to ask for fifty and stop there, so
// the fifty-first article could only be reached through search.
const PAGE_SIZE = 50;
const hasMore = ref(false);
const loadingMore = ref(false);

// Set from the saved config once it loads. Initialising straight from
// `config` read the built-in default, because at this point nothing has been
// loaded yet — which is why the saved layout never took effect.
const viewMode = ref<ViewMode>(DEFAULT_CONFIG.defaultView);

// Mobile state
const mobilePanel = ref<'list' | 'reader'>('list');
const isSidebarOpen = ref(false);

const handleSelectSourceMobile = (id: string | null) => {
  handleSelectSource(id);
  isSidebarOpen.value = false;
};
const handleSelectCategoryMobile = (id: string | null) => {
  handleSelectCategory(id);
  isSidebarOpen.value = false;
};
const handleSelectViewMobile = (view: FeedView) => {
  handleSelectView(view);
  isSidebarOpen.value = false;
};

// Resize state
const sidebarWidth = ref(260);
const articleListWidth = ref(380);
const isResizing = ref<'sidebar' | 'articleList' | null>(null);

const startResize = (panel: 'sidebar' | 'articleList', e: MouseEvent) => {
  e.preventDefault();
  isResizing.value = panel;
  const startX = e.clientX;
  const startWidth = panel === 'sidebar' ? sidebarWidth.value : articleListWidth.value;

  const onMouseMove = (e: MouseEvent) => {
    const delta = e.clientX - startX;
    if (panel === 'sidebar') {
      sidebarWidth.value = Math.max(180, Math.min(400, startWidth + delta));
    } else {
      articleListWidth.value = Math.max(280, Math.min(600, startWidth + delta));
    }
  };

  const onMouseUp = () => {
    isResizing.value = null;
    document.removeEventListener('mousemove', onMouseMove);
    document.removeEventListener('mouseup', onMouseUp);
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
  };

  document.body.style.cursor = 'col-resize';
  document.body.style.userSelect = 'none';
  document.addEventListener('mousemove', onMouseMove);
  document.addEventListener('mouseup', onMouseUp);
};

// The same edges from the keyboard: each handle is a focusable `separator`,
// the ARIA pattern for a window splitter, and the arrow keys move it.
const nudgeResize = (panel: 'sidebar' | 'articleList', by: number) => {
  if (panel === 'sidebar') {
    sidebarWidth.value = Math.max(180, Math.min(400, sidebarWidth.value + by));
  } else {
    articleListWidth.value = Math.max(280, Math.min(600, articleListWidth.value + by));
  }
};

// The feeds the current selection covers. `undefined` means every feed;
// a category resolves to its members here, where the category-to-feed
// mapping actually lives.
const scopedSourceIds = computed<string[] | undefined>(() => {
  if (selectedSourceId.value) return [selectedSourceId.value];
  if (selectedCategoryId.value) {
    return sources.value
      .filter(s => s.categoryId === selectedCategoryId.value)
      .map(s => s.id);
  }
  return undefined;
});

const currentFilter = computed<ArticleFilter>(() => ({
  sourceIds: scopedSourceIds.value,
  view: currentView.value,
  todayStart: localMidnight(),
  sort: config.value.sortOrder,
  search: searchQuery.value || undefined,
  limit: PAGE_SIZE,
  offset: 0,
}));

// Load data
const loadData = async () => {
  if (!props.vaultPath) return;
  loading.value = true;
  try {
    const [s, c, cfg, counts, views] = await Promise.all([
      feedService.getSources(),
      feedService.getCategories(),
      feedService.getConfig(),
      feedService.getUnreadCounts(),
      feedService.getViewCounts(),
    ]);
    // A feed waiting out its undo window is gone as far as the screen knows.
    sources.value = s.filter(src => !pendingRemovalIds.value.has(src.id));
    categories.value = c;
    const firstLoad = !configLoaded.value;
    config.value = cfg;
    if (firstLoad) {
      viewMode.value = cfg.defaultView;
      configLoaded.value = true;
    }
    unreadCounts.value = counts;
    viewCounts.value = views;
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
      articles.value = (await feedService.searchArticles(searchQuery.value, scopedSourceIds.value))
        .filter(a => !pendingRemovalIds.value.has(a.feedSourceId));
      // Search answers in one batch; there is no second page to ask for.
      hasMore.value = false;
    } else {
      // Reloads happen for reasons that have nothing to do with the list —
      // a vault event, a sync, marking something read. Ask for as much as is
      // already on screen so a reader four pages down is not thrown back to
      // the top by something happening in the background.
      const loaded = Math.max(PAGE_SIZE, articles.value.length);
      const page = await feedService.getArticles({ ...currentFilter.value, limit: loaded });
      articles.value = page.filter(a => !pendingRemovalIds.value.has(a.feedSourceId));
      hasMore.value = page.length === loaded;
    }
  } catch (e) {
    logger.error('Failed to load articles', e);
  }
};

const loadMoreArticles = async () => {
  if (loadingMore.value || !hasMore.value || searchQuery.value) return;
  loadingMore.value = true;
  try {
    const page = await feedService.getArticles({
      ...currentFilter.value,
      offset: articles.value.length,
    });
    // A refresh between pages can shift the window; drop anything we hold.
    const seen = new Set(articles.value.map(a => a.id));
    articles.value = [...articles.value, ...page.filter(a => !seen.has(a.id))];
    hasMore.value = page.length === PAGE_SIZE;
  } catch (e) {
    logger.error('Failed to load more articles', e);
  } finally {
    loadingMore.value = false;
  }
};

// Feeds that refused to refresh, shown until dismissed. The backend has
// always reported these; the front end used to drop them on the floor, so a
// feed that had been failing for weeks looked exactly like one that was fine.
const refreshErrors = ref<string[]>([]);

const handleRefresh = async () => {
  refreshing.value = true;
  try {
    const result = await feedService.refreshFeeds(undefined, true, false);
    refreshErrors.value = result.errors;
    // Force: articles that arrived seconds ago were not in the database when
    // another device's read state was last applied.
    await feedService.syncReadState(true).catch(() => {});
    await loadData();
  } catch (e) {
    logger.error('Failed to refresh feeds', e);
    refreshErrors.value = [t('feeds.error_refreshing')];
  } finally {
    refreshing.value = false;
  }
};

/**
 * The foreground sweep, for as long as the app is open.
 *
 * Which feeds are due is decided in the backend now — one call instead of a
 * loop that re-derived the schedule here and fetched each feed in turn. On the
 * desktop a background scheduler does the same work whether or not this view
 * is mounted; on Android, where there is no background to run in, this is the
 * only sweep there is.
 */
const autoRefresh = async () => {
  if (refreshing.value || sources.value.length === 0) return;
  refreshing.value = true;
  try {
    const result = await feedService.refreshFeeds(undefined, false, true);
    if (result.errors.length > 0) refreshErrors.value = result.errors;
    if (result.totalNew > 0 || result.errors.length > 0) {
      await feedService.syncReadState(true).catch(() => {});
      await loadData();
    }
  } catch (e) {
    logger.error('Auto-refresh failed', e);
  } finally {
    refreshing.value = false;
  }
};

// A new selection starts at the first page again.
const resetPaging = () => {
  articles.value = [];
  hasMore.value = false;
};

const handleSelectSource = (sourceId: string | null) => {
  selectedSourceId.value = sourceId;
  selectedCategoryId.value = null;
  selectedArticle.value = null;
  resetPaging();
  loadArticles();
};

const handleSelectCategory = (categoryId: string | null) => {
  selectedCategoryId.value = categoryId;
  selectedSourceId.value = null;
  selectedArticle.value = null;
  resetPaging();
  loadArticles();
};

const handleSelectView = (view: FeedView) => {
  currentView.value = view;
  selectedSourceId.value = null;
  selectedCategoryId.value = null;
  selectedArticle.value = null;
  resetPaging();
  loadArticles();
};

const handleSelectArticle = async (article: CachedArticle) => {
  // Show what the list already has straight away, then replace it with the
  // full row: the list only carries the first few hundred characters of a
  // body, which is enough for a preview card and not enough to read.
  selectedArticle.value = article;
  feedService
    .getArticle(article.id)
    .then(full => {
      if (selectedArticle.value?.id === full.id) selectedArticle.value = full;
    })
    .catch(e => logger.error('Failed to load article body', e));

  if (!article.isRead) {
    await feedService.markRead(article.id, true);
    article.isRead = true;
    const [counts, views] = await Promise.all([
      feedService.getUnreadCounts(),
      feedService.getViewCounts(),
    ]);
    unreadCounts.value = counts;
    viewCounts.value = views;
  }
  if (useMobileLayout.value) {
    mobilePanel.value = 'reader';
  }
};

// Marking read is bulk and silent, so it gets the same few seconds of grace
// the other destructive actions in the app get.
const UNDO_SECONDS = 7;
const pendingMarkRead = ref<{ ids: string[] } | null>(null);
let markReadTimer: ReturnType<typeof setTimeout> | null = null;

const markReadWithUndo = async (sourceIds: string[] | undefined) => {
  const changed = await feedService.markAllRead(sourceIds);
  await loadData();
  if (changed.length === 0) return;

  pendingMarkRead.value = { ids: changed };
  armMarkRead(UNDO_SECONDS * 1000);
};

/** When the running timer ends the undo, so a pause can keep what was left. */
let markReadDeadline = 0;
let markReadRemaining = 0;

function armMarkRead(ms: number) {
  if (markReadTimer) clearTimeout(markReadTimer);
  markReadDeadline = Date.now() + ms;
  markReadTimer = setTimeout(() => {
    pendingMarkRead.value = null;
    markReadTimer = null;
  }, ms);
}

/** Hold the undo while the pointer or focus is on its toast (WCAG 2.2.1). */
const pauseMarkRead = () => {
  if (!pendingMarkRead.value || !markReadTimer) return;
  markReadRemaining = Math.max(0, markReadDeadline - Date.now());
  clearTimeout(markReadTimer);
  markReadTimer = null;
};

const resumeMarkRead = () => {
  if (pendingMarkRead.value && !markReadTimer) armMarkRead(markReadRemaining);
};

const undoMarkRead = async () => {
  const pending = pendingMarkRead.value;
  if (!pending) return;
  pendingMarkRead.value = null;
  if (markReadTimer) {
    clearTimeout(markReadTimer);
    markReadTimer = null;
  }
  await feedService.markReadBulk(pending.ids, false);
  await loadData();
};

const handleMarkAllRead = () => markReadWithUndo(scopedSourceIds.value);

/**
 * Articles scrolled past in the list, when that is turned on.
 *
 * Batched: a flick of the wheel can put twenty rows behind you at once, and
 * one write plus two count queries per row would be sixty calls for a gesture.
 * The list updates immediately either way — the delay is only in the writing.
 */
const pendingScrollRead = new Set<string>();
let scrollReadTimer: ReturnType<typeof setTimeout> | null = null;

const flushScrollRead = async () => {
  scrollReadTimer = null;
  if (pendingScrollRead.size === 0) return;
  const ids = [...pendingScrollRead];
  pendingScrollRead.clear();

  try {
    await feedService.markReadBulk(ids, true);
    const [counts, views] = await Promise.all([
      feedService.getUnreadCounts(),
      feedService.getViewCounts(),
    ]);
    unreadCounts.value = counts;
    viewCounts.value = views;
  } catch (e) {
    logger.error('Failed to mark scrolled articles read', e);
  }
};

const handleMarkArticleRead = (articleId: string) => {
  const article = articles.value.find(a => a.id === articleId);
  if (!article || article.isRead) return;
  article.isRead = true;
  pendingScrollRead.add(articleId);
  if (scrollReadTimer) clearTimeout(scrollReadTimer);
  scrollReadTimer = setTimeout(flushScrollRead, 600);
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
const handleHighlightsToNote = (article: CachedArticle, highlights: Highlight[]) =>
  feedActions.highlightsToNote(article, highlights);
const handleQuickCapture = (article: CachedArticle) => feedActions.quickCapture(article);
const handleCreateTask = (article: CachedArticle) => feedActions.createTask(article);

const handleFeedAdded = async () => {
  showAddFeedModal.value = false;
  await loadData();
};

/**
 * Unsubscribing, held back for a few seconds rather than asked about.
 *
 * The feed leaves the sidebar (and its articles the list) at once; the real
 * removal happens when the undo window closes. Until then `loadData` keeps it
 * hidden, so a sync or a vault event in the meantime does not bring it back.
 */
/**
 * A set, not one id: a removal committing while the next one is already
 * waiting must not stop the lists hiding the second — a single id cleared by
 * the first commit's `finally` let a reload put the second feed back under
 * its own toast.
 */
const pendingRemovalIds = ref<Set<string>>(new Set());
const setRemovalPending = (id: string, on: boolean) => {
  const next = new Set(pendingRemovalIds.value);
  if (on) next.add(id);
  else next.delete(id);
  pendingRemovalIds.value = next;
};
const removal = useUndoableAction({
  onError: (e) => {
    logger.error('Failed to remove feed', e);
    showAppNotice(t('feeds.remove_failed'), 'error');
  },
});

const handleRemoveSource = async (sourceId: string) => {
  const source = sources.value.find(s => s.id === sourceId);
  const name = source?.title || sourceId;
  setRemovalPending(sourceId, true);
  sources.value = sources.value.filter(s => s.id !== sourceId);
  articles.value = articles.value.filter(a => a.feedSourceId !== sourceId);
  if (selectedArticle.value?.feedSourceId === sourceId) selectedArticle.value = null;
  if (selectedSourceId.value === sourceId) handleSelectSource(null);

  // Starting another removal commits the one before; `run` takes care of it.
  await removal.run(
    t('feeds.source_removed', { name }),
    async () => {
      try {
        await feedService.removeSource(sourceId);
      } finally {
        setRemovalPending(sourceId, false);
      }
      // The feed is gone; a reload that fails must not report the removal as
      // failed and put the feed back on screen.
      try {
        await loadData();
      } catch (e) {
        logger.error('Reload after removing a feed failed', e);
      }
    },
    () => {
      setRemovalPending(sourceId, false);
      void loadData();
    },
  );
};

const handleRenameSource = async (sourceId: string, newTitle: string) => {
  const source = sources.value.find(s => s.id === sourceId);
  if (source) {
    source.title = newTitle;
    await feedService.updateSource(source);
    await loadData();
  }
};

const handleToggleFullText = async (sourceId: string) => {
  const source = sources.value.find(s => s.id === sourceId);
  if (!source) return;
  source.fullTextFetch = !source.fullTextFetch;
  await feedService.updateSource(source);
  await loadData();
};

const handleSetScrapeContainer = async (sourceId: string, selector: string) => {
  const source = sources.value.find(s => s.id === sourceId);
  if (!source) return;
  source.scrapeContainer = selector;
  await feedService.updateSource(source);
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

const handleMarkSourceRead = (sourceId: string) => markReadWithUndo([sourceId]);

const handleImported = async () => {
  showImportExportModal.value = false;
  await loadData();
};

const handleMobileBack = () => {
  mobilePanel.value = 'list';
  selectedArticle.value = null;
};

// Changing the layout is a preference, not a session quirk.
const handleViewModeChange = async (mode: ViewMode) => {
  viewMode.value = mode;
  if (config.value.defaultView === mode) return;
  config.value = { ...config.value, defaultView: mode };
  try {
    await feedService.saveConfig(config.value);
  } catch (e) {
    logger.error('Failed to save view mode', e);
  }
};

const handleSortChange = async (sort: SortOrder) => {
  if (config.value.sortOrder === sort) return;
  config.value = { ...config.value, sortOrder: sort };
  resetPaging();
  await loadArticles();
  try {
    await feedService.saveConfig(config.value);
  } catch (e) {
    logger.error('Failed to save sort order', e);
  }
};

const handleRulesSaved = async () => {
  showRulesModal.value = false;
  await loadData();
};

const handleConfigSaved = async (updated: FeedConfig) => {
  config.value = updated;
  viewMode.value = updated.defaultView;
  showSettingsModal.value = false;
  await loadData();
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
  searchTimer = setTimeout(() => {
    resetPaging();
    loadArticles();
  }, 300);
};

// Keyboard shortcuts
const anyModalOpen = computed(
  () =>
    showAddFeedModal.value ||
    showImportExportModal.value ||
    showSettingsModal.value ||
    showRulesModal.value,
);

const handleKeyboard = (e: KeyboardEvent) => {
  // Don't trigger if typing in input
  if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
  // A dialog is in front: `j` belongs to it, not to the list behind it.
  if (anyModalOpen.value) return;
  
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
    case 'n': // Next unread
      navigateToNextUnread();
      break;
    case 'o': // Open original
      if (selectedArticle.value?.url) window.open(selectedArticle.value.url, '_blank');
      break;
    case '?':
      showShortcuts.value = !showShortcuts.value;
      break;
    case 'r': // Refresh
      if (!e.ctrlKey && !e.metaKey) handleRefresh();
      break;
    case 'Escape':
      if (showShortcuts.value) {
        showShortcuts.value = false;
        break;
      }
      selectedArticle.value = null;
      if (useMobileLayout.value) mobilePanel.value = 'list';
      break;
  }
};

/**
 * Jump to the next article below the current one that has not been read.
 *
 * Wraps to the start rather than stopping, because a reader working through a
 * backlog out of order still wants the next thing rather than nothing.
 */
const navigateToNextUnread = () => {
  if (articles.value.length === 0) return;
  const current = selectedArticle.value
    ? articles.value.findIndex(a => a.id === selectedArticle.value!.id)
    : -1;

  for (let step = 1; step <= articles.value.length; step++) {
    const candidate = articles.value[(current + step) % articles.value.length];
    if (!candidate.isRead) {
      handleSelectArticle(candidate);
      return;
    }
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
// Reconcile read state with the other devices, then reload what changed.
const reconcileReadState = async (force = false) => {
  try {
    const changed = await feedService.syncReadState(force);
    if (changed > 0) await loadData();
  } catch (e) {
    logger.error('Failed to sync read state', e);
  }
};

onMounted(async () => {
  await loadData();
  await reconcileReadState(true);

  // Feeds refreshed by the background scheduler, which runs whether or not
  // this view is mounted.
  const unlistenRefreshed = await listen('feeds:refreshed', () => {
    debouncedLoad(() => reconcileReadState(true));
  });

  bus.on('vault:file-modified', () => debouncedLoad(() => loadData()));
  bus.on('vault:file-created-deleted', () => debouncedLoad(() => loadData()));
  // A completed sync is when another device's read state can have arrived.
  bus.on('vault:sync-completed', () => debouncedLoad(() => reconcileReadState()));
  bus.on('node:updated', ({ nodeType }) => {
    if (nodeType === 'feed_source' || nodeType === 'feed_article') debouncedLoad(() => loadData());
  });

  // Keyboard shortcuts
  window.addEventListener('keydown', handleKeyboard);

  // Auto-cleanup on mount, to whatever the settings say
  const runConfiguredCleanup = () =>
    feedService
      .runCleanup(config.value.autoCleanupDays, config.value.maxArticlesPerFeed)
      .catch(() => {});
  runConfiguredCleanup();

  // Auto-refresh on mount (whatever is due)
  autoRefresh();

  // The background scheduler is started with the app, not with this view, so
  // it keeps running whether or not anyone is looking. This timer covers the
  // time the view is open, and on Android — where a background timer would
  // just be killed — it is the whole of the schedule.
  const refreshInterval = setInterval(autoRefresh, 5 * 60 * 1000);
  // Cleanup every 6 hours
  const cleanupInterval = setInterval(runConfiguredCleanup, 6 * 60 * 60 * 1000);
  onUnmounted(() => {
    window.removeEventListener('keydown', handleKeyboard);
    unlistenRefreshed();
    clearInterval(refreshInterval);
    clearInterval(cleanupInterval);
    if (markReadTimer) clearTimeout(markReadTimer);
    if (scrollReadTimer) {
      clearTimeout(scrollReadTimer);
      // Anything scrolled past in the last moment still counts as read.
      flushScrollRead().finally(() => feedService.syncReadState().catch(() => {}));
      return;
    }
    // Publish what was read here before the view goes away.
    feedService.syncReadState().catch(() => {});
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

/**
 * The article being read, for Syn — asked to summarise "this", it has to know
 * which. A function rather than a ref, so it is read at the moment a question
 * is asked. See `getCurrentArticle` in App.vue.
 */
const currentArticle = () =>
  selectedArticle.value
    ? { id: selectedArticle.value.id, title: selectedArticle.value.title || undefined }
    : null;

defineExpose({ openFeedById, openArticleById, currentArticle });
</script>

<template>
  <div class="flex-1 flex flex-col h-full bg-base dark:bg-base-dark overflow-hidden relative">
    <!-- Loading -->
    <div v-if="loading && sources.length === 0" class="absolute inset-0 flex items-center justify-center z-[100] bg-base/50 dark:bg-base-dark/50">
      <div class="animate-spin rounded-full h-8 w-8 border-b-2 border-accent"></div>
    </div>

    <AppHeader
      :title="t('feeds.title')"
      :subtitle="t('feeds.subtitle')"
      :icon="Rss"
      :sidebar-label="t('feeds.a11y_toggle_sidebar')"
      :primary-label="t('feeds.add_feed')"
      :primary-icon="Plus"
      @open-sidebar="isSidebarOpen = !isSidebarOpen"
      @primary="showAddFeedModal = true"
    >
      <template #actions>
        <button type="button" class="btn-icon disabled:opacity-50" :disabled="refreshing" :aria-label="t('feeds.refresh_all')" :title="t('feeds.refresh_all')" @click="handleRefresh">
          <RefreshCw class="w-5 h-5" :class="{ 'animate-spin': refreshing }" aria-hidden="true" />
        </button>
        <button type="button" class="btn-icon" :aria-label="t('feeds.rules')" :title="t('feeds.rules')" @click="showRulesModal = true">
          <Filter class="w-5 h-5" aria-hidden="true" />
        </button>
        <button type="button" class="btn-icon" :aria-label="t('feeds.settings')" :title="t('feeds.settings')" @click="showSettingsModal = true">
          <Settings class="w-5 h-5" aria-hidden="true" />
        </button>
      </template>
    </AppHeader>

    <!-- Main Content -->
    <div class="flex-1 flex gap-0 overflow-hidden">
      <template v-if="!useMobileLayout">
        <FeedsSidebar :sources="sources" :categories="categories" :unread-counts="unreadCounts" :view-counts="viewCounts" :selected-source-id="selectedSourceId" :selected-category-id="selectedCategoryId" :current-view="currentView" @select-source="handleSelectSource" @select-category="handleSelectCategory" @select-view="handleSelectView" @remove-source="handleRemoveSource" @rename-source="handleRenameSource" @open-opml="showImportExportModal = true" @pause-source="handlePauseSource" @mark-source-read="handleMarkSourceRead" @toggle-full-text="handleToggleFullText" @set-scrape-container="handleSetScrapeContainer" class="shrink-0 border-r border-border dark:border-border-dark" :style="{ width: sidebarWidth + 'px' }" />
        <div
          class="resize-handle"
          role="separator"
          aria-orientation="vertical"
          tabindex="0"
          :aria-label="t('feeds.resize_sidebar')"
          :aria-valuenow="sidebarWidth"
          :aria-valuemin="180"
          :aria-valuemax="400"
          @mousedown="startResize('sidebar', $event)"
          @keydown.left.prevent="nudgeResize('sidebar', -24)"
          @keydown.right.prevent="nudgeResize('sidebar', 24)"
        ><div class="resize-line"></div></div>
        <ArticleList :articles="articles" :selected-article="selectedArticle" :sources="sources" :search-query="searchQuery" :current-view="currentView" :refreshing="refreshing" :view-mode="viewMode" :has-more="hasMore" :loading-more="loadingMore" :sort-order="config.sortOrder" :mark-read-on-scroll="config.markReadOnScroll" @select-article="handleSelectArticle" @update:search-query="handleSearchUpdate" @update:view-mode="handleViewModeChange" @mark-all-read="handleMarkAllRead" @refresh="handleRefresh" @load-more="loadMoreArticles" @update:sort-order="handleSortChange" @mark-read="handleMarkArticleRead" @add-feed="showAddFeedModal = true" class="shrink-0 border-r border-border dark:border-border-dark" :style="{ width: articleListWidth + 'px' }" />
        <div
          class="resize-handle"
          role="separator"
          aria-orientation="vertical"
          tabindex="0"
          :aria-label="t('feeds.resize_article_list')"
          :aria-valuenow="articleListWidth"
          :aria-valuemin="280"
          :aria-valuemax="600"
          @mousedown="startResize('articleList', $event)"
          @keydown.left.prevent="nudgeResize('articleList', -24)"
          @keydown.right.prevent="nudgeResize('articleList', 24)"
        ><div class="resize-line"></div></div>
        <ArticleReader :article="selectedArticle" :config="config" :sources="sources" @toggle-star="handleToggleStar" @toggle-read-later="handleToggleReadLater" @clip-to-note="handleClipToNote" @quick-capture="handleQuickCapture" @create-task="handleCreateTask" @article-updated="handleArticleUpdated" @highlights-to-note="handleHighlightsToNote" class="flex-1 min-w-0" />
      </template>
      <template v-else>
        <ArticleList v-if="mobilePanel === 'list'" :articles="articles" :selected-article="selectedArticle" :sources="sources" :search-query="searchQuery" :current-view="currentView" :refreshing="refreshing" :view-mode="viewMode" :has-more="hasMore" :loading-more="loadingMore" :sort-order="config.sortOrder" :mark-read-on-scroll="config.markReadOnScroll" @select-article="handleSelectArticle" @update:search-query="handleSearchUpdate" @update:view-mode="handleViewModeChange" @mark-all-read="handleMarkAllRead" @refresh="handleRefresh" @load-more="loadMoreArticles" @update:sort-order="handleSortChange" @mark-read="handleMarkArticleRead" @add-feed="showAddFeedModal = true" class="flex-1" />
        <ArticleReader v-else :article="selectedArticle" :config="config" :sources="sources" :show-back-button="true" @back="handleMobileBack" @toggle-star="handleToggleStar" @toggle-read-later="handleToggleReadLater" @clip-to-note="handleClipToNote" @quick-capture="handleQuickCapture" @create-task="handleCreateTask" @article-updated="handleArticleUpdated" @highlights-to-note="handleHighlightsToNote" class="flex-1" />
      
        <!-- Mobile Sidebar Drawer -->
        <div v-if="isSidebarOpen" class="fixed inset-0 z-50 flex">
            <div class="absolute inset-0 bg-black/40 backdrop-blur-sm" @click="isSidebarOpen = false"></div>
            <div class="relative w-[280px] bg-base dark:bg-base-dark flex flex-col shadow-2xl h-full border-r border-border dark:border-border-dark" @click.stop>
               <div class="flex items-center justify-between p-4 shrink-0 border-b border-border dark:border-border-dark">
                    <span class="font-bold text-lg text-text dark:text-text-dark">{{ t('feeds.menu_title') }}</span>
                    <button @click="isSidebarOpen = false" class="btn-icon -mr-2" :aria-label="t('feeds.a11y_close')" :title="t('feeds.a11y_close')">
                        <X class="w-5 h-5" />
                    </button>
               </div>
               <FeedsSidebar 
                 class="flex-1 overflow-y-auto"
                 :sources="sources" 
                 :categories="categories" 
                 :unread-counts="unreadCounts" 
                 :view-counts="viewCounts" 
                 :selected-source-id="selectedSourceId" 
                 :selected-category-id="selectedCategoryId" 
                 :current-view="currentView" 
                 @select-source="handleSelectSourceMobile" 
                 @select-category="handleSelectCategoryMobile" 
                 @select-view="handleSelectViewMobile" 
                 @remove-source="handleRemoveSource" 
                 @rename-source="handleRenameSource" 
                 @open-opml="showImportExportModal = true" 
                 @pause-source="handlePauseSource" 
                 @mark-source-read="handleMarkSourceRead" @toggle-full-text="handleToggleFullText" @set-scrape-container="handleSetScrapeContainer"
               />
            </div>
        </div>

      </template>
    </div>

    <AddFeedModal v-if="showAddFeedModal" :categories="categories" @close="showAddFeedModal = false" @added="handleFeedAdded" />
    <ImportExportModal v-if="showImportExportModal" @close="showImportExportModal = false" @imported="handleImported" />

    <!-- The few seconds in which an unsubscribe can still be taken back -->
    <UndoToast
      :show="removal.show.value"
      :restart-key="removal.key.value"
      :message="removal.message.value"
      :undo-label="t('common.undo')"
      :seconds="removal.seconds"
      @undo="removal.undo"
      @pause="removal.pause"
      @resume="removal.resume"
    />

    <!-- The few seconds in which a bulk mark-read can still be taken back -->
    <UndoToast
      :show="!!pendingMarkRead"
      :restartKey="pendingMarkRead?.ids[0]"
      :message="t('feeds.marked_read_toast', { count: pendingMarkRead?.ids.length || 0 })"
      :undoLabel="t('feeds.undo')"
      :seconds="UNDO_SECONDS"
      @undo="undoMarkRead"
      @pause="pauseMarkRead"
      @resume="resumeMarkRead"
    />

    <FeedErrorToast :errors="refreshErrors" @dismiss="refreshErrors = []" />

    <FeedsShortcutsHelp :show="showShortcuts" @close="showShortcuts = false" />

    <FeedsRulesModal
      v-if="showRulesModal"
      :sources="sources"
      @close="showRulesModal = false"
      @saved="handleRulesSaved"
    />

    <FeedsSettingsModal
      v-if="showSettingsModal"
      :config="config"
      @close="showSettingsModal = false"
      @saved="handleConfigSaved"
    />
  </div>
</template>

<style scoped>
.resize-handle {
  width: 4px;
  outline: none;
  position: relative;
  cursor: col-resize;
  flex-shrink: 0;
  z-index: 10;
  display: flex;
  align-items: center;
  justify-content: center;
}

/*
  The line stays 4px; what can be grabbed does not. An invisible strip either
  side makes the target 12px, and 24px on a touch screen, without moving the
  panes apart.
*/
.resize-handle::before {
  content: '';
  position: absolute;
  inset-block: 0;
  inset-inline: -4px;
}

@media (pointer: coarse) {
  .resize-handle::before {
    inset-inline: -10px;
  }
}

.resize-handle:hover .resize-line,
.resize-handle:active .resize-line,
.resize-handle:focus-visible .resize-line {
  opacity: 1;
}

.resize-line {
  width: 2px;
  height: 100%;
  border-radius: 1px;
  background-color: var(--color-accent);
  opacity: 0;
  transition: opacity 0.15s ease;
}
</style>
