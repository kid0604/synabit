<script setup lang="ts">
/**
 * Which field to add, chosen from the ones this kind already uses.
 *
 * This is where drift is stopped, and stopping it is a matter of what is
 * cheapest rather than what is detected. A blank box makes typing the fastest
 * move, and what a person types is whatever is in their head at that moment —
 * which is how this vault ended up with `colour` on two animals and `màu` on a
 * third. The same idea, twice, because nothing showed the first one.
 *
 * Matching them afterwards is not on offer: `colour` and `màu` are not similar
 * strings, they are translations, and anything clever enough to pair them
 * would be too clever to trust with somebody's data. So the existing key is
 * simply put on screen, one click away, and the blank box moved behind a
 * deliberate second step.
 *
 * The count beside each key is doing real work — it says which of these the
 * kind is actually built on, without anybody having curated a list.
 */
import { ref, computed, nextTick } from 'vue';
import { useI18n } from 'vue-i18n';
import { Plus } from 'lucide-vue-next';

const props = defineProps<{
  /** Every key this kind carries, with how many nodes carry it. */
  known: { key: string; count: number }[];
  /** Keys already on this node, which there is no sense offering again. */
  taken: string[];
  at: { x: number; y: number };
}>();

const emit = defineEmits<{ pick: [key: string]; close: [] }>();

const { t } = useI18n();

const offered = computed(() => props.known.filter(f => !props.taken.includes(f.key)));

const naming = ref(false);
const draft = ref('');
const nameInput = ref<HTMLInputElement | null>(null);

const startNaming = async () => {
  naming.value = true;
  await nextTick();
  nameInput.value?.focus();
};

const submitName = () => {
  const name = draft.value.trim();
  draft.value = '';
  naming.value = false;
  if (name) emit('pick', name);
};

const height = computed(() => Math.min(offered.value.length, 8) * 30 + 76);

const position = computed(() => ({
  left: `${Math.max(8, Math.min(props.at.x, window.innerWidth - 232))}px`,
  top: props.at.y + height.value <= window.innerHeight - 8
    ? `${props.at.y + 4}px`
    : `${Math.max(8, props.at.y - height.value)}px`,
}));
</script>

<template>
  <Teleport to="body">
    <div class="fixed inset-0 z-[80]" @click="emit('close')" />
    <div
      :style="position"
      class="fixed w-56 z-[81] py-1 rounded-lg max-h-[60vh] overflow-y-auto
             bg-white dark:bg-[#2c2c2c] shadow-lg border border-gray-200 dark:border-gray-700"
    >
      <button
        v-for="field in offered"
        :key="field.key"
        type="button"
        @click="emit('pick', field.key)"
        class="w-full flex items-center gap-2 px-3 py-1.5 text-left
               hover:bg-gray-100 dark:hover:bg-gray-600 cursor-pointer"
      >
        <span class="flex-1 min-w-0 truncate font-mono text-xs text-[#1c1c1e] dark:text-[#f4f4f5]">
          {{ field.key }}
        </span>
        <span class="text-[11px] text-gray-400 tabular-nums">{{ field.count }}</span>
      </button>

      <p
        v-if="!offered.length"
        class="px-3 py-2 text-xs text-gray-400 dark:text-gray-500"
      >
        {{ t('things.no_known_fields') }}
      </p>

      <div v-if="offered.length" class="my-1 border-t border-gray-100 dark:border-gray-700" />

      <div v-if="naming" class="px-2 py-1">
        <input
          ref="nameInput"
          v-model="draft"
          spellcheck="false"
          :placeholder="t('things.field_name')"
          @keydown.enter.prevent="submitName"
          @keydown.esc="naming = false; draft = ''"
          @blur="submitName"
          class="w-full px-2 py-1 rounded text-xs font-mono bg-gray-50 dark:bg-white/5
                 border border-gray-200 dark:border-gray-700 outline-none
                 text-[#1c1c1e] dark:text-[#f4f4f5]"
        />
      </div>
      <button
        v-else
        type="button"
        @click="startNaming"
        class="w-full flex items-center gap-2 px-3 py-1.5 text-xs text-left text-gray-500
               dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-600 cursor-pointer"
      >
        <Plus class="w-3.5 h-3.5 flex-none" />
        {{ t('things.new_field') }}
      </button>
    </div>
  </Teleport>
</template>
