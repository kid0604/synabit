<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue';
import { useI18n } from 'vue-i18n';
import { ArrowLeft, Rss, Highlighter, Trash2 } from 'lucide-vue-next';
import { logger } from '../../../utils/logger';
import ReaderToolbar from './ReaderToolbar.vue';
import { useArticleService } from '../composables/useArticleService';
import { useImageCache } from '../composables/useImageCache';
import { applyHighlights, findMark, occurrenceOfSelection } from '../composables/useHighlights';
import type { Highlight } from '../types/feed.types';
import type { CachedArticle, FeedConfig, FeedSource } from '../types/feed.types';

const props = defineProps<{
  article: CachedArticle | null;
  config: FeedConfig;
  sources: FeedSource[];
  showBackButton?: boolean;
}>();

const emit = defineEmits<{
  'highlights-to-note': [article: CachedArticle, highlights: Highlight[]];
  'toggle-star': [id: string];
  'toggle-read-later': [id: string];
  'clip-to-note': [article: CachedArticle];
  'quick-capture': [article: CachedArticle];
  'create-task': [article: CachedArticle];
  'back': [];
  'article-updated': [article: CachedArticle];
}>();

const { t } = useI18n();
const feedService = useArticleService();
const imageCache = useImageCache();
const contentRef = ref<HTMLElement | null>(null);

/**
 * The article body with its images pointed at the local cache.
 *
 * Held separately from `article.content` because the rewrite is asynchronous —
 * the pictures have to be fetched before their paths exist — and rendering the
 * original markup in the meantime would send the very requests this avoids.
 */
const renderedContent = ref('');
const renderingContent = ref(false);

// ── Highlights ───────────────────────────────────────────────────────
const highlights = ref<Highlight[]>([]);

/** Where to put the little toolbar, and what it would act on. */
const selection = ref<{ text: string; occurrence: number; x: number; y: number } | null>(null);

const loadHighlights = async () => {
  const article = props.article;
  if (!article) {
    highlights.value = [];
    return;
  }
  try {
    const found = await feedService.getHighlights(article.id);
    if (props.article?.id === article.id) highlights.value = found;
  } catch (e) {
    logger.error('Failed to load highlights', e);
  }
};

/**
 * Draw the stored highlights over the article that is on screen.
 *
 * Runs after every render of the body, because `v-html` rebuilds the markup
 * from scratch each time — which is also what clears the previous marks, so
 * there is nothing to undo first.
 */
const paintHighlights = () => {
  if (contentRef.value && highlights.value.length > 0) {
    applyHighlights(contentRef.value, highlights.value);
  }
};

const clearSelection = () => {
  selection.value = null;
};

/**
 * Offer to highlight whatever was just selected.
 *
 * The offer is anchored to the end of the selection rather than the middle of
 * it: a paragraph-long highlight would otherwise put the button somewhere the
 * pointer is not.
 */
const handleSelectionChange = () => {
  const active = window.getSelection();
  if (!active || active.isCollapsed || active.rangeCount === 0) {
    clearSelection();
    return;
  }

  const range = active.getRangeAt(0);
  const root = contentRef.value;
  if (!root || !root.contains(range.commonAncestorContainer)) {
    clearSelection();
    return;
  }

  const text = active.toString().trim();
  if (text.length < 2) {
    clearSelection();
    return;
  }

  const rects = range.getClientRects();
  const last = rects[rects.length - 1];
  if (!last) {
    clearSelection();
    return;
  }

  selection.value = {
    text,
    occurrence: occurrenceOfSelection(root, range, text),
    x: last.right,
    y: last.bottom,
  };
};

const saveHighlight = async () => {
  const pending = selection.value;
  const article = props.article;
  if (!pending || !article) return;
  clearSelection();
  window.getSelection()?.removeAllRanges();

  try {
    const created = await feedService.addHighlight(article.id, pending.text, pending.occurrence);
    highlights.value = [...highlights.value, created];
    paintHighlights();
  } catch (e) {
    logger.error('Failed to save highlight', e);
  }
};

/** Clicking a mark takes it away again. */
const removeHighlightAt = async (target: HTMLElement) => {
  const id = target.dataset.highlightId;
  if (!id) return;
  try {
    await feedService.removeHighlight(id);
    highlights.value = highlights.value.filter(h => h.id !== id);
    // Unwrap in place rather than re-rendering the whole article, then heal
    // the seam: an unwrapped mark leaves the sentence split across sibling
    // text nodes, and a passage spanning that seam would stop being findable.
    const parent = target.parentNode;
    target.replaceWith(...Array.from(target.childNodes));
    parent?.normalize();
  } catch (e) {
    logger.error('Failed to remove highlight', e);
  }
};

/** Removing from the list below, where there is no mark to click. */
const removeHighlightById = async (id: string) => {
  try {
    await feedService.removeHighlight(id);
    highlights.value = highlights.value.filter(h => h.id !== id);
    const mark = contentRef.value ? findMark(contentRef.value, id) : null;
    if (mark) {
      const parent = mark.parentNode;
      mark.replaceWith(...Array.from(mark.childNodes));
      parent?.normalize();
    }
  } catch (e) {
    logger.error('Failed to remove highlight', e);
  }
};

const sendHighlightsToNote = () => {
  if (props.article && highlights.value.length > 0) {
    emit('highlights-to-note', props.article, highlights.value);
  }
};

const updateRenderedContent = async () => {
  const article = props.article;
  const source = article?.content || article?.summary || '';
  if (!source) {
    renderedContent.value = '';
    return;
  }
  renderingContent.value = true;
  try {
    const rewritten = await imageCache.rewriteImages(source);
    // Another article may have been opened while the images were fetched.
    if (props.article?.id === article?.id) {
      renderedContent.value = rewritten;
      nextTick(paintHighlights);
    }
  } finally {
    if (props.article?.id === article?.id) renderingContent.value = false;
  }
};

// Watches the body rather than the id: extracting the full text replaces the
// content of the article already on screen.
watch(() => [props.article?.id, props.article?.content, props.article?.summary], updateRenderedContent, {
  immediate: true,
});
const readingProgress = ref(0);
const loadingContent = ref(false);

const articleSource = computed(() =>
  props.article ? props.sources.find(s => s.id === props.article!.feedSourceId) ?? null : null
);

const sourceName = computed(() => articleSource.value?.title || '');

/**
 * Whether to go and fetch the article's own page.
 *
 * An article with no body has nothing to show and must be fetched. An article
 * with a body is only worth re-fetching if its feed is one of the many that
 * publish a teaser, and the reader has said so. `full-text` is the mark left
 * by a previous attempt — successful or not — and stops the reader trying
 * again on every open.
 */
/** An `<img>` with no source at all — a picture the article cannot show. */
const IMAGE_WITHOUT_SOURCE = /<img(?![^>]*\ssrc=)[^>]*>/i;

/**
 * Articles already re-fetched in this session in the hope of repairing them.
 *
 * The repair is driven by a condition the article itself still satisfies if
 * the second extraction is no better — a page behind a paywall, say — so
 * without this it would fetch the page again every time the article was
 * opened. Once per session is enough to find out.
 */
const repairAttempted = new Set<string>();

const needsExtraction = computed(() => {
  const article = props.article;
  if (!article) return false;

  // Articles extracted before lazy-loaded images were understood have figures
  // with captions and no picture. That is detectable, and re-extracting fixes
  // it — so an article repairs itself the first time it is opened, once, and
  // only if it is actually affected.
  if (
    article.content &&
    !repairAttempted.has(article.id) &&
    IMAGE_WITHOUT_SOURCE.test(article.content)
  ) {
    return true;
  }

  if (article.contentType === 'full-text') return false;
  if (!article.content) return true;
  return !!articleSource.value?.fullTextFetch;
});

const formattedDate = computed(() => {
  if (!props.article?.publishedAt) return '';
  return new Date(props.article.publishedAt).toLocaleDateString(undefined, {
    year: 'numeric', month: 'long', day: 'numeric', hour: '2-digit', minute: '2-digit'
  });
});

/**
 * Where the reader had got to in each article, for this session.
 *
 * Going back to something half-read and landing at the top again is the small
 * indignity every reader without this has. Module-level so it survives the
 * component being torn down and rebuilt when the layout switches between the
 * desktop and mobile panes.
 */
const readingPositions = new Map<string, number>();

const handleScroll = () => {
  if (!contentRef.value) return;
  const el = contentRef.value;
  const scrollable = el.scrollHeight - el.clientHeight;
  readingProgress.value = scrollable > 0 ? Math.min(100, (el.scrollTop / scrollable) * 100) : 100;
  if (props.article) readingPositions.set(props.article.id, el.scrollTop);
  // The offer is anchored to a place on screen; scrolling moves the words out
  // from under it.
  if (selection.value) clearSelection();
};

const openOriginal = () => {
  if (props.article?.url) {
    openExternal(props.article.url);
  }
};

// Open external URLs in default browser
const openExternal = async (url: string) => {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('open_url', { url });
  } catch {
    window.open(url, '_blank');
  }
};

// Intercept link clicks in article content
const handleContentClick = (e: MouseEvent) => {
  const mark = (e.target as HTMLElement)?.closest('mark[data-highlight-id]');
  if (mark) {
    e.preventDefault();
    removeHighlightAt(mark as HTMLElement);
    return;
  }

  const target = (e.target as HTMLElement)?.closest('a');
  if (!target) return;
  const href = target.getAttribute('href');
  if (href && (href.startsWith('http://') || href.startsWith('https://'))) {
    e.preventDefault();
    e.stopPropagation();
    openExternal(href);
  }
};



watch(() => props.article?.id, async id => {
  readingProgress.value = 0;
  const resume = id ? readingPositions.get(id) ?? 0 : 0;
  nextTick(() => {
    if (contentRef.value) contentRef.value.scrollTop = resume;
  });

  clearSelection();
  await loadHighlights();
  nextTick(paintHighlights);
  // Force when the body is already there but unusable: the plain path leaves
  // an article that has content alone.
  if (needsExtraction.value) {
    const repairing = !!props.article?.content;
    if (repairing && props.article) repairAttempted.add(props.article.id);
    await extractArticle(repairing);
  }
});

const extractArticle = async (force: boolean) => {
  const article = props.article;
  if (!article || loadingContent.value) return;
  loadingContent.value = true;
  try {
    emit('article-updated', await feedService.fetchArticleContent(article.id, force));
  } catch (e) {
    logger.error('Failed to fetch article content', e);
  } finally {
    loadingContent.value = false;
  }
};

/** The reader asking for the full article by hand, whatever the feed sent. */
const fetchFullText = () => extractArticle(true);
</script>

<template>
  <div class="flex flex-col h-full bg-base dark:bg-base-dark">
    <!-- Empty state -->
    <div v-if="!article" class="flex flex-col items-center justify-center h-full text-center px-6">
      <div class="w-20 h-20 rounded-2xl bg-gray-100 dark:bg-gray-800 flex items-center justify-center mb-4">
        <Rss class="w-10 h-10 text-gray-300 dark:text-gray-600" />
      </div>
      <p class="text-lg font-medium text-gray-500 dark:text-gray-400 mb-1">{{ t('feeds.empty_reader') }}</p>
      <p class="text-sm text-gray-400 dark:text-gray-500">{{ t('feeds.select_article_to_read') }}</p>
    </div>

    <!-- Article content -->
    <template v-else>
      <!-- Reading progress bar -->
      <div class="h-0.5 bg-gray-100 dark:bg-gray-800 shrink-0">
        <div class="h-full bg-orange-500 transition-all duration-150 ease-out" :style="{ width: readingProgress + '%' }"></div>
      </div>

      <!-- Toolbar -->
      <div class="shrink-0 border-b border-border dark:border-border-dark">
        <div class="flex items-center gap-2 px-4 py-2">
          <button v-if="showBackButton" @click="emit('back')" class="p-1.5 rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors mr-1" :aria-label="t('feeds.a11y_back_to_list')">
            <ArrowLeft class="w-5 h-5" />
          </button>
          <ReaderToolbar
            :article="article"
            @toggle-star="emit('toggle-star', article.id)"
            @toggle-read-later="emit('toggle-read-later', article.id)"
            @clip-to-note="emit('clip-to-note', article)"
            @quick-capture="emit('quick-capture', article)"
            @create-task="emit('create-task', article)"
            @open-original="openOriginal"
            :fetching-full-text="loadingContent"
            @fetch-full-text="fetchFullText"
          />
        </div>
      </div>

      <!-- Content -->
      <div
        ref="contentRef"
        @scroll="handleScroll"
        @click="handleContentClick"
        @mouseup="handleSelectionChange"
        @keyup="handleSelectionChange"
        class="flex-1 overflow-y-auto hidden-scrollbar select-text"
      >
        <article class="mx-auto py-8 px-6" :style="{ maxWidth: config.readingMaxWidth + 'px' }">
          <!-- Header -->
          <h1 class="text-2xl font-bold leading-tight text-text dark:text-text-dark mb-3" :style="{ fontSize: (config.readingFontSize + 8) + 'px' }">
            {{ article.title }}
          </h1>
          <div class="flex items-center gap-3 text-sm text-gray-500 dark:text-gray-400 mb-6">
            <span class="font-medium text-orange-600 dark:text-orange-400">{{ sourceName }}</span>
            <span v-if="article.author" class="truncate">· {{ article.author }}</span>
            <span>· {{ formattedDate }}</span>
          </div>
          <div v-if="article.readTimeMinutes" class="flex items-center gap-2 text-xs text-gray-400 mb-4">
            <span>{{ article.readTimeMinutes }} {{ t('feeds.read_time_min') }}</span>
            <span v-if="article.wordCount">· {{ article.wordCount.toLocaleString() }} {{ t('feeds.words') }}</span>
          </div>
          <a v-if="article.url" @click.prevent="openOriginal" class="inline-flex items-center gap-1.5 text-sm text-orange-500 hover:text-orange-600 font-medium mb-6 pb-6 border-b border-border dark:border-border-dark transition-colors cursor-pointer">
            {{ t('feeds.view_original') }} →
          </a>

          <!-- Loading content for scrape articles -->
          <div v-if="loadingContent" class="flex flex-col items-center justify-center py-16">
            <div class="w-8 h-8 border-2 border-orange-500 border-t-transparent rounded-full animate-spin mb-4"></div>
            <p class="text-sm text-gray-400">{{ t('feeds.loading_content') }}</p>
          </div>
          <!-- Article body -->
          <div v-else class="article-prose" :style="{ fontSize: config.readingFontSize + 'px' }" v-html="renderedContent"></div>

          <!--
            The highlights are kept on this device; the note is the durable
            copy, so the way out to one sits with them rather than in a menu.
          -->
          <section v-if="highlights.length > 0" class="mt-10 pt-6 border-t border-border dark:border-border-dark">
            <div class="flex items-center justify-between gap-3 mb-3">
              <h2 class="text-sm font-semibold text-text dark:text-text-dark">
                {{ t('feeds.highlights_count', { count: highlights.length }) }}
              </h2>
              <button
                @click="sendHighlightsToNote"
                class="px-3 py-1.5 rounded-lg bg-orange-500 text-white text-xs font-medium hover:bg-orange-600 transition-colors shadow-sm"
              >
                {{ t('feeds.highlights_to_note') }}
              </button>
            </div>
            <ul class="space-y-2">
              <li
                v-for="highlight in highlights"
                :key="highlight.id"
                class="group flex items-start gap-2 px-3 py-2 rounded-xl bg-surface dark:bg-surface-dark border border-border dark:border-border-dark"
              >
                <span class="flex-1 min-w-0 text-[13px] leading-relaxed text-gray-600 dark:text-gray-300">{{ highlight.text }}</span>
                <button
                  @click="removeHighlightById(highlight.id)"
                  class="shrink-0 p-1 rounded-md text-gray-400 opacity-0 group-hover:opacity-100 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 transition-all"
                  :aria-label="t('feeds.highlight_remove')"
                >
                  <Trash2 class="w-3.5 h-3.5" />
                </button>
              </li>
            </ul>
          </section>
        </article>
      </div>
      <!--
        Offered where the selection ends, so a long highlight does not put the
        button somewhere the pointer never was.
      -->
      <Teleport to="body">
        <button
          v-if="selection"
          @mousedown.prevent="saveHighlight"
          class="fixed z-[400] flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg bg-[#1c1c1e] text-white text-[13px] font-medium shadow-xl hover:bg-black transition-colors"
          :style="{ left: selection.x + 'px', top: selection.y + 8 + 'px' }"
        >
          <Highlighter class="w-3.5 h-3.5" />
          {{ t('feeds.highlight') }}
        </button>
      </Teleport>
    </template>
  </div>
</template>

<style scoped>
.article-prose :deep(img) {
  max-width: 100%;
  height: auto;
  border-radius: 0.75rem;
  margin: 1.5rem 0;
}

.article-prose :deep(p) {
  margin-bottom: 1rem;
  line-height: 1.75;
  color: var(--color-text, #1c1c1e);
}

:root.dark .article-prose :deep(p) {
  color: var(--color-text-dark, #e5e5e5);
}

.article-prose :deep(h1),
.article-prose :deep(h2),
.article-prose :deep(h3) {
  font-weight: 700;
  margin: 1.5rem 0 0.75rem;
  line-height: 1.3;
}

.article-prose :deep(h2) { font-size: 1.375rem; }
.article-prose :deep(h3) { font-size: 1.125rem; }

.article-prose :deep(a) {
  color: #f97316;
  text-decoration: underline;
  text-underline-offset: 2px;
}

.article-prose :deep(blockquote) {
  border-left: 3px solid #f97316;
  padding-left: 1rem;
  margin: 1.5rem 0;
  color: #6b7280;
  font-style: italic;
}

.article-prose :deep(pre) {
  background: #f3f4f6;
  border-radius: 0.75rem;
  padding: 1rem;
  overflow-x: auto;
  margin: 1rem 0;
  font-size: 0.875rem;
}

:root.dark .article-prose :deep(pre) {
  background: #1f2937;
}

.article-prose :deep(code) {
  font-size: 0.875em;
  background: #f3f4f6;
  padding: 0.125rem 0.375rem;
  border-radius: 0.25rem;
}

:root.dark .article-prose :deep(code) {
  background: #1f2937;
}

.article-prose :deep(ul),
.article-prose :deep(ol) {
  padding-left: 1.5rem;
  margin: 1rem 0;
}

.article-prose :deep(li) {
  margin-bottom: 0.5rem;
  line-height: 1.6;
}

.article-prose :deep(figure) {
  margin: 1.5rem 0;
}

.article-prose :deep(mark.feed-highlight) {
  background: rgba(249, 115, 22, 0.22);
  color: inherit;
  border-radius: 0.2rem;
  padding: 0.05em 0.1em;
  cursor: pointer;
}

.article-prose :deep(mark.feed-highlight:hover) {
  background: rgba(249, 115, 22, 0.38);
}

.article-prose :deep(figcaption) {
  text-align: center;
  font-size: 0.875rem;
  color: #9ca3af;
  margin-top: 0.5rem;
}
</style>
