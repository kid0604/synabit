<script setup lang="ts">
/**
 * "3 feeds could not be refreshed" — and, if asked, which ones.
 *
 * `feed_refresh` has always returned the list of feeds that refused; the app
 * used to drop it, so a feed that had been failing for weeks was
 * indistinguishable from one that simply had nothing new. This says so
 * without interrupting: it sits in the corner and waits to be dismissed,
 * because a broken feed is not urgent, it is just worth knowing about.
 */
import { ref, computed, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { AlertTriangle, ChevronDown, X } from 'lucide-vue-next';

const props = defineProps<{ errors: string[] }>();
const emit = defineEmits<{ dismiss: [] }>();

const { t } = useI18n();
const expanded = ref(false);

const show = computed(() => props.errors.length > 0);

// A fresh batch of failures starts collapsed again, otherwise the panel is
// left standing open showing the previous run's list.
watch(() => props.errors, () => { expanded.value = false; });
</script>

<template>
  <Teleport to="body">
    <Transition name="feed-toast">
      <div v-if="show" class="feed-error-toast">
        <div class="flex items-center gap-2.5 min-w-0">
          <AlertTriangle class="w-4 h-4 text-amber-500 shrink-0" />
          <span class="text-[13px] text-text dark:text-text-dark truncate min-w-0">
            {{ t('feeds.refresh_errors', { count: errors.length }) }}
          </span>
          <button
            @click="expanded = !expanded"
            class="ml-auto shrink-0 flex items-center gap-1 px-2 py-1 rounded-md text-[13px] font-medium text-gray-600 dark:text-gray-300 hover:bg-black/5 dark:hover:bg-white/10 transition-colors"
          >
            {{ t('feeds.error_details') }}
            <ChevronDown class="w-3.5 h-3.5 transition-transform duration-200" :class="{ 'rotate-180': expanded }" />
          </button>
          <button
            @click="emit('dismiss')"
            class="shrink-0 p-1 rounded-md text-gray-400 hover:text-gray-600 dark:hover:text-gray-200 hover:bg-black/5 dark:hover:bg-white/10 transition-colors"
            :aria-label="t('feeds.dismiss')"
          >
            <X class="w-3.5 h-3.5" />
          </button>
        </div>

        <ul v-if="expanded" class="mt-2 pt-2 border-t border-black/5 dark:border-white/10 space-y-1 max-h-40 overflow-y-auto">
          <li
            v-for="(error, i) in errors"
            :key="i"
            class="text-[12px] leading-snug text-gray-500 dark:text-gray-400 break-words"
          >
            {{ error }}
          </li>
        </ul>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
@reference "../../../style.css";
.feed-error-toast {
  @apply fixed bottom-5 right-5 z-[300] flex flex-col px-3.5 py-2.5 rounded-xl shadow-2xl max-w-[420px] min-w-[300px];
  background: rgba(255, 255, 255, 0.97);
  backdrop-filter: blur(16px);
  border: 1px solid rgba(0, 0, 0, 0.06);
}

:is(.dark) .feed-error-toast {
  background: rgba(36, 36, 36, 0.97);
  border-color: rgba(255, 255, 255, 0.06);
}

.feed-toast-enter-active, .feed-toast-leave-active {
  transition: opacity 0.18s ease, transform 0.18s ease;
}
.feed-toast-enter-from, .feed-toast-leave-to {
  opacity: 0;
  transform: translateY(8px);
}

@media (prefers-reduced-motion: reduce) {
  .feed-toast-enter-active, .feed-toast-leave-active { transition: none; }
}
</style>
