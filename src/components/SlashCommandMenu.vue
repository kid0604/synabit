<script setup lang="ts">
import { ref, watch } from 'vue';

export interface SlashCommandItem {
  title: string;
  description: string;
  icon: any;
  command: (props: { editor: any; range: any }) => void;
}

const props = defineProps<{
  items: SlashCommandItem[];
  command: (item: SlashCommandItem) => void;
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
  const el = document.querySelector('.slash-menu-item.is-selected');
  el?.scrollIntoView({ block: 'nearest' });
};

defineExpose({ onKeyDown });
</script>

<template>
  <div class="slash-command-menu" v-if="items.length > 0">
    <button
      v-for="(item, index) in items"
      :key="item.title"
      class="slash-menu-item"
      :class="{ 'is-selected': index === selectedIndex }"
      @click="selectItem(index)"
      @mouseenter="selectedIndex = index"
    >
      <div class="slash-menu-icon">
        <component :is="item.icon" class="w-4 h-4" />
      </div>
      <div class="slash-menu-text">
        <span class="slash-menu-title">{{ item.title }}</span>
        <span class="slash-menu-desc">{{ item.description }}</span>
      </div>
    </button>
  </div>
</template>

<style>
.slash-command-menu {
  background: #fff;
  border: 1px solid #e5e7eb;
  border-radius: 8px;
  box-shadow: 0 4px 16px rgba(0,0,0,0.08), 0 1px 3px rgba(0,0,0,0.06);
  padding: 4px;
  max-height: 320px;
  overflow-y: auto;
  min-width: 240px;
}

@media (prefers-color-scheme: dark) {
  .slash-command-menu {
    background: #1e1e1e;
    border-color: #333;
    box-shadow: 0 4px 16px rgba(0,0,0,0.4);
  }
}

.slash-menu-item {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 8px 10px;
  border: none;
  background: transparent;
  border-radius: 6px;
  cursor: pointer;
  text-align: left;
  transition: background 0.1s;
}

.slash-menu-item:hover,
.slash-menu-item.is-selected {
  background: #f3f4f6;
}

@media (prefers-color-scheme: dark) {
  .slash-menu-item:hover,
  .slash-menu-item.is-selected {
    background: #2a2a2a;
  }
}

.slash-menu-icon {
  width: 32px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 6px;
  background: #f9fafb;
  border: 1px solid #e5e7eb;
  flex-shrink: 0;
  color: #6b7280;
}

@media (prefers-color-scheme: dark) {
  .slash-menu-icon {
    background: #252525;
    border-color: #3a3a3a;
    color: #a1a1aa;
  }
}

.slash-menu-text {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.slash-menu-title {
  font-size: 13px;
  font-weight: 500;
  color: #111827;
  line-height: 1.3;
}

@media (prefers-color-scheme: dark) {
  .slash-menu-title {
    color: #f4f4f5;
  }
}

.slash-menu-desc {
  font-size: 11px;
  color: #9ca3af;
  line-height: 1.3;
}

@media (prefers-color-scheme: dark) {
  .slash-menu-desc {
    color: #71717a;
  }
}
</style>
