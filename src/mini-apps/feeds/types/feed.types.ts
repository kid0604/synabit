export interface FeedSource {
  id: string;
  url: string;
  siteUrl: string;
  feedType: 'rss' | 'atom' | 'json' | 'youtube' | 'reddit';
  title: string;
  description: string;
  iconUrl: string;
  categoryId: string;
  updateInterval: number;
  isPaused: boolean;
  addedAt: string;
  lastFetchedAt: string;
  lastError: string | null;
  etag: string | null;
  lastModifiedHeader: string | null;
}

export interface FeedCategory {
  id: string;
  name: string;
  color: string;
  sortOrder: number;
  isCollapsed: boolean;
}

export interface FeedConfig {
  defaultView: 'magazine' | 'cards' | 'titles';
  showReadArticles: boolean;
  markReadOnScroll: boolean;
  autoCleanupDays: number;
  maxArticlesPerFeed: number;
  globalUpdateInterval: number;
  readingFontSize: number;
  readingMaxWidth: number;
}

export interface CachedArticle {
  id: string;
  feedSourceId: string;
  guid: string;
  title: string;
  url: string;
  author: string;
  content: string;
  summary: string;
  publishedAt: string;
  fetchedAt: string;
  thumbnailUrl: string;
  wordCount: number;
  readTimeMinutes: number;
  contentType: 'article' | 'video' | 'reddit_post';
  isRead: boolean;
  isStarred: boolean;
  isReadLater: boolean;
}

export interface ArticleFilter {
  sourceId?: string;
  categoryId?: string;
  view: 'today' | 'all' | 'starred' | 'read-later' | 'unread';
  search?: string;
  limit?: number;
  offset?: number;
}

export interface DiscoveredFeed {
  url: string;
  title: string;
  feedType: string;
}

export const DEFAULT_CONFIG: FeedConfig = {
  defaultView: 'magazine',
  showReadArticles: false,
  markReadOnScroll: true,
  autoCleanupDays: 14,
  maxArticlesPerFeed: 200,
  globalUpdateInterval: 60,
  readingFontSize: 16,
  readingMaxWidth: 720,
};
