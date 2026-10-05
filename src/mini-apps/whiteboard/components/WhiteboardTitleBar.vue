<script setup lang="ts">
import { ref, nextTick } from 'vue';
import { Plus, Tag, X, PanelLeft, Presentation, History } from 'lucide-vue-next';
import NavButtons from '../../../shared/components/NavButtons.vue';

const props = defineProps<{
  boardData: any;
  isSaving: boolean;
  /** Set when the last save did not reach the disk; see the store. */
  saveProblem?: null | 'failed' | 'missing';
}>();

const emit = defineEmits<{
  (e: 'update-title', title: string): void;
  (e: 'add-tag', tag: string): void;
  (e: 'remove-tag', tag: string): void;
  (e: 'open-sidebar'): void;
  (e: 'present'): void;
  (e: 'history'): void;
}>();

// ─── Title Editing ────────────────────────────────────────
const editingTitle = ref(false);
const titleInput = ref('');
const titleInputRef = ref<HTMLInputElement | null>(null);

// The inputs here are focused by hand: `autofocus` is honoured once per
// document, so from the second edit on the box opened unfocused.
function startEditTitle() {
  if (!props.boardData) return;
  editingTitle.value = true;
  titleInput.value = props.boardData.title;
  nextTick(() => {
    titleInputRef.value?.focus();
    titleInputRef.value?.select();
  });
}

const titleButtonRef = ref<HTMLButtonElement | null>(null);

/** Escape: the title as it was, and focus back where the rename started. */
function cancelEditTitle() {
  editingTitle.value = false;
  nextTick(() => titleButtonRef.value?.focus());
}

function finishEditTitle() {
  // Enter, then the blur as the input goes: one rename, not two.
  if (!editingTitle.value) return;
  editingTitle.value = false;
  nextTick(() => titleButtonRef.value?.focus());
  if (props.boardData && titleInput.value.trim()) {
    emit('update-title', titleInput.value.trim());
  }
}

// ─── Tags ────────────────────────────────────────────────
const isAddingTag = ref(false);
const newTagInput = ref('');
const tagInputRef = ref<HTMLInputElement | null>(null);

function startAddTag() {
  isAddingTag.value = true;
  nextTick(() => tagInputRef.value?.focus());
}

function addBoardTag() {
  if (!props.boardData || !newTagInput.value.trim()) {
    // Left empty: close the box, or an untagged board keeps a tags row with
    // nothing in it but an input.
    isAddingTag.value = false;
    newTagInput.value = '';
    return;
  }
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
  <div class="wb-title-bar bg-white/85 dark:bg-surface-dark/85 backdrop-blur-md border-b border-border dark:border-border-dark">
    <div class="flex items-center gap-2 min-w-0 flex-1">
      <button @click="$emit('open-sidebar')" class="md:hidden p-1.5 -ml-2 rounded-md hover:bg-surface-hover dark:hover:bg-surface-hover-dark text-text-secondary dark:text-text-secondary-dark transition-colors" :aria-label="$t('whiteboard.open_sidebar')">
        <PanelLeft class="w-4.5 h-4.5" />
      </button>
      <NavButtons />
      <input
        v-if="editingTitle"
        ref="titleInputRef"
        v-model="titleInput"
        @blur="finishEditTitle"
        @keydown.enter="finishEditTitle"
        @keydown.escape.stop="cancelEditTitle"
        class="text-sm font-bold bg-transparent border-b border-accent dark:border-accent-dark outline-none text-text dark:text-text-dark"
        :aria-label="$t('whiteboard.board_title')"
      />
      <h1 v-else class="min-w-0">
        <!-- A button, so renaming is a Tab and an Enter away, not only a click. -->
        <button
          ref="titleButtonRef"
          type="button"
          @click="startEditTitle"
          class="block max-w-full text-sm font-bold truncate text-left text-text dark:text-text-dark cursor-text hover:text-accent dark:hover:text-accent-dark transition-colors"
          :aria-label="$t('whiteboard.rename_board', { title: boardData.title })"
        >
          {{ boardData.title }}
        </button>
      </h1>
    </div>
    <div class="flex items-center gap-1">
      <!-- The tags row only exists once there is a tag, so the first one is
           added from here. -->
      <button
        v-if="!boardData.tags?.length && !isAddingTag"
        @click="startAddTag"
        class="p-1.5 rounded-md hover:bg-surface-hover dark:hover:bg-surface-hover-dark text-muted dark:text-muted-dark hover:text-accent dark:hover:text-accent-dark transition-colors"
        :aria-label="$t('whiteboard.add_tag')"
        :title="$t('whiteboard.add_tag')"
      >
        <Tag class="w-3.5 h-3.5" />
      </button>
      <button
        @click="$emit('history')"
        class="p-1.5 rounded-md hover:bg-surface-hover dark:hover:bg-surface-hover-dark text-muted dark:text-muted-dark hover:text-accent dark:hover:text-accent-dark transition-colors"
        :aria-label="$t('whiteboard.history.title')"
        :title="$t('whiteboard.history.title')"
      >
        <History class="w-3.5 h-3.5" />
      </button>
      <button
        v-if="boardData.nodes?.length"
        data-wb-present
        @click="$emit('present')"
        class="p-1.5 rounded-md hover:bg-surface-hover dark:hover:bg-surface-hover-dark text-muted dark:text-muted-dark hover:text-accent dark:hover:text-accent-dark transition-colors"
        :aria-label="$t('whiteboard.present.start')"
        :title="$t('whiteboard.present.start')"
      >
        <Presentation class="w-3.5 h-3.5" />
      </button>
      <span v-if="isSaving" class="text-xs text-muted dark:text-muted-dark font-medium px-2">{{ $t('whiteboard.saving') }}</span>
      <!-- The work is still on screen but not on disk; the user has to know. -->
      <span
        v-else-if="saveProblem"
        role="status"
        class="text-xs font-medium px-2 text-danger"
        :title="$t(saveProblem === 'missing' ? 'whiteboard.board_missing_hint' : 'whiteboard.save_failed_hint')"
      >
        {{ $t(saveProblem === 'missing' ? 'whiteboard.board_missing' : 'whiteboard.save_failed') }}
      </span>
    </div>
  </div>

  <!-- Tags row -->
  <div v-if="boardData.tags?.length || isAddingTag" class="wb-tags-bar bg-white/85 dark:bg-surface-dark/85 backdrop-blur-md border-b border-border dark:border-border-dark">
    <Tag class="w-3.5 h-3.5 text-muted dark:text-muted-dark opacity-70" />
    <div class="flex items-center gap-1 flex-wrap min-w-0">
      <span
        v-for="tag in boardData.tags"
        :key="tag"
        class="wb-tag group bg-accent/10 text-accent dark:bg-accent-dark/15 dark:text-accent-dark"
      >
        #{{ tag }}
        <button @click.stop="removeBoardTag(tag)" class="ml-0.5 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 pointer-coarse:opacity-100 hover:text-danger transition-opacity" :aria-label="$t('whiteboard.remove_tag', { tag })">
          <X class="w-2.5 h-2.5" />
        </button>
      </span>
      <input
        v-if="isAddingTag"
        ref="tagInputRef"
        v-model="newTagInput"
        @keydown.enter="addBoardTag"
        @keydown.escape="isAddingTag = false; newTagInput = ''"
        @blur="addBoardTag"
        type="text"
        :placeholder="$t('whiteboard.tag_ph')"
        class="wb-tag-input"
      />
      <button v-if="!isAddingTag" @click="startAddTag" class="wb-tag-add border-border text-text-secondary hover:border-accent hover:text-accent dark:border-[#3f3f46] dark:text-muted-dark dark:hover:border-accent-dark dark:hover:text-accent-dark" :aria-label="$t('whiteboard.add_tag')" :title="$t('whiteboard.add_tag')">
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
  font-size: 12px;
  font-weight: 500;
  cursor: default;
}
.wb-tag-input {
  width: 60px;
  padding: 1px 6px;
  border-radius: 4px;
  font-size: 12px;
  font-weight: 500;
  border: 1px solid var(--color-accent);
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
/* The accent is 2.7:1 on the dark panel; borders, rings and text that carry
   it there take the paler dark accent. Fills under white text keep the accent. */
.dark .wb-tag-input {
  border-color: var(--color-accent-dark);
}
</style>
