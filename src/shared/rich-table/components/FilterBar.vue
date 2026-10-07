<script setup lang="ts">
/**
 * The view's filter as pills — `Loại is Ăn uống`, `Số tiền > 100.000` — joined
 * by "and". What is saved is the text form (`filter.ts`); the pills are only
 * how it is edited.
 */
import { computed, nextTick, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { Filter, Plus, X, Sigma } from 'lucide-vue-next';
import { compile, Problem } from '../../formula';
import { schemaOf } from '../formulas';
import { needsValue, opsFor, parseFilter, type Condition } from '../filter';
import {
  displayText, draftOf, isChecked, rawFromInput, type Column, type RichTable,
} from '../model';
import { setConditions, updateView } from '../ops';
import FloatingPanel from './FloatingPanel.vue';

const props = defineProps<{ table: RichTable; locale: string; readOnly?: boolean; /** Which view's filter. */ view?: number }>();
const at = computed(() => props.view ?? 0);
const emit = defineEmits<{ change: [table: RichTable] }>();
const { t } = useI18n();

const conditions = computed(() => parseFilter(props.table.views[at.value].filter));
const columnOf = (name: string): Column | undefined => props.table.columns.find((c) => c.name === name);

/** The pill being edited: its place in the list (one past the end for a new one). */
const editing = ref<{ at: number; anchor: DOMRect; draft: Condition } | null>(null);

function label(c: Condition): string {
  const column = columnOf(c.column);
  const op = t(`rich_table.ops.${c.op}`);
  if (!needsValue(c.op)) return `${c.column} ${op}`;
  if (column?.type === 'checkbox') return `${c.column} ${op} ${isChecked(c.value) ? '✓' : '✗'}`;
  return `${c.column} ${op} ${column ? displayText(column, c.value, props.locale) : c.value}`;
}

/** What a new condition on a column starts with: its first choice, where it has choices. */
function fresh(c: Column): Condition {
  const value = c.type === 'checkbox' ? '[x]'
    : (c.type === 'select' || c.type === 'multi') ? (c.options?.[0] ?? '') : '';
  return { column: c.name, op: opsFor(c.type)[0], value };
}

function open(at: number, event: MouseEvent, draft?: Condition) {
  writing.value = null;
  editing.value = {
    at,
    anchor: (event.currentTarget as HTMLElement).getBoundingClientRect(),
    draft: draft ? { ...draft } : fresh(props.table.columns[0]),
  };
  if (draft) {
    const c = columnOf(draft.column);
    if (c) editing.value.draft.value = draftOf(c, draft.value, props.locale);
  }
}

/** Start a new pill on a column, from that column's menu. */
function addFor(column: number, anchor: DOMRect) {
  writing.value = null;
  editing.value = { at: conditions.value?.length ?? 0, anchor, draft: fresh(props.table.columns[column]) };
}
/** Whether a pill or the formula is being edited — a menu over the table, to the grid. */
const isOpen = computed(() => !!editing.value || !!writing.value);
defineExpose({ addFor, isOpen });

/** Into the panel as it opens, so the keys typed next go to it, not to the grid behind. */
const panelBody = ref<HTMLElement | null>(null);
function focusPanel() {
  nextTick(() => {
    const el = panelBody.value?.querySelector<HTMLElement>('input:not([type=hidden]), textarea, select');
    el?.focus({ preventScroll: true });
  });
}

const draftColumn = computed(() => (editing.value ? columnOf(editing.value.draft.column) : undefined));

function setColumn(name: string) {
  if (!editing.value) return;
  const c = columnOf(name);
  if (!c) return;
  const draft = fresh(c);
  if (opsFor(c.type).includes(editing.value.draft.op)) draft.op = editing.value.draft.op;
  editing.value.draft = draft;
}

function apply() {
  const e = editing.value;
  const list = conditions.value;
  if (!e || !list) return;
  const column = columnOf(e.draft.column);
  if (!column) return;
  const value = column.type === 'checkbox'
    ? (isChecked(e.draft.value) ? '[x]' : '[ ]')
    : needsValue(e.draft.op) ? rawFromInput(column, e.draft.value, props.locale) : '';
  if (needsValue(e.draft.op) && column.type !== 'checkbox' && !value) return;
  const next = list.slice();
  next.splice(e.at, 1, { ...e.draft, value });
  emit('change', setConditions(props.table, next, at.value));
  editing.value = null;
}

function remove(index: number) {
  const list = conditions.value;
  if (!list) return;
  emit('change', setConditions(props.table, list.filter((_, i) => i !== index), at.value));
  editing.value = null;
}

const clearUnread = () => emit('change', updateView(props.table, { filter: undefined }, at.value));

/**
 * The filter as text, in the formula language: what pills cannot say — `or`,
 * arithmetic, functions — said directly. A filter the pills can read goes back
 * to being pills.
 */
const writing = ref<{ anchor: DOMRect; text: string } | null>(null);
watch([() => editing.value?.at, () => !!editing.value, () => !!writing.value], ([, a, b]) => { if (a || b) focusPanel(); });

function openText(event: MouseEvent) {
  editing.value = null;
  writing.value = {
    anchor: (event.currentTarget as HTMLElement).getBoundingClientRect(),
    text: props.table.views[at.value].filter ?? '',
  };
}

const textProblem = computed(() => {
  const text = writing.value?.text.trim();
  if (!text) return null;
  const compiled = compile(text, schemaOf(props.table));
  return compiled instanceof Problem ? compiled : null;
});

function problemText(p: Problem): string {
  const params: Record<string, string | number> = { ...p.params };
  if (p.key === 'arity') params.expected = p.params.min === p.params.max ? String(p.params.min) : `${p.params.min}–${p.params.max}`;
  return `${t(`rich_table.formula.codes.${p.code}`)} — ${t(`rich_table.formula.errors.${p.key}`, params)}`;
}

function applyText() {
  if (!writing.value || textProblem.value) return;
  const text = writing.value.text.trim();
  emit('change', updateView(props.table, { filter: text || undefined }, at.value));
  writing.value = null;
}
</script>

<template>
  <div class="rt-filters">
    <template v-if="conditions">
      <button
        v-for="(c, i) in conditions"
        :key="i"
        type="button"
        class="rt-pill"
        :disabled="readOnly"
        @click="open(i, $event, c)"
      >
        {{ label(c) }}
      </button>
      <button v-if="!readOnly" type="button" class="rt-bar-btn" @click="open(conditions.length, $event)">
        <component :is="conditions.length ? Plus : Filter" :size="14" />
        <span>{{ conditions.length ? '' : t('rich_table.filter.add') }}</span>
      </button>
    </template>
    <span v-else class="rt-pill" :title="table.views[view ?? 0].filter">
      <button type="button" class="rt-pill-text" :disabled="readOnly" @click="openText">
        <Sigma :size="12" />{{ table.views[view ?? 0].filter }}
      </button>
      <button v-if="!readOnly" type="button" class="rt-icon-btn" :aria-label="t('rich_table.filter.clear')" @click="clearUnread">
        <X :size="12" />
      </button>
    </span>
    <button
      v-if="!readOnly && conditions"
      type="button"
      class="rt-bar-btn"
      :title="t('rich_table.filter.as_formula')"
      :aria-label="t('rich_table.filter.as_formula')"
      @click="openText"
    >
      <Sigma :size="13" />
    </button>

    <FloatingPanel v-if="writing" :anchor="writing.anchor" :width="380" @close="writing = null">
      <div ref="panelBody" class="rt-menu rt-menu-padded">
        <div class="rt-menu-label">{{ t('rich_table.filter.as_formula') }}</div>
        <textarea
          v-model="writing.text"
          class="rt-input rt-fx-input rt-fx-plain"
          rows="2"
          spellcheck="false"
          :placeholder="t('rich_table.filter.formula_placeholder')"
          @keydown.enter.exact.prevent="applyText"
        />
        <div class="rt-fx-status" :class="{ 'is-error': textProblem }">
          {{ textProblem ? problemText(textProblem) : t('rich_table.filter.formula_hint') }}
        </div>
        <div class="rt-menu-actions">
          <button type="button" class="rt-btn" @click="writing = null">{{ t('common.cancel') }}</button>
          <button type="button" class="rt-btn rt-btn-primary" :disabled="!!textProblem" @click="applyText">{{ t('rich_table.filter.apply') }}</button>
        </div>
      </div>
    </FloatingPanel>

    <FloatingPanel v-if="editing" :anchor="editing.anchor" :width="260" @close="editing = null">
      <div ref="panelBody" class="rt-menu rt-menu-padded" @keydown.enter.prevent="apply">
        <select class="rt-select" :value="editing.draft.column" @change="setColumn(($event.target as HTMLSelectElement).value)">
          <option v-for="c in table.columns" :key="c.name" :value="c.name">{{ c.name }}</option>
        </select>
        <select v-if="draftColumn" v-model="editing.draft.op" class="rt-select">
          <option v-for="op in opsFor(draftColumn.type)" :key="op" :value="op">{{ t(`rich_table.ops.${op}`) }}</option>
        </select>
        <template v-if="draftColumn && needsValue(editing.draft.op)">
          <select v-if="draftColumn.type === 'checkbox'" v-model="editing.draft.value" class="rt-select">
            <option value="[x]">{{ t('rich_table.filter.checked') }}</option>
            <option value="[ ]">{{ t('rich_table.filter.unchecked') }}</option>
          </select>
          <select
            v-else-if="(draftColumn.type === 'select' || draftColumn.type === 'multi') && draftColumn.options?.length"
            v-model="editing.draft.value"
            class="rt-select"
          >
            <option v-for="o in draftColumn.options" :key="o" :value="o">{{ o }}</option>
          </select>
          <input
            v-else-if="draftColumn.type === 'date'"
            v-model="editing.draft.value"
            class="rt-input"
            type="date"
          >
          <input v-else v-model="editing.draft.value" class="rt-input" :placeholder="t('rich_table.filter.value')">
        </template>
        <div class="rt-menu-actions">
          <button v-if="conditions && editing.at < conditions.length" type="button" class="rt-btn" @click="remove(editing.at)">
            {{ t('common.delete') }}
          </button>
          <button type="button" class="rt-btn rt-btn-primary" @click="apply">{{ t('rich_table.filter.apply') }}</button>
        </div>
      </div>
    </FloatingPanel>
  </div>
</template>
