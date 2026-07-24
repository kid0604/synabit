<script setup lang="ts">
import { X } from 'lucide-vue-next';

defineProps<{
  show: boolean;
  title: string;
  message: string;
  confirmText?: string;
  cancelText?: string;
  isDestructive?: boolean;
}>();

const emit = defineEmits<{
  (e: 'confirm'): void;
  (e: 'cancel'): void;
}>();
</script>

<template>
  <Teleport to="body">
    <Transition name="modal">
      <div
        v-if="show"
        class="fixed inset-0 z-[10000] flex items-center justify-center p-4 bg-black/40 backdrop-blur-sm"
        @click.self="emit('cancel')"
      >
        <div class="bg-surface dark:bg-surface-dark w-full max-w-sm rounded-xl shadow-2xl border border-border dark:border-border-dark overflow-hidden flex flex-col">
          <!-- Header -->
          <div class="flex items-center justify-between p-4 border-b border-border dark:border-border-dark">
            <h3 class="text-base font-semibold text-text dark:text-text-dark">{{ title }}</h3>
            <button @click="emit('cancel')" class="p-1 rounded-md hover:bg-surface-hover dark:hover:bg-surface-hover-dark text-muted dark:text-muted-dark transition-colors cursor-pointer" aria-label="More Options">
              <X class="w-4 h-4" />
            </button>
          </div>

          <!-- Body -->
          <div class="p-4">
            <p class="text-sm text-text-secondary dark:text-text-secondary-dark leading-relaxed">
              {{ message }}
            </p>
          </div>

          <!-- Footer -->
          <div class="p-4 bg-surface-hover/30 dark:bg-surface-hover-dark/30 flex justify-end gap-3 border-t border-border dark:border-border-dark">
            <button
              @click="emit('cancel')"
              class="px-4 py-2 text-sm font-medium rounded-lg text-text dark:text-text-dark hover:bg-surface-hover dark:hover:bg-surface-hover-dark transition-colors cursor-pointer"
            >
              {{ cancelText || 'Cancel' }}
            </button>
            <button
              @click="emit('confirm')"
              :class="[
                'px-4 py-2 text-sm font-medium rounded-lg transition-colors cursor-pointer text-white',
                isDestructive ? 'bg-red-500 hover:bg-red-600' : 'bg-primary dark:bg-white dark:text-black hover:opacity-90'
              ]"
            >
              {{ confirmText || 'Confirm' }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.modal-enter-active,
.modal-leave-active {
  transition: opacity 0.2s ease;
}

.modal-enter-from,
.modal-leave-to {
  opacity: 0;
}

.modal-enter-active .bg-surface,
.modal-leave-active .bg-surface {
  transition: transform 0.2s cubic-bezier(0.16, 1, 0.3, 1);
}

.modal-enter-from .bg-surface,
.modal-leave-to .bg-surface {
  transform: translateY(10px) scale(0.98);
}
</style>
