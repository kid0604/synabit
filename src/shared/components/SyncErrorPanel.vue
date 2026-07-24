<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { AlertTriangle, X, ChevronDown, HardDrive } from 'lucide-vue-next';

// ─── Types ────────────────────────────────────────────────
interface QuotaInfo {
  used_bytes: number;
  total_bytes: number;
  message: string;
}

// ─── State ────────────────────────────────────────────────
const errors = ref<string[]>([]);
const quotaWarning = ref<QuotaInfo | null>(null);
const showAll = ref(false);
const visible = ref(false);

let unlistenProgress: UnlistenFn | null = null;
let unlistenQuota: UnlistenFn | null = null;

// ─── Computed ─────────────────────────────────────────────
const MAX_VISIBLE = 5;

const visibleErrors = computed(() => {
  if (showAll.value) return errors.value;
  return errors.value.slice(0, MAX_VISIBLE);
});

const hasMore = computed(() => errors.value.length > MAX_VISIBLE);
const remainingCount = computed(() => errors.value.length - MAX_VISIBLE);

const hasContent = computed(() => errors.value.length > 0 || quotaWarning.value !== null);

const quotaPercentage = computed(() => {
  if (!quotaWarning.value || quotaWarning.value.total_bytes <= 0) return 0;
  return Math.round((quotaWarning.value.used_bytes / quotaWarning.value.total_bytes) * 100);
});

const formatBytes = (bytes: number): string => {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1073741824) return `${(bytes / 1048576).toFixed(1)} MB`;
  return `${(bytes / 1073741824).toFixed(2)} GB`;
};

// ─── Actions ──────────────────────────────────────────────
const dismiss = () => {
  visible.value = false;
  errors.value = [];
  quotaWarning.value = null;
  showAll.value = false;
};

// ─── Lifecycle ────────────────────────────────────────────
onMounted(async () => {
  // Listen for sync-progress errors
  unlistenProgress = await listen<{ phase: string; errors: string[] }>('sync-progress', (event) => {
    if (event.payload.errors && event.payload.errors.length > 0) {
      errors.value = event.payload.errors;
      visible.value = true;
    }
  });

  // Listen for quota exceeded
  unlistenQuota = await listen<QuotaInfo>('sync-quota-exceeded', (event) => {
    quotaWarning.value = event.payload;
    visible.value = true;
  });
});

onUnmounted(() => {
  if (unlistenProgress) unlistenProgress();
  if (unlistenQuota) unlistenQuota();
});
</script>

<template>
  <Transition name="error-panel">
    <div v-if="visible && hasContent" class="sync-error-panel">
      <!-- Header -->
      <div class="flex items-center justify-between px-4 py-2.5 border-b border-red-200 dark:border-red-900/30">
        <div class="flex items-center gap-2">
          <AlertTriangle class="w-4 h-4 text-red-500 shrink-0" />
          <span class="text-[13px] font-semibold text-red-700 dark:text-red-400">
            Sync {{ errors.length > 0 ? `Error${errors.length > 1 ? 's' : ''} (${errors.length})` : 'Warning' }}
          </span>
        </div>
        <button @click="dismiss" class="p-1 rounded-md hover:bg-red-100 dark:hover:bg-red-900/30 text-red-400 dark:text-red-500 transition-colors cursor-pointer" aria-label="Dismiss">
          <X class="w-3.5 h-3.5" />
        </button>
      </div>

      <!-- Quota Warning -->
      <div v-if="quotaWarning" class="px-4 py-3 border-b border-amber-200 dark:border-amber-900/30 bg-amber-50/50 dark:bg-amber-950/20">
        <div class="flex items-start gap-2.5">
          <HardDrive class="w-4 h-4 text-amber-500 shrink-0 mt-0.5" />
          <div class="flex-1 min-w-0">
            <p class="text-[12px] font-semibold text-amber-700 dark:text-amber-400 mb-1">
              Storage Quota Exceeded
            </p>
            <p class="text-[11px] text-amber-600 dark:text-amber-500 mb-2">
              {{ quotaWarning.message }}
            </p>
            <div class="flex items-center gap-2">
              <div class="flex-1 h-[5px] rounded-full bg-amber-200 dark:bg-amber-900/40 overflow-hidden">
                <div class="h-full rounded-full bg-amber-500 transition-all" :style="{ width: `${Math.min(quotaPercentage, 100)}%` }" />
              </div>
              <span class="text-[10px] text-amber-500 dark:text-amber-400 tabular-nums shrink-0">
                {{ formatBytes(quotaWarning.used_bytes) }} / {{ formatBytes(quotaWarning.total_bytes) }}
              </span>
            </div>
          </div>
        </div>
      </div>

      <!-- Error List -->
      <div v-if="errors.length > 0" class="px-4 py-2.5 space-y-1.5">
        <p v-for="(err, i) in visibleErrors" :key="i" class="text-[11px] text-red-600 dark:text-red-400 font-mono leading-relaxed flex items-start gap-2">
          <span class="text-red-300 dark:text-red-700 shrink-0 select-none">•</span>
          <span class="truncate" :title="err">{{ err }}</span>
        </p>

        <!-- Show more / Show less -->
        <button
          v-if="hasMore"
          @click="showAll = !showAll"
          class="flex items-center gap-1 text-[11px] text-red-400 dark:text-red-500 hover:text-red-600 dark:hover:text-red-400 font-medium transition-colors cursor-pointer pt-1"
        >
          <ChevronDown class="w-3 h-3 transition-transform" :class="{ 'rotate-180': showAll }" />
          {{ showAll ? 'Show less' : `Show ${remainingCount} more` }}
        </button>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
@reference "../../style.css";
.sync-error-panel {
  @apply w-full overflow-hidden rounded-b-xl;
  background: rgba(254, 242, 242, 0.97);
  backdrop-filter: blur(12px);
  border-bottom: 1px solid rgba(239, 68, 68, 0.12);
}

:is(.dark) .sync-error-panel {
  background: rgba(40, 18, 18, 0.97);
  border-bottom-color: rgba(239, 68, 68, 0.15);
}

/* ─── Animations ────────────────────────────────────────── */
.error-panel-enter-active {
  transition: max-height 0.3s cubic-bezier(0.16, 1, 0.3, 1), opacity 0.3s ease;
}

.error-panel-leave-active {
  transition: max-height 0.2s ease, opacity 0.2s ease;
}

.error-panel-enter-from,
.error-panel-leave-to {
  max-height: 0;
  opacity: 0;
  overflow: hidden;
}
</style>
