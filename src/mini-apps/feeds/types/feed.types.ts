/**
 * What the backend writes into `feedType`. `youtube` and `reddit` sat here for
 * a long time without anything ever producing them; the adapters that derive a
 * feed address from a channel or subreddit URL now do.
 */
export type FeedType = 'rss' | 'atom' | 'json' | 'youtube' | 'reddit' | 'scrape' | 'unknown';

export interface FeedSource {
  id: string;
  url: string;
  siteUrl: string;
  feedType: FeedType;
  title: string;
  description: string;
  iconUrl: string;
  categoryId: string;
  updateInterval: number;
  isPaused: boolean;
  addedAt: string;
  lastFetchedAt: string;
  /**
   * Both of these come from this device's database, not from the vault. The
   * ETag and last-modified header the fetch uses stay in the backend
   * entirely — the front end never had a use for them, and keeping them out
   * of the synced file is the point of the split.
   */
  lastError: string | null;
  /** Fetch the article page rather than trusting the feed's own excerpt. */
  fullTextFetch: boolean;
  /** A CSS selector naming the article cards, for a scraped site the built-in
   *  guesses do not fit. Empty means use the guesses. */
  scrapeContainer: string;
}

export interface FeedCategory {
  id: string;
  name: string;
  color: string;
  sortOrder: number;
  isCollapsed: boolean;
}

export type ViewMode = 'magazine' | 'cards' | 'titles';

export type SortOrder = 'newest' | 'oldest';

export interface FeedConfig {
  defaultView: ViewMode;
  sortOrder: SortOrder;
  /** Mark an article read once it has scrolled past the top of the list. */
  markReadOnScroll: boolean;
  autoCleanupDays: number;
  maxArticlesPerFeed: number;
  /** Minutes between checks for a newly added feed. */
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
  /**
   * Free text, not a closed set: feeds supply their own MIME type. The app
   * writes `scrape` for a card lifted off a homepage and `full-text` once the
   * article's own page has been extracted.
   */
  contentType: string;
  isRead: boolean;
  isStarred: boolean;
  isReadLater: boolean;
  /** Tags a rule attached when the article arrived. */
  tags: string[];
}

export type FeedView = 'today' | 'unread' | 'all' | 'starred' | 'read-later';

export interface ArticleFilter {
  /**
   * Which feeds to draw from; omitted means all of them. A category selection
   * is resolved to its member feeds here, on the side that knows which feed
   * sits in which category.
   */
  sourceIds?: string[];
  view: FeedView;
  /** Local midnight as an instant; only the client knows the reader's zone. */
  todayStart?: string;
  sort?: SortOrder;
  search?: string;
  limit?: number;
  offset?: number;
}

/** Unread counts behind each saved view in the sidebar. */
export interface ViewCounts {
  today: number;
  unread: number;
  starred: number;
  readLater: number;
}

/** What one pass of `feed_refresh` did, including which feeds refused. */
export interface RefreshResult {
  totalFetched: number;
  totalNew: number;
  errors: string[];
}

export interface OpmlImportResult {
  added: number;
  skipped: number;
  categoriesCreated: number;
}

/** A passage the reader marked in an article. */
export interface Highlight {
  id: string;
  sourceId: string;
  guid: string;
  text: string;
  /** Which occurrence of `text` in the article, counting from zero. */
  occurrence: number;
  note: string;
  createdAt: string;
}

/** A standing instruction about arriving articles. */
export interface FeedRule {
  id: string;
  name: string;
  enabled: boolean;
  /** Which feeds it applies to; empty means all of them. */
  sourceIds: string[];
  field: 'any' | 'title' | 'summary' | 'author';
  contains: string;
  markRead: boolean;
  star: boolean;
  /** Drop the article rather than storing it. */
  mute: boolean;
  /** A tag to attach; empty attaches none. */
  tag: string;
}

export interface DiscoveredFeed {
  url: string;
  title: string;
  feedType: string;
}

/**
 * Mirrors `FeedConfig::default()` on the Rust side, which is the source of
 * truth. The two used to disagree on every value, so what you got depended on
 * whether the config file had been written yet.
 */
export const DEFAULT_CONFIG: FeedConfig = {
  defaultView: 'magazine',
  sortOrder: 'newest',
  markReadOnScroll: false,
  autoCleanupDays: 30,
  maxArticlesPerFeed: 500,
  globalUpdateInterval: 30,
  readingFontSize: 16,
  readingMaxWidth: 720,
};
