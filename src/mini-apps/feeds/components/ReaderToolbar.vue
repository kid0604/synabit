<script setup lang="ts">
import { useI18n } from 'vue-i18n';
import { Star, Bookmark, FileText, Zap, CheckSquare, ExternalLink } from 'lucide-vue-next';
import type { CachedArticle } from '../types/feed.types';

const props = defineProps<{
  article: CachedArticle;
}>();

const emit = defineEmits<{
  'toggle-star': [];
  'toggle-read-later': [];
  'clip-to-note': [];
  'quick-capture': [];
  'create-task': [];
  'open-original': [];
}>();

const { t } = useI18n();
</script>

<template>
  <div class="flex items-center gap-1 flex-1">
    <!-- Star -->
    <button
      @click="emit('toggle-star')"
      :class="[
        'p-2 rounded-lg transition-all duration-200',
        article.isStarred
          ? 'text-yellow-500 bg-yellow-50 dark:bg-yellow-900/20 hover:bg-yellow-100 dark:hover:bg-yellow-900/30'
          : 'text-gray-400 hover:text-yellow-500 hover:bg-gray-100 dark:hover:bg-gray-800'
      ]"
      :title="article.isStarred ? t('feeds.unstar') : t('feeds.star')"
    >
      <Star class="w-4 h-4" :class="{ 'fill-current': article.isStarred }" />
    </button>

    <!-- Read Later -->
    <button
      @click="emit('toggle-read-later')"
      :class="[
        'p-2 rounded-lg transition-all duration-200',
        article.isReadLater
          ? 'text-blue-500 bg-blue-50 dark:bg-blue-900/20 hover:bg-blue-100 dark:hover:bg-blue-900/30'
          : 'text-gray-400 hover:text-blue-500 hover:bg-gray-100 dark:hover:bg-gray-800'
      ]"
      :title="article.isReadLater ? t('feeds.read_later_remove') : t('feeds.read_later_add')"
    >
      <Bookmark class="w-4 h-4" :class="{ 'fill-current': article.isReadLater }" />
    </button>

    <div class="w-px h-5 bg-border dark:bg-border-dark mx-1"></div>

    <!-- Clip to Note -->
    <button @click="emit('clip-to-note')" class="p-2 rounded-lg text-gray-400 hover:text-green-600 hover:bg-gray-100 dark:hover:bg-gray-800 transition-all duration-200" :title="t('feeds.clip_to_note')">
      <FileText class="w-4 h-4" />
    </button>

    <!-- Quick Capture -->
    <button @click="emit('quick-capture')" class="p-2 rounded-lg text-gray-400 hover:text-purple-600 hover:bg-gray-100 dark:hover:bg-gray-800 transition-all duration-200" :title="t('feeds.quick_capture')">
      <Zap class="w-4 h-4" />
    </button>

    <!-- Create Task -->
    <button @click="emit('create-task')" class="p-2 rounded-lg text-gray-400 hover:text-blue-600 hover:bg-gray-100 dark:hover:bg-gray-800 transition-all duration-200" :title="t('feeds.create_task')">
      <CheckSquare class="w-4 h-4" />
    </button>

    <span class="flex-1"></span>

    <!-- Open Original -->
    <button @click="emit('open-original')" class="p-2 rounded-lg text-gray-400 hover:text-orange-500 hover:bg-gray-100 dark:hover:bg-gray-800 transition-all duration-200" :title="t('feeds.open_original')">
      <ExternalLink class="w-4 h-4" />
    </button>
  </div>
</template>
