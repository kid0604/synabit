<script setup lang="ts">
import AppDialog from '../../../../../shared/components/AppDialog.vue';
import { emojiCategories } from '../../../emojiData';

export interface EmojiItem {
  emoji: string;
  shortcode: string;
  keywords: string[];
  category?: string;
}

defineProps<{
  show: boolean;
  search: string;
  activeCategory: string;
  filteredEmojis: EmojiItem[];
}>();

const emit = defineEmits<{
  (e: 'select', emoji: string): void;
  (e: 'update:search', value: string): void;
  (e: 'update:activeCategory', value: string): void;
  (e: 'close'): void;
}>();
</script>

<template>
  <AppDialog :show="show" :aria-label="$t('note.editor.emoji.title')" size="sm" panel-class="emoji-picker-panel !max-h-[420px] !overflow-hidden flex flex-col" @close="emit('close')">
    <!-- Search -->
    <div class="p-3 border-b border-[#e5e7eb] dark:border-[#333]">
      <input
        :value="search"
        @input="emit('update:search', ($event.target as HTMLInputElement).value)"
        type="text"
        :placeholder="$t('note.editor.emoji.search')"
        class="w-full px-3 py-2 text-sm bg-[#f3f4f6] dark:bg-surface-hover-dark border border-transparent rounded-lg focus:outline-none focus:ring-1 focus:ring-accent/50 text-[#111827] dark:text-text-dark placeholder:text-gray-500 dark:placeholder:text-gray-400"
        autofocus
      />
    </div>
    <!-- Category Tabs -->
    <div v-if="!search" class="flex gap-0.5 px-2 py-1.5 border-b border-[#e5e7eb] dark:border-[#333] overflow-x-auto">
      <button
        v-for="cat in emojiCategories"
        :key="cat.id"
        @click="emit('update:activeCategory', cat.id)"
        class="px-2 py-1 text-lg rounded-md transition-colors flex-shrink-0"
        :class="activeCategory === cat.id ? 'bg-[#e5e7eb] dark:bg-[#333]' : 'hover:bg-[#f3f4f6] dark:hover:bg-surface-hover-dark'"
        :title="$t(`note.editor.emoji.categories.${cat.id}`)"
      >{{ cat.label }}</button>
    </div>
    <!-- Emoji Grid -->
    <div class="flex-1 overflow-y-auto p-2">
      <div v-if="filteredEmojis.length === 0" class="py-8 text-center text-sm text-gray-500 dark:text-gray-400">{{ $t('note.editor.emoji.none_found') }}</div>
      <div class="grid grid-cols-8 gap-0.5">
        <button
          v-for="item in filteredEmojis"
          :key="item.shortcode"
          @click="emit('select', item.emoji)"
          class="w-9 h-9 flex items-center justify-center text-xl rounded-lg hover:bg-[#f3f4f6] dark:hover:bg-surface-hover-dark transition-colors cursor-pointer"
          :title="':' + item.shortcode + ':'"
        >{{ item.emoji }}</button>
      </div>
    </div>
  </AppDialog>
</template>
