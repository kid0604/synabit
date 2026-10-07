<script setup lang="ts">
/** Everything a column offers, opened from its header. */
import { computed, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import {
  ArrowUp, ArrowDown, ArrowUpDown, Filter, Pin, PinOff, WrapText,
  BetweenVerticalStart, BetweenVerticalEnd, ChevronLeft, ChevronRight, Trash2, X, Plus, Sigma, EyeOff,
} from 'lucide-vue-next';
import {
  COLUMN_TYPES, NUMBER_FORMATS, OPTION_COLORS, freshColumnName, validColumnName,
  type ColumnType, type RichTable,
} from '../model';
import {
  deleteColumn, insertColumn, moveColumn, renameColumn, updateColumn, updateView,
} from '../ops';
import { colorName, optionColor } from '../colors';
import { TYPE_ICONS } from './icons';

const props = defineProps<{
  table: RichTable;
  index: number;
  /** The view sorting and hiding apply to. */
  view?: number;
  /** A column just added: its name is selected, to be typed over. */
  fresh?: boolean;
}>();
const at = computed(() => props.view ?? 0);
const emit = defineEmits<{
  change: [table: RichTable];
  filter: [column: number];
  /** Open the formula editor on this column. */
  formula: [column: number];
  close: [];
}>();
const { t } = useI18n();

const column = computed(() => props.table.columns[props.index]);
const name = ref(column.value.name);
watch(column, (c) => { name.value = c.name; });
const nameInput = ref<HTMLInputElement | null>(null);
onMounted(() => {
  if (!props.fresh) return;
  setTimeout(() => {
    nameInput.value?.focus();
    nameInput.value?.select();
  }, 0);
});
const nameValid = computed(() => validColumnName(props.table, name.value, props.index));
const confirmingDelete = ref(false);
/** A column with values in it, about to become a formula, which would compute over them. */
const confirmingFormula = ref(false);
const newOption = ref('');

const sortedHere = computed(() => {
  const keys = props.table.views[at.value].sort ?? [];
  if (keys[0] === column.value.name) return 'asc';
  if (keys[0] === `-${column.value.name}`) return 'desc';
  return null;
});
const frozenHere = computed(() => (props.table.freeze ?? 0) > props.index);

function commitName() {
  const next = name.value.trim();
  if (next === column.value.name) return;
  if (!nameValid.value) {
    name.value = column.value.name;
    return;
  }
  emit('change', renameColumn(props.table, props.index, next));
}

const change = (table: RichTable) => emit('change', table);
function setType(type: ColumnType) {
  if (type === column.value.type) return;
  if (type === 'formula') {
    const filled = props.table.rows.some((r) => (r[props.index] ?? '').trim());
    if (filled && !confirmingFormula.value) {
      confirmingFormula.value = true;
      return;
    }
    change(updateColumn(props.table, props.index, { type }));
    emit('formula', props.index);
    return;
  }
  // From a formula to anything else, the values it computed stay, as data.
  change(updateColumn(props.table, props.index, { type }));
}
const setFormat = (format: string) =>
  change(updateColumn(props.table, props.index, { format: format === 'number' ? undefined : format }));

function sort(direction: 'asc' | 'desc' | null) {
  const n = column.value.name;
  change(updateView(props.table, { sort: direction ? [direction === 'desc' ? `-${n}` : n] : undefined }, at.value));
  emit('close');
}

function hide() {
  const view = props.table.views[at.value];
  const hidden = new Set(view.hide ?? []);
  hidden.add(column.value.name);
  if (hidden.size < props.table.columns.length) change(updateView(props.table, { hide: [...hidden] }, at.value));
  emit('close');
}

function freeze() {
  change({ ...props.table, freeze: frozenHere.value ? undefined : props.index + 1 });
  emit('close');
}

function insert(offset: 0 | 1) {
  const fresh = { name: freshColumnName(props.table, t('rich_table.default_column')), type: 'text' as const };
  change(insertColumn(props.table, props.index + offset, fresh));
  emit('close');
}

function move(by: -1 | 1) {
  change(moveColumn(props.table, props.index, props.index + by));
  emit('close');
}

function remove() {
  if (!confirmingDelete.value) {
    confirmingDelete.value = true;
    return;
  }
  change(deleteColumn(props.table, props.index));
  emit('close');
}

function addOption() {
  const option = newOption.value.trim();
  newOption.value = '';
  if (!option || column.value.options?.includes(option)) return;
  change(updateColumn(props.table, props.index, { options: [...(column.value.options ?? []), option] }));
}

function removeOption(option: string) {
  const colors = { ...column.value.colors };
  delete colors[option];
  change(updateColumn(props.table, props.index, {
    options: column.value.options?.filter((o) => o !== option),
    colors: Object.keys(colors).length ? colors : undefined,
  }));
}

/** Click the dot to step the option through the palette. */
function cycleColor(option: string) {
  const now = colorName(column.value, option);
  const next = OPTION_COLORS[(OPTION_COLORS.indexOf(now as typeof OPTION_COLORS[number]) + 1) % OPTION_COLORS.length];
  change(updateColumn(props.table, props.index, { colors: { ...column.value.colors, [option]: next } }));
}
</script>

<template>
  <div class="rt-menu">
    <div class="rt-menu-section">
      <input
        ref="nameInput"
        v-model="name"
        class="rt-input"
        :class="{ 'is-invalid': !nameValid }"
        :aria-label="t('rich_table.column.rename')"
        @keydown.enter.prevent="commitName(); emit('close')"
        @blur="commitName"
      >
    </div>

    <div v-if="column.type === 'formula'" class="rt-menu-section">
      <button type="button" class="rt-item" @click="emit('formula', index)">
        <Sigma :size="14" />{{ t('rich_table.formula.edit') }}
      </button>
      <code class="rt-menu-note rt-fx-preview">{{ column.expr || '—' }}</code>
    </div>

    <div class="rt-menu-section">
      <div class="rt-menu-label">{{ t('rich_table.column.type') }}</div>
      <div v-if="column.foreignType" class="rt-menu-note">
        {{ t('rich_table.column.foreign', { type: column.foreignType }) }}
      </div>
      <div v-else class="rt-type-grid">
        <button
          v-for="type in COLUMN_TYPES"
          :key="type"
          type="button"
          class="rt-type"
          :class="{ 'is-active': column.type === type }"
          @click="setType(type)"
        >
          <component :is="TYPE_ICONS[type]" :size="14" />
          <span>{{ t(`rich_table.types.${type}`) }}</span>
        </button>
      </div>

      <div v-if="confirmingFormula" class="rt-menu-note rt-warn">{{ t('rich_table.formula.replaces_values') }}</div>

      <label v-if="column.type === 'number' || column.type === 'formula'" class="rt-menu-row">
        <span>{{ t('rich_table.column.format') }}</span>
        <select class="rt-select" :value="column.format ?? 'number'" @change="setFormat(($event.target as HTMLSelectElement).value)">
          <option v-for="f in NUMBER_FORMATS" :key="f" :value="f">{{ t(`rich_table.formats.${f.replace(':', '_')}`) }}</option>
        </select>
      </label>

      <label v-if="column.type === 'date'" class="rt-menu-row">
        <input
          type="checkbox"
          :checked="column.time"
          @change="change(updateColumn(table, index, { time: ($event.target as HTMLInputElement).checked || undefined }))"
        >
        <span>{{ t('rich_table.column.with_time') }}</span>
      </label>

      <div v-if="column.type === 'select' || column.type === 'multi'" class="rt-options">
        <div v-for="option in column.options ?? []" :key="option" class="rt-option-row">
          <button
            type="button"
            class="rt-dot"
            :style="optionColor(column, option)"
            :aria-label="t('rich_table.column.color')"
            @click="cycleColor(option)"
          />
          <span class="rt-chip" :style="optionColor(column, option)">{{ option }}</span>
          <button type="button" class="rt-icon-btn" :aria-label="t('common.delete')" @click="removeOption(option)">
            <X :size="12" />
          </button>
        </div>
        <div class="rt-option-row">
          <Plus :size="12" class="rt-muted" />
          <input
            v-model="newOption"
            class="rt-input rt-input-sm"
            :placeholder="t('rich_table.column.add_option')"
            @keydown.enter.prevent="addOption"
          >
        </div>
      </div>
    </div>

    <div class="rt-menu-section">
      <button v-if="sortedHere !== 'asc'" type="button" class="rt-item" @click="sort('asc')">
        <ArrowUp :size="14" />{{ t('rich_table.column.sort_asc') }}
      </button>
      <button v-if="sortedHere !== 'desc'" type="button" class="rt-item" @click="sort('desc')">
        <ArrowDown :size="14" />{{ t('rich_table.column.sort_desc') }}
      </button>
      <button v-if="sortedHere" type="button" class="rt-item" @click="sort(null)">
        <ArrowUpDown :size="14" />{{ t('rich_table.column.sort_clear') }}
      </button>
      <button type="button" class="rt-item" @click="emit('filter', index); emit('close')">
        <Filter :size="14" />{{ t('rich_table.column.filter_by') }}
      </button>
    </div>

    <div class="rt-menu-section">
      <button v-if="table.columns.length > 1" type="button" class="rt-item" @click="hide">
        <EyeOff :size="14" />{{ t('rich_table.column.hide') }}
      </button>
      <button type="button" class="rt-item" @click="freeze">
        <component :is="frozenHere ? PinOff : Pin" :size="14" />
        {{ frozenHere ? t('rich_table.column.unfreeze') : t('rich_table.column.freeze') }}
      </button>
      <button type="button" class="rt-item" @click="change(updateColumn(table, index, { wrap: !column.wrap || undefined }))">
        <WrapText :size="14" />{{ t('rich_table.column.wrap') }}
        <span v-if="column.wrap" class="rt-tick">✓</span>
      </button>
      <button type="button" class="rt-item" @click="insert(0)">
        <BetweenVerticalStart :size="14" />{{ t('rich_table.column.insert_left') }}
      </button>
      <button type="button" class="rt-item" @click="insert(1)">
        <BetweenVerticalEnd :size="14" />{{ t('rich_table.column.insert_right') }}
      </button>
      <button v-if="index > 0" type="button" class="rt-item" @click="move(-1)">
        <ChevronLeft :size="14" />{{ t('rich_table.column.move_left') }}
      </button>
      <button v-if="index < table.columns.length - 1" type="button" class="rt-item" @click="move(1)">
        <ChevronRight :size="14" />{{ t('rich_table.column.move_right') }}
      </button>
      <button
        v-if="table.columns.length > 1"
        type="button"
        class="rt-item rt-danger"
        @click="remove"
      >
        <Trash2 :size="14" />
        {{ confirmingDelete ? t('rich_table.column.delete_confirm') : t('rich_table.column.delete') }}
      </button>
    </div>
  </div>
</template>
