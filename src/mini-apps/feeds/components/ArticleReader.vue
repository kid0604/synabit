<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue';
import { useI18n } from 'vue-i18n';
import { ArrowLeft, Rss } from 'lucide-vue-next';
import ReaderToolbar from './ReaderToolbar.vue';
import type { CachedArticle, FeedConfig, FeedSource } from '../types/feed.types';

const props = defineProps<{
  article: CachedArticle | null;
  config: FeedConfig;
  sources: FeedSource[];
  showBackButton?: boolean;
}>();

const emit = defineEmits<{
  'toggle-star': [id: string];
  'toggle-read-later': [id: string];
  'clip-to-note': [article: CachedArticle];
  'quick-capture': [article: CachedArticle];
  'create-task': [article: CachedArticle];
  'back': [];
}>();

const { t } = useI18n();
const contentRef = ref<HTMLElement | null>(null);
const readingProgress = ref(0);

const sourceName = computed(() => {
  if (!props.article) return '';
  return props.sources.find(s => s.id === props.article!.feedSourceId)?.title || '';
});

const formattedDate = computed(() => {
  if (!props.article?.publishedAt) return '';
  return new Date(props.article.publishedAt).toLocaleDateString(undefined, {
    year: 'numeric', month: 'long', day: 'numeric', hour: '2-digit', minute: '2-digit'
  });
});

const handleScroll = () => {
  if (!contentRef.value) return;
  const el = contentRef.value;
  const scrollable = el.scrollHeight - el.clientHeight;
  readingProgress.value = scrollable > 0 ? Math.min(100, (el.scrollTop / scrollable) * 100) : 100;
};

const openOriginal = () => {
  if (props.article?.url) {
    window.open(props.article.url, '_blank');
  }
};

watch(() => props.article?.id, () => {
  readingProgress.value = 0;
  nextTick(() => {
    if (contentRef.value) contentRef.value.scrollTop = 0;
  });
});
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
          <button v-if="showBackButton" @click="emit('back')" class="p-1.5 rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors mr-1">
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
          />
        </div>
      </div>

      <!-- Content -->
      <div ref="contentRef" @scroll="handleScroll" class="flex-1 overflow-y-auto hidden-scrollbar">
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
          <a v-if="article.url" :href="article.url" target="_blank" rel="noopener noreferrer" class="inline-flex items-center gap-1.5 text-sm text-orange-500 hover:text-orange-600 font-medium mb-6 pb-6 border-b border-border dark:border-border-dark transition-colors">
            {{ t('feeds.view_original') }} →
          </a>

          <!-- Article body -->
          <div class="article-prose" :style="{ fontSize: config.readingFontSize + 'px' }" v-html="article.content || article.summary || ''"></div>
        </article>
      </div>
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

.article-prose :deep(figcaption) {
  text-align: center;
  font-size: 0.875rem;
  color: #9ca3af;
  margin-top: 0.5rem;
}
</style>
