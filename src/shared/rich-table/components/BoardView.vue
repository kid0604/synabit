<script setup lang="ts">
/**
 * A board: a lane for each option of a select column, a card for each row.
 * Moving a card to another lane is writing that option into the row — by
 * dragging it, or, from the keyboard, Alt+← / Alt+→ on a focused card.
 *
 * Dragging is done with pointer events rather than HTML drag and drop, which
 * a touch screen does not have.
 */
import { computed, onBeforeUnmount, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { Plus } from 'lucide-vue-next';
import { displayText, isBlank, type BoardSpec, type RichTable } from '../model';
import { insertRows, setCells } from '../ops';
import { optionColor } from '../colors';

const props = defineProps<{ table: RichTable; rows: number[]; spec?: BoardSpec; locale: string; readOnly?: boolean }>();
const emit = defineEmits<{ change: [table: RichTable]; open: [row: number] }>();
const { t } = useI18n();

const by = computed(() => props.table.columns.findIndex((c) => c.name === props.spec?.by && c.type === 'select'));
const column = computed(() => props.table.columns[by.value]);
const titleAt = computed(() => {
  const named = props.table.columns.findIndex((c) => c.name === props.spec?.title);
  if (named !== -1) return named;
  const text = props.table.columns.findIndex((c, i) => c.type === 'text' && i !== by.value);
  return text === -1 ? 0 : text;
});
const shown = computed(() => (props.spec?.show ?? [])
  .map((n) => props.table.columns.findIndex((c) => c.name === n))
  .filter((i) => i !== -1 && i !== titleAt.value && i !== by.value));

const BLANK = '';
const lanes = computed(() => {
  if (by.value === -1) return [];
  const options = column.value.options ?? [];
  const cards = new Map<string, number[]>(options.map((o) => [o, []]));
  cards.set(BLANK, []);
  for (const r of props.rows) {
    const v = (props.table.rows[r][by.value] ?? '').trim();
    if (!cards.has(v)) cards.set(v, []);
    cards.get(v)!.push(r);
  }
  return [...cards.entries()]
    .filter(([key, rs]) => key !== BLANK || rs.length)
    .map(([key, rs]) => ({ key, rows: rs }));
});

function move(row: number, lane: string) {
  if (props.readOnly || by.value === -1) return;
  if ((props.table.rows[row][by.value] ?? '').trim() === lane) return;
  emit('change', setCells(props.table, [{ row, col: by.value, raw: lane }]));
}

function add(lane: string) {
  if (props.readOnly || by.value === -1) return;
  const row = props.table.columns.map((_, i) => (i === by.value ? lane : ''));
  const at = props.table.rows.length;
  emit('change', insertRows(props.table, at, [row]));
  emit('open', at);
}

function onKey(row: number, laneIndex: number, e: KeyboardEvent) {
  if (e.key === 'Enter') {
    e.preventDefault();
    emit('open', row);
  } else if (e.altKey && (e.key === 'ArrowLeft' || e.key === 'ArrowRight')) {
    e.preventDefault();
    const next = lanes.value[laneIndex + (e.key === 'ArrowLeft' ? -1 : 1)];
    if (next) move(row, next.key);
  }
}

// ─── Dragging ───────────────────────────────────────────────────

const drag = ref<{ row: number; x: number; y: number; w: number; dx: number; dy: number; over: string | null } | null>(null);
let pending: { row: number; x: number; y: number; el: HTMLElement; id: number } | null = null;

function onDown(row: number, e: PointerEvent) {
  if (e.button !== 0) return;
  pending = { row, x: e.clientX, y: e.clientY, el: e.currentTarget as HTMLElement, id: e.pointerId };
  window.addEventListener('pointermove', onMove);
  window.addEventListener('pointerup', onUp);
  window.addEventListener('pointercancel', onCancel);
}

function stopListening() {
  window.removeEventListener('pointermove', onMove);
  window.removeEventListener('pointerup', onUp);
  window.removeEventListener('pointercancel', onCancel);
}

/** The browser took the gesture — a scroll, a second finger: nothing moves, nothing opens. */
function onCancel() {
  stopListening();
  drag.value = null;
  pending = null;
}

function laneAt(x: number, y: number): string | null {
  const el = document.elementFromPoint(x, y)?.closest('[data-lane]') as HTMLElement | null;
  return el ? el.dataset.lane ?? null : null;
}

function onMove(e: PointerEvent) {
  if (pending && !drag.value) {
    // A press that has not moved is a click, which opens the card.
    if (Math.hypot(e.clientX - pending.x, e.clientY - pending.y) < 5) return;
    // Read-only: a card opens, but goes nowhere.
    if (props.readOnly) {
      onCancel();
      return;
    }
    const box = pending.el.getBoundingClientRect();
    drag.value = { row: pending.row, x: e.clientX, y: e.clientY, w: box.width, dx: pending.x - box.left, dy: pending.y - box.top, over: null };
  }
  if (!drag.value) return;
  e.preventDefault();
  drag.value = { ...drag.value, x: e.clientX, y: e.clientY, over: laneAt(e.clientX, e.clientY) };
}

function onUp() {
  stopListening();
  const d = drag.value;
  drag.value = null;
  if (d && d.over !== null) move(d.row, d.over);
  else if (pending && !d) emit('open', pending.row);
  pending = null;
}

onBeforeUnmount(stopListening);

const laneName = (key: string) => key || t('rich_table.board.none');
const cardTitle = (row: number) => {
  const raw = props.table.rows[row][titleAt.value] ?? '';
  return isBlank(raw) ? t('rich_table.board.untitled') : displayText(props.table.columns[titleAt.value], raw, props.locale);
};
</script>

<template>
  <div class="rt-board">
    <div v-if="by === -1" class="rt-chart-empty">{{ t('rich_table.board.choose') }}</div>
    <div
      v-for="(lane, li) in lanes"
      v-else
      :key="lane.key"
      class="rt-lane"
      :class="{ 'is-over': drag?.over === lane.key }"
      :data-lane="lane.key"
    >
      <div class="rt-lane-head">
        <span v-if="lane.key" class="rt-chip" :style="optionColor(column, lane.key)">{{ laneName(lane.key) }}</span>
        <span v-else class="rt-muted">{{ laneName(lane.key) }}</span>
        <span class="rt-muted">{{ lane.rows.length }}</span>
      </div>
      <div
        v-for="r in lane.rows"
        :key="r"
        class="rt-card"
        :class="{ 'is-dragging': drag?.row === r }"
        tabindex="0"
        role="button"
        @pointerdown="onDown(r, $event)"
        @keydown="onKey(r, li, $event)"
      >
        <div class="rt-card-title">{{ cardTitle(r) }}</div>
        <div v-for="c in shown" v-show="!isBlank(table.rows[r][c] ?? '')" :key="c" class="rt-card-field">
          <span class="rt-muted">{{ table.columns[c].name }}</span>
          <span>{{ displayText(table.columns[c], table.rows[r][c] ?? '', locale) }}</span>
        </div>
      </div>
      <button v-if="!readOnly" type="button" class="rt-new-btn rt-lane-add" @click="add(lane.key)">
        <Plus :size="13" />{{ t('rich_table.new_row') }}
      </button>
    </div>

    <Teleport to="body">
      <div
        v-if="drag"
        class="rt-card rt-card-ghost"
        :style="{ left: `${drag.x - drag.dx}px`, top: `${drag.y - drag.dy}px`, width: `${drag.w}px` }"
      >
        <div class="rt-card-title">{{ cardTitle(drag.row) }}</div>
      </div>
    </Teleport>
  </div>
</template>
