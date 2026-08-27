<script setup lang="ts">
/**
 * "Deleted <note> — Undo", for the few seconds before the delete is real.
 *
 * The bar across the bottom is the point as much as the button: it says how
 * much time is left without anyone having to count, and it is why there is no
 * confirmation dialog in front of the delete.
 */
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { Trash2, Undo2 } from 'lucide-vue-next';

const { t } = useI18n();

const props = defineProps<{
  /** The notes waiting to go, or null when nothing is pending. */
  pending: { notes: { note: { id: string; title: string } }[] } | null;
  /** Seconds the undo stays available, so the bar and the timer agree. */
  seconds: number;
}>();

/**
 * One note is named; several are counted.
 *
 * Listing thirteen titles in a bar this size means truncating twelve of them,
 * which tells the reader less than the number does.
 */
const label = computed(() => {
  const held = props.pending?.notes ?? [];
  return held.length > 1
    ? { key: 'note.deleted_many_toast', params: { count: held.length } }
    : { key: 'note.deleted_toast', params: { title: held[0]?.note.title || t('note.untitled_note') } };
});

/** Restarts the countdown bar when one deletion follows another with no gap. */
const batchKey = computed(() => (props.pending?.notes ?? []).map((e) => e.note.id).join('|'));

const emit = defineEmits<{ (e: 'undo'): void }>();
</script>

<template>
  <Teleport to="body">
    <Transition name="toast">
      <!--
        The `v-if` belongs in here, not on the caller: a toggle outside the
        transition means the element is simply added and removed and the
        animation never plays. `key` restarts the countdown bar when one
        deletion follows another without a gap.
      -->
      <div v-if="pending" :key="batchKey" class="note-undo-toast">
        <Trash2 class="w-3.5 h-3.5 text-gray-400 shrink-0" />
        <span class="text-[13px] text-[#1c1c1e] dark:text-[#f4f4f5] truncate min-w-0">
          {{ $t(label.key, label.params) }}
        </span>
        <button
          @click="emit('undo')"
          class="ml-auto shrink-0 flex items-center gap-1.5 px-2.5 py-1 rounded-md text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] hover:bg-black/5 dark:hover:bg-white/10 transition-colors"
        >
          <Undo2 class="w-3.5 h-3.5" />
          {{ $t('note.undo') }}
        </button>
        <div class="toast-timer-bar" :style="{ animationDuration: seconds + 's' }" />
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
@reference "../../../style.css";
.note-undo-toast {
  @apply fixed bottom-5 left-1/2 -translate-x-1/2 z-[300] flex items-center gap-2.5 pl-3.5 pr-2 py-2.5 rounded-xl shadow-2xl max-w-[420px] min-w-[300px] overflow-hidden;
  background: rgba(255, 255, 255, 0.97);
  backdrop-filter: blur(16px);
  border: 1px solid rgba(0, 0, 0, 0.06);
}

:is(.dark) .note-undo-toast {
  background: rgba(36, 36, 36, 0.97);
  border-color: rgba(255, 255, 255, 0.06);
}

.toast-timer-bar {
  @apply absolute bottom-0 left-0 h-[2px];
  width: 100%;
  transform-origin: left;
  background: linear-gradient(to right, rgb(156 163 175 / 0.5), rgb(156 163 175 / 0.9));
  animation: undo-countdown linear forwards;
}

@keyframes undo-countdown {
  from { transform: scaleX(1); }
  to { transform: scaleX(0); }
}

.toast-enter-active, .toast-leave-active {
  transition: opacity 0.18s ease, transform 0.18s ease;
}
.toast-enter-from, .toast-leave-to {
  opacity: 0;
  transform: translate(-50%, 8px);
}

@media (prefers-reduced-motion: reduce) {
  .toast-timer-bar { animation: none; opacity: 0.4; }
  .toast-enter-active, .toast-leave-active { transition: none; }
}
</style>
