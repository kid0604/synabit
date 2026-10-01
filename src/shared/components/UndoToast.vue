<script setup lang="ts">
/**
 * "Deleted <thing> — Undo", for the few seconds before the delete is real.
 *
 * The bar across the bottom is the point as much as the button: it says how
 * much time is left without anyone having to count, and it is why the delete
 * behind it needs no confirmation dialog.
 *
 * It renders into the shell's toast stack (`#app-toasts`), so two toasts sit
 * one above the other instead of on top of each other, and on a phone the
 * stack clears the bottom tab bar. Outside the shell — a test, the quick-entry
 * window — it falls back to its own fixed spot.
 *
 * It names what went and, when it went to the trash, says so (`hint`): the
 * toast is the quick way back and the trash the sure one, and people who
 * missed the toast used to think a delete had left no way back at all.
 *
 * The countdown pauses while the pointer or keyboard focus is on the toast
 * (WCAG 2.2.1): the caller's `useUndoableAction` does the waiting, so this
 * only says when to hold it, and holds the bar with it.
 */
import { ref } from 'vue';
import { Trash2, Undo2 } from 'lucide-vue-next';

defineProps<{
  /** Whether anything is waiting. `key` restarts the bar between deletions. */
  show: boolean;
  /** Restarts the countdown when one deletion follows another with no gap. */
  restartKey?: string;
  /** The whole sentence, e.g. "Deleted Buy milk". */
  message: string;
  undoLabel: string;
  /** Seconds the undo stays available, so the bar and the timer agree. */
  seconds: number;
  /** A second, quieter line — usually `common.in_trash_hint`. */
  hint?: string;
}>();

const emit = defineEmits<{
  (e: 'undo'): void;
  (e: 'pause'): void;
  (e: 'resume'): void;
}>();

const inStack = typeof document !== 'undefined' && !!document.getElementById('app-toasts');
const target = inStack ? '#app-toasts' : 'body';
const paused = ref(false);

function hold(on: boolean) {
  if (paused.value === on) return;
  paused.value = on;
  if (on) emit('pause');
  else emit('resume');
}
</script>

<template>
  <Teleport :to="target">
    <Transition name="toast">
      <!--
        The `v-if` belongs in here, not on the caller: a toggle outside the
        transition means the element is simply added and removed and the
        animation never plays.
      -->
      <div
        v-if="show"
        :key="restartKey || message"
        class="undo-toast"
        :class="inStack ? 'in-stack' : 'standalone'"
        role="status"
        @mouseenter="hold(true)"
        @mouseleave="hold(false)"
        @focusin="hold(true)"
        @focusout="hold(false)"
      >
        <span class="shrink-0 w-8 h-8 rounded-full bg-accent/10 dark:bg-accent-dark/15 text-accent dark:text-accent-dark flex items-center justify-center" aria-hidden="true">
          <Trash2 class="w-4 h-4" />
        </span>
        <span class="min-w-0 flex-1">
          <span class="block text-sm font-medium text-text dark:text-text-dark truncate">{{ message }}</span>
          <span v-if="hint" class="block text-xs text-gray-500 dark:text-gray-400 truncate">{{ hint }}</span>
        </span>
        <button
          type="button"
          @click="emit('undo')"
          class="ml-auto shrink-0 flex items-center gap-1.5 h-9 px-3 rounded-lg text-sm font-medium text-accent dark:text-accent-dark hover:bg-accent/10 transition-colors cursor-pointer focus-visible:outline-2 focus-visible:outline-accent"
        >
          <Undo2 class="w-4 h-4" aria-hidden="true" />
          {{ undoLabel }}
        </button>
        <div
          class="toast-timer-bar"
          :style="{ animationDuration: seconds + 's', animationPlayState: paused ? 'paused' : 'running' }"
        />
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
@reference "../../style.css";
.undo-toast {
  @apply pointer-events-auto relative flex items-center gap-3 pl-3 pr-2 py-2.5 rounded-xl shadow-2xl w-full max-w-[440px] overflow-hidden;
  background: rgba(255, 255, 255, 0.97);
  backdrop-filter: blur(16px);
  border: 1px solid rgba(0, 0, 0, 0.06);
}

.undo-toast.standalone {
  @apply fixed bottom-5 left-1/2 -translate-x-1/2 z-[300] min-w-[300px];
}

:is(.dark) .undo-toast {
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
  transform: translateY(8px);
}
.standalone.toast-enter-from, .standalone.toast-leave-to {
  transform: translate(-50%, 8px);
}

@media (prefers-reduced-motion: reduce) {
  .toast-timer-bar { animation: none; opacity: 0.4; }
  .toast-enter-active, .toast-leave-active { transition: none; }
}
</style>
