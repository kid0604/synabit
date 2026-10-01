<script setup lang="ts">
import { computed } from 'vue';
import { Star } from 'lucide-vue-next';
import CachedImage from './CachedImage.vue';
import type { CachedArticle } from '../types/feed.types';
import { useI18n } from 'vue-i18n';
import { cardTime, plainPreview } from '../articleText';

const props = withDefaults(defineProps<{
  article: CachedArticle;
  isSelected: boolean;
  sourceName: string;
  viewMode?: 'magazine' | 'cards' | 'titles';
  /**
   * Whether this row starts below the fold, and so can have its rendering
   * deferred until it is scrolled near. Rows in the first screenful must not:
   * evaluating a visibility boundary before painting delays the paint.
   */
  deferred?: boolean;
}>(), {
  viewMode: 'magazine',
  deferred: false,
});

const emit = defineEmits<{ select: [] }>();
const { t, locale } = useI18n();

/** "5 phút trước", or the date; nothing when the feed gave none. */
const timeAgo = (dateStr: string): string => cardTime(dateStr, locale.value);

const displaySummary = computed(() =>
  plainPreview(props.article.summary || props.article.content || ''),
);
</script>

<template>
  <!-- Magazine layout (default - horizontal with thumbnail on right) -->
  <div
    v-if="viewMode === 'magazine'"
    :data-article-id="article.id"
    role="option"
    :aria-selected="isSelected"
    tabindex="-1"
    @click="emit('select')"
    :class="[
      'flex gap-3 px-4 py-3.5 cursor-pointer transition-all duration-200',
      deferred ? 'row-deferred row-magazine' : '',
      isSelected
        ? 'bg-accent/10'
        : 'hover:bg-gray-50 dark:hover:bg-gray-800/40'
    ]"
  >
    <!-- Unread indicator -->
    <div class="w-2 pt-2 shrink-0">
      <div v-if="!article.isRead" class="w-2 h-2 rounded-full bg-accent"></div>
    </div>

    <!-- Content -->
    <div class="flex-1 min-w-0">
      <h3 :class="[
        'text-sm leading-snug line-clamp-2 mb-1',
        article.isRead ? 'text-gray-500 dark:text-gray-400 font-normal' : 'text-text dark:text-text-dark font-semibold'
      ]">
        {{ article.title }}
      </h3>
      <p v-if="displaySummary" class="text-xs text-gray-500 dark:text-gray-400 line-clamp-2 mb-1.5">{{ displaySummary }}</p>
      <div class="flex items-center gap-2 text-xs text-gray-500 dark:text-gray-400">
        <span class="truncate max-w-[120px]">{{ sourceName }}</span>
        <template v-if="timeAgo(article.publishedAt)">
          <span aria-hidden="true">·</span>
          <span class="shrink-0 whitespace-nowrap">{{ timeAgo(article.publishedAt) }}</span>
        </template>
        <span
          v-for="tag in article.tags"
          :key="tag"
          class="px-1.5 py-0.5 rounded-full bg-gray-100 dark:bg-gray-800 text-gray-500 dark:text-gray-400 shrink-0"
        >{{ tag }}</span>
        <span v-if="article.readTimeMinutes" aria-hidden="true">·</span>
        <span v-if="article.readTimeMinutes" class="shrink-0 whitespace-nowrap">{{ article.readTimeMinutes }} {{ t('feeds.read_time_min') }}</span>
        <Star v-if="article.isStarred" class="w-3 h-3 text-yellow-500 fill-yellow-500 ml-auto shrink-0" />
      </div>
    </div>

    <!-- Thumbnail -->
    <CachedImage
      v-if="article.thumbnailUrl"
      :src="article.thumbnailUrl"
      class="w-20 h-[52px] rounded-lg object-cover shrink-0 self-start mt-0.5"
    />
  </div>

  <!-- Cards layout (vertical card: large thumbnail on top, title + meta below) -->
  <div
    v-else-if="viewMode === 'cards'"
    :data-article-id="article.id"
    role="option"
    :aria-selected="isSelected"
    tabindex="-1"
    @click="emit('select')"
    :class="[
      'flex flex-col rounded-xl overflow-hidden cursor-pointer transition-all duration-200 border',
      deferred ? 'row-deferred row-cards' : '',
      isSelected
        ? 'border-accent/60 bg-accent/10 shadow-md'
        : 'border-border dark:border-border-dark hover:border-gray-300 dark:hover:border-gray-600 hover:shadow-sm bg-surface dark:bg-surface-dark'
    ]"
  >
    <!-- Thumbnail -->
    <CachedImage
      v-if="article.thumbnailUrl"
      :src="article.thumbnailUrl"
      class="w-full h-28 object-cover"
    />
    <div v-else class="w-full h-16 bg-gradient-to-br from-accent/15 to-accent/5"></div>

    <!-- Content -->
    <div class="p-3 flex-1 flex flex-col min-w-0">
      <div class="flex items-start gap-1.5 mb-1">
        <div v-if="!article.isRead" class="w-2 h-2 rounded-full bg-accent mt-1 shrink-0"></div>
        <h3 :class="[
          'text-sm leading-snug line-clamp-2 flex-1',
          article.isRead ? 'text-gray-500 dark:text-gray-400 font-normal' : 'text-text dark:text-text-dark font-semibold'
        ]">
          {{ article.title }}
        </h3>
        <Star v-if="article.isStarred" class="w-3 h-3 text-yellow-500 fill-yellow-500 shrink-0 mt-0.5" />
      </div>
      <div class="flex items-center gap-2 text-xs text-gray-500 dark:text-gray-400 mt-auto">
        <span class="truncate max-w-[100px]">{{ sourceName }}</span>
        <template v-if="timeAgo(article.publishedAt)">
          <span aria-hidden="true">·</span>
          <span class="shrink-0 whitespace-nowrap">{{ timeAgo(article.publishedAt) }}</span>
        </template>
      </div>
    </div>
  </div>

  <!-- Titles layout (compact: unread dot + title + source + time on single line) -->
  <div
    v-else
    :data-article-id="article.id"
    role="option"
    :aria-selected="isSelected"
    tabindex="-1"
    @click="emit('select')"
    :class="[
      'flex items-center gap-2 px-4 py-2 cursor-pointer transition-all duration-200',
      deferred ? 'row-deferred row-titles' : '',
      isSelected
        ? 'bg-accent/10'
        : 'hover:bg-gray-50 dark:hover:bg-gray-800/40'
    ]"
  >
    <div class="w-2 shrink-0">
      <div v-if="!article.isRead" class="w-2 h-2 rounded-full bg-accent"></div>
    </div>
    <h3 :class="[
      'text-sm truncate flex-1 min-w-0',
      article.isRead ? 'text-gray-500 dark:text-gray-400 font-normal' : 'text-text dark:text-text-dark font-medium'
    ]">
      {{ article.title }}
    </h3>
    <Star v-if="article.isStarred" class="w-3 h-3 text-yellow-500 fill-yellow-500 shrink-0" />
    <span class="text-xs text-gray-500 dark:text-gray-400 shrink-0 truncate max-w-[80px]">{{ sourceName }}</span>
    <span class="text-xs text-gray-500 dark:text-gray-400 shrink-0">{{ timeAgo(article.publishedAt) }}</span>
  </div>
</template>

<style scoped>
/*
 * Rows below the fold do not need to be laid out until they are scrolled near.
 * `content-visibility` is Baseline Newly available, which is fine only because
 * a browser that has never heard of it ignores the declaration and renders
 * every row — exactly what this list did before. Nothing here is required for
 * correctness.
 *
 * The intrinsic size is mandatory alongside it: without a placeholder height
 * an off-screen row collapses to zero and the scrollbar jumps as you move.
 * `auto` lets the browser keep the real height once it has measured one.
 */
.row-deferred {
  content-visibility: auto;
}

.row-magazine { contain-intrinsic-size: auto none auto 84px; }
.row-cards    { contain-intrinsic-size: auto none auto 208px; }
.row-titles   { contain-intrinsic-size: auto none auto 36px; }
</style>
