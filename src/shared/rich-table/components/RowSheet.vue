<script setup lang="ts">
/**
 * One row as a form: every column a field. On a phone this is how a row is
 * edited — a grid of small cells is no place for a thumb — and on a desktop it
 * is the ↗ at the start of each row.
 */
import { computed, nextTick, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { ChevronLeft, ChevronRight, X } from 'lucide-vue-next';
import {
  draftOf, isChecked, isReadOnlyColumn, itemsOf, rawFromInput, type Column, type RichTable,
} from '../model';
import { setCells } from '../ops';
import { optionColor } from '../colors';
import { TYPE_ICONS } from './icons';

const props = defineProps<{
  table: RichTable;
  row: number;
  /** The rows in the order the view shows them, for stepping through. */
  order: number[];
  locale: string;
  readOnly?: boolean;
}>();
const emit = defineEmits<{ change: [table: RichTable]; goto: [row: number]; close: [] }>();
const { t } = useI18n();

const at = computed(() => props.order.indexOf(props.row));
const cells = computed(() => props.table.rows[props.row] ?? []);

function set(col: number, raw: string) {
  emit('change', setCells(props.table, [{ row: props.row, col, raw }]));
}

function typed(col: number, column: Column, value: string) {
  set(col, rawFromInput(column, value, props.locale));
}

function toggleItem(col: number, item: string) {
  const items = itemsOf(cells.value[col] ?? '');
  const next = items.includes(item) ? items.filter((i) => i !== item) : [...items, item];
  set(col, next.join(', '));
}

const locked = (column: Column) => props.readOnly || isReadOnlyColumn(column);

function dateInputValue(column: Column, raw: string): string {
  return column.time ? raw.replace(' ', 'T') : raw.slice(0, 10);
}

const sheet = ref<HTMLElement | null>(null);
onMounted(() => nextTick(() => {
  const first = sheet.value?.querySelector<HTMLElement>('.rt-sheet-body :is(input, textarea, select, button):not(:disabled)');
  (first ?? sheet.value)?.focus({ preventScroll: true });
}));

/** Closing keeps what was typed: a field writes on blur, and Escape would otherwise skip it. */
function close() {
  const el = document.activeElement as HTMLElement | null;
  if (el && sheet.value?.contains(el)) el.blur();
  emit('close');
}

/** Tab goes round the sheet, not out of it to the note behind. */
function trap(e: KeyboardEvent) {
  if (e.key !== 'Tab' || !sheet.value) return;
  const all = [...sheet.value.querySelectorAll<HTMLElement>('input, textarea, select, button')].filter((el) => !(el as HTMLInputElement).disabled);
  if (!all.length) return;
  const first = all[0];
  const last = all[all.length - 1];
  if (e.shiftKey && document.activeElement === first) {
    e.preventDefault();
    last.focus();
  } else if (!e.shiftKey && document.activeElement === last) {
    e.preventDefault();
    first.focus();
  }
}
</script>

<template>
  <Teleport to="body">
    <div class="rt-sheet-backdrop" @pointerdown.self="close">
      <div
        ref="sheet"
        class="rt-sheet"
        role="dialog"
        aria-modal="true"
        tabindex="-1"
        :aria-label="t('rich_table.sheet.title', { n: at + 1 })"
        @keydown.esc.stop.prevent="close"
        @keydown.tab="trap"
      >
        <header class="rt-sheet-head">
          <button type="button" class="rt-icon-btn" :disabled="at <= 0" :aria-label="t('rich_table.sheet.prev')" @click="emit('goto', order[at - 1])">
            <ChevronLeft :size="16" />
          </button>
          <span class="rt-sheet-title">{{ t('rich_table.sheet.title', { n: at + 1 }) }}</span>
          <button type="button" class="rt-icon-btn" :disabled="at >= order.length - 1" :aria-label="t('rich_table.sheet.next')" @click="emit('goto', order[at + 1])">
            <ChevronRight :size="16" />
          </button>
          <button type="button" class="rt-icon-btn rt-sheet-close" :aria-label="t('common.close')" @click="close">
            <X :size="16" />
          </button>
        </header>

        <div class="rt-sheet-body">
          <label v-for="(column, c) in table.columns" :key="`${row}-${c}`" class="rt-field">
            <span class="rt-field-label">
              <component :is="TYPE_ICONS[column.type]" :size="13" />{{ column.name }}
            </span>

            <span v-if="column.type === 'checkbox' && !column.foreignType" class="rt-field-check">
              <input
                type="checkbox"
                :checked="isChecked(cells[c] ?? '')"
                :disabled="locked(column)"
                @change="set(c, ($event.target as HTMLInputElement).checked ? '[x]' : '')"
              >
            </span>

            <select
              v-else-if="column.type === 'select' && !column.foreignType"
              class="rt-select"
              :value="(cells[c] ?? '').trim()"
              :disabled="locked(column)"
              @change="set(c, ($event.target as HTMLSelectElement).value)"
            >
              <option value="" />
              <option
                v-if="(cells[c] ?? '').trim() && !column.options?.includes((cells[c] ?? '').trim())"
                :value="(cells[c] ?? '').trim()"
              >
                {{ cells[c] }}
              </option>
              <option v-for="o in column.options ?? []" :key="o" :value="o">{{ o }}</option>
            </select>

            <span v-else-if="column.type === 'multi' && !column.foreignType" class="rt-chips rt-field-chips">
              <button
                v-for="o in column.options ?? []"
                :key="o"
                type="button"
                class="rt-chip rt-chip-toggle"
                :class="{ 'is-off': !itemsOf(cells[c] ?? '').includes(o) }"
                :style="optionColor(column, o)"
                :disabled="locked(column)"
                @click="toggleItem(c, o)"
              >{{ o }}</button>
            </span>

            <input
              v-else-if="column.type === 'date' && !column.foreignType"
              class="rt-input"
              :type="column.time ? 'datetime-local' : 'date'"
              :value="dateInputValue(column, cells[c] ?? '')"
              :disabled="locked(column)"
              @change="typed(c, column, ($event.target as HTMLInputElement).value)"
            >

            <textarea
              v-else
              class="rt-input rt-field-text"
              rows="1"
              :inputmode="column.type === 'number' ? 'decimal' : column.type === 'url' ? 'url' : 'text'"
              :value="draftOf(column, cells[c] ?? '', locale)"
              :readonly="locked(column)"
              @change="typed(c, column, ($event.target as HTMLTextAreaElement).value)"
            />
          </label>
        </div>
      </div>
    </div>
  </Teleport>
</template>
