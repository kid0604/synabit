<script setup lang="ts">
/**
 * A view's settings: its name and layout, and what that layout needs —
 * grouping, hidden columns, colouring rules and a chart above the grid for a
 * table; the chart itself; a pivot's rows, columns and measure; a board's
 * lanes and cards. Every change is written as it is made, so the view behind
 * the menu shows it at once.
 */
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import {
  Table2, ChartColumn, Grid3x3, Columns3, Copy, Trash2, Plus, X,
} from 'lucide-vue-next';
import { BUCKETS, MEASURES, formatBy, formatMeasure, parseBy, parseMeasure, type Bucket, type MeasureFn } from '../aggregate';
import { kindOf } from '../chartData';
import {
  CHART_KINDS, LAYOUTS, OPTION_COLORS, ROW_HEIGHTS, layoutOf, rowHeightOf,
  type ChartSpec, type Layout, type RichTable, type Rule, type View,
} from '../model';
import { addView, removeView, updateView } from '../ops';
import { ruleProblem } from '../rules';
import { optionColor } from '../colors';

const props = defineProps<{ table: RichTable; index: number }>();
const emit = defineEmits<{ change: [table: RichTable]; select: [index: number]; close: [] }>();
const { t } = useI18n();

const view = computed<View>(() => props.table.views[props.index] ?? {});
const layout = computed(() => layoutOf(view.value));
const name = ref(view.value.name ?? '');
watch(() => view.value.name, (n) => { name.value = n ?? ''; });

const set = (patch: Partial<View>) => emit('change', updateView(props.table, patch, props.index));
const LAYOUT_ICONS: Record<Layout, unknown> = { table: Table2, chart: ChartColumn, pivot: Grid3x3, board: Columns3 };

const columns = computed(() => props.table.columns);
const numeric = computed(() => columns.value.filter((c) => c.type === 'number' || c.type === 'formula'));
const selects = computed(() => columns.value.filter((c) => c.type === 'select'));
const isDate = (name: string | undefined) => {
  const c = columns.value.find((x) => x.name === name);
  return c?.type === 'date' || c?.type === 'formula';
};

function setLayout(next: Layout) {
  const patch: Partial<View> = { layout: next === 'table' ? undefined : next };
  // A new layout starts from something that shows, not an empty frame.
  const firstGroup = selects.value[0]?.name ?? columns.value[0]?.name;
  if (next === 'chart' && !view.value.chart?.x) patch.chart = { kind: 'bar', x: firstGroup, ...view.value.chart };
  if (next === 'pivot' && !view.value.pivot?.rows) patch.pivot = { rows: firstGroup, ...view.value.pivot };
  if (next === 'board' && !view.value.board?.by && selects.value[0]) patch.board = { by: selects.value[0].name, ...view.value.board };
  set(patch);
}

function commitName() {
  const n = name.value.trim();
  if (n !== (view.value.name ?? '')) set({ name: n || undefined });
}

// ─── "By" and "measure" pickers ─────────────────────────────────

function byColumn(spec: string | undefined) { return parseBy(spec)?.column ?? ''; }
function byBucket(spec: string | undefined) { return parseBy(spec)?.bucket ?? ''; }
function withBy(column: string, bucket: string): string | undefined {
  if (!column) return undefined;
  return formatBy({ column, bucket: isDate(column) && bucket ? (bucket as Bucket) : undefined });
}
function measureFn(spec: string | undefined) { return parseMeasure(spec)?.fn ?? 'count'; }
function measureColumn(spec: string | undefined) { return parseMeasure(spec)?.column ?? ''; }
function withMeasure(fn: string, column: string): string {
  if (fn === 'count') return 'count';
  return formatMeasure({ fn: fn as MeasureFn, column: column || numeric.value[0]?.name });
}

// ─── Chart ──────────────────────────────────────────────────────

const chart = computed<ChartSpec>(() => view.value.chart ?? {});
const chartKind = computed(() => kindOf(chart.value));
function setChart(patch: Partial<ChartSpec>) {
  const next: ChartSpec = { ...chart.value, ...patch };
  for (const k of Object.keys(patch)) if (patch[k] === undefined || patch[k] === false) delete next[k];
  set({ chart: next });
}
const pinned = computed(() => layout.value === 'table' && !!view.value.chart);
function togglePinned() {
  set({ chart: pinned.value ? undefined : { kind: 'bar', x: selects.value[0]?.name ?? columns.value[0]?.name } });
}

// ─── Rules ──────────────────────────────────────────────────────

const rules = computed<Rule[]>(() => view.value.rules ?? []);
function setRule(i: number, patch: Partial<Rule>) {
  const next = rules.value.map((r, k) => (k === i ? { ...r, ...patch } : r));
  set({ rules: next });
}
function removeRule(i: number) {
  const next = rules.value.filter((_, k) => k !== i);
  set({ rules: next.length ? next : undefined });
}
function addRule(scale: boolean) {
  const column = numeric.value[0]?.name;
  const rule: Rule = scale
    ? { scale: ['green', 'red'], column }
    : { when: column ? `[${column}] > 0` : '', style: { row: 'yellow' } };
  set({ rules: [...rules.value, rule] });
}
function problemOf(rule: Rule): string {
  const p = ruleProblem(props.table, rule);
  if (!p) return '';
  if (p === 'no_column') return t('rich_table.view.rule_no_column');
  return t(`rich_table.formula.errors.${p.key}`, { ...p.params, expected: '' });
}
const ruleTarget = (r: Rule) => (r.style?.cell ? 'cell' : 'row');
const ruleColor = (r: Rule) => r.style?.cell ?? r.style?.row ?? 'yellow';

// ─── Hidden columns, board fields ───────────────────────────────

function toggleHidden(name: string) {
  const hide = new Set(view.value.hide ?? []);
  if (hide.has(name)) hide.delete(name); else hide.add(name);
  // Never every column: a view that hides them all shows nothing to unhide from.
  if (hide.size >= columns.value.length) return;
  set({ hide: hide.size ? [...hide] : undefined });
}

function setBoard(patch: Record<string, unknown>) {
  set({ board: { ...view.value.board, ...patch } });
}
function toggleShown(name: string) {
  const show = new Set(view.value.board?.show ?? []);
  if (show.has(name)) show.delete(name); else show.add(name);
  setBoard({ show: [...show] });
}

function duplicate() {
  const copy: View = JSON.parse(JSON.stringify(view.value));
  copy.name = t('rich_table.view.copy_of', { name: view.value.name || t(`rich_table.layouts.${layout.value}`) });
  emit('change', addView(props.table, copy, props.index + 1));
  emit('select', props.index + 1);
  emit('close');
}

const confirmingDelete = ref(false);
function remove() {
  if (!confirmingDelete.value) {
    confirmingDelete.value = true;
    return;
  }
  emit('change', removeView(props.table, props.index));
  emit('select', Math.max(0, props.index - 1));
  emit('close');
}

const value = (e: Event) => (e.target as HTMLInputElement | HTMLSelectElement).value;
const checked = (e: Event) => (e.target as HTMLInputElement).checked;
</script>

<template>
  <div class="rt-menu rt-view-menu">
    <div class="rt-menu-section">
      <input
        v-model="name"
        class="rt-input"
        :placeholder="t(`rich_table.layouts.${layout}`)"
        :aria-label="t('rich_table.view.name')"
        @keydown.enter.prevent="commitName"
        @blur="commitName"
      >
      <div class="rt-segmented" role="radiogroup" :aria-label="t('rich_table.view.layout')">
        <button
          v-for="l in LAYOUTS"
          :key="l"
          type="button"
          role="radio"
          :aria-checked="layout === l"
          :class="{ 'is-active': layout === l }"
          @click="setLayout(l)"
        >
          <component :is="LAYOUT_ICONS[l]" :size="14" />{{ t(`rich_table.layouts.${l}`) }}
        </button>
      </div>
    </div>

    <!-- ─── Table ─── -->
    <template v-if="layout === 'table'">
      <div class="rt-menu-section">
        <div class="rt-menu-label">{{ t('rich_table.view.row_height') }}</div>
        <div class="rt-segmented" role="radiogroup" :aria-label="t('rich_table.view.row_height')">
          <button
            v-for="h in ROW_HEIGHTS"
            :key="h"
            type="button"
            role="radio"
            :aria-checked="rowHeightOf(view) === h"
            :class="{ 'is-active': rowHeightOf(view) === h }"
            @click="set({ rowHeight: h === 'short' ? undefined : h })"
          >{{ t(`rich_table.row_heights.${h}`) }}</button>
        </div>
      </div>

      <div class="rt-menu-section">
        <div class="rt-menu-label">{{ t('rich_table.view.group') }}</div>
        <div class="rt-menu-pair">
          <select class="rt-select" :value="byColumn(view.group)" @change="set({ group: withBy(value($event), byBucket(view.group) || 'month') })">
            <option value="">{{ t('rich_table.view.none') }}</option>
            <option v-for="c in columns" :key="c.name" :value="c.name">{{ c.name }}</option>
          </select>
          <select v-if="isDate(byColumn(view.group))" class="rt-select" :value="byBucket(view.group) || 'day'" @change="set({ group: withBy(byColumn(view.group), value($event)) })">
            <option v-for="b in BUCKETS" :key="b" :value="b">{{ t(`rich_table.buckets.${b}`) }}</option>
          </select>
        </div>
      </div>

      <div class="rt-menu-section">
        <div class="rt-menu-label">{{ t('rich_table.view.columns') }}</div>
        <label v-for="c in columns" :key="c.name" class="rt-check-row">
          <input type="checkbox" :checked="!view.hide?.includes(c.name)" @change="toggleHidden(c.name)">
          <span>{{ c.name }}</span>
        </label>
      </div>

      <div class="rt-menu-section">
        <div class="rt-menu-label">{{ t('rich_table.view.rules') }}</div>
        <div v-for="(r, i) in rules" :key="i" class="rt-rule">
          <template v-if="r.scale">
            <div class="rt-menu-pair">
              <span class="rt-muted">{{ t('rich_table.view.scale') }}</span>
              <select class="rt-select" :value="r.column" @change="setRule(i, { column: value($event) })">
                <option v-for="c in numeric" :key="c.name" :value="c.name">{{ c.name }}</option>
              </select>
              <button type="button" class="rt-icon-btn" :aria-label="t('common.delete')" @click="removeRule(i)"><X :size="12" /></button>
            </div>
            <div class="rt-menu-pair">
              <select class="rt-select" :value="r.scale[0]" @change="setRule(i, { scale: [value($event), r.scale[1]] })">
                <option v-for="c in OPTION_COLORS" :key="c" :value="c">{{ t(`rich_table.colors.${c}`) }}</option>
              </select>
              <span class="rt-muted">→</span>
              <select class="rt-select" :value="r.scale[1]" @change="setRule(i, { scale: [r.scale![0], value($event)] })">
                <option v-for="c in OPTION_COLORS" :key="c" :value="c">{{ t(`rich_table.colors.${c}`) }}</option>
              </select>
            </div>
          </template>
          <template v-else>
            <div class="rt-menu-pair">
              <input
                class="rt-input rt-mono"
                :value="r.when"
                :placeholder="'[Số tiền] > 1000000'"
                :aria-label="t('rich_table.view.when')"
                @change="setRule(i, { when: value($event) })"
              >
              <button type="button" class="rt-icon-btn" :aria-label="t('common.delete')" @click="removeRule(i)"><X :size="12" /></button>
            </div>
            <div class="rt-menu-pair">
              <select class="rt-select" :value="ruleTarget(r)" @change="setRule(i, { style: value($event) === 'cell' ? { cell: ruleColor(r) } : { row: ruleColor(r) } })">
                <option value="row">{{ t('rich_table.view.target_row') }}</option>
                <option value="cell">{{ t('rich_table.view.target_cell') }}</option>
              </select>
              <span class="rt-swatches">
                <button
                  v-for="c in OPTION_COLORS"
                  :key="c"
                  type="button"
                  class="rt-dot"
                  :class="{ 'is-active': ruleColor(r) === c }"
                  :style="optionColor({ name: '', type: 'select', colors: { x: c } }, 'x')"
                  :aria-label="t(`rich_table.colors.${c}`)"
                  @click="setRule(i, { style: ruleTarget(r) === 'cell' ? { cell: c } : { row: c } })"
                />
              </span>
            </div>
          </template>
          <div v-if="problemOf(r)" class="rt-menu-note rt-error-text">{{ problemOf(r) }}</div>
        </div>
        <div class="rt-menu-pair">
          <button type="button" class="rt-item" @click="addRule(false)"><Plus :size="13" />{{ t('rich_table.view.add_rule') }}</button>
          <button v-if="numeric.length" type="button" class="rt-item" @click="addRule(true)"><Plus :size="13" />{{ t('rich_table.view.add_scale') }}</button>
        </div>
      </div>

      <div class="rt-menu-section">
        <label class="rt-check-row">
          <input type="checkbox" :checked="pinned" @change="togglePinned">
          <span>{{ t('rich_table.view.pin_chart') }}</span>
        </label>
      </div>
    </template>

    <!-- ─── Chart (its own view, or above a table) ─── -->
    <div v-if="layout === 'chart' || pinned" class="rt-menu-section">
      <div class="rt-menu-label">{{ t('rich_table.view.chart') }}</div>
      <div class="rt-kind-grid">
        <button
          v-for="k in CHART_KINDS"
          :key="k"
          type="button"
          class="rt-type"
          :class="{ 'is-active': chartKind === k }"
          @click="setChart({ kind: k })"
        >{{ t(`rich_table.charts.${k}`) }}</button>
      </div>

      <template v-if="chartKind === 'scatter'">
        <label class="rt-menu-row"><span>{{ t('rich_table.view.x') }}</span>
          <select class="rt-select" :value="byColumn(chart.x)" @change="setChart({ x: value($event) || undefined })">
            <option v-for="c in numeric" :key="c.name" :value="c.name">{{ c.name }}</option>
          </select>
        </label>
        <label class="rt-menu-row"><span>{{ t('rich_table.view.y') }}</span>
          <select class="rt-select" :value="chart.y" @change="setChart({ y: value($event) || undefined })">
            <option v-for="c in numeric" :key="c.name" :value="c.name">{{ c.name }}</option>
          </select>
        </label>
      </template>
      <template v-else>
        <label class="rt-menu-row">
          <span>{{ chartKind === 'number' ? t('rich_table.view.period') : t('rich_table.view.x') }}</span>
          <span class="rt-menu-pair">
            <select class="rt-select" :value="byColumn(chart.x)" @change="setChart({ x: withBy(value($event), byBucket(chart.x) || 'month') })">
              <option v-if="chartKind === 'number'" value="">{{ t('rich_table.view.none') }}</option>
              <option v-for="c in columns" :key="c.name" :value="c.name">{{ c.name }}</option>
            </select>
            <select v-if="isDate(byColumn(chart.x))" class="rt-select" :value="byBucket(chart.x) || 'day'" @change="setChart({ x: withBy(byColumn(chart.x), value($event)) })">
              <option v-for="b in BUCKETS" :key="b" :value="b">{{ t(`rich_table.buckets.${b}`) }}</option>
            </select>
          </span>
        </label>
        <label class="rt-menu-row">
          <span>{{ t('rich_table.view.y') }}</span>
          <span class="rt-menu-pair">
            <select class="rt-select" :value="measureFn(chart.y)" @change="setChart({ y: withMeasure(value($event), measureColumn(chart.y)) })">
              <option v-for="f in MEASURES" :key="f" :value="f">{{ t(`rich_table.measures.${f}`) }}</option>
            </select>
            <select v-if="measureFn(chart.y) !== 'count'" class="rt-select" :value="measureColumn(chart.y)" @change="setChart({ y: withMeasure(measureFn(chart.y), value($event)) })">
              <option v-for="c in numeric" :key="c.name" :value="c.name">{{ c.name }}</option>
            </select>
          </span>
        </label>
      </template>

      <label v-if="chartKind !== 'donut' && chartKind !== 'number'" class="rt-menu-row">
        <span>{{ t('rich_table.view.series') }}</span>
        <span class="rt-menu-pair">
          <select class="rt-select" :value="byColumn(chart.series)" @change="setChart({ series: withBy(value($event), isDate(value($event)) ? 'month' : '') })">
            <option value="">{{ t('rich_table.view.none') }}</option>
            <option v-for="c in columns" :key="c.name" :value="c.name">{{ c.name }}</option>
          </select>
          <select v-if="isDate(byColumn(chart.series))" class="rt-select" :value="byBucket(chart.series) || 'day'" @change="setChart({ series: withBy(byColumn(chart.series), value($event)) })">
            <option v-for="b in BUCKETS" :key="b" :value="b">{{ t(`rich_table.buckets.${b}`) }}</option>
          </select>
        </span>
      </label>
      <label v-if="['bar', 'hbar', 'area'].includes(chartKind) && chart.series" class="rt-check-row">
        <input type="checkbox" :checked="!!chart.stack" @change="setChart({ stack: checked($event) })">
        <span>{{ t('rich_table.view.stack') }}</span>
      </label>
      <label v-if="['line', 'area', 'bar'].includes(chartKind)" class="rt-check-row">
        <input type="checkbox" :checked="!!chart.cumulative" @change="setChart({ cumulative: checked($event) })">
        <span>{{ t('rich_table.view.cumulative') }}</span>
      </label>
    </div>

    <!-- ─── Pivot ─── -->
    <div v-if="layout === 'pivot'" class="rt-menu-section">
      <label v-for="axis in (['rows', 'columns'] as const)" :key="axis" class="rt-menu-row">
        <span>{{ t(`rich_table.view.pivot_${axis}`) }}</span>
        <span class="rt-menu-pair">
          <select class="rt-select" :value="byColumn(view.pivot?.[axis])" @change="set({ pivot: { ...view.pivot, [axis]: withBy(value($event), byBucket(view.pivot?.[axis])) } })">
            <option v-if="axis === 'columns'" value="">{{ t('rich_table.view.none') }}</option>
            <option v-for="c in columns" :key="c.name" :value="c.name">{{ c.name }}</option>
          </select>
          <select v-if="isDate(byColumn(view.pivot?.[axis]))" class="rt-select" :value="byBucket(view.pivot?.[axis]) || 'day'" @change="set({ pivot: { ...view.pivot, [axis]: withBy(byColumn(view.pivot?.[axis]), value($event)) } })">
            <option v-for="b in BUCKETS" :key="b" :value="b">{{ t(`rich_table.buckets.${b}`) }}</option>
          </select>
        </span>
      </label>
      <label class="rt-menu-row">
        <span>{{ t('rich_table.view.value') }}</span>
        <span class="rt-menu-pair">
          <select class="rt-select" :value="measureFn(view.pivot?.value)" @change="set({ pivot: { ...view.pivot, value: withMeasure(value($event), measureColumn(view.pivot?.value)) } })">
            <option v-for="f in MEASURES" :key="f" :value="f">{{ t(`rich_table.measures.${f}`) }}</option>
          </select>
          <select v-if="measureFn(view.pivot?.value) !== 'count'" class="rt-select" :value="measureColumn(view.pivot?.value)" @change="set({ pivot: { ...view.pivot, value: withMeasure(measureFn(view.pivot?.value), value($event)) } })">
            <option v-for="c in numeric" :key="c.name" :value="c.name">{{ c.name }}</option>
          </select>
        </span>
      </label>
    </div>

    <!-- ─── Board ─── -->
    <div v-if="layout === 'board'" class="rt-menu-section">
      <div v-if="!selects.length" class="rt-menu-note">{{ t('rich_table.board.choose') }}</div>
      <template v-else>
        <label class="rt-menu-row">
          <span>{{ t('rich_table.view.lanes') }}</span>
          <select class="rt-select" :value="view.board?.by" @change="setBoard({ by: value($event) })">
            <option v-for="c in selects" :key="c.name" :value="c.name">{{ c.name }}</option>
          </select>
        </label>
        <label class="rt-menu-row">
          <span>{{ t('rich_table.view.card_title') }}</span>
          <select class="rt-select" :value="view.board?.title ?? ''" @change="setBoard({ title: value($event) || undefined })">
            <option value="">{{ t('rich_table.view.automatic') }}</option>
            <option v-for="c in columns" :key="c.name" :value="c.name">{{ c.name }}</option>
          </select>
        </label>
        <div class="rt-menu-label">{{ t('rich_table.view.card_fields') }}</div>
        <label v-for="c in columns.filter((x) => x.name !== view.board?.by)" :key="c.name" class="rt-check-row">
          <input type="checkbox" :checked="view.board?.show?.includes(c.name)" @change="toggleShown(c.name)">
          <span>{{ c.name }}</span>
        </label>
      </template>
    </div>

    <div class="rt-menu-section">
      <button type="button" class="rt-item" @click="duplicate"><Copy :size="14" />{{ t('rich_table.view.duplicate') }}</button>
      <button v-if="table.views.length > 1" type="button" class="rt-item rt-danger" @click="remove">
        <Trash2 :size="14" />{{ confirmingDelete ? t('rich_table.column.delete_confirm') : t('rich_table.view.delete') }}
      </button>
    </div>
  </div>
</template>
