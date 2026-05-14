<script setup lang="ts">
import { ref, nextTick } from 'vue';
import { X, Plus, ExternalLink } from 'lucide-vue-next';
import type { FileMetadata, FileReference } from '../composables/useFileStore';
import type { useFileStore } from '../composables/useFileStore';
import { watch, onMounted } from 'vue';

const props = defineProps<{
  file: FileMetadata;
  store: ReturnType<typeof useFileStore>;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
}>();

const isAddingTag = ref(false);
const newTagInput = ref('');
const isSaving = ref(false);
const infoPanelTagRef = ref<HTMLInputElement | null>(null);

const startAddingTag = async () => {
  isAddingTag.value = true;
  await nextTick();
  infoPanelTagRef.value?.focus();
};

const handleAddTag = async () => {
  if (!newTagInput.value.trim() || isSaving.value) return;
  isSaving.value = true;
  await props.store.addTag(props.file, newTagInput.value);
  newTagInput.value = '';
  isAddingTag.value = false;
  isSaving.value = false;
};

const handleRemoveTag = async (tag: string) => {
  if (isSaving.value) return;
  isSaving.value = true;
  await props.store.removeTag(props.file, tag);
  isSaving.value = false;
};

const isAssetsFile = props.file.path.includes('/assets/');

const fileRefs = ref<FileReference[]>([]);
const isLoadingRefs = ref(false);

const checkReferences = async () => {
  fileRefs.value = [];
  isLoadingRefs.value = true;
  try {
    fileRefs.value = await props.store.getFileReferences(props.file.filename);
  } catch (e) {
    console.error(e);
  } finally {
    isLoadingRefs.value = false;
  }
};

const isRenaming = ref(false);
const renameInput = ref('');
const renameInputRef = ref<HTMLInputElement | null>(null);

const startRename = async () => {
  if (isLoadingRefs.value || fileRefs.value.length > 0) return;
  isRenaming.value = true;
  renameInput.value = props.file.filename;
  await nextTick();
  if (renameInputRef.value) {
    renameInputRef.value.focus();
    const extIdx = renameInput.value.lastIndexOf('.');
    if (extIdx > 0) {
      renameInputRef.value.setSelectionRange(0, extIdx);
    } else {
      renameInputRef.value.select();
    }
  }
};

const handleRename = async () => {
  if (!isRenaming.value) return;
  const newName = renameInput.value.trim();
  if (newName && newName !== props.file.filename) {
    await props.store.saveFileName(props.file, newName);
  }
  isRenaming.value = false;
};

watch(() => props.file.filename, () => {
  isRenaming.value = false;
  checkReferences();
});
onMounted(checkReferences);
</script>

<template>
  <div class="w-72 flex-shrink-0 bg-white/70 dark:bg-white/[0.03] backdrop-blur-xl border-l border-gray-200/50 dark:border-white/5 flex flex-col overflow-hidden">
    <!-- Header -->
    <div class="h-12 px-4 flex items-center justify-between border-b border-gray-200/50 dark:border-white/5 flex-shrink-0">
      <h3 class="text-xs font-bold text-gray-400 uppercase tracking-wider">Info</h3>
      <button @click="emit('close')" class="p-1 hover:bg-gray-100 dark:hover:bg-white/10 rounded-md text-gray-400 cursor-pointer"><X class="w-3.5 h-3.5" /></button>
    </div>

    <div class="flex-1 overflow-y-auto p-4 space-y-5">
      <!-- Filename -->
      <div>
        <h4 class="text-xs font-bold text-gray-400 uppercase tracking-wider mb-1">Name</h4>
        <input v-if="isRenaming" ref="renameInputRef" v-model="renameInput" @blur="handleRename" @keydown.enter="handleRename" @keydown.esc="isRenaming = false" class="w-full text-sm font-semibold text-gray-900 dark:text-white break-words bg-transparent border-b-2 border-indigo-500 focus:outline-none" />
        <p v-else @click="startRename" class="text-sm font-semibold text-gray-900 dark:text-white break-words" :class="(isLoadingRefs || fileRefs.length > 0) ? '' : 'cursor-text hover:underline decoration-dashed decoration-gray-400 underline-offset-4'" :title="(isLoadingRefs || fileRefs.length > 0) ? 'Cannot rename while referenced' : 'Click to rename'">{{ file.filename }}</p>
      </div>

      <!-- Properties -->
      <div class="p-3 rounded-xl bg-gray-50/50 dark:bg-black/20 border border-gray-100 dark:border-white/5 space-y-2 text-xs">
        <div class="flex justify-between"><span class="text-gray-500">Type</span><span class="font-medium uppercase text-gray-900 dark:text-white">{{ file.extension }}</span></div>
        <div class="flex justify-between"><span class="text-gray-500">Size</span><span class="font-medium text-gray-900 dark:text-white">{{ store.formatSize(file.size) }}</span></div>
        <div class="flex justify-between"><span class="text-gray-500">Modified</span><span class="font-medium text-gray-900 dark:text-white">{{ file.modified_at.split(' ')[0] }}</span></div>
        <div class="flex justify-between"><span class="text-gray-500">Created</span><span class="font-medium text-gray-900 dark:text-white">{{ file.created_at.split(' ')[0] }}</span></div>
      </div>

      <!-- Location -->
      <div>
        <h4 class="text-xs font-bold text-gray-400 uppercase tracking-wider mb-1">Location</h4>
        <p class="text-[10px] font-mono text-gray-500 break-all p-2 bg-white dark:bg-black/40 rounded-lg border border-gray-200/50 dark:border-white/5">{{ file.path }}</p>
      </div>

      <!-- Tags -->
      <div>
        <h4 class="text-xs font-bold text-gray-400 uppercase tracking-wider mb-2">Tags</h4>
        <div class="flex flex-wrap items-center gap-1.5">
          <span v-for="tag in file.tags" :key="tag" class="group relative px-2 py-0.5 bg-indigo-50 dark:bg-indigo-500/10 text-indigo-600 dark:text-indigo-400 rounded-md text-[11px] font-medium border border-indigo-100 dark:border-indigo-500/20 flex items-center gap-1">
            #{{ tag }}
            <button v-if="!isAssetsFile" @click="handleRemoveTag(tag)" class="opacity-0 group-hover:opacity-100 hover:text-red-500 transition-opacity cursor-pointer" :disabled="isSaving">
              <X class="w-2.5 h-2.5" />
            </button>
          </span>
          <template v-if="!isAssetsFile">
            <input v-if="isAddingTag" ref="infoPanelTagRef" v-model="newTagInput" @keydown.enter="handleAddTag" @blur="isAddingTag = false; newTagInput = ''"
              type="text" placeholder="tag..."
              class="px-2 py-0.5 bg-white dark:bg-black/40 border border-indigo-300 dark:border-indigo-500/50 rounded-md text-[11px] font-medium focus:outline-none w-16" />
            <button v-else @click="startAddingTag" class="px-2 py-0.5 bg-white dark:bg-white/5 border border-dashed border-gray-300 dark:border-gray-600 rounded-md text-[11px] font-medium text-gray-400 hover:text-indigo-500 cursor-pointer">
              <Plus class="w-3 h-3 inline" /> Add
            </button>
          </template>
        </div>
      </div>

      <!-- Used by -->
      <div>
        <h4 class="text-xs font-bold text-gray-400 uppercase tracking-wider mb-2">Used by</h4>
        <div v-if="isLoadingRefs" class="text-xs text-gray-400">Checking references...</div>
        <div v-else-if="fileRefs.length === 0" class="flex items-center gap-2 p-3 rounded-xl bg-green-50 dark:bg-green-500/10 border border-green-200 dark:border-green-500/20">
          <svg class="w-4 h-4 text-green-500 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"/></svg>
          <span class="text-xs font-medium text-green-600 dark:text-green-400">Not used by any node</span>
        </div>
        <div v-else class="space-y-1.5">
          <div class="flex items-center gap-2 p-2 rounded-lg bg-red-50 dark:bg-red-500/10 border border-red-200 dark:border-red-500/20 mb-2">
            <svg class="w-3.5 h-3.5 text-red-500 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L4.082 16.5c-.77.833.192 2.5 1.732 2.5z"/></svg>
            <span class="text-[10px] font-bold text-red-600 dark:text-red-400">Referenced by {{ fileRefs.length }} node(s)</span>
          </div>
          <div v-for="ref_ in fileRefs" :key="ref_.node_id" class="flex items-center gap-2 px-3 py-2 bg-white dark:bg-black/30 rounded-lg border border-gray-200/50 dark:border-white/5">
            <span class="px-1.5 py-0.5 bg-indigo-100 dark:bg-indigo-500/20 text-indigo-600 dark:text-indigo-400 rounded text-[9px] font-bold uppercase flex-shrink-0">{{ ref_.node_type }}</span>
            <span class="text-xs text-gray-700 dark:text-gray-300 truncate">{{ ref_.title || 'Untitled' }}</span>
          </div>
        </div>
      </div>
    </div>

    <!-- Open Externally -->
    <div class="p-3 border-t border-gray-200/50 dark:border-white/5 flex-shrink-0">
      <button @click="store.openLocalFile(file.path)" class="w-full py-2 rounded-lg bg-gray-100 dark:bg-white/5 text-gray-700 dark:text-gray-300 text-xs font-semibold flex items-center justify-center gap-1.5 hover:bg-gray-200 dark:hover:bg-white/10 transition-colors cursor-pointer">
        <ExternalLink class="w-3.5 h-3.5" /> Open Externally
      </button>
    </div>
  </div>
</template>
