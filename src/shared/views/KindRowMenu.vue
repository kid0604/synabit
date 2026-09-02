<script setup lang="ts">
/**
 * What a kind can be asked to do, from the page that lists them.
 *
 * Renaming lived on the kind's own page and deleting lived here, so managing
 * kinds meant knowing which of two screens held which verb. They belong
 * together, and a menu is how this app has put two verbs on a row everywhere
 * else — a second bare icon beside the first is the thing that had to be
 * explained the last time it was tried.
 */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { ArrowRight, Trash2 } from 'lucide-vue-next';

const props = defineProps<{
  nodeType: string;
  /** Where the button that opened this sits, in screen coordinates. */
  at: { x: number; y: number };
}>();

const emit = defineEmits<{ rename: []; remove: []; close: [] }>();

const { t } = useI18n();

const WIDTH = 192;
const HEIGHT = 2 * 33 + 9 + 8;

/** Worked out rather than measured: measuring means drawing it twice. */
const position = computed(() => {
  const below = props.at.y + 4;
  const fits = below + HEIGHT <= window.innerHeight - 8;
  return {
    left: `${Math.max(8, Math.min(props.at.x - WIDTH, window.innerWidth - WIDTH - 8))}px`,
    top: fits ? `${below}px` : `${Math.max(8, props.at.y - HEIGHT)}px`,
  };
});
</script>

<template>
  <Teleport to="body">
    <div class="fixed inset-0 z-[80]" @click="emit('close')" />
    <div
      :style="position"
      class="fixed w-48 z-[81] py-1 rounded-lg overflow-hidden
             bg-white dark:bg-[#2c2c2c] shadow-lg border border-gray-200 dark:border-gray-700"
    >
      <button
        type="button"
        @click="emit('rename')"
        class="w-full flex items-center gap-2 px-3 py-2 text-xs text-left cursor-pointer
               hover:bg-gray-100 dark:hover:bg-gray-600"
      >
        <ArrowRight class="w-3.5 h-3.5 text-gray-400" />
        {{ t('things.rename_kind_short') }}
      </button>

      <div class="my-1 border-t border-gray-100 dark:border-gray-700" />

      <button
        type="button"
        @click="emit('remove')"
        :title="t('things.delete_kind_hint', { type: nodeType })"
        class="w-full flex items-center gap-2 px-3 py-2 text-xs text-left cursor-pointer
               text-red-600 dark:text-red-400
               hover:bg-red-50 dark:hover:bg-red-900/30"
      >
        <Trash2 class="w-3.5 h-3.5" />
        {{ t('things.remove_kind_short') }}
      </button>
    </div>
  </Teleport>
</template>
