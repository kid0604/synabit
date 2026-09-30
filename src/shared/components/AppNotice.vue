<script setup lang="ts">
/**
 * The shell's messages (`showAppNotice`), rendered in the toast stack.
 *
 * Errors are announced as alerts and stay until closed, so a slow reader is
 * never told something went wrong by a message that has already left.
 */
import { AlertCircle, Info, X } from 'lucide-vue-next';
import { appNotices, dismissAppNotice } from '../../composables/useAppNotice';
import { i18n } from '../../i18n';

const t = i18n.global.t;
</script>

<template>
  <TransitionGroup name="app-notice">
    <div
      v-for="n in appNotices"
      :key="n.id"
      :role="n.kind === 'error' ? 'alert' : 'status'"
      class="app-notice pointer-events-auto flex items-start gap-2.5 pl-3.5 pr-2 py-2.5 rounded-xl shadow-2xl border w-full max-w-[420px]"
      :class="n.kind === 'error'
        ? 'bg-surface dark:bg-surface-dark border-danger/40 text-text dark:text-text-dark'
        : 'bg-surface dark:bg-surface-dark border-border dark:border-border-dark text-text dark:text-text-dark'"
    >
      <AlertCircle v-if="n.kind === 'error'" class="w-4 h-4 mt-0.5 shrink-0 text-danger" aria-hidden="true" />
      <Info v-else class="w-4 h-4 mt-0.5 shrink-0 text-muted dark:text-muted-dark" aria-hidden="true" />
      <div class="flex-1 min-w-0">
        <p class="text-sm">{{ n.text }}</p>
        <div v-if="n.actions?.length" class="flex flex-wrap gap-2 mt-2">
          <button
            v-for="(a, i) in n.actions"
            :key="i"
            type="button"
            :class="i === 0 ? 'btn-primary' : 'btn-secondary'"
            @click="dismissAppNotice(n.id); a.run()"
          >
            {{ a.label }}
          </button>
        </div>
      </div>
      <button type="button" class="btn-icon shrink-0 -my-1" :aria-label="t('common.close')" :title="t('common.close')" @click="dismissAppNotice(n.id)">
        <X class="w-4 h-4" aria-hidden="true" />
      </button>
    </div>
  </TransitionGroup>
</template>

<style scoped>
.app-notice-enter-active, .app-notice-leave-active { transition: opacity 0.18s ease, transform 0.18s ease; }
.app-notice-enter-from, .app-notice-leave-to { opacity: 0; transform: translateY(8px); }
@media (prefers-reduced-motion: reduce) {
  .app-notice-enter-active, .app-notice-leave-active { transition: none; }
}
</style>
