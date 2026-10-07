<script setup lang="ts">
/**
 * Writing a column's formula, under the column, with the grid in view and
 * showing what the formula gives as it is typed (design §6.7). Notion's
 * formulas live in a small box far from the data, and say "invalid formula";
 * here the column answers, and a mistake is underlined where it is.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { Sigma } from 'lucide-vue-next';
import { FUNCTIONS, Problem, type Span } from '../../formula';
import { FUNCTION_DOCS } from '../../formula/docs';
import { computeTable, schemaOf } from '../formulas';
import { compile } from '../../formula';
import { displayText, isErrorCell, type RichTable } from '../model';
import { updateColumn } from '../ops';

const props = defineProps<{
  table: RichTable;
  index: number;
  locale: string;
  /** The other named tables of the note, for `gia[Giá]`. */
  others: Record<string, RichTable>;
  /** The rows the view shows, in order: the first of them is the example. */
  order: number[];
}>();
const emit = defineEmits<{
  /** The table as it would be with this formula — shown in the grid, not written. */
  preview: [table: RichTable | null];
  apply: [expr: string];
  cancel: [];
}>();
const { t } = useI18n();

const column = computed(() => props.table.columns[props.index]);
const text = ref(column.value.expr ?? '');
const area = ref<HTMLTextAreaElement | null>(null);
const caret = ref(text.value.length);

// ─── Checking, and the preview ──────────────────────────────────

const candidate = computed(() => updateColumn(props.table, props.index, { type: 'formula', expr: text.value }));

/** Checked as it is typed: compiling is cheap, and the underline follows the keys. */
const compiled = computed(() => compile(text.value, schemaOf(candidate.value, props.others)));

/**
 * The text the column is computed for — the typed text after a moment's
 * pause, so a column of two thousand rows is not recomputed per keystroke.
 */
const settled = ref(text.value);
let timer: ReturnType<typeof setTimeout> | undefined;
watch(text, (v) => {
  clearTimeout(timer);
  timer = setTimeout(() => { settled.value = v; }, 120);
});
onBeforeUnmount(() => clearTimeout(timer));

const done = computed(() => {
  if (compiled.value instanceof Problem) return null;
  return computeTable(updateColumn(props.table, props.index, { type: 'formula', expr: settled.value }), props.others);
});

const result = computed(() => {
  if (compiled.value instanceof Problem) return { problem: compiled.value, table: null };
  return { problem: done.value?.problems.get(props.index) ?? null, table: done.value?.table ?? null };
});

watch(() => result.value.table, (table) => emit('preview', table), { immediate: true });

const applyKeys = /Mac|iPhone|iPad/.test(navigator.userAgent) ? '⌘↵' : 'Ctrl+↵';

function message(p: Problem): string {
  const key = `rich_table.formula.errors.${p.key}`;
  const params = { ...p.params };
  if (p.key === 'arity') {
    params.expected = p.params.min === p.params.max ? String(p.params.min) : `${p.params.min}–${p.params.max}`;
  }
  return t(key, params);
}

/** What the first shown row comes to: "this gives…". */
const example = computed(() => {
  const table = result.value.table;
  if (!table || !props.order.length) return null;
  const raw = table.rows[props.order[0]]?.[props.index] ?? '';
  if (isErrorCell(raw)) return { error: true, text: t(`rich_table.formula.codes.${raw.slice(1)}`) };
  return { error: false, text: displayText({ ...column.value, type: 'formula' }, raw, props.locale) || '—' };
});

// ─── The mirror behind the textarea, carrying the underline ─────

const errorSpan = computed<Span | null>(() => {
  const p = result.value.problem;
  if (!p || p.code === 'CYCLE') return null;
  const s = p.span;
  // An error at the very end — "expected more" — underlines the last character.
  return s.from === s.to ? { from: Math.max(0, s.from - 1), to: s.from } : s;
});

const mirror = computed(() => {
  const s = errorSpan.value;
  if (!s) return [{ text: text.value, mark: false }];
  return [
    { text: text.value.slice(0, s.from), mark: false },
    { text: text.value.slice(s.from, s.to) || ' ', mark: true },
    { text: text.value.slice(s.to), mark: false },
  ];
});

// ─── Suggestions ────────────────────────────────────────────────

type Suggestion =
  | { kind: 'column'; name: string; from: number; table?: string }
  | { kind: 'function'; name: string; from: number };

const highlight = ref(0);
const dismissed = ref(false);

const fold = (s: string) => s.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase();

const suggestions = computed<Suggestion[]>(() => {
  if (dismissed.value) return [];
  const before = text.value.slice(0, caret.value);
  // Inside `[` … : columns — of this table, or of the table named right before it.
  const open = before.lastIndexOf('[');
  if (open > before.lastIndexOf(']') && !before.slice(open).includes('\n')) {
    const query = fold(before.slice(open + 1));
    const qualifier = /([\p{L}_][\p{L}\p{N}_]*)$/u.exec(before.slice(0, open))?.[1];
    const other = qualifier && qualifier.toLowerCase() !== 'table' ? props.others[qualifier] : undefined;
    const names = (other ?? props.table).columns.map((c) => c.name)
      .filter((n) => other || n !== column.value.name || /prev\(\s*$/i.test(before.slice(0, open)));
    return names.filter((n) => fold(n).includes(query)).slice(0, 8)
      .map((name) => ({ kind: 'column', name, from: open, table: other ? qualifier : undefined }));
  }
  // A word being typed: functions that start with it, columns that hold it.
  const word = /([\p{L}_][\p{L}\p{N}_]*)$/u.exec(before);
  if (!word || /["\d]$/.test(before.slice(0, word.index))) return [];
  const query = word[1];
  const fns = Object.keys(FUNCTION_DOCS).filter((n) => n.startsWith(query.toUpperCase()));
  const cols = props.table.columns.map((c) => c.name)
    .filter((n) => n !== column.value.name && fold(n).includes(fold(query)));
  return [
    ...fns.map((name) => ({ kind: 'function' as const, name, from: word.index })),
    ...cols.map((name) => ({ kind: 'column' as const, name, from: word.index })),
  ].slice(0, 8);
});

watch(suggestions, () => { highlight.value = 0; });

function accept(s: Suggestion) {
  const end = caret.value;
  let insert: string;
  let after = text.value.slice(end);
  if (s.kind === 'function') {
    insert = `${s.name}(`;
    if (after.startsWith('(')) after = after.slice(1);
  } else if (s.table || text.value[s.from] === '[') {
    insert = `[${s.name}]`;
    if (after.startsWith(']')) after = after.slice(1);
  } else {
    insert = `[${s.name}]`;
  }
  text.value = text.value.slice(0, s.from) + insert + after;
  const at = s.from + insert.length;
  nextTick(() => {
    area.value?.focus();
    area.value?.setSelectionRange(at, at);
    caret.value = at;
  });
}

/** The function whose parentheses the caret is in, or the one highlighted. */
const doc = computed(() => {
  const s = suggestions.value[highlight.value];
  if (s?.kind === 'function') return { name: s.name, ...FUNCTION_DOCS[s.name] };
  const before = text.value.slice(0, caret.value);
  let depth = 0;
  for (let i = before.length - 1; i >= 0; i--) {
    const ch = before[i];
    if (ch === ')') depth++;
    else if (ch === '(') {
      if (depth === 0) {
        const name = /([\p{L}_][\p{L}\p{N}_]*)\s*$/u.exec(before.slice(0, i))?.[1]?.toUpperCase();
        if (name && FUNCTION_DOCS[name]) return { name, ...FUNCTION_DOCS[name] };
        return null;
      }
      depth--;
    }
  }
  return null;
});

// ─── Keys ───────────────────────────────────────────────────────

function sync() {
  caret.value = area.value?.selectionStart ?? text.value.length;
  grow();
}

function grow() {
  const el = area.value;
  if (!el) return;
  el.style.height = 'auto';
  el.style.height = `${el.scrollHeight}px`;
}

function onInput() {
  dismissed.value = false;
  sync();
}

function apply() {
  if (!canApply.value) return;
  emit('apply', text.value);
}

function onKeydown(e: KeyboardEvent) {
  if (e.isComposing) return;
  const list = suggestions.value;
  if (list.length) {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      const step = e.key === 'ArrowDown' ? 1 : -1;
      highlight.value = (highlight.value + step + list.length) % list.length;
      return;
    }
    if ((e.key === 'Enter' && !e.metaKey && !e.ctrlKey) || e.key === 'Tab') {
      e.preventDefault();
      accept(list[highlight.value]);
      return;
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      dismissed.value = true;
      return;
    }
  }
  if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
    e.preventDefault();
    apply();
  } else if (e.key === 'Escape') {
    e.preventDefault();
    e.stopPropagation();
    emit('cancel');
  }
}

onMounted(() => {
  area.value?.focus();
  area.value?.setSelectionRange(text.value.length, text.value.length);
  grow();
});

const columnLabel = (s: Suggestion) => (s.kind === 'column' && s.table ? `${s.table}[${s.name}]` : s.name);
/** A formula that cannot run is not written: an empty one may be, and gives blanks. */
const canApply = computed(() => !result.value.problem || !text.value.trim());
const knownFunctions = Object.keys(FUNCTIONS).length;
</script>

<template>
  <div class="rt-fx" @pointerdown.stop>
    <div class="rt-fx-head">
      <Sigma :size="14" />
      <span class="rt-fx-name">{{ column.name }}</span>
      <span class="rt-muted">=</span>
    </div>

    <div class="rt-fx-box">
      <div class="rt-fx-mirror" aria-hidden="true"><template v-for="(part, i) in mirror" :key="i"><mark v-if="part.mark" class="rt-fx-err">{{ part.text }}</mark><template v-else>{{ part.text }}</template></template>{{ '​' }}</div>
      <textarea
        ref="area"
        v-model="text"
        class="rt-fx-input"
        rows="1"
        spellcheck="false"
        autocomplete="off"
        autocapitalize="off"
        :aria-label="t('rich_table.formula.label', { name: column.name })"
        :placeholder="t('rich_table.formula.placeholder')"
        @input="onInput"
        @keydown="onKeydown"
        @keyup="sync"
        @click="sync"
      />
    </div>

    <div v-if="suggestions.length" class="rt-fx-suggest" role="listbox">
      <button
        v-for="(s, i) in suggestions"
        :key="`${s.kind}:${s.name}`"
        type="button"
        class="rt-item"
        role="option"
        :aria-selected="i === highlight"
        :class="{ 'is-highlighted': i === highlight }"
        @pointerdown.prevent
        @click="accept(s)"
      >
        <span class="rt-fx-kind">{{ s.kind === 'function' ? 'ƒ' : '[ ]' }}</span>
        <span>{{ columnLabel(s) }}</span>
        <span v-if="s.kind === 'function'" class="rt-fx-sig">{{ FUNCTION_DOCS[s.name]?.sig }}</span>
      </button>
    </div>

    <div v-if="doc" class="rt-fx-doc">
      <code>{{ doc.sig }}</code>
      <span>{{ locale.startsWith('vi') ? doc.vi : doc.en }}</span>
      <code class="rt-muted">{{ doc.ex }}</code>
    </div>

    <div class="rt-fx-status" :class="{ 'is-error': result.problem }">
      <template v-if="!text.trim()">{{ t('rich_table.formula.hint', { n: knownFunctions, apply: applyKeys }) }}</template>
      <template v-else-if="result.problem">{{ t(`rich_table.formula.codes.${result.problem.code}`) }} — {{ message(result.problem) }}</template>
      <template v-else-if="example">
        {{ t('rich_table.formula.example') }}
        <b :class="{ 'rt-error-text': example.error }">{{ example.text }}</b>
      </template>
    </div>

    <div class="rt-menu-actions">
      <button type="button" class="rt-btn" @click="emit('cancel')">{{ t('common.cancel') }}</button>
      <button type="button" class="rt-btn rt-btn-primary" :disabled="!canApply" @click="apply">
        {{ t('rich_table.formula.apply') }} <span class="rt-kbd">⌘↵</span>
      </button>
    </div>
  </div>
</template>
