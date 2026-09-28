<script setup lang="ts">
/**
 * The open Safe: a sidebar of views, the list, and the item.
 *
 * The list is summaries only — title, username, host — so it can be searched
 * and scrolled without a single secret leaving Rust. Every call can find the
 * Safe locked (it locks itself when left alone); `locked` sends the whole app
 * back to the unlock screen rather than showing an error on each pane.
 */
import { computed, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { AlertTriangle, LayoutGrid, Lock, Plus, Search, Settings2, Star, Tag, Trash2 } from 'lucide-vue-next';
import NavButtons from '../../shared/components/NavButtons.vue';
import { useEventBus } from '../../composables/useEventBus';
import { safeCode, type Filter, type ItemKind, type ItemSummary, type ItemView, type Overview, type SafeApi } from './api';
import { KINDS, kindInfo } from './kinds';
import ItemDetail from './ItemDetail.vue';
import ItemEditor from './ItemEditor.vue';
import SafeSettings from './SafeSettings.vue';
import { useSafeError } from './useSafeError';

const props = defineProps<{ api: SafeApi }>();
const emit = defineEmits<{ (e: 'locked'): void }>();
const { t } = useI18n();
const explain = useSafeError();

const overview = ref<Overview | null>(null);
const filter = ref<Filter>({ by: 'all' });
const query = ref('');
const items = ref<ItemSummary[]>([]);
const selected = ref<ItemView | null>(null);
const error = ref('');

/** Every failure goes through here: a locked Safe leaves the screen. */
function fail(e: unknown) {
  if (safeCode(e) === 'locked') {
    emit('locked');
    return;
  }
  error.value = explain(e);
}

async function load() {
  try {
    const [o, list] = await Promise.all([props.api.overview(), props.api.list(filter.value, query.value)]);
    overview.value = o;
    items.value = list;
    error.value = '';
  } catch (e) {
    fail(e);
  }
}

async function select(id: string | null) {
  if (!id) {
    selected.value = null;
    return;
  }
  try {
    selected.value = await props.api.get(id);
  } catch (e) {
    if (safeCode(e) === 'not_found') selected.value = null;
    else fail(e);
  }
}

async function refreshAll() {
  await load();
  const id = selected.value?.id;
  if (id && items.value.some((i) => i.id === id)) await select(id);
  else selected.value = null;
}

let searchTimer: ReturnType<typeof setTimeout> | undefined;
watch(query, () => {
  clearTimeout(searchTimer);
  searchTimer = setTimeout(load, 120);
});
watch(filter, load, { deep: true });
onMounted(load);

/*
 * Another device's items arrive by sync while this screen is open. Rust has
 * already decided which version of each file to keep; `refresh` reads them
 * again and folds in any version it set aside. Only when something under
 * `Safe/` came in — a note arriving is no reason to decrypt every item.
 */
const bus = useEventBus();
bus.on('vault:sync-completed', async (payload) => {
  const files = payload?.pulled_files;
  if (files && !files.some((f) => f.replace(/\\/g, '/').startsWith('Safe/'))) return;
  try {
    await props.api.refresh();
    await refreshAll();
  } catch (e) {
    fail(e);
  }
});

function isFilter(f: Filter) {
  return JSON.stringify(f) === JSON.stringify(filter.value);
}

const kindCounts = computed(() => new Map(overview.value?.kinds ?? []));

const editing = ref<{ item: ItemView | null; kind: ItemKind } | null>(null);
const showNewMenu = ref(false);

function startNew(kind: ItemKind) {
  showNewMenu.value = false;
  editing.value = { item: null, kind };
}

async function saved(item: ItemView) {
  editing.value = null;
  await load();
  selected.value = item;
}

const showSettings = ref(false);

async function lock() {
  await props.api.lock().catch(() => undefined);
  emit('locked');
}

const navButton = 'w-full flex items-center gap-2.5 px-2.5 py-1.5 rounded-lg text-sm text-left';
const navActive = 'bg-accent/10 text-accent font-medium';
const navIdle = 'hover:bg-surface-hover dark:hover:bg-surface-hover-dark';
</script>

<template>
  <div class="h-full flex">
    <!-- Sidebar -->
    <nav class="w-56 flex-shrink-0 border-r border-border dark:border-border-dark bg-surface dark:bg-surface-dark flex flex-col">
      <div class="h-14 px-3 flex items-center gap-2 font-semibold border-b border-border dark:border-border-dark" data-tauri-drag-region>
        <NavButtons />
        <span>{{ t('safe.name') }}</span>
      </div>
      <div class="flex-1 overflow-y-auto p-2 space-y-4">
        <div class="space-y-0.5">
          <button :class="[navButton, isFilter({ by: 'all' }) ? navActive : navIdle]" @click="filter = { by: 'all' }">
            <LayoutGrid class="w-4 h-4" /><span class="flex-1">{{ t('safe.sidebar.all') }}</span>
            <span class="text-xs tabular-nums text-text-tertiary dark:text-text-tertiary-dark">{{ overview?.all ?? '' }}</span>
          </button>
          <button :class="[navButton, isFilter({ by: 'favorites' }) ? navActive : navIdle]" @click="filter = { by: 'favorites' }">
            <Star class="w-4 h-4" /><span class="flex-1">{{ t('safe.sidebar.favorites') }}</span>
            <span class="text-xs tabular-nums text-text-tertiary dark:text-text-tertiary-dark">{{ overview?.favorites || '' }}</span>
          </button>
        </div>

        <div class="space-y-0.5">
          <p class="px-2.5 pb-1 text-xs font-medium text-text-tertiary dark:text-text-tertiary-dark">{{ t('safe.sidebar.kinds') }}</p>
          <template v-for="k in KINDS" :key="k.kind">
            <button v-if="kindCounts.get(k.kind)" :class="[navButton, isFilter({ by: 'kind', kind: k.kind }) ? navActive : navIdle]" @click="filter = { by: 'kind', kind: k.kind }">
              <component :is="k.icon" class="w-4 h-4" /><span class="flex-1">{{ t(`safe.kind.${k.kind}`) }}</span>
              <span class="text-xs tabular-nums text-text-tertiary dark:text-text-tertiary-dark">{{ kindCounts.get(k.kind) }}</span>
            </button>
          </template>
        </div>

        <div v-if="overview?.tags.length" class="space-y-0.5">
          <p class="px-2.5 pb-1 text-xs font-medium text-text-tertiary dark:text-text-tertiary-dark">{{ t('safe.sidebar.tags') }}</p>
          <button v-for="[tag, n] in overview.tags" :key="tag" :class="[navButton, isFilter({ by: 'tag', tag }) ? navActive : navIdle]" @click="filter = { by: 'tag', tag }">
            <Tag class="w-4 h-4" /><span class="flex-1 truncate">{{ tag }}</span>
            <span class="text-xs tabular-nums text-text-tertiary dark:text-text-tertiary-dark">{{ n }}</span>
          </button>
        </div>

        <button :class="[navButton, isFilter({ by: 'trash' }) ? navActive : navIdle]" @click="filter = { by: 'trash' }">
          <Trash2 class="w-4 h-4" /><span class="flex-1">{{ t('safe.sidebar.trash') }}</span>
          <span class="text-xs tabular-nums text-text-tertiary dark:text-text-tertiary-dark">{{ overview?.trash || '' }}</span>
        </button>
      </div>
      <div class="p-2 border-t border-border dark:border-border-dark space-y-0.5">
        <button :class="[navButton, navIdle]" @click="showSettings = true">
          <Settings2 class="w-4 h-4" />{{ t('safe.sidebar.settings') }}
        </button>
        <button :class="[navButton, navIdle]" @click="lock">
          <Lock class="w-4 h-4" />{{ t('safe.sidebar.lock') }}
        </button>
      </div>
    </nav>

    <!-- List -->
    <section class="w-80 flex-shrink-0 border-r border-border dark:border-border-dark flex flex-col">
      <div class="h-14 px-3 flex items-center gap-2 border-b border-border dark:border-border-dark" data-tauri-drag-region>
        <div class="relative flex-1">
          <Search class="w-4 h-4 absolute left-2.5 top-1/2 -translate-y-1/2 text-text-tertiary dark:text-text-tertiary-dark" />
          <input v-model="query" type="search" :placeholder="t('safe.list.search')" :aria-label="t('safe.list.search')" spellcheck="false" class="w-full pl-8 pr-2 py-1.5 rounded-lg bg-surface dark:bg-surface-dark text-sm focus:outline-none focus:ring-2 focus:ring-accent" />
        </div>
        <div class="relative">
          <button class="p-1.5 rounded-lg bg-accent text-white hover:opacity-90" :aria-label="t('safe.list.new')" :title="t('safe.list.new')" :aria-expanded="showNewMenu" @click="showNewMenu = !showNewMenu">
            <Plus class="w-4 h-4" />
          </button>
          <div v-if="showNewMenu" class="absolute right-0 top-9 z-20 w-48 p-1 rounded-xl border border-border dark:border-border-dark bg-base dark:bg-base-dark shadow-xl" role="menu">
            <button v-for="k in KINDS" :key="k.kind" role="menuitem" class="w-full flex items-center gap-2.5 px-2.5 py-1.5 rounded-lg text-sm hover:bg-surface-hover dark:hover:bg-surface-hover-dark" @click="startNew(k.kind)">
              <component :is="k.icon" class="w-4 h-4 text-text-secondary dark:text-text-secondary-dark" />{{ t(`safe.kind.${k.kind}`) }}
            </button>
          </div>
        </div>
      </div>

      <div v-if="overview?.unreadable.length" class="m-2 p-2.5 rounded-lg bg-warning/10 text-xs flex gap-2">
        <AlertTriangle class="w-4 h-4 flex-shrink-0 text-warning" />
        <span>{{ t('safe.sidebar.unreadable', { n: overview.unreadable.length }) }}</span>
      </div>

      <ul class="flex-1 overflow-y-auto p-2 space-y-0.5" role="listbox" :aria-label="t('safe.name')">
        <li
          v-for="s in items"
          :key="s.id"
          role="option"
          :aria-selected="selected?.id === s.id"
          tabindex="0"
          class="safe-row flex items-center gap-3 px-2.5 py-2 rounded-lg cursor-pointer"
          :class="selected?.id === s.id ? 'bg-accent/10' : 'hover:bg-surface-hover dark:hover:bg-surface-hover-dark'"
          @click="select(s.id)"
          @keydown.enter="select(s.id)"
        >
          <div class="w-8 h-8 rounded-lg bg-surface dark:bg-surface-dark border border-border dark:border-border-dark flex items-center justify-center flex-shrink-0">
            <component :is="kindInfo(s.kind).icon" class="w-4 h-4 text-text-secondary dark:text-text-secondary-dark" />
          </div>
          <div class="flex-1 min-w-0">
            <p class="text-sm font-medium truncate">{{ s.title }}</p>
            <p class="text-xs text-text-secondary dark:text-text-secondary-dark truncate">{{ s.subtitle }}</p>
          </div>
          <Star v-if="s.favorite" class="w-3.5 h-3.5 fill-warning text-warning flex-shrink-0" />
        </li>
        <li v-if="!items.length" class="px-3 py-10 text-center text-sm text-text-tertiary dark:text-text-tertiary-dark">
          {{ query ? t('safe.list.no_match', { q: query }) : t('safe.list.empty') }}
        </li>
      </ul>
    </section>

    <!-- Detail -->
    <main class="flex-1 min-w-0">
      <p v-if="error" class="m-4 p-3 rounded-lg bg-danger/10 text-sm text-danger" role="alert">{{ error }}</p>
      <ItemDetail
        v-if="selected"
        :api="api"
        :item="selected"
        @edit="editing = { item: selected, kind: selected.kind }"
        @changed="refreshAll"
        @error="fail"
      />
      <div v-else class="h-full flex items-center justify-center text-sm text-text-tertiary dark:text-text-tertiary-dark">
        {{ t('safe.detail.pick') }}
      </div>
    </main>

    <ItemEditor v-if="editing" :api="api" :item="editing.item" :kind="editing.kind" @saved="saved" @close="editing = null" />
    <SafeSettings v-if="showSettings" :api="api" @close="showSettings = false" @changed="load" />
  </div>
</template>

<style scoped>
/*
 * A long Safe is a long list. `content-visibility` skips rendering rows that
 * are off screen; a WebView that does not know it renders every row, which is
 * what the list did anyway — see CLAUDE.md for why that makes it fine here.
 */
.safe-row {
  content-visibility: auto;
  contain-intrinsic-size: auto 48px;
}
</style>
