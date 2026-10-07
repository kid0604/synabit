<script setup lang="ts">
/**
 * One row of a Rich Table. A component of its own so that Vue skips it when
 * its props are the same as last time: an edit replaces only the edited
 * row's array, so moving round two thousand rows redraws the selection
 * outline, and editing a cell redraws the one row it is in. Its tint comes as
 * a string for the same reason — an object rebuilt on every change would
 * redraw every tinted row.
 */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import {
  displayText, fits, formulaKind, isBlank, isChecked, itemsOf, noteTarget,
  type Column,
} from '../model';
import { cellHtml } from '../cellHtml';
import { optionColor } from '../colors';
import type { RowStyle } from '../rules';

const props = defineProps<{
  row: string[];
  columns: Column[];
  /** The row's place in the table, and in the view. */
  ri: number;
  di: number;
  /** Left offset of each frozen column, `null` for the ones that scroll. */
  stuck: (number | null)[];
  hidden: ReadonlySet<number>;
  /** The row's tint from the view's rules, as `styleKey` writes it. */
  tintKey: string;
  locale: string;
  /** Changes when widths or wrapping change. */
  layoutKey: string;
  errorLabels: Record<string, string>;
}>();
const { t } = useI18n();

const tint = computed<RowStyle | undefined>(() => (props.tintKey ? JSON.parse(props.tintKey) : undefined));

function urlLabel(raw: string): string {
  try {
    const url = new URL(raw.trim());
    return url.host + (url.pathname === '/' ? '' : url.pathname);
  } catch {
    return raw;
  }
}
</script>

<template>
  <div
    class="rt-row rt-body-row"
    role="row"
    :data-r="di"
    :style="tint?.row ? { '--rt-row-tint': tint.row } : undefined"
  >
    <div class="rt-gutter" role="rowheader" :data-gutter="di">
      <span class="rt-rownum">{{ di + 1 }}</span>
      <button class="rt-open" type="button" :data-open="ri" tabindex="-1" :aria-label="t('rich_table.open_row', { n: di + 1 })">↗</button>
    </div>
    <div
      v-for="(column, c) in columns"
      :key="c"
      class="rt-cell"
      role="gridcell"
      :data-c="c"
      :class="[
        `rt-t-${column.type}`,
        {
          'rt-hidden': hidden.has(c),
          'rt-wrap': column.wrap,
          'rt-stuck': stuck[c] !== null,
          'rt-misfit': !fits(column, row[c] ?? ''),
          'rt-foreign': column.foreignType,
          'rt-t-number': column.type === 'formula' && formulaKind(row[c] ?? '') === 'number',
        },
      ]"
      :style="{
        ...(stuck[c] !== null ? { left: `${stuck[c]}px` } : {}),
        ...(tint?.cells?.[c] ? { '--rt-cell-tint': tint.cells[c] } : {}),
      }"
    >
      <template v-if="column.type === 'formula'">
        <span v-if="formulaKind(row[c] ?? '') === 'error'" class="rt-error">{{ errorLabels[row[c].trim()] ?? row[c] }}</span>
        <span v-else-if="formulaKind(row[c] ?? '') === 'checkbox'" class="rt-check is-readonly" :class="{ 'is-on': isChecked(row[c]) }" />
        <span v-else class="rt-text" :class="{ 'rt-num': formulaKind(row[c] ?? '') === 'number' }">{{ displayText(column as Column, row[c] ?? '', locale) }}</span>
      </template>
      <template v-else-if="isBlank(row[c] ?? '')">
        <span v-if="column.type === 'checkbox'" class="rt-check" />
      </template>
      <template v-else-if="!fits(column, row[c]) || column.foreignType">
        <span class="rt-text">{{ row[c] }}</span>
      </template>
      <span v-else-if="column.type === 'checkbox'" class="rt-check" :class="{ 'is-on': isChecked(row[c]) }" />
      <span v-else-if="column.type === 'select'" class="rt-chip" :style="optionColor(column as Column, row[c].trim())">{{ row[c] }}</span>
      <span v-else-if="column.type === 'multi'" class="rt-chips">
        <span v-for="x in itemsOf(row[c])" :key="x" class="rt-chip" :style="optionColor(column as Column, x)">{{ x }}</span>
      </span>
      <a v-else-if="column.type === 'url'" class="rt-link" :data-href="row[c].trim()">{{ urlLabel(row[c]) }}</a>
      <a v-else-if="column.type === 'note'" class="rt-wikilink" :data-note="noteTarget(row[c]) ?? ''">{{ displayText(column as Column, row[c], locale) }}</a>
      <!-- eslint-disable-next-line vue/no-v-html -- sanitised in cellHtml -->
      <span v-else-if="column.type === 'text'" class="rt-text" v-html="cellHtml(row[c])" />
      <span v-else class="rt-text">{{ displayText(column as Column, row[c], locale) }}</span>
    </div>
  </div>
</template>
