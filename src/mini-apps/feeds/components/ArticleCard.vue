<script setup lang="ts">
import { computed } from 'vue';
import { Star } from 'lucide-vue-next';
import type { CachedArticle } from '../types/feed.types';
import { useI18n } from 'vue-i18n';

const props = withDefaults(defineProps<{
  article: CachedArticle;
  isSelected: boolean;
  sourceName: string;
  viewMode?: 'magazine' | 'cards' | 'titles';
}>(), {
  viewMode: 'magazine',
});

const emit = defineEmits<{ select: [] }>();
const { t } = useI18n();

const timeAgo = (dateStr: string): string => {
  if (!dateStr) return '';
  const now = Date.now();
  const then = new Date(dateStr).getTime();
  const diff = now - then;
  const minutes = Math.floor(diff / 60000);
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days}d`;
  const weeks = Math.floor(days / 7);
  if (weeks < 4) return `${weeks}w`;
  return new Date(dateStr).toLocaleDateString();
};

const displaySummary = computed(() => {
  const text = props.article.summary || props.article.content || '';
  // Strip HTML tags and truncate
  const clean = text.replace(/<[^>]*>/g, '').trim();
  return clean.length > 120 ? clean.substring(0, 120) + '...' : clean;
});
</script>

<template>
  <!-- Magazine layout (default - horizontal with thumbnail on right) -->
  <div
    v-if="viewMode === 'magazine'"
    @click="emit('select')"
    :class="[
      'flex gap-3 px-4 py-3.5 cursor-pointer transition-all duration-200',
      isSelected
        ? 'bg-orange-50/80 dark:bg-orange-900/15'
        : 'hover:bg-gray-50 dark:hover:bg-gray-800/40'
    ]"
  >
    <!-- Unread indicator -->
    <div class="w-2 pt-2 shrink-0">
      <div v-if="!article.isRead" class="w-2 h-2 rounded-full bg-orange-500"></div>
    </div>

    <!-- Content -->
    <div class="flex-1 min-w-0">
      <h3 :class="[
        'text-sm leading-snug line-clamp-2 mb-1',
        article.isRead ? 'text-gray-500 dark:text-gray-400 font-normal' : 'text-text dark:text-text-dark font-semibold'
      ]">
        {{ article.title }}
      </h3>
      <p v-if="displaySummary" class="text-xs text-gray-400 dark:text-gray-500 line-clamp-2 mb-1.5">{{ displaySummary }}</p>
      <div class="flex items-center gap-2 text-[11px] text-gray-400 dark:text-gray-500">
        <span class="truncate max-w-[120px]">{{ sourceName }}</span>
        <span>·</span>
        <span>{{ timeAgo(article.publishedAt) }}</span>
        <span v-if="article.readTimeMinutes">·</span>
        <span v-if="article.readTimeMinutes">{{ article.readTimeMinutes }} {{ t('feeds.read_time_min') }}</span>
        <Star v-if="article.isStarred" class="w-3 h-3 text-yellow-500 fill-yellow-500 ml-auto shrink-0" />
      </div>
    </div>

    <!-- Thumbnail -->
    <img
      v-if="article.thumbnailUrl"
      :src="article.thumbnailUrl"
      class="w-20 h-[52px] rounded-lg object-cover shrink-0 self-start mt-0.5"
      @error="($event.target as HTMLImageElement).style.display='none'"
    />
  </div>

  <!-- Cards layout (vertical card: large thumbnail on top, title + meta below) -->
  <div
    v-else-if="viewMode === 'cards'"
    @click="emit('select')"
    :class="[
      'flex flex-col rounded-xl overflow-hidden cursor-pointer transition-all duration-200 border',
      isSelected
        ? 'border-orange-400 bg-orange-50/80 dark:bg-orange-900/15 dark:border-orange-500/50 shadow-md'
        : 'border-border dark:border-border-dark hover:border-gray-300 dark:hover:border-gray-600 hover:shadow-sm bg-surface dark:bg-surface-dark'
    ]"
  >
    <!-- Thumbnail -->
    <img
      v-if="article.thumbnailUrl"
      :src="article.thumbnailUrl"
      class="w-full h-28 object-cover"
      @error="($event.target as HTMLImageElement).style.display='none'"
    />
    <div v-else class="w-full h-16 bg-gradient-to-br from-orange-100 to-orange-50 dark:from-orange-900/20 dark:to-orange-900/5"></div>

    <!-- Content -->
    <div class="p-3 flex-1 flex flex-col min-w-0">
      <div class="flex items-start gap-1.5 mb-1">
        <div v-if="!article.isRead" class="w-2 h-2 rounded-full bg-orange-500 mt-1 shrink-0"></div>
        <h3 :class="[
          'text-sm leading-snug line-clamp-2 flex-1',
          article.isRead ? 'text-gray-500 dark:text-gray-400 font-normal' : 'text-text dark:text-text-dark font-semibold'
        ]">
          {{ article.title }}
        </h3>
        <Star v-if="article.isStarred" class="w-3 h-3 text-yellow-500 fill-yellow-500 shrink-0 mt-0.5" />
      </div>
      <div class="flex items-center gap-2 text-[11px] text-gray-400 dark:text-gray-500 mt-auto">
        <span class="truncate max-w-[100px]">{{ sourceName }}</span>
        <span>·</span>
        <span>{{ timeAgo(article.publishedAt) }}</span>
      </div>
    </div>
  </div>

  <!-- Titles layout (compact: unread dot + title + source + time on single line) -->
  <div
    v-else
    @click="emit('select')"
    :class="[
      'flex items-center gap-2 px-4 py-2 cursor-pointer transition-all duration-200',
      isSelected
        ? 'bg-orange-50/80 dark:bg-orange-900/15'
        : 'hover:bg-gray-50 dark:hover:bg-gray-800/40'
    ]"
  >
    <div class="w-2 shrink-0">
      <div v-if="!article.isRead" class="w-2 h-2 rounded-full bg-orange-500"></div>
    </div>
    <h3 :class="[
      'text-sm truncate flex-1 min-w-0',
      article.isRead ? 'text-gray-500 dark:text-gray-400 font-normal' : 'text-text dark:text-text-dark font-medium'
    ]">
      {{ article.title }}
    </h3>
    <Star v-if="article.isStarred" class="w-3 h-3 text-yellow-500 fill-yellow-500 shrink-0" />
    <span class="text-[11px] text-gray-400 dark:text-gray-500 shrink-0 truncate max-w-[80px]">{{ sourceName }}</span>
    <span class="text-[11px] text-gray-400 dark:text-gray-500 shrink-0">{{ timeAgo(article.publishedAt) }}</span>
  </div>
</template>
