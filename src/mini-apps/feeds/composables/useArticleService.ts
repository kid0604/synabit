import { invoke } from '@tauri-apps/api/core';
import { useAppStore } from '../../../stores/useAppStore';
import { useEventBus } from '../../../composables/useEventBus';
import { storeToRefs } from 'pinia';
import type { FeedSource, FeedCategory, FeedConfig, CachedArticle, ArticleFilter, DiscoveredFeed } from '../types/feed.types';
import { DEFAULT_CONFIG } from '../types/feed.types';

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
    return await invoke<CachedArticle[]>('feed_get_articles', { vaultPath: vaultPath.value, filter });
  }

  async function searchArticles(query: string, limit?: number): Promise<CachedArticle[]> {
    return await invoke<CachedArticle[]>('feed_search_articles', { vaultPath: vaultPath.value, query, limit: limit || 50 });
  }

  async function getUnreadCounts(): Promise<Record<string, number>> {
    return await invoke<Record<string, number>>('feed_get_unread_counts', { vaultPath: vaultPath.value });
  }

  async function getTotalUnread(): Promise<number> {
    return await invoke<number>('feed_get_total_unread', { vaultPath: vaultPath.value });
  }

  // Article actions
  async function markRead(articleId: string, read: boolean): Promise<void> {
    await invoke('feed_mark_read', { articleId, read });
    bus.emit('node:updated', { nodeType: 'feed_article', id: articleId, title: '' });
  }

  async function markAllRead(sourceId?: string, categoryId?: string): Promise<void> {
    await invoke('feed_mark_all_read', { vaultPath: vaultPath.value, sourceId, categoryId });
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
  async function refreshFeeds(sourceId?: string): Promise<void> {
    await invoke('feed_refresh', { vaultPath: vaultPath.value, sourceId });
    bus.emit('feed:refreshed', { sourceId });
  }

  async function discoverFeeds(url: string): Promise<DiscoveredFeed[]> {
    return await invoke<DiscoveredFeed[]>('feed_discover', { vaultPath: vaultPath.value, url });
  }

  async function runCleanup(maxAgeDays = 14, maxPerFeed = 200): Promise<void> {
    await invoke('feed_run_cleanup', { maxAgeDays, maxPerFeed });
  }

  // OPML
  async function importOpml(opmlContent: string): Promise<any[]> {
    const feeds = await invoke<any[]>('feed_import_opml', { vaultPath: vaultPath.value, opmlContent });
    bus.emit('node:created', { nodeType: 'feed_source', id: 'opml-import', title: 'OPML Import' });
    return feeds;
  }

  async function exportOpml(): Promise<string> {
    return await invoke<string>('feed_export_opml', { vaultPath: vaultPath.value });
  }

  return {
    getSources, addSource, removeSource, updateSource,
    getCategories, saveCategories,
    getConfig, saveConfig,
    getArticles, searchArticles, getUnreadCounts, getTotalUnread,
    markRead, markAllRead, toggleStar, toggleReadLater,
    refreshFeeds, discoverFeeds, runCleanup,
    importOpml, exportOpml,
  };
}
