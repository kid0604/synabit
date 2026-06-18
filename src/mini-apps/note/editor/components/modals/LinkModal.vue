<script setup lang="ts">
defineProps<{
  show: boolean;
  url: string;
}>();

const emit = defineEmits<{
  (e: 'update:show', value: boolean): void;
  (e: 'update:url', value: string): void;
  (e: 'confirm'): void;
  (e: 'remove'): void;
}>();
</script>

<template>
  <Teleport to="body">
    <div v-if="show" class="fixed inset-0 z-[999] flex items-center justify-center bg-black/40 backdrop-blur-sm" @click.self="emit('update:show', false)">
      <div class="bg-white dark:bg-[#2a2a2a] rounded-2xl shadow-2xl p-6 w-96 border border-[#e6e6e6] dark:border-[#3a3a3a]">
        <h3 class="text-base font-semibold text-[#1c1c1e] dark:text-[#f4f4f5] mb-4">Insert Link</h3>
        <input
          :value="url"
          @input="emit('update:url', ($event.target as HTMLInputElement).value)"
          type="url"
          placeholder="https://example.com"
          class="w-full px-3 py-2 rounded-lg border border-[#e0e0e0] dark:border-[#444] bg-white dark:bg-[#1e1e1e] text-[#1c1c1e] dark:text-[#f4f4f5] text-sm focus:outline-none focus:ring-2 focus:ring-black/10 dark:focus:ring-white/20"
          @keydown.enter="emit('confirm')"
          autofocus
        />
        <div class="flex justify-end gap-2 mt-4">
          <button @click="emit('remove')" class="px-4 py-1.5 text-sm rounded-lg text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors">Remove Link</button>
          <button @click="emit('update:show', false)" class="px-4 py-1.5 text-sm rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-[#333] transition-colors">Cancel</button>
          <button @click="emit('confirm')" class="px-4 py-1.5 text-sm rounded-lg bg-black dark:bg-white text-white dark:text-black font-medium hover:opacity-80 transition-opacity">Apply</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
