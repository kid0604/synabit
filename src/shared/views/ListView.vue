<script setup lang="ts">
/**
 * A list of nodes, of whatever type.
 *
 * The first view primitive. It is handed a `QueryResult` and knows nothing
 * about what is in it — no type is named anywhere below, and the only thing
 * `node_type` is used for is choosing an icon.
 *
 * `content-visibility` on the rows, for the reason `TaskListView` has it: a
 * vault can hold thousands of nodes, and the property is Baseline Newly
 * available, so a WebView that has never heard of it renders every row exactly
 * as it did before. See CLAUDE.md — it is only allowed here because it
 * degrades to the previous behaviour on its own.
 */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { Loader2, MoreHorizontal } from 'lucide-vue-next';
import { iconForNodeType } from './nodeTypeIcon';
import type { QueryResult, QueryRow } from './types';

const props = defineProps<{
  result: QueryResult | null;
  loading?: boolean;
  selectedId?: string | null;
  /** Shown when a node has no title of its own. */
  untitledLabel?: string;
  /** The row whose menu is open, so the button stays visible under it. */
  menuFor?: string | null;
  /**
   * A column to break the list into sections by.
   *
   * Named rather than indexed, because the caller knows the field and the view
   * knows where it landed — `columns` comes back from the engine after it has
   * dropped any name it could not read.
   */
  groupBy?: string;
}>();

const emit = defineEmits<{
  open: [row: QueryRow];
  /**
   * The row whose menu was asked for, or `null` to close.
   *
   * The menu itself is the caller's — it needs the vault, the node service and
   * the undo toast, none of which a view primitive should acquire. The list's
   * job is knowing which row was clicked.
   */
  menu: [row: QueryRow | null, at: { x: number; y: number } | null];
}>();

const { t } = useI18n();

const rows = computed(() => props.result?.rows ?? []);

/**
 * Whether the list is a page of something longer.
 *
 * `total` is the real count and `rows` is what came back under the limit, so
 * the two disagreeing is the normal case for a large result, not an error.
 */
const hasMore = computed(
  () => !!props.result && props.result.total > props.result.rows.length,
);

/**
 * The columns worth showing beside a title.
 *
 * `title` is already the row's own line, so repeating it as a cell would print
 * every name twice.
 */
const detailColumns = computed(() => {
  const columns = props.result?.columns ?? [];
  return columns
    .map((name, index) => ({ name, index }))
    .filter(c => c.name !== 'title');
});

const cellsFor = (row: QueryRow) =>
  detailColumns.value
    .map(c => row.cells[c.index])
    .filter(v => v !== undefined && v !== '');

/**
 * Where the menu should appear, in screen coordinates.
 *
 * The button's own position, read at the moment it is clicked, because the
 * menu cannot be drawn inside the row. Two things clip it there: the list
 * scrolls, and the row carries `content-visibility: auto`, which brings paint
 * containment with it and cuts off anything crossing the row's edge. The
 * caller renders it in a layer over the page instead.
 */
const openMenu = (event: MouseEvent, row: QueryRow) => {
  if (props.menuFor === row.id) {
    emit('menu', null, null);
    return;
  }
  const button = event.currentTarget as HTMLElement;
  const box = button.getBoundingClientRect();
  emit('menu', row, { x: box.right, y: box.bottom });
};

/**
 * The rows, in sections, when a group column was asked for.
 *
 * Insertion order rather than sorted: the rows arrive in the order the engine
 * sorted them, so the first group is the one holding the first row, and the
 * sections follow the sort instead of fighting it.
 *
 * A node with no value for the field gets its own section rather than being
 * dropped. "Nothing here yet" is a real answer about a book with no rating, and
 * hiding those rows would make the list disagree with its own count.
 */
const sections = computed(() => {
  const columns = props.result?.columns ?? [];
  const at = props.groupBy ? columns.indexOf(props.groupBy) : -1;
  if (at < 0) return null;

  const groups = new Map<string, QueryRow[]>();
  for (const row of rows.value) {
    const key = row.cells[at] || '';
    const bucket = groups.get(key);
    if (bucket) bucket.push(row);
    else groups.set(key, [row]);
  }
  return [...groups.entries()].map(([value, items]) => ({ value, items }));
});
</script>

<template>
  <div class="h-full flex flex-col min-h-0">
    <div v-if="loading" class="flex-1 flex items-center justify-center text-gray-400">
      <Loader2 class="w-5 h-5 animate-spin" />
    </div>

    <div
      v-else-if="rows.length === 0"
      class="flex-1 flex items-center justify-center px-6 text-center text-sm text-gray-400 dark:text-gray-500"
    >
      {{ t('things.nothing_here') }}
    </div>

    <div v-else class="flex-1 overflow-y-auto min-h-0">
      <template v-for="section in (sections ?? [{ value: null, items: rows }])" :key="section.value ?? '·'">
        <div
          v-if="section.value !== null"
          class="sticky top-0 z-10 px-4 py-1.5 bg-gray-50/95 dark:bg-[#141416]/95 backdrop-blur-sm
                 border-b border-gray-100 dark:border-[#232326]
                 text-[11px] font-semibold uppercase tracking-wider text-gray-400 dark:text-gray-500"
        >
          {{ section.value || t('things.no_value') }}
          <span class="ml-1.5 font-normal tabular-nums">{{ section.items.length }}</span>
        </div>

      <div
        v-for="row in section.items"
        :key="row.id"
        class="group relative flex items-start border-b border-gray-100 dark:border-[#232326]
               hover:bg-gray-50 dark:hover:bg-white/5 transition-colors"
        :class="row.id === selectedId ? 'bg-gray-100 dark:bg-white/10' : ''"
        style="content-visibility: auto; contain-intrinsic-size: auto 56px;"
      >
        <button
          type="button"
          @click="emit('open', row)"
          class="min-w-0 flex-1 text-left pl-4 pr-1 py-2.5 flex items-start gap-3 cursor-pointer"
        >
          <component
            :is="iconForNodeType(row.node_type)"
            class="w-4 h-4 mt-0.5 flex-shrink-0 text-gray-400 dark:text-gray-500"
          />
          <span class="min-w-0 flex-1">
            <span class="block truncate text-sm text-[#1c1c1e] dark:text-[#f4f4f5]">
              {{ row.title || untitledLabel || row.id }}
            </span>
            <span
              v-if="cellsFor(row).length"
              class="block truncate text-xs text-gray-400 dark:text-gray-500 mt-0.5"
            >
              {{ cellsFor(row).join(' · ') }}
            </span>
          </span>
        </button>

        <!--
          Kept out of the way until the row is under the cursor, and kept
          visible while its own menu is open — otherwise moving the mouse
          towards the menu makes the button that opened it disappear.
        -->
        <button
          type="button"
          @click.stop="openMenu($event, row)"
          class="flex-none mt-2 mr-2 p-1 rounded text-gray-400 transition-opacity cursor-pointer
                 hover:bg-gray-200/70 dark:hover:bg-white/10 hover:text-gray-600 dark:hover:text-gray-300
                 focus:opacity-100"
          :class="menuFor === row.id ? 'opacity-100' : 'opacity-0 group-hover:opacity-100'"
          :aria-label="t('things.row_actions')"
        >
          <MoreHorizontal class="w-4 h-4" />
        </button>

      </div>
      </template>

      <!--
        Said rather than implied. A list that silently stops at the limit reads
        as "this is everything", which is the same failure `total` used to have
        in the other direction.
      -->
      <p
        v-if="hasMore"
        class="px-4 py-3 text-xs text-gray-400 dark:text-gray-500"
      >
        {{ t('things.showing_of', { shown: rows.length, total: result?.total ?? 0 }) }}
      </p>
    </div>
  </div>
</template>
