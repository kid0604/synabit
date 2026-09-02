<script setup lang="ts">
/**
 * Designing a kind before anything of that kind exists.
 *
 * Until now a kind came into being by accident: you made a thing, typed a word
 * for what it was, and the kind existed because a file said so. That is the
 * right way round for capture and the wrong way round for design — sometimes
 * you know you are going to keep track of books long before you have a book to
 * enter, and working out what a book *is* first is the whole point.
 *
 * What it writes is a schema file and nothing else. No placeholder node, no
 * empty folder — a kind with no things in it appears in the manager marked as
 * designed, and stays that way until somebody makes one.
 *
 * A page rather than a dialog, and part of the manager rather than a screen of
 * its own: designing a kind and looking over the kinds you have are the same
 * job five seconds apart.
 */
import { ref, computed, nextTick } from 'vue';
import { useI18n } from 'vue-i18n';
import { Plus, X } from 'lucide-vue-next';
import type { FieldKind } from '../fieldValue';
import FieldKindPicker from './FieldKindPicker.vue';

const props = defineProps<{
  /** Kinds that already exist, to warn rather than to forbid. */
  existing: string[];
}>();

const emit = defineEmits<{
  create: [nodeType: string, fields: { key: string; kind: FieldKind }[]];
  cancel: [];
}>();

const { t } = useI18n();


const name = ref('');
const rows = ref<{ key: string; kind: FieldKind }[]>([{ key: '', kind: 'text' }]);
const keyBox = ref<HTMLInputElement[] | null>(null);

/** Lower-cased, so `Book` and `book` never become two kinds. */
const cleanName = computed(() => name.value.trim().toLowerCase());

const taken = computed(() => !!cleanName.value && props.existing.includes(cleanName.value));

const usable = computed(() =>
  rows.value.map(r => ({ key: r.key.trim(), kind: r.kind })).filter(r => r.key),
);

const addRow = async () => {
  rows.value.push({ key: '', kind: 'text' });
  await nextTick();
  const boxes = keyBox.value;
  boxes?.[boxes.length - 1]?.focus();
};

const submit = () => {
  if (!cleanName.value || taken.value) return;
  emit('create', cleanName.value, usable.value);
};
</script>

<template>
  <div class="w-full max-w-2xl mx-auto">
    <label class="block text-[11px] uppercase tracking-wider text-gray-400 mb-1.5">
      {{ t('things.kind_name') }}
    </label>
    <input
      v-model="name"
      type="text"
      spellcheck="false"
      autofocus
      :placeholder="t('things.kind_name_hint')"
      @keydown.enter.prevent="submit"
      class="w-full px-3 py-2 rounded-lg font-mono text-sm outline-none
             bg-gray-50 dark:bg-white/5 border text-[#1c1c1e] dark:text-[#f4f4f5]
             placeholder-gray-300"
      :class="taken
        ? 'border-amber-400 dark:border-amber-500/50'
        : 'border-gray-200 dark:border-gray-700'"
    />
    <!--
      A name in use is not refused. Making a second schema for one kind is the
      failure this page could otherwise introduce, so it points at the one that
      exists instead.
    -->
    <p v-if="taken" class="mt-1.5 text-xs text-amber-700 dark:text-amber-400">
      {{ t('things.kind_name_taken', { name: cleanName }) }}
    </p>

    <label class="block text-[11px] uppercase tracking-wider text-gray-400 mt-7 mb-1.5">
      {{ t('things.kind_fields') }}
    </label>
    <div v-for="(row, index) in rows" :key="index" class="flex items-center gap-2 mb-1.5">
      <input
        ref="keyBox"
        v-model="row.key"
        type="text"
        spellcheck="false"
        :placeholder="t('things.field_name')"
        @keydown.enter.prevent="addRow"
        class="flex-1 min-w-0 px-3 py-2 rounded-lg font-mono text-xs outline-none
               bg-gray-50 dark:bg-white/5 border border-gray-200 dark:border-gray-700
               text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-300"
      />
      <FieldKindPicker v-model="row.kind" />
      <button
        type="button"
        @click="rows.splice(index, 1)"
        class="p-1.5 rounded text-gray-300 hover:text-red-500 cursor-pointer flex-none"
        :aria-label="t('things.remove_field')"
      >
        <X class="w-3.5 h-3.5" />
      </button>
    </div>

    <button
      type="button"
      @click="addRow"
      class="flex items-center gap-1.5 mt-1 px-2 py-1 text-xs text-gray-400
             hover:text-gray-600 dark:hover:text-gray-300 cursor-pointer"
    >
      <Plus class="w-3 h-3" /> {{ t('things.add_field') }}
    </button>

    <div class="flex items-center gap-2 mt-8 pt-5 border-t border-gray-100 dark:border-[#232326]">
      <button
        type="button"
        :disabled="!cleanName || taken"
        @click="submit"
        class="px-3.5 py-2 rounded-lg text-xs font-medium text-white bg-blue-600
               hover:bg-blue-700 disabled:opacity-40 disabled:cursor-not-allowed cursor-pointer"
      >
        {{ t('things.new_kind_save') }}
      </button>
      <button
        type="button"
        @click="emit('cancel')"
        class="px-3 py-2 rounded-lg text-xs text-gray-600 dark:text-gray-300
               hover:bg-gray-100 dark:hover:bg-white/5 cursor-pointer"
      >
        {{ t('things.cancel') }}
      </button>
      <p class="ml-auto text-[11px] text-gray-400 max-w-xs text-right leading-relaxed">
        {{ t('things.new_kind_note') }}
      </p>
    </div>
  </div>
</template>
