<script setup lang="ts">
/**
 * A delete that asks, without opening anything.
 *
 * Two presses of the same button, the second on a control that has visibly
 * changed and says what it will do. It interrupts far less than a modal — no
 * focus moved, no keyboard trap, no dialog covering the task you are about to
 * delete — while still being an explicit second act rather than one stray
 * click. Anything typed or scrolled stays where it was.
 *
 * The armed state times out. A button left saying "Delete?" is a trap for the
 * next click that lands anywhere near it, so it goes back to a bin on its own,
 * and on the first mouse-out.
 */
import { ref, onUnmounted } from 'vue';
import { Trash2 } from 'lucide-vue-next';

const props = withDefaults(defineProps<{
  /** 'inline' arms first; 'dialog' and 'undo' both fire on the first press. */
  mode?: 'dialog' | 'inline' | 'undo';
  /** Tighter styling for the board and matrix cards. */
  compact?: boolean;
}>(), { mode: 'inline', compact: false });

const emit = defineEmits<{ (e: 'confirm'): void }>();

/** Long enough to move the mouse a few pixels and press again, no longer. */
const ARMED_MS = 3000;

const armed = ref(false);
let timer: ReturnType<typeof setTimeout> | undefined;

const disarm = () => {
  armed.value = false;
  clearTimeout(timer);
};

const onClick = () => {
  if (props.mode !== 'inline') {
    emit('confirm');
    return;
  }
  if (armed.value) {
    disarm();
    emit('confirm');
    return;
  }
  armed.value = true;
  clearTimeout(timer);
  timer = setTimeout(disarm, ARMED_MS);
};

onUnmounted(disarm);
</script>

<template>
  <button
    @click.stop="onClick"
    @mouseleave="disarm"
    class="rounded-md transition-colors cursor-pointer flex items-center gap-1"
    :class="[
      compact ? 'p-0.5' : 'p-1.5',
      armed
        ? 'bg-red-500 text-white px-2 hover:bg-red-600'
        : 'text-gray-400 hover:text-red-500 hover:bg-gray-100 dark:hover:bg-[#2c2c2c]',
    ]"
    :aria-label="armed ? $t('task.delete_confirm') : $t('task.a11y_delete_task')"
  >
    <Trash2 :class="compact ? 'w-3.5 h-3.5' : 'w-4 h-4'" />
    <span v-if="armed" class="text-[11px] font-semibold whitespace-nowrap">{{ $t('task.delete_confirm') }}</span>
  </button>
</template>
