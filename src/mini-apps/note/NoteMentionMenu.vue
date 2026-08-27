<script setup lang="ts">
import { ref, watch } from 'vue';
import { FileText, CheckSquare, Zap, Users, Box } from 'lucide-vue-next';

const getIcon = (type: string) => {
  if (type === 'task') return CheckSquare;
  if (type === 'quickcap') return Zap;
  if (type === 'person') return Users;
  if (type === 'note') return FileText;
  return Box;
};

export interface MentionItem {
  id: string;
  title: string;
  /** Set when the query carried an `alias` after a `|`. */
  alias: string;
  summary: string;
  node_type: string;
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
  // Nothing to choose from, so nothing to claim. This matters because the
  // mention query may now contain spaces, which keeps the suggestion alive to
  // the end of the line: a stray `@` earlier in a paragraph would otherwise
  // leave this menu invisibly swallowing every Enter the writer pressed.
  // `% 0` is also how `selectedIndex` became NaN.
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
        <component :is="getIcon(item.node_type)" class="w-4 h-4" />
      </div>
      <div class="slash-menu-text">
        <span class="slash-menu-title">{{ item.title || 'Untitled' }}</span>
        <!--
          When an alias was typed, show what the link will actually read as.
          The row is otherwise labelled with the title, which is precisely the
          text that is about to *not* appear in the note.
        -->
        <span v-if="item.alias" class="slash-menu-desc truncate max-w-[200px] italic">
          {{ $t('note.mention_shows_as', { alias: item.alias }) }}
        </span>
        <span v-else class="slash-menu-desc truncate max-w-[200px]">{{ item.summary }}</span>
      </div>
    </button>
  </div>
  <div class="slash-command-menu p-3 px-4 text-xs text-gray-500" v-else>
    {{ $t('note.mention_no_match') }}
    <div class="mt-1 opacity-70">{{ $t('note.mention_alias_hint') }}</div>
  </div>
</template>
