<script setup lang="ts">
import { ref, watch } from 'vue';
import { FileText } from 'lucide-vue-next';

export interface MentionItem {
  id: string;
  title: string;
  summary: string;
}

const props = defineProps<{
  items: MentionItem[];
  command: (item: MentionItem) => void;
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
  const el = document.querySelector('.mention-menu-item.is-selected');
  el?.scrollIntoView({ block: 'nearest' });
};

defineExpose({ onKeyDown });
</script>

<template>
  <div class="slash-command-menu" v-if="items.length > 0">
    <button
      v-for="(item, index) in items"
      :key="item.id"
      class="slash-menu-item"
      :class="{ 'is-selected': index === selectedIndex }"
      @click="selectItem(index)"
      @mouseenter="selectedIndex = index"
    >
      <div class="slash-menu-icon !bg-blue-50 dark:!bg-blue-500/20 !text-blue-600 dark:!text-blue-400 !border-blue-100 dark:!border-blue-500/30">
        <FileText class="w-4 h-4" />
      </div>
      <div class="slash-menu-text">
        <span class="slash-menu-title">{{ item.title || 'Untitled' }}</span>
        <span class="slash-menu-desc truncate max-w-[200px]">{{ item.summary }}</span>
      </div>
    </button>
  </div>
  <div class="slash-command-menu p-3 px-4 text-xs text-gray-500" v-else>
    No matching notes found...
  </div>
</template>
