<script setup lang="ts">
import { ref } from 'vue';
import { Plus, Tag, X } from 'lucide-vue-next';
import NavButtons from '../../../shared/components/NavButtons.vue';

const props = defineProps<{
  boardData: any;
  isSaving: boolean;
}>();

const emit = defineEmits<{
  (e: 'update-title', title: string): void;
  (e: 'add-tag', tag: string): void;
  (e: 'remove-tag', tag: string): void;
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
  <div class="wb-title-bar">
    <div class="flex items-center gap-2 min-w-0 flex-1">
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
  <div class="wb-tags-bar">
    <Tag class="w-3 h-3 text-muted dark:text-muted-dark flex-shrink-0" />
    <div class="flex items-center gap-1 flex-wrap min-w-0">
      <span
        v-for="tag in boardData.tags"
        :key="tag"
        class="wb-tag group"
      >
        #{{ tag }}
        <button @click.stop="removeBoardTag(tag)" class="ml-0.5 opacity-0 group-hover:opacity-100 hover:text-danger transition-opacity">
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
      <button v-else @click="isAddingTag = true" class="wb-tag-add" :title="$t('whiteboard.add_tag')">
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
  padding: 8px 72px 8px 72px;
  background: var(--color-surface, #fff);
  border-bottom: 1px solid var(--color-border, #e6e6e6);
  backdrop-filter: blur(8px);
  background: rgba(255,255,255,0.85);
}
:global(.dark) .wb-title-bar {
  background: rgba(30,30,30,0.85);
  border-color: var(--color-border-dark, #2c2c2c);
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
  padding: 4px 72px;
  background: rgba(255,255,255,0.85);
  backdrop-filter: blur(8px);
  border-bottom: 1px solid var(--color-border, #e6e6e6);
}
:global(.dark) .wb-tags-bar {
  background: rgba(30,30,30,0.85);
  border-color: var(--color-border-dark, #2c2c2c);
}
.wb-tag {
  display: inline-flex;
  align-items: center;
  padding: 1px 6px;
  border-radius: 4px;
  font-size: 11px;
  font-weight: 500;
  background: rgba(124, 58, 237, 0.1);
  color: var(--color-accent, #7c3aed);
  cursor: default;
}
:global(.dark) .wb-tag {
  background: rgba(167, 139, 250, 0.12);
  color: #a78bfa;
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
  border: 1px dashed var(--color-border, #d4d4d8);
  background: transparent;
  color: var(--color-text-secondary, #a1a1aa);
  cursor: pointer;
  transition: all 0.15s;
}
.wb-tag-add:hover {
  border-color: var(--color-accent, #7c3aed);
  color: var(--color-accent, #7c3aed);
}
:global(.dark) .wb-tag-add {
  border-color: var(--color-border-dark, #3f3f46);
  color: var(--color-text-secondary-dark, #71717a);
}
:global(.dark) .wb-tag-add:hover {
  border-color: #a78bfa;
  color: #a78bfa;
}
</style>
