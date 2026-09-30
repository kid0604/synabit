<script setup lang="ts">
import { ref, watch } from 'vue';

export interface SlashCommandItem {
  /** English name, also matched by the search. */
  title: string;
  titleKey: string;
  descriptionKey: string;
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
  // With no items there is nothing to choose, so the key belongs to the
  // editor. Claiming it anyway left a hidden menu eating Enter after a query
  // that matched nothing, and `% 0` turned `selectedIndex` into NaN.
  if (props.items.length === 0) return false;

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
        <span class="slash-menu-title">{{ $t(item.titleKey) }}</span>
        <span class="slash-menu-desc">{{ $t(item.descriptionKey) }}</span>
      </div>
    </button>
  </div>
</template>

<style>
/* Dark follows the app's own theme (the `.dark` class on <html>), not the
   operating system's — the two differ whenever someone picks a theme in
   Settings. The menu is mounted on <body>, so the style is global. */
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

.dark .slash-command-menu {
  background: #1e1e1e;
  border-color: #333;
  box-shadow: 0 4px 16px rgba(0,0,0,0.4);
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

.dark .slash-menu-item:hover,
.dark .slash-menu-item.is-selected {
  background: #2a2a2a;
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

.dark .slash-menu-icon {
  background: #252525;
  border-color: #3a3a3a;
  color: #a1a1aa;
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

.dark .slash-menu-title {
  color: #f4f4f5;
}

.slash-menu-desc {
  font-size: 12px;
  color: #6b7280;
  line-height: 1.3;
}

.dark .slash-menu-desc {
  color: #a1a1aa;
}
</style>
