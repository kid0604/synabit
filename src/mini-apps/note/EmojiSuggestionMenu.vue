<script setup lang="ts">
import { ref, watch } from 'vue';
import type { EmojiItem } from './emojiData';

const props = defineProps<{
  items: EmojiItem[];
  command: (item: EmojiItem) => void;
}>();

const selectedIndex = ref(0);

watch(() => props.items, () => {
  selectedIndex.value = 0;
});

const onKeyDown = (e: KeyboardEvent) => {
  if (e.key === 'ArrowUp') {
    e.preventDefault();
    selectedIndex.value = (selectedIndex.value + props.items.length - 1) % props.items.length;
    scrollToSelected();
    return true;
  }
  if (e.key === 'ArrowDown') {
    e.preventDefault();
    selectedIndex.value = (selectedIndex.value + 1) % props.items.length;
    scrollToSelected();
    return true;
  }
  if (e.key === 'Enter') {
    e.preventDefault();
    selectItem(selectedIndex.value);
    return true;
  }
  return false;
};

const selectItem = (index: number) => {
  const item = props.items[index];
  if (item) {
    props.command(item);
  }
};

const scrollToSelected = () => {
  const el = document.querySelector('.emoji-menu-item.is-selected');
  el?.scrollIntoView({ block: 'nearest' });
};

defineExpose({ onKeyDown });
</script>

<template>
  <div class="emoji-suggestion-menu" v-if="items.length > 0">
    <button
      v-for="(item, index) in items"
      :key="item.shortcode"
      class="emoji-menu-item"
      :class="{ 'is-selected': index === selectedIndex }"
      @click="selectItem(index)"
      @mouseenter="selectedIndex = index"
    >
      <span class="emoji-menu-char">{{ item.emoji }}</span>
      <span class="emoji-menu-code">:{{ item.shortcode }}:</span>
    </button>
  </div>
</template>

<style>
.emoji-suggestion-menu {
  background: #fff;
  border: 1px solid #e5e7eb;
  border-radius: 8px;
  box-shadow: 0 4px 16px rgba(0,0,0,0.08), 0 1px 3px rgba(0,0,0,0.06);
  padding: 4px;
  max-height: 240px;
  overflow-y: auto;
  min-width: 200px;
  max-width: 280px;
}

.dark .emoji-suggestion-menu {
  background: #1e1e1e;
  border-color: #333;
  box-shadow: 0 4px 16px rgba(0,0,0,0.4);
}

.emoji-menu-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 6px 8px;
  border: none;
  background: transparent;
  border-radius: 6px;
  cursor: pointer;
  text-align: left;
  transition: background 0.1s;
}

.emoji-menu-item:hover,
.emoji-menu-item.is-selected {
  background: #f3f4f6;
}

.dark .emoji-menu-item:hover,
.dark .emoji-menu-item.is-selected {
  background: #2a2a2a;
}

.emoji-menu-char {
  font-size: 20px;
  line-height: 1;
  width: 28px;
  text-align: center;
  flex-shrink: 0;
}

.emoji-menu-code {
  font-size: 12px;
  color: #6b7280;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dark .emoji-menu-code {
  color: #a1a1aa;
}
</style>
