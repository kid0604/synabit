<script setup lang="ts">
import AppDialog from '../../../../../shared/components/AppDialog.vue';
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
  <AppDialog :show="show" labelledby="note-link-title" size="sm" panel-class="p-6" @close="emit('update:show', false)">
    <h3 id="note-link-title" class="text-base font-semibold text-text dark:text-text-dark mb-4">{{ $t('note.link_title') }}</h3>

    <label class="block text-xs font-semibold text-muted dark:text-muted-dark uppercase tracking-wider mb-1.5">{{ $t('note.link_destination') }}</label>
    <input
      :value="url"
      @input="emit('update:url', ($event.target as HTMLInputElement).value)"
      type="text"
      placeholder="https://example.com"
      class="w-full px-3 py-2 rounded-lg border border-border-subtle dark:border-[#444] bg-white dark:bg-surface-dark text-text dark:text-text-dark text-sm focus:outline-none focus:ring-2 focus:ring-black/10 dark:focus:ring-white/20"
      @keydown.enter="emit('confirm')"
      autofocus
    />

    <!--
      The text and the destination are separate things. A note titled
      "Công ty cổ phần ABC" is worth calling "công ty cũ" in the middle of a
      sentence, and the link still points at the same note either way.
    -->
    <label class="block text-xs font-semibold text-muted dark:text-muted-dark uppercase tracking-wider mt-4 mb-1.5">{{ $t('note.link_display_text') }}</label>
    <input
      :value="text"
      @input="emit('update:text', ($event.target as HTMLInputElement).value)"
      type="text"
      :placeholder="$t('note.link_display_placeholder')"
      class="w-full px-3 py-2 rounded-lg border border-border-subtle dark:border-[#444] bg-white dark:bg-surface-dark text-text dark:text-text-dark text-sm focus:outline-none focus:ring-2 focus:ring-black/10 dark:focus:ring-white/20"
      @keydown.enter="emit('confirm')"
    />

    <div class="flex justify-end gap-2 mt-5">
      <button @click="emit('remove')" class="px-4 py-1.5 text-sm rounded-lg text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors">{{ $t('note.link_remove') }}</button>
      <button @click="emit('update:show', false)" class="px-4 py-1.5 text-sm rounded-lg text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#333] transition-colors">{{ $t('note.cancel') }}</button>
      <button @click="emit('confirm')" class="btn-primary">{{ $t('note.link_apply') }}</button>
    </div>
  </AppDialog>
</template>
