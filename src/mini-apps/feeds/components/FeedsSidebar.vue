<script setup lang="ts">
import { ref, computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { Calendar, Star, BookmarkPlus, Inbox, ChevronRight, Rss, Folder, FileText } from 'lucide-vue-next';
import type { FeedSource, FeedCategory } from '../types/feed.types';
import FeedSourceItem from './FeedSourceItem.vue';

const props = defineProps<{
  sources: FeedSource[];
  categories: FeedCategory[];
  unreadCounts: Record<string, number>;
  totalUnread: number;
  selectedSourceId: string | null;
  selectedCategoryId: string | null;
  currentView: 'today' | 'all' | 'starred' | 'read-later' | 'unread';
}>();

const emit = defineEmits<{
  'select-source': [id: string | null];
  'select-category': [id: string | null];
  'select-view': [view: 'today' | 'all' | 'starred' | 'read-later' | 'unread'];
  'remove-source': [id: string];
  'rename-source': [id: string, newTitle: string];
  'open-opml': [];
  'pause-source': [id: string];
  'mark-source-read': [id: string];
}>();

const { t } = useI18n();
const collapsedCategories = ref<Set<string>>(new Set());

const toggleCategory = (catId: string) => {
  const s = new Set(collapsedCategories.value);
  if (s.has(catId)) s.delete(catId); else s.add(catId);
  collapsedCategories.value = s;
};

const uncategorizedSources = computed(() => 
  props.sources.filter(s => !s.categoryId || !props.categories.find(c => c.id === s.categoryId))
);

const getSourcesForCategory = (catId: string) => 
  props.sources.filter(s => s.categoryId === catId);

const getCategoryUnread = (catId: string) => {
  return getSourcesForCategory(catId).reduce((sum, s) => sum + (props.unreadCounts[s.id] || 0), 0);
};

const smartViews = computed(() => [
  { id: 'today' as const, label: t('feeds.today'), icon: Calendar, count: props.totalUnread },
  { id: 'starred' as const, label: t('feeds.starred'), icon: Star, count: 0 },
  { id: 'read-later' as const, label: t('feeds.read_later'), icon: BookmarkPlus, count: 0 },
  { id: 'all' as const, label: t('feeds.all_articles'), icon: Inbox, count: 0 },
]);

</script>

<template>
  <div class="flex flex-col h-full overflow-y-auto hidden-scrollbar bg-base dark:bg-base-dark">
    <!-- Smart Views -->
    <div class="p-3 space-y-0.5">
      <button
        v-for="view in smartViews"
        :key="view.id"
        @click="emit('select-view', view.id)"
        :class="[
          'w-full flex items-center gap-3 px-3 py-2 rounded-xl text-sm transition-all duration-200',
          currentView === view.id && !selectedSourceId && !selectedCategoryId
            ? 'bg-orange-50 dark:bg-orange-900/20 text-orange-600 dark:text-orange-400 font-semibold'
            : 'text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800/60'
        ]"
      >
        <component :is="view.icon" class="w-4 h-4 shrink-0" />
        <span class="flex-1 text-left truncate">{{ view.label }}</span>
        <span v-if="view.count > 0" class="min-w-[20px] h-5 px-1.5 bg-orange-500 text-white text-[11px] font-bold rounded-full flex items-center justify-center">{{ view.count > 99 ? '99+' : view.count }}</span>
      </button>
    </div>

    <div class="mx-3 border-t border-border dark:border-border-dark"></div>

    <!-- Categories + Sources -->
    <div class="p-3 space-y-1 flex-1">
      <div class="px-3 py-1.5 text-[11px] font-semibold uppercase tracking-wider text-gray-400 dark:text-gray-500">{{ t('feeds.sources') }}</div>
      
      <!-- Categorized feeds -->
      <div v-for="cat in categories" :key="cat.id" class="space-y-0.5">
        <button
          @click="toggleCategory(cat.id)"
          :class="[
            'w-full flex items-center gap-2 px-3 py-1.5 rounded-lg text-sm transition-all duration-200',
            selectedCategoryId === cat.id
              ? 'bg-orange-50 dark:bg-orange-900/20 text-orange-600 dark:text-orange-400 font-medium'
              : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800/60'
          ]"
        >
          <ChevronRight class="w-3.5 h-3.5 shrink-0 transition-transform duration-200" :class="{ 'rotate-90': !collapsedCategories.has(cat.id) }" />
          <div class="w-2.5 h-2.5 rounded-full shrink-0" :style="{ backgroundColor: cat.color || '#6b7280' }"></div>
          <span class="flex-1 text-left truncate" @click.stop="emit('select-category', cat.id)">{{ cat.name }}</span>
          <span v-if="getCategoryUnread(cat.id) > 0" class="min-w-[18px] h-[18px] px-1 bg-gray-200 dark:bg-gray-700 text-gray-600 dark:text-gray-300 text-[10px] font-semibold rounded-full flex items-center justify-center">{{ getCategoryUnread(cat.id) }}</span>
        </button>

        <div v-if="!collapsedCategories.has(cat.id)" class="pl-4 space-y-0.5">
          <FeedSourceItem
            v-for="source in getSourcesForCategory(cat.id)"
            :key="source.id"
            :source="source"
            :unread-count="unreadCounts[source.id] || 0"
            :is-selected="selectedSourceId === source.id"
            @select="emit('select-source', source.id)"
            @remove="emit('remove-source', source.id)"
            @rename-source="(newTitle: string) => emit('rename-source', source.id, newTitle)"
            @pause-source="emit('pause-source', source.id)"
            @mark-source-read="emit('mark-source-read', source.id)"
          />
        </div>
      </div>

      <!-- Uncategorized feeds -->
      <div v-if="uncategorizedSources.length > 0" class="space-y-0.5">
        <div class="px-3 py-1.5 text-[11px] font-medium text-gray-400 dark:text-gray-500">{{ t('feeds.uncategorized') }}</div>
        <FeedSourceItem
          v-for="source in uncategorizedSources"
          :key="source.id"
          :source="source"
          :unread-count="unreadCounts[source.id] || 0"
          :is-selected="selectedSourceId === source.id"
          @select="emit('select-source', source.id)"
          @remove="emit('remove-source', source.id)"
          @rename-source="(newTitle: string) => emit('rename-source', source.id, newTitle)"
          @pause-source="emit('pause-source', source.id)"
          @mark-source-read="emit('mark-source-read', source.id)"
        />
      </div>

      <!-- Empty state -->
      <div v-if="sources.length === 0" class="flex flex-col items-center justify-center py-8 px-4 text-center">
        <Rss class="w-10 h-10 text-gray-300 dark:text-gray-600 mb-3" />
        <p class="text-sm text-gray-500 dark:text-gray-400">{{ t('feeds.no_sources') }}</p>
        <p class="text-xs text-gray-400 dark:text-gray-500 mt-1">{{ t('feeds.add_your_first') }}</p>
      </div>
    </div>

    <!-- OPML Import/Export -->
    <div class="p-3 border-t border-border dark:border-border-dark">
      <button @click="$emit('open-opml')" class="w-full text-xs text-gray-500 dark:text-gray-400 hover:text-orange-500 transition-colors flex items-center justify-center gap-1.5 py-2">
        <FileText class="w-3.5 h-3.5" />
        {{ t('feeds.import_export_opml') }}
      </button>
    </div>
  </div>
</template>
