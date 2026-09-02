<script setup lang="ts">
/**
 * What kind of thing this is, chosen from the kinds that exist.
 *
 * It replaces a `<datalist>`, which was wrong twice over. WKWebView draws that
 * control itself with system chrome and ignores every style the app sets, so
 * on macOS it arrived as a grey popup from another application. And it is a
 * text box first and a list second: the fast path was typing, which meant one
 * slip minted a permanent new type — there is a `type: abc` in the vault with
 * a single untitled member, which is exactly that happening.
 *
 * So the list is the control, counts and all, and inventing a kind is a
 * separate deliberate act at the bottom of it.
 */
import { ref, computed, nextTick } from 'vue';
import { useI18n } from 'vue-i18n';
import { Check, Plus } from 'lucide-vue-next';
import { iconForNodeType } from './nodeTypeIcon';

const props = defineProps<{
  /** The kinds the vault already holds, with how many of each. */
  types: { node_type: string; count: number }[];
  current: string;
  /** Where the chip that opened this sits, in screen coordinates. */
  at: { x: number; y: number };
}>();

const emit = defineEmits<{ pick: [type: string]; close: [] }>();

const { t } = useI18n();

const naming = ref(false);
const draft = ref('');
const nameInput = ref<HTMLInputElement | null>(null);

const startNaming = async () => {
  naming.value = true;
  await nextTick();
  nameInput.value?.focus();
};

/**
 * A new kind, lower-cased and nothing else.
 *
 * Matching an existing kind by name rather than adding a second spelling of
 * it: `Book` and `book` are one kind, and a vault that holds both is a vault
 * whose queries silently miss half of what they mean.
 */
const submitName = () => {
  const name = draft.value.trim().toLowerCase();
  draft.value = '';
  naming.value = false;
  if (name) emit('pick', name);
};

const height = computed(() => Math.min(props.types.length, 8) * 32 + 84);

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
        v-for="entry in types"
        :key="entry.node_type"
        type="button"
        @click="emit('pick', entry.node_type)"
        class="w-full flex items-center gap-2 px-3 py-1.5 text-xs text-left
               hover:bg-gray-100 dark:hover:bg-gray-600 cursor-pointer"
      >
        <component :is="iconForNodeType(entry.node_type)" class="w-3.5 h-3.5 text-gray-400 flex-none" />
        <span class="flex-1 min-w-0 truncate text-[#1c1c1e] dark:text-[#f4f4f5]">
          {{ entry.node_type }}
        </span>
        <span class="text-gray-400 tabular-nums">{{ entry.count }}</span>
        <Check v-if="entry.node_type === current" class="w-3 h-3 text-blue-500 flex-none" />
      </button>

      <div class="my-1 border-t border-gray-100 dark:border-gray-700" />

      <!--
        Deliberate, and visibly a different act from picking one. Falling into
        this by mistyping is what the old text box allowed.
      -->
      <div v-if="naming" class="px-2 py-1">
        <input
          ref="nameInput"
          v-model="draft"
          spellcheck="false"
          :placeholder="t('things.new_type_name')"
          @keydown.enter.prevent="submitName"
          @keydown.esc="naming = false; draft = ''"
          @blur="submitName"
          class="w-full px-2 py-1 rounded text-xs bg-gray-50 dark:bg-white/5
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
        {{ t('things.new_type') }}
      </button>
    </div>
  </Teleport>
</template>
