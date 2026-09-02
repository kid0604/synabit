<script setup lang="ts">
/**
 * One property's value, drawn as the kind of thing it is.
 *
 * The panel this replaces printed every value through `String()`, so a note
 * showed `full_width  false`, `linked_projects  []` and `tags
 * ["mdp","network"]`. All three are a person being handed the serialisation
 * and asked to do the decoding — and the last one is worse than ugly, because
 * the only way to add a tag was to type JSON punctuation correctly.
 *
 * The text is still the model. Everything here writes back a string, and the
 * string is what `valueOf` compares against the original to decide whether the
 * field was touched at all — so a switch that is never clicked leaves the file
 * byte-identical.
 */
import { computed, ref } from 'vue';
import { X, Plus } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';
import type { FieldKind } from '../fieldValue';

const props = defineProps<{
  kind: FieldKind;
  readonly?: boolean;
  placeholder?: string;
}>();

const model = defineModel<string>({ required: true });
const emit = defineEmits<{ change: [] }>();

const { t } = useI18n();

/**
 * Enter ends the edit, the way leaving the field does.
 *
 * It did nothing here, so a value typed into a new field sat there looking
 * saved and was not: the only way to commit it was to click somewhere else.
 * Blurring rather than emitting directly, so there is one path out of the
 * field and Enter and a click cannot drift apart.
 */
const box = ref<HTMLInputElement | null>(null);
defineExpose({ focus: () => box.value?.focus() });

const commit = (next: string) => {
  model.value = next;
  emit('change');
};

/* ── boolean ──────────────────────────────────────────── */

const on = computed(() => model.value.trim() === 'true');
const toggle = () => {
  if (props.readonly) return;
  commit(on.value ? 'false' : 'true');
};

/* ── list ─────────────────────────────────────────────── */

/**
 * The list as chips, or nothing if the text is not a list after all.
 *
 * A merge between two devices can leave anything in a field, so this never
 * assumes the parse succeeds; when it does not, the row falls back to plain
 * text and the value is still editable rather than stuck behind a widget that
 * cannot represent it.
 */
const items = computed<string[] | null>(() => {
  const t = model.value.trim();
  // An unfilled list is an empty list, not a text box. This is the one moment
  // a declared kind is all there is to go on, and falling through here asked
  // for JSON punctuation by hand — the thing chips exist to end.
  if (!t) return [];
  if (!t.startsWith('[')) return null;
  try {
    const parsed = JSON.parse(t);
    return Array.isArray(parsed) ? parsed.map(v => (typeof v === 'string' ? v : JSON.stringify(v))) : null;
  } catch {
    return null;
  }
});

const writeItems = (next: string[]) => commit(JSON.stringify(next));

const draft = ref('');
const adding = ref(false);

const addItem = () => {
  const value = draft.value.trim();
  draft.value = '';
  adding.value = false;
  if (!value) return;
  writeItems([...(items.value ?? []), value]);
};

const removeItem = (index: number) => {
  writeItems((items.value ?? []).filter((_, i) => i !== index));
};
</script>

<template>
  <!-- A switch, because the value has two states and a text box has infinite. -->
  <button
    v-if="kind === 'boolean'"
    type="button"
    :disabled="readonly"
    @click="toggle"
    class="mt-1 inline-flex items-center gap-2 text-sm cursor-pointer disabled:cursor-default group/switch"
  >
    <span
      class="w-8 h-[18px] rounded-full flex items-center transition-colors px-0.5"
      :class="on ? 'bg-blue-500' : 'bg-gray-200 dark:bg-gray-700'"
    >
      <span
        class="w-3.5 h-3.5 rounded-full bg-white shadow-sm transition-transform"
        :class="on ? 'translate-x-[14px]' : 'translate-x-0'"
      />
    </span>
    <span class="text-gray-500 dark:text-gray-400 text-xs">
      {{ on ? t('things.value_yes') : t('things.value_no') }}
    </span>
  </button>

  <!-- Chips, so adding a tag is typing a word rather than JSON punctuation. -->
  <div v-else-if="kind === 'list' && items" class="flex flex-wrap items-center gap-1.5 pt-0.5 min-w-0">
    <span
      v-for="(item, index) in items"
      :key="`${item}-${index}`"
      class="inline-flex items-center gap-1 pl-2 pr-1 py-0.5 rounded-full text-xs
             bg-gray-100 dark:bg-white/10 text-gray-600 dark:text-gray-300 max-w-full"
    >
      <span class="truncate">{{ item }}</span>
      <button
        v-if="!readonly"
        type="button"
        @click="removeItem(index)"
        class="p-0.5 rounded-full text-gray-400 hover:text-red-500 cursor-pointer"
        :aria-label="t('things.remove_field')"
      >
        <X class="w-2.5 h-2.5" />
      </button>
    </span>

    <input
      v-if="adding"
      v-model="draft"
      autofocus
      :placeholder="t('things.field_value')"
      @keydown.enter.prevent="addItem"
      @keydown.esc="adding = false; draft = ''"
      @blur="addItem"
      class="px-2 py-0.5 min-w-[80px] w-[110px] rounded-full text-xs bg-gray-50 dark:bg-white/5
             border border-gray-200 dark:border-gray-700 outline-none
             text-[#1c1c1e] dark:text-[#f4f4f5]"
    />
    <button
      v-else-if="!readonly"
      type="button"
      @click="adding = true"
      class="inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded-full text-xs text-gray-400
             hover:text-gray-600 dark:hover:text-gray-300 cursor-pointer"
    >
      <Plus class="w-3 h-3" />
      <span v-if="!items.length">{{ t('things.field_value') }}</span>
    </button>
  </div>

  <!--
    A date picker for a date, a number pad for a number. `type` is the whole
    change: the value still travels as the same text either way.
  -->
  <input
    v-else
    ref="box"
    v-model="model"
    :type="kind === 'date' && model.length <= 10 ? 'date' : kind === 'number' ? 'number' : 'text'"
    :readonly="readonly"
    spellcheck="false"
    :placeholder="placeholder"
    @blur="emit('change')"
    @keydown.enter.prevent="box?.blur()"
    class="flex-1 min-w-0 px-2 py-1 rounded bg-transparent hover:bg-gray-50 dark:hover:bg-white/5
           focus:bg-gray-50 dark:focus:bg-white/5 border border-transparent focus:border-gray-200
           dark:focus:border-gray-700 outline-none text-sm read-only:text-gray-500
           text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-300 transition-colors"
    :class="kind === 'json' ? 'font-mono text-xs' : ''"
  />
</template>
