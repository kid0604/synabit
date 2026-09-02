<script setup lang="ts">
/**
 * What a shape row can be asked to do.
 *
 * These were four words printed on every row — up, down, Remove, Rename,
 * Delete — beside a count, a kind and a warning. Eleven things on one line at
 * six hundred pixels, and no two rows aligned because the warning only appears
 * on some of them.
 *
 * The severities are still separated, because the words still sound alike and
 * still do different things: removing takes a field out of the shape and
 * touches no file, while deleting ends it on every node of the kind. The
 * divider and the colour are carrying that difference now instead of an amber
 * pill halfway along a row.
 */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { ChevronUp, ChevronDown, ArrowRight, X, Trash2 } from 'lucide-vue-next';

const props = defineProps<{
  fieldKey: string;
  /** Where the button that opened this sits, in screen coordinates. */
  at: { x: number; y: number };
  /** Hidden at the ends of the list, where the move would do nothing. */
  canMoveUp: boolean;
  canMoveDown: boolean;
}>();

const emit = defineEmits<{
  move: [by: number];
  drop: [];
  rename: [];
  erase: [];
  close: [];
}>();

const { t } = useI18n();

const WIDTH = 208;
const ITEM = 33;
const PADDING = 8;

/**
 * Worked out rather than measured, for the reason the row menu is: measuring
 * means drawing once in the wrong place and moving, which the eye catches.
 */
const height = computed(
  () => (2 + (props.canMoveUp ? 1 : 0) + (props.canMoveDown ? 1 : 0) + 1) * ITEM + 9 + PADDING,
);

const position = computed(() => {
  const below = props.at.y + 4;
  const fits = below + height.value <= window.innerHeight - PADDING;
  return {
    left: `${Math.max(PADDING, Math.min(props.at.x - WIDTH, window.innerWidth - WIDTH - PADDING))}px`,
    top: fits ? `${below}px` : `${Math.max(PADDING, props.at.y - height.value)}px`,
  };
});
</script>

<template>
  <Teleport to="body">
    <div class="fixed inset-0 z-[80]" @click="emit('close')" />
    <div
      :style="position"
      class="fixed w-52 z-[81] py-1 rounded-lg overflow-hidden
             bg-white dark:bg-[#2c2c2c] shadow-lg border border-gray-200 dark:border-gray-700"
    >
      <button
        v-if="canMoveUp"
        type="button"
        @click="emit('move', -1)"
        class="w-full flex items-center gap-2 px-3 py-2 text-xs text-left cursor-pointer
               hover:bg-gray-100 dark:hover:bg-gray-600"
      >
        <ChevronUp class="w-3.5 h-3.5 text-gray-400" />
        {{ t('things.move_up') }}
      </button>
      <button
        v-if="canMoveDown"
        type="button"
        @click="emit('move', 1)"
        class="w-full flex items-center gap-2 px-3 py-2 text-xs text-left cursor-pointer
               hover:bg-gray-100 dark:hover:bg-gray-600"
      >
        <ChevronDown class="w-3.5 h-3.5 text-gray-400" />
        {{ t('things.move_down') }}
      </button>

      <button
        type="button"
        @click="emit('rename')"
        :title="t('things.rename_field_hint')"
        class="w-full flex items-center gap-2 px-3 py-2 text-xs text-left cursor-pointer
               text-amber-700 dark:text-amber-400
               hover:bg-amber-50 dark:hover:bg-amber-900/30"
      >
        <ArrowRight class="w-3.5 h-3.5" />
        {{ t('things.rename_short') }}
      </button>

      <div class="my-1 border-t border-gray-100 dark:border-gray-700" />

      <!-- Takes nothing off any file, which is why it is not red. -->
      <button
        type="button"
        @click="emit('drop')"
        :title="t('things.drop_from_shape')"
        class="w-full flex items-center gap-2 px-3 py-2 text-xs text-left cursor-pointer
               hover:bg-gray-100 dark:hover:bg-gray-600"
      >
        <X class="w-3.5 h-3.5 text-gray-400" />
        {{ t('things.drop_short') }}
      </button>
      <button
        type="button"
        @click="emit('erase')"
        :title="t('things.delete_field_hint')"
        class="w-full flex items-center gap-2 px-3 py-2 text-xs text-left cursor-pointer
               text-red-600 dark:text-red-400
               hover:bg-red-50 dark:hover:bg-red-900/30"
      >
        <Trash2 class="w-3.5 h-3.5" />
        {{ t('things.erase_short') }}
      </button>
    </div>
  </Teleport>
</template>
