<script setup lang="ts">
defineProps<{
  show: boolean;
  url: string;
  text: string;
}>();

const emit = defineEmits<{
  (e: 'update:show', value: boolean): void;
  (e: 'update:url', value: string): void;
  (e: 'update:text', value: string): void;
  (e: 'confirm'): void;
  (e: 'remove'): void;
}>();
</script>

<template>
  <Teleport to="body">
    <div v-if="show" class="fixed inset-0 z-[999] flex items-center justify-center bg-black/40 backdrop-blur-sm" @click.self="emit('update:show', false)">
      <div class="bg-white dark:bg-[#2a2a2a] rounded-2xl shadow-2xl p-6 w-96 border border-[#e6e6e6] dark:border-[#3a3a3a]">
        <h3 class="text-base font-semibold text-[#1c1c1e] dark:text-[#f4f4f5] mb-4">{{ $t('note.link_title') }}</h3>

        <label class="block text-[11px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mb-1.5">{{ $t('note.link_destination') }}</label>
        <input
          :value="url"
          @input="emit('update:url', ($event.target as HTMLInputElement).value)"
          type="text"
          placeholder="https://example.com"
          class="w-full px-3 py-2 rounded-lg border border-[#e0e0e0] dark:border-[#444] bg-white dark:bg-[#1e1e1e] text-[#1c1c1e] dark:text-[#f4f4f5] text-sm focus:outline-none focus:ring-2 focus:ring-black/10 dark:focus:ring-white/20"
          @keydown.enter="emit('confirm')"
          autofocus
        />

        <!--
          The text and the destination are separate things. A note titled
          "Công ty cổ phần ABC" is worth calling "công ty cũ" in the middle of a
          sentence, and the link still points at the same note either way.
        -->
        <label class="block text-[11px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider mt-4 mb-1.5">{{ $t('note.link_display_text') }}</label>
        <input
          :value="text"
          @input="emit('update:text', ($event.target as HTMLInputElement).value)"
          type="text"
          :placeholder="$t('note.link_display_placeholder')"
          class="w-full px-3 py-2 rounded-lg border border-[#e0e0e0] dark:border-[#444] bg-white dark:bg-[#1e1e1e] text-[#1c1c1e] dark:text-[#f4f4f5] text-sm focus:outline-none focus:ring-2 focus:ring-black/10 dark:focus:ring-white/20"
          @keydown.enter="emit('confirm')"
        />

        <div class="flex justify-end gap-2 mt-5">
          <button @click="emit('remove')" class="px-4 py-1.5 text-sm rounded-lg text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors">{{ $t('note.link_remove') }}</button>
          <button @click="emit('update:show', false)" class="px-4 py-1.5 text-sm rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-[#333] transition-colors">{{ $t('note.cancel') }}</button>
          <button @click="emit('confirm')" class="px-4 py-1.5 text-sm rounded-lg bg-black dark:bg-white text-white dark:text-black font-medium hover:opacity-80 transition-opacity">{{ $t('note.link_apply') }}</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
