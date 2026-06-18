<script setup lang="ts">
import { Navigation as NavigationIcon } from 'lucide-vue-next';

defineProps<{
  show: boolean;
  urlInput: string;
  label: string;
  error: string;
  isValid: boolean;
}>();

const emit = defineEmits<{
  (e: 'update:urlInput', value: string): void;
  (e: 'update:label', value: string): void;
  (e: 'confirm'): void;
  (e: 'close'): void;
}>();
</script>

<template>
  <Teleport to="body">
    <div v-if="show" class="fixed inset-0 z-[999] flex items-center justify-center bg-black/40 backdrop-blur-sm" @click.self="emit('close')">
      <div class="bg-white dark:bg-[#1e1e1e] rounded-2xl shadow-2xl border border-[#e5e7eb] dark:border-[#333] w-[420px] max-w-[95vw] p-5" @keydown.esc="emit('close')">
        <div class="flex items-center gap-2 mb-4">
          <NavigationIcon class="w-4 h-4 text-indigo-500" />
          <h3 class="text-sm font-semibold text-gray-800 dark:text-gray-200">Insert Route</h3>
        </div>

        <!-- URL Input -->
        <div class="mb-3">
          <label class="text-[10px] font-semibold text-gray-400 uppercase tracking-wider mb-1 block">Directions URL</label>
          <input
            :value="urlInput"
            @input="emit('update:urlInput', ($event.target as HTMLInputElement).value)"
            type="text"
            placeholder="Paste Google Maps or OpenStreetMap directions URL..."
            class="w-full px-3 py-2 text-sm rounded-lg border border-[#e0e0e0] dark:border-[#444] bg-[#fafafa] dark:bg-[#252525] text-gray-800 dark:text-gray-200 outline-none focus:border-indigo-400 transition-colors"
            @keydown.enter.stop="emit('confirm')"
          />
          <p class="text-[10px] text-gray-400 mt-1">Supports Google Maps and OpenStreetMap directions links.</p>
        </div>

        <!-- Optional Label -->
        <div class="mb-3">
          <label class="text-[10px] font-semibold text-gray-400 uppercase tracking-wider mb-1 block">Label <span class="text-gray-300 dark:text-gray-500">(optional)</span></label>
          <input
            :value="label"
            @input="emit('update:label', ($event.target as HTMLInputElement).value)"
            type="text"
            placeholder="e.g., Home → Office"
            class="w-full px-3 py-2 text-sm rounded-lg border border-[#e0e0e0] dark:border-[#444] bg-[#fafafa] dark:bg-[#252525] text-gray-800 dark:text-gray-200 outline-none focus:border-indigo-400 transition-colors"
            @keydown.enter.stop="emit('confirm')"
          />
        </div>

        <div v-if="error" class="text-xs text-red-400 mb-2">{{ error }}</div>

        <div class="flex justify-end gap-2 mt-4">
          <button @click="emit('close')" class="px-4 py-1.5 text-sm rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-[#333] transition-colors">Cancel</button>
          <button
            @click="emit('confirm')"
            :disabled="!isValid"
            class="px-4 py-1.5 text-sm rounded-lg bg-indigo-500 text-white font-medium hover:bg-indigo-600 transition-colors disabled:opacity-40 disabled:cursor-not-allowed flex items-center gap-1.5"
          >
            <NavigationIcon class="w-3.5 h-3.5" />
            Insert Route
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
