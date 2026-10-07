<script setup lang="ts">
/**
 * A Rich Table on screen: the grid, its keyboard, its menus.
 *
 * It knows nothing of ProseMirror. It is handed a table and hands back a new
 * one (`change`); whoever mounts it decides where that goes. In a note that is
 * a new attribute on the node, which is also what makes undo work.
 *
 * One `<textarea>` does three jobs. While a cell is only selected it sits,
 * invisible, over that cell and holds the focus: keys land in it, copy and
 * paste fire on it — a focused text field is the one element every WebView
 * reliably sends clipboard events to. When a key is typed, the same textarea
 * becomes the cell's editor, so the keystroke that started the edit is already
 * in it — which is what lets a Vietnamese input method compose `â` across that
 * first keystroke, where moving focus to a fresh field would break it.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, shallowRef, useId, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { openUrl } from '@tauri-apps/plugin-opener';
import { invoke } from '@tauri-apps/api/core';
import { save } from '@tauri-apps/plugin-dialog';
import { writeTextFile } from '@tauri-apps/plugin-fs';
import { type } from '@tauri-apps/plugin-os';
import { toCsv, toSheetRows, viewGrid } from '../export';
import {
  ArrowUp, ArrowDown, X, Search, Ellipsis, Plus, Trash2, ListOrdered,
  Maximize2, ArrowUpToLine, ArrowDownToLine, Copy, ChevronUp, ChevronDown, Settings2,
  Table2, ChartColumn, Grid3x3, Columns3, ClipboardCopy, Eraser, BetweenVerticalStart, BetweenVerticalEnd, FileDown,
} from 'lucide-vue-next';
import {
  dateOf, displayText, draftOf, formatNumber, freshColumnName, inferType, isChecked, isErrorCell, isReadOnlyColumn,
  itemsOf, layoutOf, rawFromInput, rowHeightOf, LAYOUTS, TABLE_NAME, type Column, type Layout, type RichTable, type View,
} from '../model';
import {
  addView, deleteColumn, deleteRows, fillValues, insertColumn, insertRows, moveColumn, moveRow, reorderRows, setCells,
  updateColumn, updateView,
  type CellEdit,
} from '../ops';
import { BLANK_KEY, compareKeys, keysOf, parseBy, type By, type Key } from '../aggregate';
import { ruleStyles } from '../rules';
import { chartModel } from '../chartData';
import type { BodyItem } from './bodyItems';
import ChartView from './ChartView.vue';
import PivotView from './PivotView.vue';
import BoardView from './BoardView.vue';
import ViewMenu from './ViewMenu.vue';
import { shownRows, sortKeys } from '../view';
import { computeTable } from '../formulas';
import { isSummaryKind, selectionStats, summariesFor, summarize, type SummaryKind } from '../summary';
import { htmlHasHeader, readClipboard, toHtml, toTsv } from '../clipboard';
import { optionColor } from '../colors';
import RichTableBody from './RichTableBody.vue';
import ColumnMenu from './ColumnMenu.vue';
import FormulaEditor from './FormulaEditor.vue';
import FilterBar from './FilterBar.vue';
import FloatingPanel from './FloatingPanel.vue';
import RowSheet from './RowSheet.vue';
import { TYPE_ICONS } from './icons';
import '../richTable.css';

const props = defineProps<{
  table: RichTable;
  locale: string;
  readOnly?: boolean;
  /** Something to say above the table: why it cannot be edited, say. */
  notice?: string;
  /** The other named tables of the note, for formulas that read them: `gia[Giá]`. */
  others?: () => Record<string, RichTable>;
  /** Notes whose title matches, for a Note column's picker. Without it, a note is typed by hand. */
  searchNotes?: (query: string) => Promise<{ id: string; title: string }[]>;
}>();
const emit = defineEmits<{
  change: [table: RichTable];
  /** Leave the table for the text around it. */
  exit: [where: 'up' | 'down' | 'select'];
  undo: [];
  redo: [];
  remove: [];
  navigate: [note: string];
}>();
const { t } = useI18n();

/**
 * How many rows the table had as this grid last wrote it. A table that
 * arrives with another count had rows added or taken elsewhere — undo, Syn,
 * sync. (The rows themselves come back re-read, so they cannot be compared.)
 */
let wroteLength = -1;
function change(next: RichTable) {
  wroteLength = next.rows.length;
  emit('change', next);
}

// ─── What is shown ──────────────────────────────────────────────

/**
 * The table as shown: itself, or — while a formula is being written — itself
 * with that formula's results, which nothing has written yet.
 */
const preview = shallowRef<RichTable | null>(null);
const display = computed(() => preview.value ?? props.table);

/** Which view is showing: the first, until a tab is chosen. Not saved — opening a note never writes it. */
const activeView = ref(0);
watch(() => props.table.views.length, (n) => { if (activeView.value >= n) activeView.value = Math.max(0, n - 1); });
const view = computed(() => props.table.views[activeView.value] ?? {});
const layout = computed(() => layoutOf(view.value));
const setView = (patch: Partial<View>) => change(updateView(props.table, patch, activeView.value));

/** `fresh`: a view deleted or added under the same index is another view, though its number is the same. */
function chooseView(i: number, fresh = false) {
  if (i === activeView.value && !fresh) return;
  if (editing.value) commit();
  activeView.value = i;
  active.value = null;
  anchor.value = null;
  focus.value = null;
  collapsed.value = new Set();
  keep.value = new Set();
}
/** Rows added in this session, shown whatever the filter says until it is changed. */
const keep = shallowRef<ReadonlySet<number>>(new Set());
const search = ref('');
const searching = ref(false);
const shown = computed(() => shownRows(display.value, view.value, props.locale, keep.value, search.value));
const columns = computed(() => display.value.columns);

/**
 * A group picked in the chart above the grid — a bar clicked — narrowing the
 * grid to it. Not saved: it is a look, not a filter.
 */
const focus = ref<{ by: By; key: string; label: string } | null>(null);
const base = computed(() => {
  const f = focus.value;
  if (!f) return shown.value.rows;
  const at = columns.value.findIndex((c) => c.name === f.by.column);
  if (at === -1) return shown.value.rows;
  return shown.value.rows.filter((r) =>
    keysOf(columns.value[at], display.value.rows[r][at] ?? '', f.by.bucket, props.locale).some((k) => k.key === f.key));
});

// ─── Groups ─────────────────────────────────────────────────────

const collapsed = ref(new Set<string>());
const groupBy = computed(() => {
  const by = parseBy(view.value.group);
  return by && columns.value.some((c) => c.name === by.column) ? by : null;
});

/**
 * The view's rows in groups, the groups in order. A row sits in the group
 * of its first value: a grid row can be in one place only, unlike a card on
 * a board.
 */
const groups = computed(() => {
  const by = groupBy.value;
  if (!by) return null;
  const at = columns.value.findIndex((c) => c.name === by.column);
  const column = columns.value[at];
  const found = new Map<string, Key & { rows: number[] }>();
  for (const r of base.value) {
    const k = keysOf(column, display.value.rows[r][at] ?? '', by.bucket, props.locale)[0];
    let g = found.get(k.key);
    if (!g) {
      g = { ...k, rows: [] };
      found.set(k.key, g);
    }
    g.rows.push(r);
  }
  return { column, groups: [...found.values()].sort(compareKeys) };
});

const order = computed(() => (groups.value
  ? groups.value.groups.filter((g) => !collapsed.value.has(g.key)).flatMap((g) => g.rows)
  : base.value));

const items = computed<BodyItem[]>(() => {
  if (!groups.value) return order.value.map((ri, di) => ({ kind: 'row', ri, di }));
  const out: BodyItem[] = [];
  const kinds = view.value.summary ?? {};
  const subtotals = Object.keys(kinds).length > 0;
  const { column } = groups.value;
  let di = 0;
  for (const g of groups.value.groups) {
    const isCollapsed = collapsed.value.has(g.key);
    out.push({
      kind: 'group',
      key: g.key,
      label: g.key === BLANK_KEY ? t('rich_table.chart.blank') : g.label,
      count: g.rows.length,
      collapsed: isCollapsed,
      color: column.type === 'select' && g.key !== BLANK_KEY ? optionColor(column, g.key) : undefined,
    });
    if (isCollapsed) continue;
    for (const ri of g.rows) out.push({ kind: 'row', ri, di: di++ });
    if (subtotals) {
      out.push({
        kind: 'subtotal',
        key: g.key,
        values: columns.value.map((c, i) => {
          const kind = kinds[c.name];
          if (!isSummaryKind(c, kind)) return null;
          return summarize(c, kind, g.rows.map((r) => display.value.rows[r][i] ?? ''), props.locale);
        }),
      });
    }
  }
  return out;
});

function toggleGroup(key: string) {
  const next = new Set(collapsed.value);
  if (next.has(key)) next.delete(key); else next.add(key);
  collapsed.value = next;
}

// ─── Hidden columns, tints ──────────────────────────────────────

const hidden = computed(() => new Set((view.value.hide ?? [])
  .map((n) => columns.value.findIndex((c) => c.name === n))
  .filter((i) => i !== -1)));

/** The nearest column the view shows, from `c` toward `dir`; -1 when there is none that way. */
function visibleFrom(c: number, dir: 1 | -1): number {
  for (let i = c; i >= 0 && i < columns.value.length; i += dir) if (!hidden.value.has(i)) return i;
  return -1;
}

const styles = computed(() => ruleStyles(display.value, view.value, order.value));
/** The last column shown: its header leaves room for the "+" that adds one after it. */
const lastVisible = computed(() => visibleFrom(columns.value.length - 1, -1));
const sorted = computed(() => sortKeys(props.table, view.value).length > 0);

watch(() => view.value.filter, () => { keep.value = new Set(); });
// Rows kept in sight are kept by their place in the file. When rows come or
// go from elsewhere — an undo of the row just added — those places mean
// other rows now, and the filter applies to all of them again.
watch(() => props.table.rows.length, (length) => {
  if (length !== wroteLength && keep.value.size) keep.value = new Set();
});

const GUTTER = 44;
function defaultWidth(column: Column): number {
  switch (column.type) {
    case 'checkbox': return 84;
    case 'number': return 128;
    case 'date': return 140;
    default: return 180;
  }
}
const liveWidth = ref<{ col: number; width: number } | null>(null);
/** A column's width: this view's, else the column's own, else one by its type. */
const widths = computed(() => columns.value.map((c, i) =>
  (liveWidth.value?.col === i ? liveWidth.value.width : view.value.widths?.[c.name] ?? c.width ?? defaultWidth(c))));
const freeze = computed(() => Math.min(props.table.freeze ?? 0, columns.value.length));
const stuck = computed(() => {
  let x = GUTTER;
  return widths.value.map((w, i) => {
    if (i >= freeze.value || hidden.value.has(i)) return null;
    const at = x;
    x += w;
    return at;
  });
});
const layoutKey = computed(() =>
  `${widths.value.join(',')}|${columns.value.map((c) => (c.wrap ? 1 : 0)).join('')}|${[...hidden.value].join(',')}`);
/** How much of the left edge the frozen columns cover, so scrolling into view clears them. */
const frozenWidth = computed(() => GUTTER + widths.value.slice(0, freeze.value)
  .reduce((a, w, i) => a + (hidden.value.has(i) ? 0 : w), 0));
/**
 * The last column takes whatever room is left, so a narrow table fills the
 * block instead of stopping short of its own border; a wide one scrolls.
 */
const gridStyle = computed(() => {
  const shown = widths.value.filter((_, i) => !hidden.value.has(i));
  const tracks = shown.map((w, i) => (i === shown.length - 1 ? `minmax(${w}px, 1fr)` : `${w}px`));
  return { '--rt-cols': `${GUTTER}px ${tracks.join(' ')}` };
});

const locked = (c: number) => props.readOnly || !columns.value[c] || isReadOnlyColumn(columns.value[c]);

// ─── Selection ──────────────────────────────────────────────────

interface Pos { r: number; c: number }
const active = ref<Pos | null>(null);
const anchor = ref<Pos | null>(null);
const range = computed(() => {
  if (!active.value) return null;
  const a = anchor.value ?? active.value;
  return {
    r1: Math.min(a.r, active.value.r), r2: Math.max(a.r, active.value.r),
    c1: Math.min(a.c, active.value.c), c2: Math.max(a.c, active.value.c),
  };
});
const multi = computed(() => !!range.value && (range.value.r1 !== range.value.r2 || range.value.c1 !== range.value.c2));

function select(pos: Pos, extend = false) {
  const r = Math.max(0, Math.min(pos.r, order.value.length - 1));
  let c = Math.max(0, Math.min(pos.c, columns.value.length - 1));
  // Onto a column the view hides: on to the next shown one, the way it was going.
  if (hidden.value.has(c)) {
    const dir = active.value && pos.c < active.value.c ? -1 : 1;
    const next = visibleFrom(c, dir);
    c = next !== -1 ? next : Math.max(0, visibleFrom(c, dir === 1 ? -1 : 1));
  }
  if (order.value.length === 0) {
    active.value = null;
    anchor.value = null;
    return;
  }
  if (!extend || !active.value) anchor.value = { r, c };
  active.value = { r, c };
}

// Keep the selection inside the table as rows come and go — and off a
// column the view has just hidden.
watch([() => order.value.length, () => columns.value.length, hidden], () => {
  if (active.value) select(active.value, true);
  if (anchor.value && active.value) {
    anchor.value = {
      r: Math.min(anchor.value.r, order.value.length - 1),
      c: Math.min(anchor.value.c, columns.value.length - 1),
    };
  }
});

/** Each selected cell, as row in the table and column. */
function selectedCells(): { row: number; col: number }[] {
  const g = range.value;
  if (!g) return [];
  const cells: { row: number; col: number }[] = [];
  for (let r = g.r1; r <= g.r2; r++) {
    for (let c = g.c1; c <= g.c2; c++) if (!hidden.value.has(c)) cells.push({ row: order.value[r], col: c });
  }
  return cells;
}

const stats = computed(() => {
  if (!multi.value) return null;
  return selectionStats(selectedCells().map(({ row, col }) => ({
    raw: props.table.rows[row]?.[col] ?? '',
    column: columns.value[col],
  })));
});

// ─── Geometry: where the selection and the editor are drawn ─────

const root = ref<HTMLElement | null>(null);
const scroller = ref<HTMLElement | null>(null);
const gridEl = ref<HTMLElement | null>(null);
const sink = ref<HTMLTextAreaElement | null>(null);
const dateInput = ref<HTMLInputElement | null>(null);

interface Box { top: number; left: number; width: number; height: number }
const activeBox = ref<Box | null>(null);
const rangeBox = ref<Box | null>(null);

function cellEl(pos: Pos): HTMLElement | null {
  const row = gridEl.value?.querySelector(`.rt-body-row[data-r="${pos.r}"]`);
  return (row?.children[pos.c + 1] as HTMLElement | undefined) ?? null;
}

function boxOf(el: HTMLElement): Box {
  const g = gridEl.value!.getBoundingClientRect();
  const r = el.getBoundingClientRect();
  return { top: r.top - g.top, left: r.left - g.left, width: r.width, height: r.height };
}

function measure() {
  if (!active.value || !gridEl.value) {
    activeBox.value = null;
    rangeBox.value = null;
    return;
  }
  const el = cellEl(active.value);
  activeBox.value = el ? boxOf(el) : null;
  const g = range.value;
  if (g && multi.value) {
    const a = cellEl({ r: g.r1, c: g.c1 });
    const b = cellEl({ r: g.r2, c: g.c2 });
    if (a && b) {
      const ba = boxOf(a);
      const bb = boxOf(b);
      rangeBox.value = {
        top: ba.top, left: ba.left,
        width: bb.left + bb.width - ba.left, height: bb.top + bb.height - ba.top,
      };
    }
  } else {
    rangeBox.value = null;
  }
}

let frame = 0;
const measureSoon = () => {
  cancelAnimationFrame(frame);
  frame = requestAnimationFrame(measure);
};
watch([active, anchor, () => props.table, order, layoutKey], () => nextTick(measure));

const box = (b: Box | null) => (b ? {
  top: `${b.top}px`, left: `${b.left}px`, width: `${b.width}px`, height: `${b.height}px`,
} : { display: 'none' });

/** Above a frozen column when the selection is in one; under it otherwise. */
const overlayLayer = computed(() => (active.value && active.value.c < freeze.value ? 'is-frozen' : ''));

function reveal() {
  if (!active.value) return;
  cellEl(active.value)?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
}

// ─── Focus ──────────────────────────────────────────────────────

const focused = ref(false);

function focusSink() {
  sink.value?.focus({ preventScroll: true });
}

function onFocusIn() {
  focused.value = true;
  if (!active.value && order.value.length) select({ r: 0, c: 0 });
}

function onFocusOut(e: FocusEvent) {
  const next = e.relatedTarget as Node | null;
  if (next && root.value?.contains(next)) {
    // A button above the grid — which Chromium focuses on a click, and
    // WebKit does not — ends the edit as a click anywhere else would.
    if (editing.value && editing.value.mode !== 'pick' && (next as Element).closest?.('.rt-bar')) commit();
    return;
  }
  focused.value = false;
  // Leaving the table — for the text around it, for a menu — keeps what was typed.
  if (editing.value && editing.value.mode !== 'pick') commit();
}

// ─── Editing ────────────────────────────────────────────────────

type Mode = 'text' | 'pick' | 'date' | 'note';
/**
 * The open editor, and the cell it edits — as a row of the table, taken when
 * editing starts. Read at commit instead, a row that a sort or a change from
 * outside moved meanwhile would take somebody else's value.
 */
const editing = ref<{ mode: Mode; row: number; col: number } | null>(null);
/** Open the editor on the active cell. */
function opened(mode: Mode) {
  editing.value = { mode, row: order.value[active.value!.r], col: active.value!.c };
}
/** The choice being made in a select or multi-select column. */
const pick = ref<{ items: string[]; highlight: number }>({ items: [], highlight: 0 });
const dateDraft = ref('');
const query = ref('');

const activeColumn = computed(() => (active.value ? columns.value[active.value.c] : undefined));
const uid = useId();
/** What a screen reader says of the focused textarea: which cell it is on, and what is in it. */
const sinkLabel = computed(() => {
  if (!active.value || !activeColumn.value) return t('rich_table.cell_editor');
  return t('rich_table.cell_at', {
    column: activeColumn.value.name,
    row: active.value.r + 1,
    value: displayText(activeColumn.value, activeRaw.value, props.locale) || t('rich_table.empty_cell'),
  });
});
/** The highlighted choice of an open picker, for `aria-activedescendant`. */
const activeOption = computed(() => {
  if (editing.value?.mode === 'pick' && choices.value.length) return `${uid}-pick-${pick.value.highlight}`;
  if (editing.value?.mode === 'note' && notes.value.length) return `${uid}-note-${noteHighlight.value}`;
  return undefined;
});
const activeRaw = computed(() => (active.value ? props.table.rows[order.value[active.value.r]]?.[active.value.c] ?? '' : ''));

const choices = computed(() => {
  const column = activeColumn.value;
  if (!column) return [];
  const q = query.value.trim().toLocaleLowerCase(props.locale);
  const options = (column.options ?? []).filter((o) => o.toLocaleLowerCase(props.locale).includes(q));
  const exact = (column.options ?? []).some((o) => o.toLocaleLowerCase(props.locale) === q);
  return q && !exact ? [...options, { create: query.value.trim() }] : options;
});

function inputModeFor(column: Column | undefined): string {
  if (column?.type === 'number') return 'decimal';
  if (column?.type === 'url') return 'url';
  return 'text';
}

/**
 * Start editing the active cell. `typed` is what was typed to start it,
 * already in the textarea; without it the cell's value is put there.
 * `touch` brings up the on-screen keyboard, which the textarea otherwise
 * keeps down while it is only holding the focus.
 */
function startEdit(opts: { typed?: boolean; touch?: boolean } = {}) {
  const column = activeColumn.value;
  const el = sink.value;
  if (active.value && el && column?.type === 'formula') {
    // A formula's cells are computed: editing one means editing the formula.
    el.value = '';
    if (!props.readOnly) openFormulaEditor(active.value.c);
    return;
  }
  if (!active.value || !column || !el || locked(active.value.c)) {
    if (el && !editing.value) el.value = '';
    return;
  }
  if (column.type === 'checkbox') {
    // A checkbox is ticked with Space or a click; a letter typed at it means nothing.
    el.value = '';
    if (!opts.typed) toggleChecks();
    return;
  }
  if (column.type === 'date') {
    el.value = '';
    dateDraft.value = column.time ? activeRaw.value.replace(' ', 'T') : activeRaw.value.slice(0, 10);
    if (activeRaw.value && !dateOf(activeRaw.value)) dateDraft.value = '';
    opened('date');
    nextTick(() => {
      dateInput.value?.focus();
      try { dateInput.value?.showPicker(); } catch { /* not offered, or not now */ }
    });
    return;
  }
  if (column.type === 'note' && props.searchNotes) {
    // The textarea is the search box; what it holds when nothing is picked
    // is kept as typed, in brackets, as before there was a picker.
    if (!opts.typed) el.value = activeRaw.value.replace(/^\[\[(?:[^\]|]*\|)?([^\]]*)\]\]$/, '$1');
    opened('note');
    noteHighlight.value = 0;
    findNotes(el.value);
    if (opts.touch) {
      el.inputMode = 'text';
      el.focus();
    }
    // What was typed has its caret already; moving it mid-word would break an input method's composition.
    if (!opts.typed) nextTick(() => el.setSelectionRange(el.value.length, el.value.length));
    return;
  }
  if (column.type === 'select' || column.type === 'multi') {
    if (!opts.typed) el.value = '';
    query.value = el.value;
    pick.value = {
      items: column.type === 'multi' ? itemsOf(activeRaw.value) : (activeRaw.value.trim() ? [activeRaw.value.trim()] : []),
      highlight: 0,
    };
    opened('pick');
  } else {
    if (!opts.typed) el.value = draftOf(column, activeRaw.value, props.locale);
    opened('text');
  }
  if (opts.touch) {
    el.inputMode = inputModeFor(column);
    el.focus();
  }
  nextTick(() => {
    // Typed: the caret is where the typing left it, and an input method may
    // still be composing there (`â` from `a` `a`) — moving it would end that.
    if (!opts.typed) el.setSelectionRange(el.value.length, el.value.length);
    grow();
  });
}

function grow() {
  const el = sink.value;
  if (!el || !editing.value || editing.value.mode !== 'text') return;
  el.style.height = 'auto';
  el.style.height = `${Math.max(el.scrollHeight, activeBox.value?.height ?? 0)}px`;
}

function endEdit() {
  editing.value = null;
  query.value = '';
  notes.value = [];
  if (sink.value) {
    sink.value.value = '';
    sink.value.style.height = '';
    sink.value.inputMode = 'none';
  }
}

function cancel() {
  endEdit();
  focusSink();
}

// ─── Picking a note ─────────────────────────────────────────────

const notes = ref<{ id: string; title: string }[]>([]);
const noteHighlight = ref(0);
let noteTimer: ReturnType<typeof setTimeout> | undefined;
let noteAsk = 0;

function findNotes(query: string) {
  clearTimeout(noteTimer);
  const mine = ++noteAsk;
  noteTimer = setTimeout(async () => {
    if (!query.trim() || !props.searchNotes) {
      notes.value = [];
      return;
    }
    try {
      const found = await props.searchNotes(query.trim());
      if (mine === noteAsk) notes.value = found.slice(0, 8);
    } catch {
      if (mine === noteAsk) notes.value = [];
    }
  }, 150);
}

/** Write a picked note as a wikilink to its path, showing its title. */
function chooseNote(note: { id: string; title: string }) {
  if (sink.value) sink.value.value = `[[${note.id}|${note.title.replace(/[[\]|]/g, ' ')}]]`;
  commit([1, 0]);
}

/** The raw value the open editor holds. */
function editedRaw(): string | null {
  const column = editing.value ? columns.value[editing.value.col] : undefined;
  if (!editing.value || !column) return null;
  switch (editing.value.mode) {
    case 'text':
    case 'note': return rawFromInput(column, sink.value?.value ?? '', props.locale);
    case 'date': return rawFromInput(column, dateDraft.value, props.locale);
    case 'pick': return column.type === 'multi' ? pick.value.items.join(', ') : pick.value.items[0] ?? '';
  }
}

/**
 * Write what the editor holds, then move. With `all`, write it into every
 * selected cell, as Excel's Ctrl+Enter does.
 */
function commit(move?: [number, number], all = false) {
  const raw = editedRaw();
  const at = editing.value;
  if (raw === null || !at || !active.value) return;
  const targets = all ? selectedCells() : [{ row: at.row, col: at.col }];
  // Where to go next, as a row of the table: the edit may sort its own row
  // elsewhere, and Enter means the row below as it was shown.
  const from = order.value.indexOf(at.row);
  const next = move && from !== -1 ? order.value[Math.max(0, Math.min(from + move[0], order.value.length - 1))] : undefined;
  endEdit();
  const edits = targets.filter(({ col }) => !locked(col)).map(({ row, col }) => ({ row, col, raw }));
  apply(edits);
  // An edited row stays in sight until the filter changes, though its new
  // value no longer passes — as a row just added does.
  if (edits.length && view.value.filter) keep.value = new Set([...keep.value, ...edits.map((e) => e.row)]);
  if (move) {
    const c = at.col + move[1];
    nextTick(() => {
      const r = next === undefined ? -1 : order.value.indexOf(next);
      if (r !== -1) select({ r, c });
      else moveBy(move[0], move[1]);
      nextTick(reveal);
    });
  }
  focusSink();
}

function apply(edits: CellEdit[]) {
  if (!edits.length) return;
  const next = setCells(props.table, edits);
  if (next !== props.table) change(next);
}

function choose(choice: string | { create: string }) {
  const column = activeColumn.value;
  if (!column) return;
  const value = typeof choice === 'string' ? choice : choice.create;
  if (column.type === 'multi') {
    const items = pick.value.items;
    pick.value = {
      items: items.includes(value) ? items.filter((i) => i !== value) : [...items, value],
      highlight: pick.value.highlight,
    };
    if (sink.value) sink.value.value = '';
    query.value = '';
  } else {
    pick.value = { items: [value], highlight: 0 };
    commit([1, 0]);
  }
}

function toggleChecks() {
  const cells = selectedCells().filter(({ col }) => columns.value[col].type === 'checkbox' && !locked(col));
  if (!cells.length) return;
  const on = !isChecked(props.table.rows[cells[0].row][cells[0].col] ?? '');
  apply(cells.map(({ row, col }) => ({ row, col, raw: on ? '[x]' : '' })));
}

function onInput() {
  if (editing.value) {
    if (editing.value.mode === 'note') {
      noteHighlight.value = 0;
      findNotes(sink.value?.value ?? '');
      return;
    }
    if (editing.value.mode === 'pick') {
      query.value = sink.value?.value ?? '';
      pick.value = { ...pick.value, highlight: 0 };
    } else {
      grow();
    }
    return;
  }
  if (!sink.value?.value) return;
  if (menuOpen.value) {
    sink.value.value = '';
    return;
  }
  startEdit({ typed: true });
}

// ─── Keys ───────────────────────────────────────────────────────

function moveBy(dr: number, dc: number, extend = false) {
  if (!active.value) return;
  select({ r: active.value.r + dr, c: active.value.c + dc }, extend);
  nextTick(reveal);
}

function onKeydown(e: KeyboardEvent) {
  if (e.isComposing || e.keyCode === 229) return;
  // A menu is open over the table: its keys are its own.
  if (menuOpen.value) {
    e.preventDefault();
    return;
  }
  const mod = e.metaKey || e.ctrlKey;
  if (editing.value) {
    onEditKey(e, mod);
    return;
  }
  if (!active.value) return;
  const last = order.value.length - 1;
  const lastCol = columns.value.length - 1;
  const handled = () => e.preventDefault();

  switch (e.key) {
    case 'ArrowUp':
      handled();
      if (active.value.r === 0 && !e.shiftKey) emit('exit', 'up');
      else moveBy(mod ? -active.value.r : -1, 0, e.shiftKey);
      return;
    case 'ArrowDown':
      handled();
      if (active.value.r >= last && !e.shiftKey) emit('exit', 'down');
      else moveBy(mod ? last - active.value.r : 1, 0, e.shiftKey);
      return;
    case 'ArrowLeft':
      handled();
      moveBy(0, mod ? -active.value.c : -1, e.shiftKey);
      return;
    case 'ArrowRight':
      handled();
      moveBy(0, mod ? lastCol - active.value.c : 1, e.shiftKey);
      return;
    case 'Home':
      handled();
      moveBy(mod ? -active.value.r : 0, -active.value.c, e.shiftKey);
      return;
    case 'End':
      handled();
      moveBy(mod ? last - active.value.r : 0, lastCol - active.value.c, e.shiftKey);
      return;
    case 'PageUp':
    case 'PageDown':
      handled();
      moveBy(e.key === 'PageUp' ? -10 : 10, 0, e.shiftKey);
      return;
    case 'Tab': {
      handled();
      const { r, c } = active.value;
      const prev = visibleFrom(c - 1, -1);
      const next = visibleFrom(c + 1, 1);
      if (e.shiftKey) {
        if (prev !== -1) select({ r, c: prev });
        else if (r > 0) select({ r: r - 1, c: visibleFrom(lastCol, -1) });
      } else if (next !== -1) select({ r, c: next });
      else if (r < last) select({ r: r + 1, c: visibleFrom(0, 1) });
      nextTick(reveal);
      return;
    }
    case 'Enter':
    case 'F2':
      handled();
      if (mod && e.key === 'Enter') return;
      startEdit();
      return;
    case 'ContextMenu':
    case 'F10': {
      // The menu a right click opens, from the keyboard: the Menu key, or Shift+F10.
      if (e.key === 'F10' && !e.shiftKey) return;
      handled();
      const box = cellEl(active.value)?.getBoundingClientRect();
      if (box && !props.readOnly) cellMenu.value = { anchor: { left: box.left, right: box.right, top: box.top, bottom: box.bottom }, ...active.value };
      return;
    }
    case 'Escape':
      handled();
      if (multi.value) select(active.value);
      else emit('exit', 'select');
      return;
    case 'Delete':
    case 'Backspace':
      handled();
      apply(selectedCells().filter(({ col }) => !locked(col)).map(({ row, col }) => ({ row, col, raw: '' })));
      return;
    case ' ':
      if (activeColumn.value?.type === 'checkbox') {
        handled();
        toggleChecks();
      }
      return;
  }

  if (!mod) return;
  const key = e.key.toLowerCase();
  if (key === 'z') {
    handled();
    if (e.shiftKey) emit('redo');
    else emit('undo');
  } else if (key === 'y') {
    handled();
    emit('redo');
  } else if (key === 'a') {
    handled();
    anchor.value = { r: 0, c: 0 };
    active.value = { r: last, c: lastCol };
  } else if (key === 'd') {
    handled();
    fillDown();
  } else if (key === 'c' || key === 'x') {
    // Put the selection in the textarea and select it, so the copy that
    // follows has something to copy; `onCopy` then writes both forms.
    prepareCopy();
  }
}

function onEditKey(e: KeyboardEvent, mod: boolean) {
  const mode = editing.value!.mode;
  const column = activeColumn.value;
  if (e.key === 'Escape') {
    e.preventDefault();
    e.stopPropagation();
    if (mode === 'pick' && column?.type === 'multi') commit();
    else cancel();
    return;
  }
  if (e.key === 'Tab') {
    e.preventDefault();
    commit([0, e.shiftKey ? -1 : 1]);
    return;
  }
  if (mode === 'note') {
    const count = notes.value.length;
    if ((e.key === 'ArrowDown' || e.key === 'ArrowUp') && count) {
      e.preventDefault();
      noteHighlight.value = (noteHighlight.value + (e.key === 'ArrowDown' ? 1 : -1) + count) % count;
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const note = notes.value[noteHighlight.value];
      if (note) chooseNote(note);
      else commit([1, 0]);
    }
    return;
  }
  if (mode === 'pick') {
    const count = choices.value.length;
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      if (count) {
        const step = e.key === 'ArrowDown' ? 1 : -1;
        pick.value = { ...pick.value, highlight: (pick.value.highlight + step + count) % count };
      }
    } else if (e.key === 'Enter') {
      e.preventDefault();
      const choice = choices.value[pick.value.highlight];
      if (choice !== undefined) choose(choice);
      else commit([1, 0]);
    } else if (e.key === 'Backspace' && !query.value && column?.type === 'multi') {
      e.preventDefault();
      pick.value = { ...pick.value, items: pick.value.items.slice(0, -1) };
    }
    return;
  }
  if (e.key === 'Enter') {
    if (mod) {
      e.preventDefault();
      commit(undefined, true);
    } else if ((e.shiftKey || e.altKey) && column?.type === 'text' && mode === 'text') {
      if (e.altKey) {
        e.preventDefault();
        sink.value?.setRangeText('\n', sink.value.selectionStart, sink.value.selectionEnd, 'end');
        grow();
      }
    } else {
      e.preventDefault();
      commit([e.shiftKey ? -1 : 1, 0]);
    }
  }
}

function onDateKey(e: KeyboardEvent) {
  if (e.key === 'Enter') {
    e.preventDefault();
    commit([1, 0]);
  } else if (e.key === 'Escape') {
    e.preventDefault();
    cancel();
  } else if (e.key === 'Tab') {
    e.preventDefault();
    commit([0, e.shiftKey ? -1 : 1]);
  }
}

/** The columns of a selection that the view shows: a hidden one is never copied, filled or pasted into. */
function shownColumns(c1: number, c2: number): number[] {
  const out: number[] = [];
  for (let c = c1; c <= c2; c++) if (!hidden.value.has(c)) out.push(c);
  return out;
}

function fillDown() {
  const g = range.value;
  if (!g) return;
  const source = g.r1 === g.r2 ? g.r1 - 1 : g.r1;
  if (source < 0) return;
  const from = order.value[source];
  const edits: CellEdit[] = [];
  for (let r = Math.max(g.r1, source + 1); r <= g.r2; r++) {
    for (const c of shownColumns(g.c1, g.c2)) {
      if (!locked(c)) edits.push({ row: order.value[r], col: c, raw: props.table.rows[from][c] ?? '' });
    }
  }
  apply(edits);
}

// ─── Clipboard ──────────────────────────────────────────────────

let copied: { tsv: string; html: string } | null = null;

function selectionGrid(): string[][] {
  const g = range.value;
  if (!g) return [];
  const grid: string[][] = [];
  const cols = shownColumns(g.c1, g.c2);
  for (let r = g.r1; r <= g.r2; r++) {
    const row: string[] = [];
    for (const c of cols) {
      const raw = props.table.rows[order.value[r]]?.[c] ?? '';
      row.push(columns.value[c].type === 'checkbox' ? (isChecked(raw) ? 'TRUE' : 'FALSE') : raw);
    }
    grid.push(row);
  }
  return grid;
}

function prepareCopy() {
  const grid = selectionGrid();
  if (!grid.length || !sink.value) return;
  const g = range.value!;
  const header = shownColumns(g.c1, g.c2).map((c) => columns.value[c].name);
  copied = { tsv: toTsv(grid), html: toHtml(grid, multi.value ? header : undefined) };
  sink.value.value = copied.tsv;
  sink.value.select();
}

function onCopy(e: ClipboardEvent, cut = false) {
  if (editing.value) return;
  if (!copied) prepareCopy();
  if (!copied || !e.clipboardData) return;
  e.preventDefault();
  e.clipboardData.setData('text/plain', copied.tsv);
  e.clipboardData.setData('text/html', copied.html);
  copied = null;
  if (sink.value) sink.value.value = '';
  if (cut) {
    apply(selectedCells().filter(({ col }) => !locked(col)).map(({ row, col }) => ({ row, col, raw: '' })));
  }
}

function onCut(e: ClipboardEvent) {
  onCopy(e, true);
}

/**
 * Paste a block of cells at the active one, growing the table to fit. Into a
 * table with nothing in it yet, a block whose first row is a header —
 * `<th>`s, as an ordinary table copied out of a note has — brings its own
 * columns, with their types guessed from what is under them. That is how an
 * ordinary table is brought into a Rich Table: copy it, paste it here.
 */
function onPaste(e: ClipboardEvent) {
  if (editing.value) return;
  e.preventDefault();
  if (props.readOnly || !active.value || !e.clipboardData) return;
  let grid = readClipboard(e.clipboardData);
  if (!grid?.length || !grid[0].length) return;

  let table = props.table;
  let at = { ...active.value };
  let shownNow = order.value;
  const empty = table.rows.every((r) => r.every((v) => !v.trim()));
  const withHeader = empty && at.r === 0 && at.c === 0 && grid.length > 1 && htmlHasHeader(e.clipboardData.getData('text/html'));
  if (withHeader) {
    const [head, ...body] = grid;
    const names = head.map((h, i) => h.replace(/[|[\]\n]/g, ' ').trim() || `${t('rich_table.default_column')} ${i + 1}`);
    // `A, A, A 2` must not become `A, A 2, A 2`: each name checked against all taken so far.
    const taken = new Set<string>();
    const unique = names.map((n) => {
      let name = n;
      for (let k = 2; taken.has(name); k++) name = `${n} ${k}`;
      taken.add(name);
      return name;
    });
    // The views stay: their names, layouts and charts were set up for this
    // table before it had anything in it.
    table = {
      ...table,
      columns: unique.map((name, i) => ({ name, type: inferType(body.map((r) => r[i] ?? ''), props.locale) })),
      rows: [],
      freeze: undefined,
    };
    grid = body;
    at = { r: 0, c: 0 };
    shownNow = [];
  }

  const width = grid.reduce((w, r) => Math.max(w, r.length), 0);
  // The columns pasted into, from the active one: those the view shows, then new ones past the end.
  const into = withHeader
    ? table.columns.map((_, i) => i)
    : shownColumns(at.c, table.columns.length - 1);
  while (into.length < width) {
    const i = table.columns.length;
    table = insertColumn(table, i, { name: freshColumnName(table, t('rich_table.default_column')), type: 'text' });
    into.push(i);
  }
  const rowsNeeded = at.r + grid.length - shownNow.length;
  const added: number[] = [];
  if (rowsNeeded > 0) {
    const first = table.rows.length;
    table = insertRows(table, first, rowsNeeded);
    for (let i = 0; i < rowsNeeded; i++) added.push(first + i);
  }
  const targets = [...shownNow, ...added];
  const edits: CellEdit[] = [];
  grid.forEach((cells, dr) => cells.forEach((value, dc) => {
    const col = into[dc];
    const column = table.columns[col];
    if (!column || isReadOnlyColumn(column)) return;
    edits.push({ row: targets[at.r + dr], col, raw: rawFromInput(column, value, props.locale) });
  }));
  table = setCells(table, edits);
  if (added.length) keep.value = new Set([...keep.value, ...added]);
  change(table);
  nextTick(() => {
    anchor.value = at;
    active.value = { r: at.r + grid!.length - 1, c: into[width - 1] };
    select(active.value, true);
  });
}

// ─── Pointer ────────────────────────────────────────────────────

/** Gestures under way, each with its way to stop — all stopped if the table goes away mid-drag. */
const gestures = new Set<() => void>();

/**
 * Follow a pointer until it lifts. `end` is told whether the browser took the
 * gesture away instead (`pointercancel` — a scroll, a second finger), in
 * which case nothing it was going to do should happen.
 */
function gesture(target: EventTarget, move: (e: PointerEvent) => void, end: (cancelled: boolean) => void) {
  const onMove = (e: Event) => move(e as PointerEvent);
  const stop = () => {
    target.removeEventListener('pointermove', onMove);
    target.removeEventListener('pointerup', onUp);
    target.removeEventListener('pointercancel', onCancel);
    gestures.delete(stop);
  };
  const onUp = () => { stop(); end(false); };
  const onCancel = () => { stop(); end(true); };
  target.addEventListener('pointermove', onMove);
  target.addEventListener('pointerup', onUp);
  target.addEventListener('pointercancel', onCancel);
  gestures.add(stop);
}

let dragging = false;
let lastPointer: string = 'mouse';

function posOf(target: EventTarget | null): Pos | null {
  const cell = (target as HTMLElement | null)?.closest?.('.rt-cell') as HTMLElement | null;
  const row = cell?.parentElement as HTMLElement | null;
  if (!cell || !row?.dataset.r) return null;
  return { r: Number(row.dataset.r), c: Number(cell.dataset.c) };
}

/** Follow a link in a cell: a note in the vault, or a web address. */
function follow(target: HTMLElement): boolean {
  const link = target.closest('a') as HTMLAnchorElement | null;
  if (!link) return false;
  const note = link.dataset.note;
  const href = link.dataset.href ?? link.getAttribute('href');
  if (note) emit('navigate', note);
  else if (href && /^(https?|mailto):/i.test(href)) openUrl(href).catch(() => {});
  else return false;
  return true;
}

// ─── Moving rows and columns by dragging ────────────────────────

/** Where a dragged row or column would land: a line between two of them. */
const dropLine = ref<Box | null>(null);
let suppressClick = false;

/**
 * Rows can be dragged only when the view shows them in file order — sorted
 * or grouped, "above" on screen is not "above" in the file.
 */
const reorderable = computed(() => !props.readOnly && !sorted.value && !groupBy.value);

function startDrag(e: PointerEvent, kind: 'row' | 'column', from: number) {
  if (e.pointerType !== 'mouse' || e.button !== 0) return;
  if (kind === 'row' && !reorderable.value) return;
  if (kind === 'column' && props.readOnly) return;
  const x0 = e.clientX;
  const y0 = e.clientY;
  let moving = false;
  let to: number | null = null;

  const onMove = (ev: PointerEvent) => {
    if (!moving && Math.hypot(ev.clientX - x0, ev.clientY - y0) < 5) return;
    moving = true;
    ev.preventDefault();
    const el = document.elementFromPoint(ev.clientX, ev.clientY) as HTMLElement | null;
    const grid = gridEl.value?.getBoundingClientRect();
    if (!grid) return;
    if (kind === 'row') {
      const row = el?.closest('.rt-body-row') as HTMLElement | null;
      if (!row) return;
      const r = row.getBoundingClientRect();
      const before = ev.clientY < r.top + r.height / 2;
      const di = Number(row.dataset.r);
      to = before ? di : di + 1;
      dropLine.value = { top: (before ? r.top : r.bottom) - grid.top - 1, left: 0, width: grid.width, height: 2 };
    } else {
      const th = el?.closest('.rt-th') as HTMLElement | null;
      if (!th) return;
      const r = th.getBoundingClientRect();
      const before = ev.clientX < r.left + r.width / 2;
      const c = Number(th.dataset.c);
      to = before ? c : c + 1;
      dropLine.value = { top: 0, left: (before ? r.left : r.right) - grid.left - 1, width: 2, height: grid.height };
    }
  };
  const onUp = (cancelled: boolean) => {
    dropLine.value = null;
    if (cancelled || !moving || to === null) return;
    suppressClick = true;
    setTimeout(() => { suppressClick = false; }, 0);
    if (kind === 'row') {
      // Positions on screen, turned into places in the file.
      const fromFile = order.value[from];
      const target = to >= order.value.length ? props.table.rows.length : order.value[to];
      const landing = target > fromFile ? target - 1 : target;
      if (landing === fromFile) return;
      keep.value = new Set();
      change(moveRow(props.table, fromFile, landing));
      nextTick(() => {
        const r = order.value.indexOf(landing);
        if (r !== -1) select({ r, c: active.value?.c ?? 0 });
      });
    } else {
      const landing = to > from ? to - 1 : to;
      if (landing !== from) change(moveColumn(props.table, from, landing));
    }
  };
  gesture(window, onMove, onUp);
}

// ─── The fill handle ────────────────────────────────────────────

/** Where a fill would reach while the handle is dragged, drawn as a dashed outline. */
const fillBox = ref<Box | null>(null);

/**
 * Drag the square at the selection's corner down or right: the selected
 * values go on as a series, or repeat (`fillValues`). Never past the last
 * row or column — a fill does not grow the table.
 */
function startFill(e: PointerEvent) {
  const g = range.value;
  if (!g || props.readOnly || e.button !== 0) return;
  e.preventDefault();
  e.stopPropagation();
  if (editing.value) commit();
  let reach: { r: number; c: number } | null = null;

  const onMove = (ev: PointerEvent) => {
    const pos = posOf(document.elementFromPoint(ev.clientX, ev.clientY));
    if (!pos) return;
    const down = pos.r - g.r2;
    const right = pos.c - g.c2;
    reach = down > 0 && down >= right ? { r: pos.r, c: g.c2 } : right > 0 ? { r: g.r2, c: pos.c } : null;
    const a = cellEl({ r: g.r1, c: g.c1 });
    const b = reach ? cellEl(reach) : null;
    if (a && b) {
      const ba = boxOf(a);
      const bb = boxOf(b);
      fillBox.value = { top: ba.top, left: ba.left, width: bb.left + bb.width - ba.left, height: bb.top + bb.height - ba.top };
    } else {
      fillBox.value = null;
    }
  };
  const onUp = (cancelled: boolean) => {
    fillBox.value = null;
    if (cancelled || !reach) return;
    const edits: CellEdit[] = [];
    if (reach.r > g.r2) {
      for (let c = g.c1; c <= g.c2; c++) {
        if (locked(c) || hidden.value.has(c)) continue;
        const source = order.value.slice(g.r1, g.r2 + 1).map((ri) => props.table.rows[ri][c] ?? '');
        const filled = fillValues(columns.value[c], source, reach.r - g.r2);
        filled.forEach((raw, k) => edits.push({ row: order.value[g.r2 + 1 + k], col: c, raw }));
      }
    } else {
      for (let r = g.r1; r <= g.r2; r++) {
        const ri = order.value[r];
        const source = [];
        for (let c = g.c1; c <= g.c2; c++) if (!hidden.value.has(c)) source.push(props.table.rows[ri][c] ?? '');
        let k = 0;
        for (let c = g.c2 + 1; c <= reach.c; c++) {
          if (locked(c) || hidden.value.has(c)) continue;
          edits.push({ row: ri, col: c, raw: source[k++ % source.length] });
        }
      }
    }
    apply(edits);
    anchor.value = { r: g.r1, c: g.c1 };
    active.value = { r: Math.max(g.r2, reach.r), c: Math.max(g.c2, reach.c) };
    focusSink();
  };
  gesture(window, onMove, onUp);
}

/** The handle sits on the selection's bottom-right corner. */
const fillAt = computed(() => {
  const b = multi.value ? rangeBox.value : activeBox.value;
  if (!b || props.readOnly || editing.value || !focused.value) return null;
  return { top: `${b.top + b.height - 4}px`, left: `${b.left + b.width - 4}px` };
});

// ─── The menu on a right-click ──────────────────────────────────

const cellMenu = ref<{ anchor: Anchor; r: number; c: number } | null>(null);

function onContextMenu(e: MouseEvent) {
  if (props.readOnly) return;
  const pos = posOf(e.target);
  if (!pos) return;
  e.preventDefault();
  if (editing.value) commit();
  const g = range.value;
  if (!g || pos.r < g.r1 || pos.r > g.r2 || pos.c < g.c1 || pos.c > g.c2) select(pos);
  focusSink();
  cellMenu.value = { anchor: { left: e.clientX, right: e.clientX, top: e.clientY, bottom: e.clientY }, ...pos };
}

/** The rows of the table the selection covers, in file order. */
function selectedRows(): number[] {
  const g = range.value;
  if (!g) return [];
  return [...new Set(order.value.slice(g.r1, g.r2 + 1))].sort((a, b) => a - b);
}

function cellAction(action: 'above' | 'below' | 'left' | 'right' | 'copy' | 'clear' | 'open' | 'delete-rows' | 'delete-column') {
  const menu = cellMenu.value;
  cellMenu.value = null;
  if (!menu) return;
  const ri = order.value[menu.r];
  switch (action) {
    case 'above': addRow(ri); break;
    case 'below': addRow(ri + 1); break;
    case 'left':
    case 'right': {
      const at = action === 'left' ? menu.c : menu.c + 1;
      change(insertColumn(props.table, at, { name: freshColumnName(props.table, t('rich_table.default_column')), type: 'text' }));
      break;
    }
    case 'copy': {
      const grid = selectionGrid();
      navigator.clipboard?.writeText(toTsv(grid)).catch(() => {});
      break;
    }
    case 'clear':
      apply(selectedCells().filter(({ col }) => !locked(col)).map(({ row, col }) => ({ row, col, raw: '' })));
      break;
    case 'open': sheetRow.value = ri; break;
    case 'delete-rows': {
      const rows = selectedRows();
      keep.value = new Set();
      change(deleteRows(props.table, rows));
      anchor.value = null;
      break;
    }
    case 'delete-column':
      if (columns.value.length > 1) change(deleteColumn(props.table, menu.c));
      break;
  }
  focusSink();
}

/** A new column at the end, its menu open on its name, ready to be named. */
function addColumn() {
  if (props.readOnly) return;
  if (editing.value) commit();
  const at = props.table.columns.length;
  change(insertColumn(props.table, at, { name: freshColumnName(props.table, t('rich_table.default_column')), type: 'text' }));
  nextTick(() => {
    const header = gridEl.value?.querySelector(`.rt-th[data-c="${at}"]`);
    if (header) {
      header.scrollIntoView({ block: 'nearest', inline: 'nearest' });
      columnMenu.value = { col: at, anchor: rect(header), fresh: true };
    }
  });
}

function onHeaderClick(c: number, e: MouseEvent) {
  if (suppressClick) return;
  openColumnMenu(c, e);
}

function onBodyPointerDown(e: PointerEvent) {
  lastPointer = e.pointerType;
  const target = e.target as HTMLElement;
  const gutter = target.closest('[data-gutter]') as HTMLElement | null;
  if (gutter && !target.closest('[data-open]')) startDrag(e, 'row', Number(gutter.dataset.gutter));
  if (e.pointerType !== 'mouse' || e.button !== 0) return;
  if (target.closest('[data-open]') || gutter) return;
  const pos = posOf(target);
  if (!pos) return;
  e.preventDefault();
  if (editing.value) commit();
  if (target.closest('a') && follow(target)) return;
  const wasActive = active.value?.r === pos.r && active.value?.c === pos.c && !multi.value;
  select(pos, e.shiftKey);
  dragging = !e.shiftKey;
  focusSink();
  if (target.closest('.rt-check') && (wasActive || columns.value[pos.c].type === 'checkbox')) toggleChecks();
}

function onBodyPointerOver(e: PointerEvent) {
  if (!dragging) return;
  const pos = posOf(e.target);
  if (pos) active.value = pos;
}

function onPointerUp() {
  dragging = false;
}

/** Touch has no hover and no double-click: a tap selects, a second tap edits. */
function onBodyClick(e: MouseEvent) {
  const target = e.target as HTMLElement;
  const group = target.closest('[data-group]') as HTMLElement | null;
  if (group) {
    toggleGroup(group.dataset.group ?? '');
    return;
  }
  const open = target.closest('[data-open]') as HTMLElement | null;
  if (open) {
    sheetRow.value = Number(open.dataset.open);
    return;
  }
  const gutter = target.closest('[data-gutter]') as HTMLElement | null;
  if (gutter) {
    if (!suppressClick) openRowMenu(Number(gutter.dataset.gutter), gutter);
    return;
  }
  if (lastPointer === 'mouse') return;
  const pos = posOf(target);
  if (!pos) return;
  if (target.closest('a') && follow(target)) return;
  if (editing.value) commit();
  const again = active.value?.r === pos.r && active.value?.c === pos.c;
  select(pos);
  if (again) startEdit({ touch: true });
  else if (columns.value[pos.c].type === 'checkbox' && target.closest('.rt-check')) toggleChecks();
}

function onBodyDblClick(e: MouseEvent) {
  const pos = posOf(e.target);
  if (!pos || columns.value[pos.c].type === 'checkbox') return;
  select(pos);
  startEdit();
}

// ─── Column widths ──────────────────────────────────────────────

function startResize(col: number, e: PointerEvent) {
  if (props.readOnly) return;
  e.preventDefault();
  e.stopPropagation();
  const startX = e.clientX;
  const target = e.currentTarget as HTMLElement;
  // From the width it is drawn at: the last column may be wider than declared.
  const start = Math.round(target.parentElement?.getBoundingClientRect().width ?? widths.value[col]);
  target.setPointerCapture(e.pointerId);
  const onMove = (ev: PointerEvent) => {
    liveWidth.value = { col, width: Math.max(60, Math.round(start + ev.clientX - startX)) };
  };
  const onUp = (cancelled: boolean) => {
    const width = cancelled ? undefined : liveWidth.value?.width;
    liveWidth.value = null;
    // Saved in the view: one view may want a column wide that another keeps narrow.
    if (width && width !== start) setView({ widths: { ...view.value.widths, [columns.value[col].name]: width } });
  };
  gesture(target, onMove, onUp);
}

// ─── Menus ──────────────────────────────────────────────────────

type Anchor = { left: number; top: number; bottom: number; right: number };
const columnMenu = ref<{ col: number; anchor: Anchor; fresh?: boolean } | null>(null);
const footerMenu = ref<{ col: number; anchor: Anchor } | null>(null);
const rowMenu = ref<{ r: number; anchor: Anchor } | null>(null);
const tableMenu = ref<Anchor | null>(null);
const sheetRow = ref<number | null>(null);
// The sheet's row follows its row when rows above it come or go — from
// Syn, from another device. A row edited keeps its place, as a new array.
watch(() => props.table.rows, (rows, before) => {
  if (sheetRow.value === null || !before) return;
  const was = before[sheetRow.value];
  const now = rows.indexOf(was);
  if (now !== -1) sheetRow.value = now;
  else if (rows.length !== before.length) sheetRow.value = null;
});
const filterBar = ref<InstanceType<typeof FilterBar> | null>(null);

const rect = (el: Element): Anchor => el.getBoundingClientRect();
const formulaEditor = ref<{ col: number; anchor: Anchor; others: Record<string, RichTable> } | null>(null);
const viewMenu = ref<Anchor | null>(null);
const hasSummary = computed(() => columns.value.some((c, i) => !hidden.value.has(i) && isSummaryKind(c, view.value.summary?.[c.name])));
const menuOpen = computed(() => !!(
  columnMenu.value || footerMenu.value || rowMenu.value || tableMenu.value || formulaEditor.value || viewMenu.value || cellMenu.value
  || filterBar.value?.isOpen || sheetRow.value !== null
));

const LAYOUT_ICONS: Record<Layout, unknown> = { table: Table2, chart: ChartColumn, pivot: Grid3x3, board: Columns3 };

/** The chart a chart view shows, and the one pinned above a table: both of the view's rows. */
const viewModel = computed(() => chartModel(display.value, view.value.chart, base.value, props.locale));
const pinnedModel = computed(() => chartModel(display.value, view.value.chart, shown.value.rows, props.locale));

/** A bar clicked in the chart above the grid narrows the grid to it; clicked again, lets go. */
function onPick(key: string) {
  const by = parseBy(view.value.chart?.x);
  if (!by) return;
  if (focus.value?.key === key) {
    focus.value = null;
    return;
  }
  const cat = pinnedModel.value.categories.find((c) => c.key === key);
  focus.value = { by, key, label: key === BLANK_KEY ? t('rich_table.chart.blank') : cat?.label ?? key };
  active.value = null;
  anchor.value = null;
}

/**
 * The table's name: how a chart elsewhere in the note, or another table's
 * formula, points at it — `chi-tieu[Số tiền]`. One word; hyphens allowed.
 */
const tableName = ref('');
watch(() => [props.table.name, tableMenu.value], () => { tableName.value = props.table.name ?? ''; }, { immediate: true });
const tableNameProblem = computed<'' | 'form' | 'taken'>(() => {
  const name = tableName.value.trim();
  if (!name || name === props.table.name) return '';
  if (!TABLE_NAME.test(name)) return 'form';
  // Two tables of one name and a formula could not tell which it reads.
  return props.others && name in props.others() ? 'taken' : '';
});
const tableNameValid = computed(() => !tableNameProblem.value);
function commitTableName() {
  const next = tableName.value.trim();
  if (!tableNameValid.value || next === (props.table.name ?? '')) return;
  change({ ...props.table, name: next || undefined });
}

// ─── Export ─────────────────────────────────────────────────────

const exportError = ref('');
/** A workbook is written by the desktop app only; a phone gets CSV. */
const workbooks = ref(true);
onMounted(async () => {
  try {
    workbooks.value = !['android', 'ios'].includes(await type());
  } catch { /* not in the app: a browser preview keeps the button */ }
});

/** The columns and rows this view shows, for exporting. */
function exportShape() {
  const cols = columns.value.map((_, i) => i).filter((i) => !hidden.value.has(i));
  return { cols, rows: base.value };
}

async function exportAs(kind: 'csv' | 'xlsx') {
  tableMenu.value = null;
  exportError.value = '';
  const name = (props.table.name || view.value.name || t('rich_table.export.file')).replace(/[\\/:*?"<>|]/g, ' ').trim();
  try {
    const path = await save({
      defaultPath: `${name}.${kind}`,
      filters: [kind === 'csv' ? { name: 'CSV', extensions: ['csv'] } : { name: 'Excel', extensions: ['xlsx'] }],
    });
    if (!path) return;
    const { cols, rows } = exportShape();
    if (kind === 'csv') {
      await writeTextFile(path, toCsv(viewGrid(display.value, rows, cols)));
    } else {
      await invoke('export_table_xlsx', {
        destination: path,
        // Named by the view or the table; the writer names it when neither is.
        sheets: [{ name: view.value.name || props.table.name || '', rows: toSheetRows(display.value, rows, cols) }],
      });
    }
  } catch (e) {
    exportError.value = t('rich_table.export.failed', { reason: String((e as { message?: string })?.message ?? e) });
  }
}

function newView(kind: Layout) {
  const fresh: View = kind === 'table' ? {} : { layout: kind };
  const firstSelect = columns.value.find((c) => c.type === 'select')?.name;
  const firstGroup = firstSelect ?? columns.value[0]?.name;
  if (kind === 'chart') fresh.chart = { kind: 'bar', x: firstGroup };
  if (kind === 'pivot') fresh.pivot = { rows: firstGroup };
  if (kind === 'board' && firstSelect) fresh.board = { by: firstSelect };
  fresh.name = t(`rich_table.layouts.${kind}`);
  // The first view gets a name too, once there are tabs to tell apart.
  let table = props.table;
  if (table.views.length === 1 && !table.views[0].name) {
    table = updateView(table, { name: t(`rich_table.layouts.${layoutOf(table.views[0])}`) }, 0);
  }
  change(addView(table, fresh));
  tableMenu.value = null;
  nextTick(() => chooseView(props.table.views.length - 1));
}

function openFormulaEditor(col: number) {
  if (editing.value) commit();
  columnMenu.value = null;
  const header = gridEl.value?.querySelector(`.rt-th[data-c="${col}"]`);
  if (!header) return;
  formulaEditor.value = { col, anchor: rect(header), others: props.others?.() ?? {} };
}

function closeFormulaEditor(expr?: string) {
  const open = formulaEditor.value;
  formulaEditor.value = null;
  preview.value = null;
  if (open && expr !== undefined) {
    change(updateColumn(props.table, open.col, { type: 'formula', expr }));
  }
  focusSink();
}

/** How each error code reads in the interface: `#DIV0` → `#CHIA0` in Vietnamese. */
const errorLabels = computed(() => Object.fromEntries(
  ['NAME', 'TYPE', 'DIV0', 'CYCLE', 'VALUE'].map((code) => [`#${code}`, t(`rich_table.formula.codes.${code}`)]),
));

/**
 * Why the selected cell is an error, in words — the cell shows only its code.
 * Worked out when asked for: one formula, run at one row.
 */
let problemCache: { table: RichTable; result: ReturnType<typeof computeTable> } | null = null;
const cellProblem = computed(() => {
  const column = activeColumn.value;
  if (!active.value || column?.type !== 'formula' || !isErrorCell(activeRaw.value)) return null;
  const row = order.value[active.value.r];
  // Stepping from one error cell to the next asks again of the same table.
  if (problemCache?.table !== props.table) problemCache = { table: props.table, result: computeTable(props.table, props.others?.() ?? {}) };
  const { result } = problemCache;
  const p = result.problems.get(active.value.c) ?? result.errors.get(`${row}:${active.value.c}`);
  if (!p) return null;
  const params: Record<string, string | number> = { ...p.params };
  if (p.key === 'arity') params.expected = p.params.min === p.params.max ? String(p.params.min) : `${p.params.min}–${p.params.max}`;
  return `${t(`rich_table.formula.codes.${p.code}`)} — ${t(`rich_table.formula.errors.${p.key}`, params)}`;
});

function openColumnMenu(col: number, e: MouseEvent) {
  if (props.readOnly) return;
  if (editing.value) commit();
  columnMenu.value = { col, anchor: rect(e.currentTarget as Element) };
}

function openFooterMenu(col: number, e: MouseEvent) {
  if (props.readOnly) return;
  footerMenu.value = { col, anchor: rect(e.currentTarget as Element) };
}

function openRowMenu(r: number, el: Element) {
  if (props.readOnly) {
    sheetRow.value = order.value[r];
    return;
  }
  if (editing.value) commit();
  const inRange = range.value && r >= range.value.r1 && r <= range.value.r2;
  if (!inRange) {
    anchor.value = { r, c: 0 };
    active.value = { r, c: columns.value.length - 1 };
  }
  rowMenu.value = { r, anchor: rect(el) };
}

function filterBy(col: number) {
  const header = gridEl.value?.querySelector(`.rt-th[data-c="${col}"]`);
  if (header) filterBar.value?.addFor(col, header.getBoundingClientRect());
}

function setSummary(col: number, kind: SummaryKind | null) {
  const name = columns.value[col].name;
  const summary = { ...view.value.summary };
  if (kind) summary[name] = kind;
  else delete summary[name];
  setView({ summary: Object.keys(summary).length ? summary : undefined });
  footerMenu.value = null;
}

/** Each footer's summary, worked out once per change rather than once per render. */
const summaries = computed(() => columns.value.map((_, c) => (hidden.value.has(c) ? null : summarize1(c))));
const summaryOf = (col: number) => summaries.value[col] ?? null;

function summarize1(col: number): string | null {
  const column = columns.value[col];
  const kind = view.value.summary?.[column.name];
  if (!isSummaryKind(column, kind)) return null;
  const values = order.value.map((ri) => display.value.rows[ri][col] ?? '');
  const value = summarize(column, kind, values, props.locale);
  // Errors are left out of a formula column's sum; say how many were.
  const errors = column.type === 'formula' ? values.filter(isErrorCell).length : 0;
  return errors ? `${value ?? '—'} (${t('rich_table.formula.error_count', errors)})` : value;
}

/** Which rows of the table the row menu acts on: the selected ones, or the one it was opened on. */
function menuRows(): number[] {
  const menu = rowMenu.value;
  if (!menu) return [];
  const g = range.value;
  if (g && menu.r >= g.r1 && menu.r <= g.r2) {
    return order.value.slice(g.r1, g.r2 + 1);
  }
  return [order.value[menu.r]];
}

function shiftKept(at: number, by: number) {
  keep.value = new Set([...keep.value].map((i) => (i >= at ? i + by : i)));
}

function addRow(at = props.table.rows.length) {
  if (props.readOnly) return;
  shiftKept(at, 1);
  keep.value = new Set([...keep.value, at]);
  change(insertRows(props.table, at, 1));
  rowMenu.value = null;
  nextTick(() => {
    const r = order.value.indexOf(at);
    if (r !== -1) {
      select({ r, c: 0 });
      focusSink();
      nextTick(reveal);
    }
  });
}

function rowAction(action: 'open' | 'above' | 'below' | 'duplicate' | 'up' | 'down' | 'delete') {
  const menu = rowMenu.value;
  if (!menu) return;
  const ri = order.value[menu.r];
  // Read while the menu still says which rows it was opened on.
  const rows = menuRows();
  rowMenu.value = null;
  switch (action) {
    case 'open':
      sheetRow.value = ri;
      return;
    case 'above':
      addRow(ri);
      return;
    case 'below':
      addRow(ri + 1);
      return;
    case 'duplicate':
      shiftKept(ri + 1, 1);
      keep.value = new Set([...keep.value, ri + 1]);
      change(insertRows(props.table, ri + 1, [props.table.rows[ri].slice()]));
      return;
    case 'up':
    case 'down': {
      const moved = moveRow(props.table, ri, ri + (action === 'up' ? -1 : 1));
      if (moved === props.table) return;
      keep.value = new Set();
      change(moved);
      nextTick(() => {
        const r = order.value.indexOf(ri + (action === 'up' ? -1 : 1));
        if (r !== -1) select({ r, c: active.value?.c ?? 0 });
      });
      return;
    }
    case 'delete': {
      keep.value = new Set();
      const at = active.value;
      change(deleteRows(props.table, rows));
      anchor.value = null;
      // Onto the row that took the first deleted one's place, once the table has it.
      const first = Math.min(...rows.map((row) => order.value.indexOf(row)).filter((r) => r !== -1));
      nextTick(() => {
        if (at && order.value.length) select({ r: Math.min(Number.isFinite(first) ? first : at.r, order.value.length - 1), c: at.c });
      });
      return;
    }
  }
}

/**
 * Write the view's sort into the table for good. The whole table, by the sort
 * alone: the rows the filter hides, and the order groups put rows in, are not
 * what "this order" means — taken from the screen, hidden rows would all
 * have gone to the end.
 */
function applyOrder() {
  const all = shownRows(props.table, { sort: view.value.sort }, props.locale).rows;
  change(updateView(reorderRows(props.table, all), { sort: undefined }, activeView.value));
  keep.value = new Set();
  tableMenu.value = null;
}

const clearSort = () => setView({ sort: undefined });

function toggleSearch() {
  searching.value = !searching.value;
  if (!searching.value) search.value = '';
}

// ─── Lifecycle ──────────────────────────────────────────────────

let observer: ResizeObserver | null = null;
onMounted(() => {
  window.addEventListener('pointerup', onPointerUp);
  window.addEventListener('pointercancel', onPointerUp);
  scroller.value?.addEventListener('scroll', measureSoon, { passive: true });
  if (typeof ResizeObserver !== 'undefined' && gridEl.value) {
    observer = new ResizeObserver(measureSoon);
    observer.observe(gridEl.value);
  }
});
onBeforeUnmount(() => {
  for (const stop of [...gestures]) stop();
  window.removeEventListener('pointerup', onPointerUp);
  window.removeEventListener('pointercancel', onPointerUp);
  scroller.value?.removeEventListener('scroll', measureSoon);
  observer?.disconnect();
  cancelAnimationFrame(frame);
});

/** Put the focus in the table, on its first or last row — arriving from the text above or below. */
function enter(from: 'top' | 'bottom') {
  if (!order.value.length) return;
  select({ r: from === 'top' ? 0 : order.value.length - 1, c: 0 });
  focusSink();
  nextTick(reveal);
}
defineExpose({ enter });

const sortLabel = computed(() => sortKeys(props.table, view.value).map(({ column, descending }) => ({
  name: columns.value[column].name, descending,
})));

const sumLabel = (n: number) => formatNumber(n, props.locale);
</script>

<template>
  <div
    ref="root"
    class="rt"
    :class="{ 'is-focused': focused, 'is-readonly': readOnly }"
    :data-row-height="rowHeightOf(view)"
    data-rt-interactive
    @focusin="onFocusIn"
    @focusout="onFocusOut"
  >
    <div v-if="notice" class="rt-notice">{{ notice }}</div>

    <div v-if="table.views.length > 1" class="rt-tabs" role="tablist">
      <button
        v-for="(v, i) in table.views"
        :key="i"
        type="button"
        role="tab"
        class="rt-tab"
        :aria-selected="i === activeView"
        :class="{ 'is-active': i === activeView }"
        @click="chooseView(i)"
      >
        <component :is="LAYOUT_ICONS[layoutOf(v)]" :size="13" />{{ v.name || t(`rich_table.layouts.${layoutOf(v)}`) }}
      </button>
    </div>

    <div class="rt-bar">
      <FilterBar ref="filterBar" :table="table" :view="activeView" :locale="locale" :read-only="readOnly" @change="change($event)" />
      <span v-if="focus" class="rt-pill">
        {{ focus.by.column }}: {{ focus.label }}
        <button type="button" class="rt-icon-btn" :aria-label="t('rich_table.filter.clear')" @click="focus = null"><X :size="12" /></button>
      </span>
      <span v-for="s in sortLabel" :key="s.name" class="rt-pill">
        <component :is="s.descending ? ArrowDown : ArrowUp" :size="12" />{{ s.name }}
        <button v-if="!readOnly" type="button" class="rt-icon-btn" :aria-label="t('rich_table.column.sort_clear')" @click="clearSort">
          <X :size="12" />
        </button>
      </span>
      <span class="rt-bar-spacer" />
      <input
        v-if="searching"
        v-model="search"
        class="rt-input rt-search"
        :placeholder="t('rich_table.search')"
        @keydown.esc="toggleSearch"
      >
      <button type="button" class="rt-bar-btn" :aria-label="t('rich_table.search')" :title="t('rich_table.search')" @click="toggleSearch">
        <Search :size="14" />
      </button>
      <button
        v-if="!readOnly"
        type="button"
        class="rt-bar-btn"
        :aria-label="t('rich_table.view.settings')"
        :title="t('rich_table.view.settings')"
        @click="viewMenu = rect($event.currentTarget as Element)"
      >
        <Settings2 :size="14" />
      </button>
      <button
        v-if="!readOnly"
        type="button"
        class="rt-bar-btn"
        :aria-label="t('common.more_actions')"
        :title="t('common.more_actions')"
        @click="tableMenu = rect($event.currentTarget as Element)"
      >
        <Ellipsis :size="14" />
      </button>
    </div>

    <ChartView
      v-if="layout === 'table' && view.chart"
      class="rt-chart-pinned"
      :model="pinnedModel"
      :locale="locale"
      :title="table.name || view.name"
      :height="200"
      @pick="onPick"
    />
    <ChartView v-if="layout === 'chart'" :model="viewModel" :locale="locale" :title="table.name || view.name" />
    <PivotView v-else-if="layout === 'pivot'" :table="display" :rows="base" :spec="view.pivot" :locale="locale" />
    <BoardView
      v-else-if="layout === 'board'"
      :table="display"
      :rows="base"
      :spec="view.board"
      :locale="locale"
      :read-only="readOnly"
      @change="change($event)"
      @open="sheetRow = $event"
    />

    <div v-if="layout === 'table'" ref="scroller" class="rt-scroll" :style="{ scrollPaddingLeft: `${frozenWidth}px` }">
      <div ref="gridEl" class="rt-grid" role="grid" :aria-rowcount="order.length + 1" :style="gridStyle">
        <div class="rt-row rt-head" role="row">
          <div class="rt-gutter rt-corner" />
          <div
            v-for="(column, c) in columns"
            :key="c"
            class="rt-th"
            role="columnheader"
            :tabindex="readOnly || hidden.has(c) ? -1 : 0"
            :data-c="c"
            :class="{ 'rt-stuck': stuck[c] !== null, 'rt-hidden': hidden.has(c), 'is-last': c === lastVisible }"
            :style="stuck[c] !== null ? { left: `${stuck[c]}px` } : undefined"
            @pointerdown.prevent="startDrag($event, 'column', c)"
            @click="onHeaderClick(c, $event)"
            @keydown.enter.prevent="openColumnMenu(c, $event as unknown as MouseEvent)"
            @keydown.space.prevent="openColumnMenu(c, $event as unknown as MouseEvent)"
          >
            <component :is="TYPE_ICONS[column.type]" :size="13" class="rt-th-icon" />
            <span class="rt-th-name">{{ column.name }}</span>
            <span
              class="rt-resize"
              @click.stop
              @pointerdown="startResize(c, $event)"
            />
          </div>
          <button
            v-if="!readOnly"
            type="button"
            class="rt-add-col"
            :title="t('rich_table.add_column')"
            :aria-label="t('rich_table.add_column')"
            @pointerdown.prevent
            @click="addColumn"
          >
            <Plus :size="14" />
          </button>
        </div>

        <div
          class="rt-body"
          @pointerdown="onBodyPointerDown"
          @pointerover="onBodyPointerOver"
          @click="onBodyClick"
          @dblclick="onBodyDblClick"
          @contextmenu="onContextMenu"
        >
          <RichTableBody
            :table="display"
            :items="items"
            :hidden="hidden"
            :styles="styles"
            :stuck="stuck"
            :locale="locale"
            :layout-key="layoutKey"
            :error-labels="errorLabels"
          />
        </div>

        <div v-if="!readOnly" class="rt-row rt-new">
          <button type="button" class="rt-new-btn" @click="addRow()">
            <Plus :size="14" />{{ t('rich_table.new_row') }}
          </button>
        </div>

        <div class="rt-row rt-foot" :class="{ 'is-empty': !hasSummary }" role="row">
          <div class="rt-gutter" />
          <button
            v-for="(column, c) in columns"
            :key="c"
            type="button"
            class="rt-foot-cell"
            :class="{ 'rt-stuck': stuck[c] !== null, 'has-value': view.summary?.[column.name], 'rt-hidden': hidden.has(c) }"
            :style="stuck[c] !== null ? { left: `${stuck[c]}px` } : undefined"
            :disabled="readOnly"
            @click="openFooterMenu(c, $event)"
          >
            <template v-if="isSummaryKind(column, view.summary?.[column.name])">
              <span class="rt-foot-kind" :title="t(`rich_table.summary.${view.summary![column.name]}`)">{{ t(`rich_table.summary.${view.summary![column.name]}`) }}</span>
              <span class="rt-foot-value">{{ summaryOf(c) ?? '—' }}</span>
            </template>
            <span v-else class="rt-foot-add">{{ t('rich_table.summary.add') }}</span>
          </button>
        </div>

        <div class="rt-drop-line" :style="box(dropLine)" />
        <div class="rt-fill-preview" :style="box(fillBox)" />
        <div v-if="fillAt" class="rt-fill" :class="overlayLayer" :style="fillAt" @pointerdown="startFill" />
        <div class="rt-range" :class="overlayLayer" :style="box(focused ? rangeBox : null)" />
        <div class="rt-active" :class="[overlayLayer, { 'is-blurred': !focused }]" :style="box(activeBox)" />

        <textarea
          ref="sink"
          class="rt-editor"
          :class="[overlayLayer, { 'is-editing': editing?.mode === 'text' || editing?.mode === 'pick' || editing?.mode === 'note', 'is-wrap': activeColumn?.type === 'text' }]"
          :style="activeBox ? { top: `${activeBox.top}px`, left: `${activeBox.left}px`, width: `${activeBox.width}px`, minHeight: `${activeBox.height}px` } : { top: '0', left: '0' }"
          inputmode="none"
          autocomplete="off"
          autocapitalize="off"
          spellcheck="false"
          rows="1"
          :aria-label="sinkLabel"
          :aria-activedescendant="activeOption"
          :readonly="readOnly"
          @keydown="onKeydown"
          @input="onInput"
          @copy="onCopy"
          @cut="onCut"
          @paste="onPaste"
        />

        <input
          v-if="editing?.mode === 'date' && activeBox"
          ref="dateInput"
          v-model="dateDraft"
          class="rt-date-editor"
          :type="activeColumn?.time ? 'datetime-local' : 'date'"
          :style="{ top: `${activeBox.top}px`, left: `${activeBox.left}px`, width: `${Math.max(activeBox.width, 160)}px`, height: `${activeBox.height}px` }"
          @keydown="onDateKey"
          @blur="commit()"
        >
      </div>
    </div>

    <div v-if="stats && layout === 'table'" class="rt-status">
      <span>{{ t('rich_table.stats.sum') }} <b>{{ sumLabel(stats.sum) }}</b></span>
      <span>{{ t('rich_table.stats.avg') }} <b>{{ sumLabel(stats.avg) }}</b></span>
      <span>{{ t('rich_table.stats.count') }} <b>{{ stats.count }}</b></span>
      <span>{{ t('rich_table.stats.min') }} <b>{{ sumLabel(stats.min) }}</b></span>
      <span>{{ t('rich_table.stats.max') }} <b>{{ sumLabel(stats.max) }}</b></span>
    </div>
    <div v-else-if="exportError" class="rt-status rt-status-warn">{{ exportError }}</div>
    <div v-else-if="cellProblem" class="rt-status rt-status-warn">{{ cellProblem }}</div>
    <div v-else-if="shown.filterUnread" class="rt-status rt-status-warn">{{ t('rich_table.filter.unread_note') }}</div>

    <!-- Choosing in a select or multi-select column. -->
    <FloatingPanel
      v-if="editing?.mode === 'pick' && activeBox && activeColumn"
      :anchor="cellEl(active!)?.getBoundingClientRect() ?? { left: 0, top: 0, bottom: 0, right: 0 }"
      :width="Math.max(activeBox.width, 220)"
      own-escape
      @close="commit()"
    >
      <div class="rt-menu rt-pick" role="listbox" :aria-multiselectable="activeColumn.type === 'multi'" @pointerdown.prevent>
        <div v-if="activeColumn.type === 'multi' && pick.items.length" class="rt-chips rt-pick-chosen">
          <span v-for="item in pick.items" :key="item" class="rt-chip" :style="optionColor(activeColumn, item)">
            {{ item }}
            <button type="button" class="rt-chip-x" @click="choose(item)"><X :size="10" /></button>
          </span>
        </div>
        <button
          v-for="(choice, i) in choices"
          :key="typeof choice === 'string' ? choice : `+${choice.create}`"
          :id="`${uid}-pick-${i}`"
          type="button"
          role="option"
          class="rt-item"
          :class="{ 'is-highlighted': i === pick.highlight }"
          :aria-selected="typeof choice === 'string' && pick.items.includes(choice)"
          @click="choose(choice)"
        >
          <template v-if="typeof choice === 'string'">
            <span class="rt-chip" :style="optionColor(activeColumn, choice)">{{ choice }}</span>
            <span v-if="pick.items.includes(choice)" class="rt-tick">✓</span>
          </template>
          <span v-else>{{ t('rich_table.pick.create', { name: choice.create }) }}</span>
        </button>
        <div v-if="!choices.length" class="rt-menu-note">{{ t('rich_table.pick.empty') }}</div>
        <button v-if="pick.items.length && activeColumn.type === 'select'" type="button" class="rt-item rt-muted" @click="pick = { items: [], highlight: 0 }; commit([1, 0])">
          {{ t('rich_table.pick.clear') }}
        </button>
      </div>
    </FloatingPanel>

    <!-- Picking a note for a Note column. -->
    <FloatingPanel
      v-if="editing?.mode === 'note' && activeBox && active"
      :anchor="cellEl(active)?.getBoundingClientRect() ?? { left: 0, top: 0, bottom: 0, right: 0 }"
      :width="Math.max(activeBox.width, 260)"
      own-escape
      @close="commit()"
    >
      <div class="rt-menu rt-pick" role="listbox" @pointerdown.prevent>
        <button
          v-for="(note, i) in notes"
          :id="`${uid}-note-${i}`"
          :key="note.id"
          type="button"
          role="option"
          :aria-selected="i === noteHighlight"
          class="rt-item"
          :class="{ 'is-highlighted': i === noteHighlight }"
          @click="chooseNote(note)"
        >
          <span class="rt-note-title">{{ note.title }}</span>
          <span class="rt-muted rt-note-path">{{ note.id }}</span>
        </button>
        <div v-if="!notes.length" class="rt-menu-note">{{ t('rich_table.note_picker.hint') }}</div>
      </div>
    </FloatingPanel>

    <FloatingPanel v-if="cellMenu" :anchor="cellMenu.anchor" :width="230" @close="cellMenu = null">
      <div class="rt-menu">
        <button type="button" class="rt-item" @click="cellAction('above')"><ArrowUpToLine :size="14" />{{ t('rich_table.row.insert_above') }}</button>
        <button type="button" class="rt-item" @click="cellAction('below')"><ArrowDownToLine :size="14" />{{ t('rich_table.row.insert_below') }}</button>
        <button type="button" class="rt-item" @click="cellAction('left')"><BetweenVerticalStart :size="14" />{{ t('rich_table.column.insert_left') }}</button>
        <button type="button" class="rt-item" @click="cellAction('right')"><BetweenVerticalEnd :size="14" />{{ t('rich_table.column.insert_right') }}</button>
        <div class="rt-menu-section" />
        <button type="button" class="rt-item" @click="cellAction('copy')"><ClipboardCopy :size="14" />{{ t('rich_table.ctx.copy') }}</button>
        <button type="button" class="rt-item" @click="cellAction('clear')"><Eraser :size="14" />{{ t('rich_table.ctx.clear') }}</button>
        <button type="button" class="rt-item" @click="cellAction('open')"><Maximize2 :size="14" />{{ t('rich_table.row.open') }}</button>
        <div class="rt-menu-section" />
        <button type="button" class="rt-item rt-danger" @click="cellAction('delete-rows')">
          <Trash2 :size="14" />{{ t('rich_table.row.delete', selectedRows().length) }}
        </button>
        <button v-if="columns.length > 1" type="button" class="rt-item rt-danger" @click="cellAction('delete-column')">
          <Trash2 :size="14" />{{ t('rich_table.column.delete') }}
        </button>
      </div>
    </FloatingPanel>

    <FloatingPanel v-if="columnMenu" :anchor="columnMenu.anchor" :width="260" @close="columnMenu = null">
      <ColumnMenu
        :table="table"
        :index="columnMenu.col"
        :view="activeView"
        :fresh="columnMenu.fresh"
        @change="change($event)"
        @filter="filterBy"
        @formula="openFormulaEditor"
        @close="columnMenu = null; focusSink()"
      />
    </FloatingPanel>

    <FloatingPanel
      v-if="formulaEditor"
      :anchor="formulaEditor.anchor"
      :width="440"
      own-escape
      @close="closeFormulaEditor()"
    >
      <FormulaEditor
        :table="table"
        :index="formulaEditor.col"
        :locale="locale"
        :others="formulaEditor.others"
        :order="order"
        @preview="preview = $event"
        @apply="closeFormulaEditor($event)"
        @cancel="closeFormulaEditor()"
      />
    </FloatingPanel>

    <FloatingPanel v-if="footerMenu" :anchor="footerMenu.anchor" :width="200" @close="footerMenu = null">
      <div class="rt-menu">
        <button type="button" class="rt-item" @click="setSummary(footerMenu.col, null)">{{ t('rich_table.summary.none') }}</button>
        <button
          v-for="kind in summariesFor(columns[footerMenu.col])"
          :key="kind"
          type="button"
          class="rt-item"
          @click="setSummary(footerMenu.col, kind)"
        >
          {{ t(`rich_table.summary.${kind}`) }}
          <span v-if="view.summary?.[columns[footerMenu.col].name] === kind" class="rt-tick">✓</span>
        </button>
      </div>
    </FloatingPanel>

    <FloatingPanel v-if="rowMenu" :anchor="rowMenu.anchor" :width="220" @close="rowMenu = null">
      <div class="rt-menu">
        <button type="button" class="rt-item" @click="rowAction('open')"><Maximize2 :size="14" />{{ t('rich_table.row.open') }}</button>
        <button type="button" class="rt-item" @click="rowAction('above')"><ArrowUpToLine :size="14" />{{ t('rich_table.row.insert_above') }}</button>
        <button type="button" class="rt-item" @click="rowAction('below')"><ArrowDownToLine :size="14" />{{ t('rich_table.row.insert_below') }}</button>
        <button type="button" class="rt-item" @click="rowAction('duplicate')"><Copy :size="14" />{{ t('rich_table.row.duplicate') }}</button>
        <template v-if="!sorted">
          <button type="button" class="rt-item" @click="rowAction('up')"><ChevronUp :size="14" />{{ t('rich_table.row.move_up') }}</button>
          <button type="button" class="rt-item" @click="rowAction('down')"><ChevronDown :size="14" />{{ t('rich_table.row.move_down') }}</button>
        </template>
        <button type="button" class="rt-item rt-danger" @click="rowAction('delete')">
          <Trash2 :size="14" />{{ t('rich_table.row.delete', menuRows().length) }}
        </button>
      </div>
    </FloatingPanel>

    <FloatingPanel v-if="viewMenu" :anchor="viewMenu" align-end :width="300" @close="viewMenu = null">
      <ViewMenu
        :table="table"
        :index="activeView"
        @change="change($event)"
        @select="chooseView($event, true)"
        @close="viewMenu = null"
      />
    </FloatingPanel>

    <FloatingPanel v-if="tableMenu" :anchor="tableMenu" align-end :width="240" @close="tableMenu = null">
      <div class="rt-menu">
        <div class="rt-menu-label">{{ t('rich_table.table_name') }}</div>
        <div class="rt-menu-section">
          <input
            class="rt-input"
            :class="{ 'is-invalid': !tableNameValid }"
            :value="tableName"
            :placeholder="t('rich_table.table_name_hint')"
            @input="tableName = ($event.target as HTMLInputElement).value"
            @keydown.enter.prevent="commitTableName"
            @blur="commitTableName"
          >
          <div v-if="tableNameProblem === 'taken'" class="rt-menu-note is-error">{{ t('rich_table.table_name_taken') }}</div>
        </div>
        <div class="rt-menu-label">{{ t('rich_table.view.add') }}</div>
        <button v-for="l in LAYOUTS" :key="l" type="button" class="rt-item" @click="newView(l)">
          <component :is="LAYOUT_ICONS[l]" :size="14" />{{ t(`rich_table.layouts.${l}`) }}
        </button>
        <div class="rt-menu-section" />
        <div class="rt-menu-label">{{ t('rich_table.export.title') }}</div>
        <button type="button" class="rt-item" @click="exportAs('csv')"><FileDown :size="14" />CSV</button>
        <button v-if="workbooks" type="button" class="rt-item" @click="exportAs('xlsx')"><FileDown :size="14" />Excel (.xlsx)</button>
        <div class="rt-menu-section" />
        <button v-if="sorted" type="button" class="rt-item" @click="applyOrder">
          <ListOrdered :size="14" />{{ t('rich_table.apply_order') }}
        </button>
        <button type="button" class="rt-item rt-danger" @click="tableMenu = null; emit('remove')">
          <Trash2 :size="14" />{{ t('rich_table.delete_table') }}
        </button>
      </div>
    </FloatingPanel>

    <RowSheet
      v-if="sheetRow !== null && table.rows[sheetRow]"
      :table="table"
      :row="sheetRow"
      :order="order"
      :locale="locale"
      :read-only="readOnly"
      @change="change($event)"
      @goto="sheetRow = $event"
      @close="sheetRow = null; focusSink()"
    />
  </div>
</template>
