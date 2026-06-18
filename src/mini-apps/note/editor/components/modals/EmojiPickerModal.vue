<script setup lang="ts">
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
  <Teleport to="body">
    <div v-if="show" class="fixed inset-0 z-[999] flex items-center justify-center bg-black/40 backdrop-blur-sm" @click.self="emit('close')">
      <div class="emoji-picker-panel bg-white dark:bg-[#1e1e1e] rounded-2xl shadow-2xl border border-[#e5e7eb] dark:border-[#333] w-[360px] max-h-[420px] flex flex-col overflow-hidden">
        <!-- Search -->
        <div class="p-3 border-b border-[#e5e7eb] dark:border-[#333]">
          <input
            :value="search"
            @input="emit('update:search', ($event.target as HTMLInputElement).value)"
            type="text"
            placeholder="Search emoji..."
            class="w-full px-3 py-2 text-sm bg-[#f3f4f6] dark:bg-[#2a2a2a] border border-transparent rounded-lg focus:outline-none focus:ring-1 focus:ring-purple-500/50 text-[#111827] dark:text-[#f4f4f5] placeholder:text-gray-400"
            autofocus
            @keydown.esc="emit('close')"
          />
        </div>
        <!-- Category Tabs -->
        <div v-if="!search" class="flex gap-0.5 px-2 py-1.5 border-b border-[#e5e7eb] dark:border-[#333] overflow-x-auto">
          <button
            v-for="cat in emojiCategories"
            :key="cat.id"
            @click="emit('update:activeCategory', cat.id)"
            class="px-2 py-1 text-lg rounded-md transition-colors flex-shrink-0"
            :class="activeCategory === cat.id ? 'bg-[#e5e7eb] dark:bg-[#333]' : 'hover:bg-[#f3f4f6] dark:hover:bg-[#2a2a2a]'"
            :title="cat.title"
          >{{ cat.label }}</button>
        </div>
        <!-- Emoji Grid -->
        <div class="flex-1 overflow-y-auto p-2">
          <div v-if="filteredEmojis.length === 0" class="py-8 text-center text-sm text-gray-400">No emoji found</div>
          <div class="grid grid-cols-8 gap-0.5">
            <button
              v-for="item in filteredEmojis"
              :key="item.shortcode"
              @click="emit('select', item.emoji)"
              class="w-9 h-9 flex items-center justify-center text-xl rounded-lg hover:bg-[#f3f4f6] dark:hover:bg-[#2a2a2a] transition-colors cursor-pointer"
              :title="':' + item.shortcode + ':'"
            >{{ item.emoji }}</button>
          </div>
        </div>
      </div>
    </div>
  </Teleport>
</template>
