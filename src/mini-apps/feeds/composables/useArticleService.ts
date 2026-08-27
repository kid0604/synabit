import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '../../../stores/useAppStore';
import { useEventBus } from '../../../composables/useEventBus';
import { storeToRefs } from 'pinia';
import type { FeedSource, FeedCategory, FeedConfig, CachedArticle, ArticleFilter, DiscoveredFeed, ViewCounts, OpmlImportResult, RefreshResult, Highlight, FeedRule } from '../types/feed.types';
import { DEFAULT_CONFIG } from '../types/feed.types';

/**
 * Midnight this morning where the reader is, as an instant.
 *
 * The database stores publication times with whatever offset the publisher
 * used, so "today" can only be decided by someone who knows the reader's
 * timezone — which the backend does not.
 */
export function localMidnight(): string {
  const midnight = new Date();
  midnight.setHours(0, 0, 0, 0);
  return midnight.toISOString();
}

export function useArticleService() {
  const appStore = useAppStore();
  const { vaultPath } = storeToRefs(appStore);
  const bus = useEventBus();

  // Sources
  async function getSources(): Promise<FeedSource[]> {
    return await invoke<FeedSource[]>('feed_get_sources', { vaultPath: vaultPath.value });
  }

  async function addSource(url: string, categoryId: string): Promise<FeedSource> {
    const source = await invoke<FeedSource>('feed_add_source', { vaultPath: vaultPath.value, url, categoryId });
    bus.emit('node:created', { nodeType: 'feed_source', id: source.id, title: source.title });
    return source;
  }

  async function removeSource(sourceId: string): Promise<void> {
    await invoke('feed_remove_source', { vaultPath: vaultPath.value, sourceId });
    bus.emit('node:deleted', { nodeType: 'feed_source', id: sourceId });
  }

  async function updateSource(source: FeedSource): Promise<void> {
    await invoke('feed_update_source', { vaultPath: vaultPath.value, source });
    bus.emit('node:updated', { nodeType: 'feed_source', id: source.id, title: source.title });
  }

  // Categories
  async function getCategories(): Promise<FeedCategory[]> {
    return await invoke<FeedCategory[]>('feed_get_categories', { vaultPath: vaultPath.value });
  }

  async function saveCategories(categories: FeedCategory[]): Promise<void> {
    await invoke('feed_save_categories', { vaultPath: vaultPath.value, categories });
  }

  // Config
  async function getConfig(): Promise<FeedConfig> {
    try {
      return await invoke<FeedConfig>('feed_get_config', { vaultPath: vaultPath.value });
    } catch {
      return { ...DEFAULT_CONFIG };
    }
  }

  async function saveConfig(config: FeedConfig): Promise<void> {
    await invoke('feed_save_config', { vaultPath: vaultPath.value, config });
  }

  // Articles
  async function getArticles(filter: ArticleFilter): Promise<CachedArticle[]> {
    return await invoke<CachedArticle[]>('feed_get_articles', { filter });
  }

  async function searchArticles(query: string, sourceIds?: string[], limit?: number): Promise<CachedArticle[]> {
    return await invoke<CachedArticle[]>('feed_search_articles', { query, sourceIds, limit: limit || 50 });
  }

  async function getViewCounts(): Promise<ViewCounts> {
    return await invoke<ViewCounts>('feed_get_view_counts', { todayStart: localMidnight() });
  }

  async function getUnreadCounts(): Promise<Record<string, number>> {
    return await invoke<Record<string, number>>('feed_get_unread_counts', { vaultPath: vaultPath.value });
  }

  // Article actions
  async function markRead(articleId: string, read: boolean): Promise<void> {
    await invoke('feed_mark_read', { articleId, read });
    bus.emit('node:updated', { nodeType: 'feed_article', id: articleId, title: '' });
  }

  /** Returns the articles this call changed, so the caller can offer an undo. */
  async function markAllRead(sourceIds?: string[]): Promise<string[]> {
    const changed = await invoke<string[]>('feed_mark_all_read', { sourceIds });
    bus.emit('node:updated', { nodeType: 'feed_article', id: 'all', title: '' });
    return changed;
  }

  async function markReadBulk(articleIds: string[], read: boolean): Promise<void> {
    await invoke('feed_mark_read_bulk', { articleIds, read });
    bus.emit('node:updated', { nodeType: 'feed_article', id: 'all', title: '' });
  }

  async function toggleStar(articleId: string): Promise<void> {
    await invoke('feed_toggle_star', { vaultPath: vaultPath.value, articleId });
    bus.emit('node:updated', { nodeType: 'feed_article', id: articleId, title: '' });
  }

  async function toggleReadLater(articleId: string): Promise<void> {
    await invoke('feed_toggle_read_later', { vaultPath: vaultPath.value, articleId });
    bus.emit('node:updated', { nodeType: 'feed_article', id: articleId, title: '' });
  }

  // Feed operations
  /**
   * `manual` marks a fetch the reader asked for, which ignores the backoff a
   * failing feed is under. `dueOnly` asks for just the feeds whose own
   * interval has elapsed, which is what a timer wants. Which feeds are due is
   * decided in the backend, so the scheduler and this call agree.
   */
  async function refreshFeeds(
    sourceId?: string,
    manual = false,
    dueOnly = false,
  ): Promise<RefreshResult> {
    const result = await invoke<RefreshResult>('feed_refresh', {
      vaultPath: vaultPath.value, sourceId, manual, dueOnly,
    });
    bus.emit('feed:refreshed', { sourceId });
    return result;
  }

  /** The full row, including the article body the list only carries a slice of. */
  async function getArticle(articleId: string): Promise<CachedArticle> {
    return await invoke<CachedArticle>('feed_get_article', { articleId });
  }

  /**
   * Publish this device's read state and take in the other devices'.
   * `force` re-applies even when no file has changed — needed after a refresh,
   * because state waiting for an article can only be applied once it exists.
   */
  async function syncReadState(force = false): Promise<number> {
    return await invoke<number>('feed_state_sync', { vaultPath: vaultPath.value, force });
  }


  async function discoverFeeds(url: string): Promise<DiscoveredFeed[]> {
    return await invoke<DiscoveredFeed[]>('feed_discover', { vaultPath: vaultPath.value, url });
  }

  /** Defaults mirror `FeedConfig`, so a caller without config still matches it. */
  async function runCleanup(
    maxAgeDays = DEFAULT_CONFIG.autoCleanupDays,
    maxPerFeed = DEFAULT_CONFIG.maxArticlesPerFeed,
  ): Promise<void> {
    await invoke('feed_run_cleanup', { maxAgeDays, maxPerFeed });
  }

  /** `force` re-extracts an article that already has a body. */
  async function fetchArticleContent(articleId: string, force = false): Promise<CachedArticle> {
    return await invoke<CachedArticle>('feed_fetch_article_content', { articleId, force });
  }

  // Highlights
  async function getHighlights(articleId: string): Promise<Highlight[]> {
    return await invoke<Highlight[]>('feed_get_highlights', { articleId });
  }

  async function addHighlight(articleId: string, text: string, occurrence: number, note?: string): Promise<Highlight> {
    return await invoke<Highlight>('feed_add_highlight', { articleId, text, occurrence, note });
  }

  async function removeHighlight(highlightId: string): Promise<void> {
    await invoke('feed_remove_highlight', { highlightId });
  }

  // Rules
  async function getRules(): Promise<FeedRule[]> {
    return await invoke<FeedRule[]>('feed_get_rules', { vaultPath: vaultPath.value });
  }

  async function saveRules(rules: FeedRule[]): Promise<void> {
    await invoke('feed_save_rules', { vaultPath: vaultPath.value, rules });
  }

  /** Run the rules over articles already cached. Muting is not applied here. */
  async function applyRules(): Promise<number> {
    return await invoke<number>('feed_apply_rules', { vaultPath: vaultPath.value });
  }

  // OPML
  async function importOpml(opmlContent: string): Promise<OpmlImportResult> {
    const result = await invoke<OpmlImportResult>('feed_import_opml', { vaultPath: vaultPath.value, opmlContent });
    bus.emit('node:created', { nodeType: 'feed_source', id: 'opml-import', title: 'OPML Import' });
    return result;
  }

  async function exportOpml(): Promise<string> {
    return await invoke<string>('feed_export_opml', { vaultPath: vaultPath.value });
  }

  return {
    getSources, addSource, removeSource, updateSource,
    getCategories, saveCategories,
    getConfig, saveConfig,
    getArticles, getArticle, searchArticles, getUnreadCounts, getViewCounts,
    markRead, markAllRead, markReadBulk, toggleStar, toggleReadLater,
    refreshFeeds, discoverFeeds, runCleanup, syncReadState,
    getHighlights, addHighlight, removeHighlight,
    getRules, saveRules, applyRules,
    fetchArticleContent,
    importOpml, exportOpml,
  };
}
