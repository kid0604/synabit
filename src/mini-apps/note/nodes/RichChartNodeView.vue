<script setup lang="ts">
/**
 * A chart of a Rich Table, away from the table. It finds the table by its
 * `name:` every time the note changes, so it draws what the table holds now:
 * edit a cell above, and the chart further down follows.
 *
 * Choosing a table that has no name gives it one — a chart can only point at
 * a table by name — and choosing a table without a chart view offers to make
 * one, so the block is never a dead end.
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { NodeViewWrapper } from '@tiptap/vue-3';
import { Settings2, ChartColumn } from 'lucide-vue-next';
import ChartView from '../../../shared/rich-table/components/ChartView.vue';
import FloatingPanel from '../../../shared/rich-table/components/FloatingPanel.vue';
import { parseRichTable, serializeRichTable } from '../../../shared/rich-table/markdown';
import { chartModel } from '../../../shared/rich-table/chartData';
import { shownRows } from '../../../shared/rich-table/view';
import { layoutOf, type RichTable, type View } from '../../../shared/rich-table/model';
import { addView } from '../../../shared/rich-table/ops';
import '../../../shared/rich-table/richTable.css';

const props = defineProps<{
  node: any;
  editor: any;
  selected: boolean;
  updateAttributes: (attrs: Record<string, unknown>) => void;
  deleteNode: () => void;
}>();
const { t, locale } = useI18n();

// The note changes under this block; it re-reads the tables when it does.
const version = ref(0);
const bump = () => { version.value++; };
onMounted(() => props.editor?.on('update', bump));
onBeforeUnmount(() => props.editor?.off('update', bump));

interface Found { table: RichTable; pos: number; label: string; readOnly: boolean }

/** Tables read before, by node: typing elsewhere in the note re-reads none of them. */
const parsedNodes = new WeakMap<object, ReturnType<typeof parseRichTable>>();

const tables = computed<Found[]>(() => {
  void version.value;
  const out: Found[] = [];
  let unnamed = 0;
  props.editor?.state.doc.forEach((node: any, pos: number) => {
    if (node.type.name !== 'richTable') return;
    let parsed = parsedNodes.get(node);
    if (!parsed) {
      parsed = parseRichTable(node.attrs.source);
      parsedNodes.set(node, parsed);
    }
    const label = parsed.table.name ?? t('rich_table.chart_block.unnamed', { n: ++unnamed });
    out.push({ table: parsed.table, pos, label, readOnly: !!parsed.readOnly });
  });
  return out;
});

/** The views of a table that have a chart: chart views, and table views with one pinned. */
const chartViews = (table: RichTable): View[] =>
  table.views.filter((v) => v.chart && (layoutOf(v) === 'chart' || layoutOf(v) === 'table'));

const target = computed(() => tables.value.find((f) => f.table.name === props.node.attrs.of) ?? null);
const view = computed(() => {
  if (!target.value) return null;
  const views = chartViews(target.value.table);
  return views.find((v) => v.name === props.node.attrs.view) ?? views[0] ?? null;
});
const model = computed(() => {
  if (!target.value || !view.value) return null;
  const rows = shownRows(target.value.table, view.value, locale.value).rows;
  return chartModel(target.value.table, view.value.chart, rows, locale.value);
});

// ─── Choosing ───────────────────────────────────────────────────

const picker = ref<DOMRect | null>(null);

/** A name for a table that has none: `bang`, `bang-2`… — what a formula can call it too. */
function freshName(): string {
  const taken = new Set(tables.value.map((f) => f.table.name).filter(Boolean));
  const base = t('rich_table.chart_block.name_base');
  if (!taken.has(base)) return base;
  for (let i = 2; ; i++) if (!taken.has(`${base}-${i}`)) return `${base}-${i}`;
}

/** A table this block may write to — never one shown read-only, whose source must stay as it is. */
function rewrite(found: Found, table: RichTable): boolean {
  if (found.readOnly) return false;
  return props.editor.commands.setRichTableSource(found.pos, serializeRichTable(table));
}

function chooseTable(found: Found) {
  let table = found.table;
  if (!table.name) {
    table = { ...table, name: freshName() };
    if (!rewrite(found, table)) return;
  }
  const first = chartViews(table)[0];
  props.updateAttributes({ of: table.name, view: first?.name ?? '' });
}

function chooseView(name: string) {
  props.updateAttributes({ view: name });
  picker.value = null;
}

/** The table has no chart yet: give it a chart view, grouped by its first select column. */
function makeChartView() {
  const found = target.value;
  if (!found) return;
  const group = found.table.columns.find((c) => c.type === 'select')?.name ?? found.table.columns[0]?.name;
  // A name no view of the table has, so this block finds this view and no other.
  const base = t('rich_table.layouts.chart');
  const taken = new Set(found.table.views.map((v) => v.name));
  let name = base;
  for (let i = 2; taken.has(name); i++) name = `${base} ${i}`;
  if (!rewrite(found, addView(found.table, { name, layout: 'chart', chart: { kind: 'bar', x: group } }))) return;
  props.updateAttributes({ view: name });
}

const caption = computed(() => {
  if (!target.value) return t('rich_table.chart_block.title');
  return view.value?.name ? `${target.value.label} · ${view.value.name}` : target.value.label;
});
</script>

<template>
  <NodeViewWrapper class="rt-chart-block" :class="{ 'is-selected': selected }">
    <div class="rt" data-rt-interactive>
      <div class="rt-bar">
        <span class="rt-chart-caption"><ChartColumn :size="13" />{{ caption }}</span>
        <span class="rt-bar-spacer" />
        <button
          v-if="editor.isEditable"
          type="button"
          class="rt-bar-btn"
          :title="t('rich_table.chart_block.choose')"
          :aria-label="t('rich_table.chart_block.choose')"
          @click="picker = ($event.currentTarget as HTMLElement).getBoundingClientRect()"
        >
          <Settings2 :size="14" />
        </button>
      </div>

      <ChartView v-if="model" :model="model" :locale="locale" :title="target?.table.name" />
      <div v-else class="rt-chart rt-chart-empty">
        <template v-if="!tables.length">{{ t('rich_table.chart_block.no_tables') }}</template>
        <template v-else-if="!target">
          {{ t('rich_table.chart_block.pick') }}
          <div class="rt-chart-choices">
            <button v-for="f in tables" :key="f.pos" type="button" class="rt-btn" :disabled="f.readOnly && !f.table.name" @click="chooseTable(f)">{{ f.label }}</button>
          </div>
        </template>
        <template v-else>
          {{ t('rich_table.chart_block.no_chart') }}
          <div v-if="!target.readOnly" class="rt-chart-choices">
            <button type="button" class="rt-btn rt-btn-primary" @click="makeChartView">{{ t('rich_table.chart_block.make') }}</button>
          </div>
        </template>
      </div>

      <FloatingPanel v-if="picker" :anchor="picker" align-end :width="260" @close="picker = null">
        <div class="rt-menu">
          <div class="rt-menu-label">{{ t('rich_table.chart_block.table') }}</div>
          <button
            v-for="f in tables"
            :key="f.pos"
            type="button"
            class="rt-item"
            :disabled="f.readOnly && !f.table.name"
            @click="chooseTable(f)"
          >
            {{ f.label }}<span v-if="f.table.name === node.attrs.of" class="rt-tick">✓</span>
          </button>
          <template v-if="target">
            <div class="rt-menu-label">{{ t('rich_table.chart_block.view') }}</div>
            <button
              v-for="(v, i) in chartViews(target.table)"
              :key="i"
              type="button"
              class="rt-item"
              @click="chooseView(v.name ?? '')"
            >
              {{ v.name || t(`rich_table.layouts.${layoutOf(v)}`) }}<span v-if="v === view" class="rt-tick">✓</span>
            </button>
          </template>
          <div class="rt-menu-section" />
          <button type="button" class="rt-item rt-danger" @click="deleteNode()">{{ t('rich_table.chart_block.remove') }}</button>
        </div>
      </FloatingPanel>
    </div>
  </NodeViewWrapper>
</template>

<style>
.rt-chart-block .rt { margin: 8px 0; }
.rt-chart-block.is-selected .rt-chart { box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-accent, #1a66cc) 45%, transparent); }
.rt-chart-caption { display: inline-flex; align-items: center; gap: 6px; color: var(--rt-muted); font-size: 12.5px; }
.rt-chart-choices { display: flex; flex-wrap: wrap; justify-content: center; gap: 6px; margin-top: 10px; }
</style>
