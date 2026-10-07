<script setup lang="ts">
/**
 * The body of a Rich Table: its rows (`RichTableRow`), and — when the view
 * groups — each group's heading and subtotals. Nothing here changes when the
 * selection moves.
 */
import type { RichTable } from '../model';
import { styleKey, type RowStyle } from '../rules';
import { ChevronRight } from 'lucide-vue-next';
import type { BodyItem } from './bodyItems';
import RichTableRow from './RichTableRow.vue';

defineProps<{
  table: RichTable;
  /** Left offset of each frozen column, `null` for the ones that scroll. */
  stuck: (number | null)[];
  locale: string;
  /** Changes when widths or wrapping change, so memoised rows re-render. */
  layoutKey: string;
  /** How an error code reads in the interface: `#DIV0` → `#CHIA0`. */
  errorLabels: Record<string, string>;
  items: BodyItem[];
  /** Columns this view hides, by index: kept in the DOM, so a cell's index is its column's. */
  hidden: ReadonlySet<number>;
  /** Tints from the view's colouring rules, by row of the table. */
  styles: ReadonlyMap<number, RowStyle>;
}>();
</script>

<template>
  <template v-for="item in items" :key="item.kind === 'row' ? item.ri : `${item.kind}:${item.key}`">
    <RichTableRow
      v-if="item.kind === 'row'"
      :row="table.rows[item.ri]"
      :columns="table.columns"
      :ri="item.ri"
      :di="item.di"
      :stuck="stuck"
      :hidden="hidden"
      :tint-key="styleKey(styles.get(item.ri))"
      :locale="locale"
      :layout-key="layoutKey"
      :error-labels="errorLabels"
    />

    <!-- A group's heading: click to fold it away. -->
    <div v-else-if="item.kind === 'group'" class="rt-row rt-group" role="row" :data-group="item.key">
      <div
        class="rt-group-head"
        role="rowheader"
        tabindex="0"
        :aria-expanded="!item.collapsed"
        @keydown.enter.prevent="($event.currentTarget as HTMLElement).click()"
        @keydown.space.prevent="($event.currentTarget as HTMLElement).click()"
      >
        <ChevronRight :size="14" class="rt-group-chevron" :class="{ 'is-open': !item.collapsed }" />
        <span v-if="item.color" class="rt-chip" :style="item.color">{{ item.label }}</span>
        <span v-else class="rt-group-label">{{ item.label }}</span>
        <span class="rt-muted">{{ item.count }}</span>
      </div>
    </div>

    <!-- A group's subtotals, in the footer's summaries. -->
    <div v-else class="rt-row rt-subtotal" role="row">
      <div class="rt-gutter" />
      <div
        v-for="(column, c) in table.columns"
        :key="c"
        class="rt-subtotal-cell"
        :class="{ 'rt-hidden': hidden.has(c), 'rt-stuck': stuck[c] !== null }"
        :style="stuck[c] !== null ? { left: `${stuck[c]}px` } : undefined"
      >{{ item.values[c] ?? '' }}</div>
    </div>
  </template>
</template>
