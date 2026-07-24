<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { Check, X, GitMerge } from 'lucide-vue-next';

// ─── Types ────────────────────────────────────────────────
interface SyncConflictEvent {
  merged_files: string[];
  total: number;
}

// ─── State ────────────────────────────────────────────────
const visible = ref(false);
const mergedFiles = ref<string[]>([]);
const totalMerged = ref(0);

let dismissTimer: number | null = null;
let unlistenConflict: UnlistenFn | null = null;

// ─── Computed ─────────────────────────────────────────────
const MAX_SHOWN = 3;

const displayFiles = computed(() => mergedFiles.value.slice(0, MAX_SHOWN));
const remainingCount = computed(() => totalMerged.value - MAX_SHOWN);
const hasMore = computed(() => totalMerged.value > MAX_SHOWN);

const truncateName = (name: string): string => {
  // Show just filename from path
  const parts = name.split('/');
  const filename = parts[parts.length - 1] || name;
  if (filename.length <= 28) return filename;
  return filename.slice(0, 14) + '…' + filename.slice(-13);
};

// ─── Actions ──────────────────────────────────────────────
const dismiss = () => {
  visible.value = false;
  if (dismissTimer !== null) {
    window.clearTimeout(dismissTimer);
    dismissTimer = null;
  }
};

const scheduleAutoDismiss = () => {
  if (dismissTimer !== null) window.clearTimeout(dismissTimer);
  dismissTimer = window.setTimeout(() => {
    visible.value = false;
  }, 5000);
};

// ─── Lifecycle ────────────────────────────────────────────
onMounted(async () => {
  unlistenConflict = await listen<SyncConflictEvent>('sync-conflict', (event) => {
    const payload = event.payload;
    mergedFiles.value = payload.merged_files || [];
    totalMerged.value = payload.total || mergedFiles.value.length;
    visible.value = true;
    scheduleAutoDismiss();
  });
});

onUnmounted(() => {
  if (unlistenConflict) unlistenConflict();
  if (dismissTimer !== null) window.clearTimeout(dismissTimer);
});
</script>

<template>
  <Teleport to="body">
    <Transition name="toast">
      <div v-if="visible" class="sync-conflict-toast">
        <!-- Header -->
        <div class="flex items-center gap-2.5 mb-1.5">
          <div class="w-6 h-6 rounded-full bg-emerald-100 dark:bg-emerald-900/40 flex items-center justify-center shrink-0">
            <Check class="w-3.5 h-3.5 text-emerald-600 dark:text-emerald-400" />
          </div>
          <span class="text-[13px] font-semibold text-[#1c1c1e] dark:text-[#f4f4f5]">
            {{ totalMerged }} file{{ totalMerged !== 1 ? 's' : '' }} merged automatically
          </span>
          <button @click="dismiss" class="ml-auto p-0.5 rounded hover:bg-black/5 dark:hover:bg-white/5 transition-colors cursor-pointer text-gray-400 dark:text-gray-500" aria-label="Dismiss">
            <X class="w-3.5 h-3.5" />
          </button>
        </div>

        <!-- File list -->
        <div class="pl-[34px] space-y-0.5">
          <div v-for="(file, i) in displayFiles" :key="i" class="flex items-center gap-1.5 text-[11px] text-gray-500 dark:text-gray-400">
            <GitMerge class="w-3 h-3 text-emerald-400 dark:text-emerald-600 shrink-0" />
            <span class="truncate font-mono" :title="file">{{ truncateName(file) }}</span>
          </div>
          <p v-if="hasMore" class="text-[11px] text-gray-400 dark:text-gray-500 italic pl-[18px]">
            and {{ remainingCount }} more…
          </p>
        </div>

        <!-- Auto-dismiss progress bar -->
        <div class="toast-timer-bar" />
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
@reference "../../style.css";
.sync-conflict-toast {
  @apply fixed bottom-5 right-5 z-[300] p-3.5 rounded-xl shadow-2xl max-w-[320px] min-w-[260px] overflow-hidden;
  background: rgba(255, 255, 255, 0.97);
  backdrop-filter: blur(16px);
  border: 1px solid rgba(0, 0, 0, 0.06);
}

:is(.dark) .sync-conflict-toast {
  background: rgba(36, 36, 36, 0.97);
  border-color: rgba(255, 255, 255, 0.06);
}

/* ─── Auto-dismiss timer bar ────────────────────────────── */
.toast-timer-bar {
  @apply absolute bottom-0 left-0 h-[2px] bg-emerald-400/60 dark:bg-emerald-500/50 rounded-full;
  animation: toast-shrink 5s linear forwards;
}

@keyframes toast-shrink {
  from { width: 100%; }
  to { width: 0%; }
}

/* ─── Transitions ───────────────────────────────────────── */
.toast-enter-active {
  transition: transform 0.35s cubic-bezier(0.16, 1, 0.3, 1), opacity 0.35s ease;
}

.toast-leave-active {
  transition: transform 0.25s cubic-bezier(0.5, 0, 0.75, 0), opacity 0.25s ease;
}

.toast-enter-from {
  transform: translateX(120%);
  opacity: 0;
}

.toast-leave-to {
  transform: translateX(120%);
  opacity: 0;
}
</style>
