<script setup lang="ts">
import { ref, computed } from 'vue';
import { useFocusTrap } from '../composables/useFocusTrap';
import { useI18n } from 'vue-i18n';
import AppDialog from '../../../shared/components/AppDialog.vue';
import { X, Search, Rss, Plus, Loader2, Check, FolderPlus, Globe } from 'lucide-vue-next';
import { useArticleService } from '../composables/useArticleService';
import type { FeedCategory, DiscoveredFeed } from '../types/feed.types';
import { feedErrorKey } from '../feedErrors';

const props = defineProps<{
  categories: FeedCategory[];
}>();

const emit = defineEmits<{
  close: [];
  added: [];
}>();

const { t } = useI18n();
const feedService = useArticleService();

const dialog = ref<HTMLElement | null>(null);
useFocusTrap(dialog);

// State
const url = ref('');
const discovering = ref(false);
const discoveredFeeds = ref<DiscoveredFeed[]>([]);
const selectedFeeds = ref<Set<string>>(new Set());
const selectedCategoryId = ref('');
const creatingCategory = ref(false);
const newCategoryName = ref('');
const newCategoryColor = ref('#f97316');
const adding = ref(false);
const error = ref('');
const discoveryDone = ref(false);

const COLORS = ['#f97316', '#ef4444', '#8b5cf6', '#3b82f6', '#10b981', '#f59e0b', '#ec4899', '#6366f1'];

const useScrapeMode = ref(false);

const canAdd = computed(() => {
  const hasCategory = selectedCategoryId.value || creatingCategory.value;
  if (useScrapeMode.value) return hasCategory;
  return selectedFeeds.value.size > 0 && hasCategory;
});

const handleDiscover = async () => {
  if (!url.value.trim()) return;
  discovering.value = true;
  error.value = '';
  discoveredFeeds.value = [];
  selectedFeeds.value = new Set();
  discoveryDone.value = false;
  try {
    const feeds = await feedService.discoverFeeds(url.value.trim());
    discoveredFeeds.value = feeds;
    if (feeds.length === 1) {
      selectedFeeds.value = new Set([feeds[0].url]);
    }
    discoveryDone.value = true;
  } catch (e: any) {
    const { key, params } = feedErrorKey(e, 'feeds.no_feeds_found');
    error.value = t(key, params ?? {});
    discoveryDone.value = true;
  } finally {
    discovering.value = false;
  }
};

const toggleFeed = (feedUrl: string) => {
  useScrapeMode.value = false; // deselect scrape when picking RSS
  const s = new Set(selectedFeeds.value);
  if (s.has(feedUrl)) s.delete(feedUrl); else s.add(feedUrl);
  selectedFeeds.value = s;
};

const toggleScrape = () => {
  useScrapeMode.value = !useScrapeMode.value;
  if (useScrapeMode.value) {
    selectedFeeds.value = new Set(); // deselect RSS when picking scrape
  }
};

const handleAdd = async () => {
  if (!canAdd.value) return;
  adding.value = true;
  error.value = '';
  try {
    let catId = selectedCategoryId.value;
    if (creatingCategory.value && newCategoryName.value.trim()) {
      const newCat = {
        id: `cat-${Date.now()}`,
        name: newCategoryName.value.trim(),
        color: newCategoryColor.value,
        sortOrder: props.categories.length,
        isCollapsed: false,
      };
      await feedService.saveCategories([...props.categories, newCat]);
      catId = newCat.id;
    }

    if (useScrapeMode.value) {
      await feedService.addSource(url.value.trim(), catId);
    } else {
      for (const feedUrl of selectedFeeds.value) {
        await feedService.addSource(feedUrl, catId);
      }
    }
    emit('added');
  } catch (e: any) {
    const { key, params } = feedErrorKey(e, 'feeds.add_failed');
    error.value = t(key, params ?? {});
  } finally {
    adding.value = false;
  }
};

// Escape belongs to AppDialog now; Enter still starts discovery.
const handleKeydown = (e: KeyboardEvent) => {
  if (e.key === 'Enter' && !discovering.value && !discoveryDone.value) handleDiscover();
};
</script>

<template>
  <AppDialog :show="true" :aria-label="t('feeds.add_feed_title')" size="md" @close="emit('close')">
    <div ref="dialog" @keydown="handleKeydown">
      <!-- Header -->
      <div class="flex items-center justify-between px-6 py-4 border-b border-gray-200 dark:border-border-dark">
        <h2 class="text-lg font-bold flex items-center gap-2">
          <Rss class="w-5 h-5 text-accent dark:text-accent-dark" />
          {{ t('feeds.add_feed_title') }}
        </h2>
        <button @click="emit('close')" class="p-1.5 rounded-lg text-gray-500 dark:text-gray-400 hover:text-gray-600 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors" :aria-label="t('feeds.a11y_close')">
          <X class="w-5 h-5" />
        </button>
      </div>

      <!-- Body -->
      <div class="px-6 py-5 space-y-4 max-h-[60vh] overflow-y-auto">
        <!-- URL Input -->
        <div>
          <label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1.5">URL</label>
          <div class="flex gap-2">
            <div class="relative flex-1">
              <Search class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-500 dark:text-gray-400" />
              <input
                v-model="url"
                :placeholder="t('feeds.enter_url')"
                class="w-full pl-9 pr-3 py-2.5 rounded-xl bg-gray-50 dark:bg-[#111] border border-gray-200 dark:border-[#333] text-sm focus:outline-none focus:ring-2 focus:ring-accent/30 focus:border-accent transition-all"
                @keydown.enter.prevent="handleDiscover"
                autofocus
              />
            </div>
            <button
              @click="handleDiscover"
              :disabled="!url.trim() || discovering"
              class="btn-secondary shrink-0 self-stretch h-auto disabled:opacity-50 disabled:cursor-not-allowed"
            >
              <Loader2 v-if="discovering" class="w-4 h-4 animate-spin" />
              <Search v-else class="w-4 h-4" />
              {{ discovering ? t('feeds.discovering') : t('feeds.discover') }}
            </button>
          </div>
        </div>

        <!-- Error -->
        <div v-if="error" class="px-4 py-3 rounded-xl bg-red-50 dark:bg-red-900/20 text-red-600 dark:text-red-400 text-sm">
          {{ error }}
        </div>

        <!-- Discovered options (RSS feeds + Web Scrape) -->
        <div v-if="discoveryDone && !error" class="space-y-2">
          <label class="block text-sm font-medium text-gray-700 dark:text-gray-300">{{ discoveredFeeds.length > 0 ? t('feeds.discovered_feeds') : t('feeds.no_feeds_found') }}</label>
          <div class="space-y-1.5">
            <!-- RSS feeds -->
            <button
              v-for="feed in discoveredFeeds"
              :key="feed.url"
              @click="toggleFeed(feed.url)"
              :class="[
                'w-full flex items-center gap-3 px-4 py-3 rounded-xl border text-left transition-all duration-200',
                selectedFeeds.has(feed.url)
                  ? 'border-accent bg-accent/10'
                  : 'border-gray-200 dark:border-[#333] hover:border-gray-300 dark:hover:border-[#444]'
              ]"
            >
              <div :class="['w-5 h-5 rounded-full border-2 flex items-center justify-center shrink-0 transition-colors', selectedFeeds.has(feed.url) ? 'border-accent bg-accent' : 'border-gray-300 dark:border-gray-600']">
                <Check v-if="selectedFeeds.has(feed.url)" class="w-3 h-3 text-white" />
              </div>
              <Rss class="w-4 h-4 text-accent dark:text-accent-dark shrink-0" />
              <div class="flex-1 min-w-0">
                <p class="text-sm font-medium truncate">{{ feed.title || feed.url }}</p>
                <p class="text-xs text-gray-500 dark:text-gray-400 truncate">{{ feed.feedType.toUpperCase() }} • {{ feed.url }}</p>
              </div>
            </button>

            <!-- Web Scrape option (always shown) -->
            <button
              @click="toggleScrape"
              :class="[
                'w-full flex items-center gap-3 px-4 py-3 rounded-xl border text-left transition-all duration-200',
                useScrapeMode
                  ? 'border-accent bg-accent/10'
                  : 'border-gray-200 dark:border-[#333] hover:border-gray-300 dark:hover:border-[#444]'
              ]"
            >
              <div :class="['w-5 h-5 rounded-full border-2 flex items-center justify-center shrink-0 transition-colors', useScrapeMode ? 'border-accent bg-accent' : 'border-gray-300 dark:border-gray-600']">
                <Check v-if="useScrapeMode" class="w-3 h-3 text-white" />
              </div>
              <Globe class="w-4 h-4 text-blue-400 shrink-0" />
              <div class="flex-1 min-w-0">
                <p class="text-sm font-medium">{{ t('feeds.add_as_scrape') }}</p>
                <p class="text-xs text-gray-500 dark:text-gray-400">{{ t('feeds.scrape_description') }}</p>
              </div>
            </button>
          </div>
        </div>

        <!-- Category selector (show after discovery) -->
        <div v-if="discoveryDone && !error" class="space-y-3">
          <label class="block text-sm font-medium text-gray-700 dark:text-gray-300">{{ t('feeds.select_category') }}</label>
          
          <div class="flex flex-wrap gap-2">
            <button
              v-for="cat in categories"
              :key="cat.id"
              @click="selectedCategoryId = cat.id; creatingCategory = false"
              :class="[
                'flex items-center gap-2 px-3 py-1.5 rounded-lg text-sm border transition-all',
                selectedCategoryId === cat.id && !creatingCategory
                  ? 'border-accent bg-accent/10 font-medium'
                  : 'border-gray-200 dark:border-[#333] hover:border-gray-300'
              ]"
            >
              <div class="w-2.5 h-2.5 rounded-full" :style="{ backgroundColor: cat.color }"></div>
              {{ cat.name }}
            </button>
            <button
              @click="creatingCategory = !creatingCategory; selectedCategoryId = ''"
              :class="[
                'flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-sm border transition-all',
                creatingCategory
                  ? 'border-accent bg-accent/10 font-medium'
                  : 'border-dashed border-gray-300 dark:border-gray-600 text-gray-500 dark:text-gray-400 hover:border-accent hover:text-accent dark:hover:text-accent-dark'
              ]"
            >
              <FolderPlus class="w-3.5 h-3.5" />
              {{ t('feeds.create_category') }}
            </button>
          </div>

          <!-- New category form -->
          <div v-if="creatingCategory" class="flex items-center gap-3 pl-1">
            <input
              v-model="newCategoryName"
              :placeholder="t('feeds.new_category_name')"
              class="flex-1 px-3 py-2 rounded-lg bg-gray-50 dark:bg-[#111] border border-gray-200 dark:border-[#333] text-sm focus:outline-none focus:ring-2 focus:ring-accent/30 focus:border-accent"
            />
            <div class="flex gap-1">
              <button
                v-for="color in COLORS"
                :key="color"
                @click="newCategoryColor = color"
                class="w-6 h-6 rounded-full border-2 transition-transform hover:scale-110"
                :style="{ backgroundColor: color, borderColor: newCategoryColor === color ? '#fff' : 'transparent', boxShadow: newCategoryColor === color ? '0 0 0 2px ' + color : 'none' }"
               :aria-label="color"></button>
            </div>
          </div>
        </div>
      </div>

      <!-- Footer -->
      <div class="flex items-center justify-end gap-3 px-6 py-4 border-t border-gray-200 dark:border-border-dark">
        <button @click="emit('close')" class="px-4 py-2 rounded-xl text-sm font-medium text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors">
          {{ t('feeds.cancel') }}
        </button>
        <button
          @click="handleAdd"
          :disabled="!canAdd || adding"
          class="btn-primary"
        >
          <Loader2 v-if="adding" class="w-4 h-4 animate-spin" />
          <Plus v-else class="w-4 h-4" />
          {{ t('feeds.add_selected') }}
        </button>
      </div>
    </div>
  </AppDialog>
</template>

