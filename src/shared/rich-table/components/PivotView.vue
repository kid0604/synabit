<script setup lang="ts">
/**
 * A pivot: rows grouped by one column, columns by another, each cell one
 * measure of the rows in both — Excel's PIVOTBY, as a view. Totals at the
 * right and the foot. Built on the same grouping and measuring as the charts,
 * so a bar and the cell under it never disagree.
 */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { BLANK_KEY, groupRows, measure, measureFormat, parseBy, parseMeasure, type Group } from '../aggregate';
import { formatNumber, type PivotSpec, type RichTable } from '../model';

const props = defineProps<{ table: RichTable; rows: number[]; spec?: PivotSpec; locale: string }>();
const { t } = useI18n();

const rowBy = computed(() => {
  const by = parseBy(props.spec?.rows);
  return by && props.table.columns.some((c) => c.name === by.column) ? by : null;
});
const colBy = computed(() => {
  const by = parseBy(props.spec?.columns);
  return by && props.table.columns.some((c) => c.name === by.column) ? by : null;
});
const m = computed(() => parseMeasure(props.spec?.value) ?? { fn: 'count' as const });
const format = computed(() => measureFormat(props.table, m.value));

const rowGroups = computed(() => (rowBy.value ? groupRows(props.table, props.rows, rowBy.value, props.locale) : []));
const colGroups = computed<Group[]>(() => (colBy.value ? groupRows(props.table, props.rows, colBy.value, props.locale) : []));

const label = (g: Group) => (g.key === BLANK_KEY ? t('rich_table.chart.blank') : g.label);
const fmt = (n: number | null) => (n === null ? '' : formatNumber(n, props.locale, format.value));

function cell(r: Group, c: Group | null): string {
  const rows = c ? r.rows.filter((x) => c.rows.includes(x)) : r.rows;
  return rows.length ? fmt(measure(props.table, rows, m.value)) : '';
}
const columnTotal = (c: Group) => fmt(measure(props.table, c.rows, m.value));
const total = computed(() => fmt(measure(props.table, props.rows, m.value)));
const measureLabel = computed(() => (m.value.column ? `${t(`rich_table.measures.${m.value.fn}`)} · ${m.value.column}` : t('rich_table.measures.count')));
</script>

<template>
  <div class="rt-pivot">
    <div v-if="!rowBy" class="rt-chart-empty">{{ t('rich_table.pivot.choose_rows') }}</div>
    <div v-else class="rt-pivot-scroll">
      <table class="rt-pivot-table">
        <thead>
          <tr>
            <th class="rt-pivot-corner">{{ rowBy.column }} <span class="rt-muted">/ {{ measureLabel }}</span></th>
            <th v-for="c in colGroups" :key="c.key">{{ label(c) }}</th>
            <th class="rt-pivot-total">{{ t('rich_table.pivot.total') }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="r in rowGroups" :key="r.key">
            <th>{{ label(r) }}</th>
            <td v-for="c in colGroups" :key="c.key">{{ cell(r, c) }}</td>
            <td class="rt-pivot-total">{{ cell(r, null) }}</td>
          </tr>
        </tbody>
        <tfoot>
          <tr>
            <th>{{ t('rich_table.pivot.total') }}</th>
            <td v-for="c in colGroups" :key="c.key">{{ columnTotal(c) }}</td>
            <td class="rt-pivot-total">{{ total }}</td>
          </tr>
        </tfoot>
      </table>
    </div>
  </div>
</template>
