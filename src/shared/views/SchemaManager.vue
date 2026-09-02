<script setup lang="ts">
/**
 * Every kind in the vault, in one place, with the numbers that matter.
 *
 * A page rather than a modal, following the Notes manager: it takes the main
 * pane, keeps the rail beside it, and leaves by a back arrow. A modal was
 * wrong for it — this is somewhere you go and stay for a while, sorting and
 * searching and fixing things, not a question to answer and dismiss.
 *
 * The rail lists kinds too, but it lists them to be browsed: a column of names
 * you click to see things. This is the other question — not "show me my
 * animals" but "what kinds do I have, which have drifted, which need fixing".
 *
 * The columns are chosen so a problem is visible without opening anything.
 * `Loose` is the one that earns its place: keys on the files that are not part
 * of the kind's shape. A kind with a loose key is either a kind still settling
 * or a kind with two words for one idea, and both are worth a look.
 */
import { ref, computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { Search, Plus, ArrowLeft, ArrowUpDown, MoreHorizontal } from 'lucide-vue-next';
import KindRowMenu from './KindRowMenu.vue';
import { iconForNodeType } from './nodeTypeIcon';
import { isAppOwned, GOVERNED } from '../fieldRegistry';
import KindDesigner from './KindDesigner.vue';
import type { FieldKind } from '../fieldValue';

export interface ManagedKind {
  nodeType: string;
  /** Nodes of this kind. Zero for a kind designed before anything was made. */
  count: number;
  /** Every key seen on the files, with how many carry it. */
  observed: { key: string; count: number }[];
  /** The kind's shape, declared or inferred. */
  shape: string[];
  /** Whether a schema file exists, rather than the shape being a guess. */
  declared: boolean;
}

const props = defineProps<{ kinds: ManagedKind[] }>();

const emit = defineEmits<{
  open: [nodeType: string];
  /** Take a kind away, which means taking away the files that make it one. */
  remove: [nodeType: string];
  /** Say a different word on every node of a kind. */
  rename: [nodeType: string];
  create: [nodeType: string, fields: { key: string; kind: FieldKind }[]];
  close: [];
}>();

const { t } = useI18n();

/** The page has two states and no second screen: the list, or the designer. */
const designing = ref(false);

/** The row whose menu is open, and where to draw it. */
const menuFor = ref<string | null>(null);
const menuAt = ref<{ x: number; y: number } | null>(null);

const openMenu = (nodeType: string, event: MouseEvent) => {
  const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
  menuFor.value = nodeType;
  menuAt.value = { x: box.right, y: box.bottom };
};

const closeMenu = () => {
  menuFor.value = null;
  menuAt.value = null;
};

const search = ref('');
type Column = 'name' | 'count' | 'loose';
const sortBy = ref<Column>('count');

/** Keys on the files that the kind's shape does not account for. */
const looseKeys = (kind: ManagedKind) =>
  kind.observed.filter(
    f =>
      !kind.shape.includes(f.key) &&
      !isAppOwned(kind.nodeType, f.key) &&
      !GOVERNED.has(f.key),
  );

const rows = computed(() => {
  const needle = search.value.trim().toLowerCase();
  const matched = props.kinds.filter(k => {
    if (!needle) return true;
    // Field names are searched too: looking for `birthday` and being told
    // which kinds have one is the question somebody actually asks.
    return (
      k.nodeType.toLowerCase().includes(needle) ||
      k.observed.some(f => f.key.toLowerCase().includes(needle))
    );
  });

  return matched
    .map(k => ({ ...k, loose: looseKeys(k).length, fields: k.shape.length }))
    .sort((a, b) => {
      if (sortBy.value === 'name') return a.nodeType.localeCompare(b.nodeType);
      if (sortBy.value === 'loose') return b.loose - a.loose || b.count - a.count;
      return b.count - a.count || a.nodeType.localeCompare(b.nodeType);
    });
});

const cycleSort = () => {
  sortBy.value = sortBy.value === 'count' ? 'loose' : sortBy.value === 'loose' ? 'name' : 'count';
};

/** The vault as a whole, which is the thing this page is about. */
const totalThings = computed(() => props.kinds.reduce((n, k) => n + k.count, 0));
const totalLoose = computed(() => props.kinds.reduce((n, k) => n + looseKeys(k).length, 0));
</script>

<template>
  <div class="flex-1 flex flex-col min-h-0 overflow-y-auto bg-white dark:bg-[#141416]">
    <header
      class="flex items-center gap-3 px-6 h-11 shrink-0 sticky top-0 z-10
             bg-white dark:bg-[#141416] border-b border-gray-100 dark:border-[#232326]"
    >
      <button
        type="button"
        @click="designing ? (designing = false) : emit('close')"
        class="p-1.5 -ml-1.5 rounded-md text-gray-500 cursor-pointer
               hover:bg-gray-100 dark:hover:bg-white/5 transition-colors"
        :aria-label="t('things.back')"
      >
        <ArrowLeft class="w-4.5 h-4.5" />
      </button>
      <h1 class="text-base font-semibold text-[#1c1c1e] dark:text-[#f4f4f5]">
        {{ designing ? t('things.new_kind_title') : t('things.manager_title') }}
      </h1>
      <span
        v-if="!designing"
        class="text-[11px] font-medium px-2 py-0.5 rounded-full
               bg-gray-100 dark:bg-white/10 text-gray-500 dark:text-gray-400"
      >
        {{ kinds.length }}
      </span>

    </header>

    <div class="flex-1 px-6 md:px-10 py-8 w-full max-w-4xl mx-auto">
      <KindDesigner
        v-if="designing"
        :existing="kinds.map(k => k.nodeType)"
        @create="(type, fields) => { designing = false; emit('create', type, fields); }"
        @cancel="designing = false"
      />

      <template v-else>
        <!-- The vault in one line, before the list of its parts. -->
        <p class="text-xs text-gray-400 mb-6">
          {{ t('things.manager_summary', { things: totalThings, loose: totalLoose }) }}
        </p>

        <div class="flex items-center gap-2 mb-5">
          <div class="relative flex-1">
            <Search class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400" />
            <input
              v-model="search"
              type="text"
              spellcheck="false"
              :placeholder="t('things.manager_search')"
              class="w-full pl-9 pr-3 py-2 rounded-lg text-sm outline-none
                     bg-gray-50 dark:bg-white/5 border border-gray-200 dark:border-gray-700
                     text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-400"
            />
          </div>
          <button
            type="button"
            @click="cycleSort"
            class="flex items-center gap-1.5 px-3 py-2 rounded-lg text-xs text-gray-500
                   dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-white/5 cursor-pointer
                   border border-gray-200 dark:border-gray-700"
          >
            <ArrowUpDown class="w-3.5 h-3.5" />
            {{ t(`things.sort_${sortBy}`) }}
          </button>

          <!--
            Beside the search rather than up in the title bar.
            
            Up there it sat in the corner opposite the back arrow, at the edge
            of a wide empty header, and went unnoticed. Here it is at the end
            of the row somebody is already using — and it needs no `v-if` of
            its own, because this row only exists on the list.
          -->
          <button
            type="button"
            @click="designing = true"
            class="flex items-center gap-1.5 px-3 py-2 rounded-lg text-xs font-medium
                   text-white bg-blue-600 hover:bg-blue-700 cursor-pointer whitespace-nowrap"
          >
            <Plus class="w-3.5 h-3.5" />
            {{ t('things.new_kind') }}
          </button>
        </div>

        <div
          class="grid grid-cols-[1fr_72px_72px_72px_28px] gap-3 px-2 pb-2
                 border-b border-gray-200 dark:border-[#2c2c2c]
                 text-[10px] uppercase tracking-wider text-gray-400"
        >
          <span>{{ t('things.col_kind') }}</span>
          <span class="text-right">{{ t('things.col_things') }}</span>
          <span class="text-right">{{ t('things.col_fields') }}</span>
          <span class="text-right">{{ t('things.col_loose') }}</span>
          <span />
        </div>

        <!--
          The row opens a kind; the bin takes it away. Two acts on one line, so
          two targets — a row that did both on one click would be a row nobody
          could use.
        -->
        <div
          v-for="row in rows"
          :key="row.nodeType"
          class="group grid grid-cols-[1fr_72px_72px_72px_28px] gap-3 px-2 py-2.5
                 border-b border-gray-50 dark:border-[#1c1c1e] items-center rounded-md
                 hover:bg-gray-50 dark:hover:bg-white/5"
        >
          <button
            type="button"
            @click="emit('open', row.nodeType)"
            class="flex items-center gap-2 min-w-0 text-left cursor-pointer"
          >
            <component :is="iconForNodeType(row.nodeType)" class="w-4 h-4 text-gray-400 flex-none" />
            <span class="font-mono text-xs text-[#1c1c1e] dark:text-[#f4f4f5] truncate">
              {{ row.nodeType }}
            </span>
          </button>

          <span class="text-right font-mono text-xs text-gray-500 dark:text-gray-400 tabular-nums">
            {{ row.count }}
          </span>
          <span class="text-right font-mono text-xs text-gray-500 dark:text-gray-400 tabular-nums">
            {{ row.fields }}
          </span>
          <span
            class="text-right font-mono text-xs tabular-nums"
            :class="row.loose ? 'text-amber-600 dark:text-amber-400' : 'text-gray-300 dark:text-gray-600'"
          >
            {{ row.loose || '—' }}
          </span>

          <!--
            Both verbs in one place. Renaming lived on the kind's own page and
            deleting lived here, so managing kinds meant knowing which screen
            held which — and a second bare icon beside the bin is the thing
            that had to be explained the last time it was tried.
          -->
          <button
            type="button"
            @click="openMenu(row.nodeType, $event)"
            :title="t('things.row_actions')"
            class="justify-self-end p-1 rounded-md cursor-pointer transition-colors
                   text-gray-300 dark:text-gray-600 group-hover:text-gray-400
                   hover:bg-gray-100 dark:hover:bg-white/10"
          >
            <MoreHorizontal class="w-4 h-4" />
          </button>
        </div>


        <KindRowMenu
          v-if="menuFor && menuAt"
          :node-type="menuFor"
          :at="menuAt"
          @rename="emit('rename', menuFor!); closeMenu()"
          @remove="emit('remove', menuFor!); closeMenu()"
          @close="closeMenu"
        />

        <p v-if="!rows.length" class="py-10 text-center text-xs text-gray-400">
          {{ t('things.manager_nothing') }}
        </p>

        <p class="mt-8 text-[11px] text-gray-400 leading-relaxed max-w-lg">
          {{ t('things.manager_note') }}
        </p>
      </template>
    </div>
  </div>
</template>
