<script setup lang="ts">
import { ref } from 'vue';
import { Plus, Tag, X, PanelLeft } from 'lucide-vue-next';
import NavButtons from '../../../shared/components/NavButtons.vue';

const props = defineProps<{
  boardData: any;
  isSaving: boolean;
}>();

const emit = defineEmits<{
  (e: 'update-title', title: string): void;
  (e: 'add-tag', tag: string): void;
  (e: 'remove-tag', tag: string): void;
  (e: 'open-sidebar'): void;
}>();

// ─── Title Editing ────────────────────────────────────────
const editingTitle = ref(false);
const titleInput = ref('');

function startEditTitle() {
  if (!props.boardData) return;
  editingTitle.value = true;
  titleInput.value = props.boardData.title;
}

function finishEditTitle() {
  editingTitle.value = false;
  if (props.boardData && titleInput.value.trim()) {
    emit('update-title', titleInput.value.trim());
  }
}

// ─── Tags ────────────────────────────────────────────────
const isAddingTag = ref(false);
const newTagInput = ref('');

function addBoardTag() {
  if (!props.boardData || !newTagInput.value.trim()) return;
  const tag = newTagInput.value.trim().toLowerCase();
  if (props.boardData.tags.includes(tag)) {
    newTagInput.value = '';
    isAddingTag.value = false;
    return;
  }
  emit('add-tag', tag);
  newTagInput.value = '';
  isAddingTag.value = false;
}

function removeBoardTag(tag: string) {
  if (!props.boardData) return;
  emit('remove-tag', tag);
}
</script>

<template>
  <!-- Title bar -->
  <div class="wb-title-bar bg-white/85 dark:bg-[#1e1e1e]/85 backdrop-blur-md border-b border-border dark:border-border-dark">
    <div class="flex items-center gap-2 min-w-0 flex-1">
      <button @click="$emit('open-sidebar')" class="md:hidden p-1.5 -ml-2 rounded-md hover:bg-surface-hover dark:hover:bg-surface-hover-dark text-text-secondary dark:text-text-secondary-dark transition-colors" aria-label="Open Sidebar">
        <PanelLeft class="w-4.5 h-4.5" />
      </button>
      <NavButtons />
      <input
        v-if="editingTitle"
        v-model="titleInput"
        @blur="finishEditTitle"
        @keydown.enter="finishEditTitle"
        class="text-sm font-bold bg-transparent border-b border-accent dark:border-accent-dark outline-none text-text dark:text-text-dark"
        autofocus
      />
      <h1
        v-else
        @click="startEditTitle"
        class="text-sm font-bold truncate text-text dark:text-text-dark cursor-text hover:text-accent dark:hover:text-accent-dark transition-colors"
      >
        {{ boardData.title }}
      </h1>
    </div>
    <div class="flex items-center gap-1">
      <span v-if="isSaving" class="text-[10px] text-muted dark:text-muted-dark font-medium px-2">{{ $t('whiteboard.saving') }}</span>
    </div>
  </div>

  <!-- Tags row -->
  <div v-if="boardData.tags?.length" class="wb-tags-bar bg-white/85 dark:bg-[#1e1e1e]/85 backdrop-blur-md border-b border-border dark:border-border-dark">
    <Tag class="w-3.5 h-3.5 text-muted dark:text-muted-dark opacity-70" />
    <div class="flex items-center gap-1 flex-wrap min-w-0">
      <span
        v-for="tag in boardData.tags"
        :key="tag"
        class="wb-tag group bg-accent/10 text-accent dark:bg-[#a78bfa]/15 dark:text-[#a78bfa]"
      >
        #{{ tag }}
        <button @click.stop="removeBoardTag(tag)" class="ml-0.5 opacity-0 group-hover:opacity-100 hover:text-danger transition-opacity" aria-label="Remove Board Tag">
          <X class="w-2.5 h-2.5" />
        </button>
      </span>
      <input
        v-if="isAddingTag"
        v-model="newTagInput"
        @keydown.enter="addBoardTag"
        @keydown.escape="isAddingTag = false; newTagInput = ''"
        @blur="addBoardTag"
        type="text"
        :placeholder="$t('whiteboard.tag_ph')"
        class="wb-tag-input"
        autofocus
      />
      <button v-if="!isAddingTag" @click="isAddingTag = true" class="wb-tag-add border-border text-text-secondary hover:border-accent hover:text-accent dark:border-[#3f3f46] dark:text-[#71717a] dark:hover:border-[#a78bfa] dark:hover:text-[#a78bfa]" aria-label="Add Tag" :title="$t('whiteboard.add_tag')">
        <Plus class="w-3 h-3" />
      </button>
    </div>
  </div>
</template>

<style scoped>
.wb-title-bar {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  z-index: 45;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
}
@media (min-width: 768px) {
  .wb-title-bar {
    padding: 8px 72px;
  }
}
.wb-tags-bar {
  position: absolute;
  top: 37px;
  left: 0;
  right: 0;
  z-index: 45;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 12px;
}
@media (min-width: 768px) {
  .wb-tags-bar {
    padding: 4px 72px;
  }
}
.wb-tag {
  display: inline-flex;
  align-items: center;
  padding: 1px 6px;
  border-radius: 4px;
  font-size: 11px;
  font-weight: 500;
  cursor: default;
}
.wb-tag-input {
  width: 60px;
  padding: 1px 6px;
  border-radius: 4px;
  font-size: 11px;
  font-weight: 500;
  border: 1px solid var(--color-accent, #7c3aed);
  background: transparent;
  color: inherit;
  outline: none;
}
.wb-tag-add {
  width: 20px;
  height: 20px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 4px;
  border-width: 1px;
  border-style: dashed;
  background: transparent;
  cursor: pointer;
  transition: all 0.15s;
}
</style>
