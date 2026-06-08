<script setup lang="ts">
import { useI18n } from 'vue-i18n';
import { Rss, MoreHorizontal, Trash2, PauseCircle, PlayCircle, CheckCheck } from 'lucide-vue-next';
import type { FeedSource } from '../types/feed.types';
import { ref } from 'vue';

const props = defineProps<{
  source: FeedSource;
  unreadCount: number;
  isSelected: boolean;
}>();

const emit = defineEmits<{
  select: [];
  remove: [];
  'pause-source': [];
  'mark-source-read': [];
}>();

const { t } = useI18n();
const showMenu = ref(false);
</script>

<template>
  <div
    @click="emit('select')"
    :class="[
      'relative group flex items-center gap-2.5 px-3 py-2 rounded-xl text-sm cursor-pointer transition-all duration-200',
      isSelected
        ? 'bg-orange-50 dark:bg-orange-900/20 text-orange-600 dark:text-orange-400 font-medium shadow-sm'
        : 'text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-gray-800/60'
    ]"
  >
    <img v-if="source.iconUrl" :src="source.iconUrl" class="w-5 h-5 rounded shrink-0" @error="($event.target as HTMLImageElement).style.display='none'" />
    <Rss v-else class="w-4 h-4 shrink-0 text-gray-400" />
    <span class="flex-1 truncate" :class="{ 'opacity-50': source.isPaused }">{{ source.title }}</span>
    <span v-if="unreadCount > 0" class="min-w-[20px] h-5 px-1.5 bg-orange-500 text-white text-[11px] font-bold rounded-full flex items-center justify-center">{{ unreadCount > 99 ? '99+' : unreadCount }}</span>
    
    <button @click.stop="showMenu = !showMenu" class="p-1 rounded-lg opacity-0 group-hover:opacity-100 hover:bg-gray-200 dark:hover:bg-gray-700 transition-all">
      <MoreHorizontal class="w-4 h-4" />
    </button>

    <div v-if="showMenu" class="absolute right-2 top-full mt-1 w-48 py-1.5 bg-white dark:bg-[#1a1a1a] rounded-xl shadow-xl border border-gray-200 dark:border-[#2c2c2c] z-50">
      <button @click.stop="emit('pause-source'); showMenu = false" class="w-full flex items-center gap-2.5 px-3 py-2 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors">
        <PauseCircle v-if="!source.isPaused" class="w-4 h-4" />
        <PlayCircle v-else class="w-4 h-4" />
        {{ source.isPaused ? t('feeds.resume_feed') : t('feeds.pause_feed') }}
      </button>
      <button @click.stop="emit('mark-source-read'); showMenu = false" class="w-full flex items-center gap-2.5 px-3 py-2 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors">
        <CheckCheck class="w-4 h-4" />
        {{ t('feeds.mark_feed_read') }}
      </button>
      <div class="my-1 border-t border-gray-200 dark:border-[#2c2c2c]"></div>
      <button @click.stop="emit('remove'); showMenu = false" class="w-full flex items-center gap-2.5 px-3 py-2 text-sm text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors">
        <Trash2 class="w-4 h-4" />
        {{ t('feeds.remove_source') }}
      </button>
    </div>
    <div v-if="showMenu" class="fixed inset-0 z-40" @click.stop="showMenu = false"></div>
  </div>
</template>
