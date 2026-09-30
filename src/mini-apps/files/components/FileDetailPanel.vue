<script setup lang="ts">
/**
 * One file, described: what it is, what it is tagged with, who is in it,
 * which notes use it, and what can be done to it.
 *
 * It used to be written out three times — beside the list, beside the
 * duplicate finder, and beside an open file — and the three had drifted:
 * different dates, different rename rules, one that could tag and one that
 * could not. The `variant` says where it sits, and only decides what that
 * place needs:
 *
 * * `browse` — beside the list. Preview, tags, people, open and delete.
 * * `duplicates` — beside a duplicate group. Preview and location instead of
 *   tags, since what matters there is which copy is which.
 * * `info` — beside an open file, which is already the preview. Narrower,
 *   with the created date, history, and opening in another app. A file in the
 *   vault's own assets folder is shown but not tagged from here, as before.
 */
import { ref, computed, watch, nextTick, onMounted } from 'vue';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { X, Plus, ExternalLink, History, Pencil, Check, AlertTriangle, FileText, ImageIcon, Video, Music, Code, FileArchive, FileType } from 'lucide-vue-next';
import NoteHistoryModal from '../../note/NoteHistoryModal.vue';
import type { FileMetadata, FileReference, useFileStore } from '../composables/useFileStore';
import { localDay } from '../../../shared/localDay';

const props = withDefaults(defineProps<{
  file: FileMetadata;
  store: ReturnType<typeof useFileStore>;
  vaultPath: string;
  variant?: 'browse' | 'duplicates' | 'info';
}>(), { variant: 'browse' });

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'open', file: FileMetadata): void;
  (e: 'delete', file: FileMetadata): void;
}>();

const isInfo = computed(() => props.variant === 'info');
const showPreview = computed(() => props.variant !== 'info');
const showTags = computed(() => props.variant !== 'duplicates');
const showLocation = computed(() => props.variant !== 'browse');
const heading = computed(() =>
  props.variant === 'info' ? 'file.info' : props.variant === 'duplicates' ? 'file.preview' : 'file.details'
);

/** A file in the vault's own assets folder belongs to the note that holds it. */
const isAssetsFile = computed(() => isInfo.value && props.file.path.includes('/assets/'));

const getFileIcon = (ext: string) => {
  const e = ext.toLowerCase();
  if (['jpg','jpeg','png','gif','webp','bmp','svg','heic'].includes(e)) return ImageIcon;
  if (['mp4','mkv','avi','mov','webm'].includes(e)) return Video;
  if (['mp3','wav','flac','ogg','m4a'].includes(e)) return Music;
  if (['pdf','doc','docx','txt','md'].includes(e)) return FileText;
  if (['zip','rar','7z','tar','gz'].includes(e)) return FileArchive;
  if (['js','ts','vue','json','html','css','rs','py'].includes(e)) return Code;
  return FileType;
};
const ext = computed(() => props.file.extension.toLowerCase());

// ─── Which notes use it ─────────────────────────────────────
const fileRefs = ref<FileReference[]>([]);
const isLoadingRefs = ref(false);
/** Renaming or deleting a file a note points at would break the note. */
const isInUse = computed(() => isLoadingRefs.value || fileRefs.value.length > 0);

const checkReferences = async () => {
  const id = props.file.id;
  fileRefs.value = [];
  isLoadingRefs.value = true;
  try {
    const found = await props.store.getFileReferences(id);
    // Another file may have been picked while this one was being asked about.
    if (props.file.id === id) fileRefs.value = found;
  } catch (e) {
    console.error(e);
  } finally {
    if (props.file.id === id) isLoadingRefs.value = false;
  }
};

// ─── Rename ─────────────────────────────────────────────────
const isRenaming = ref(false);
const renameInput = ref('');
const renameInputRef = ref<HTMLInputElement | null>(null);

const startRename = async () => {
  if (isInUse.value) return;
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

// ─── Tags ───────────────────────────────────────────────────
const isAddingTag = ref(false);
const newTagInput = ref('');
const isSaving = ref(false);
const tagInputRef = ref<HTMLInputElement | null>(null);

const startAddingTag = async () => {
  isAddingTag.value = true;
  await nextTick();
  tagInputRef.value?.focus();
};

const handleAddTag = async () => {
  const tag = newTagInput.value.trim();
  if (!tag || isSaving.value) { isAddingTag.value = false; return; }
  isSaving.value = true;
  try {
    await props.store.addTag(props.file, tag);
  } finally {
    newTagInput.value = '';
    isAddingTag.value = false;
    isSaving.value = false;
  }
};

const handleRemoveTag = async (tag: string) => {
  if (isSaving.value) return;
  isSaving.value = true;
  try {
    await props.store.removeTag(props.file, tag);
  } finally {
    isSaving.value = false;
  }
};

// ─── People ─────────────────────────────────────────────────
/** The name out of a note-style link, `[Name](synabit://person/…)`. */
const getPersonName = (link: string) => {
  const match = link.match(/\[([^\]]*)\]/);
  return match ? match[1] : link;
};

interface PersonNode { id: string; title: string; }
const allPeople = ref<PersonNode[]>([]);
const searchPeopleQuery = ref('');
const showPeopleDropdown = ref(false);
const peopleInputRef = ref<HTMLInputElement | null>(null);

const fetchAllPeople = async () => {
  try {
    const nodes = await invoke<{ id: string; title: string }[]>('get_nodes', { nodeType: 'person' });
    allPeople.value = nodes.map(n => ({ id: n.id, title: n.title }));
  } catch (e) {
    console.error('Failed to fetch people', e);
  }
};

const filteredPeople = computed(() => {
  const q = searchPeopleQuery.value.toLowerCase();
  return allPeople.value.filter(p => {
    if (!p.title.toLowerCase().includes(q)) return false;
    // Somebody already linked is not a suggestion.
    if (props.file.people?.some(link => link.includes(p.id))) return false;
    return true;
  });
});

const openPeopleSearch = async () => {
  showPeopleDropdown.value = true;
  await nextTick();
  peopleInputRef.value?.focus();
};

const handleSelectPerson = async (person: PersonNode) => {
  if (isSaving.value) return;
  isSaving.value = true;
  try {
    await props.store.addPerson(props.file, `[${person.title}](synabit://person/${person.id})`);
  } finally {
    searchPeopleQuery.value = '';
    showPeopleDropdown.value = false;
    isSaving.value = false;
  }
};

const handleRemovePerson = async (link: string) => {
  if (isSaving.value) return;
  isSaving.value = true;
  try {
    await props.store.removePerson(props.file, link);
  } finally {
    isSaving.value = false;
  }
};

// A delay, so a click on a suggestion lands before the list disappears.
const handlePeopleDropdownBlur = () => window.setTimeout(() => showPeopleDropdown.value = false, 150);

// ─── History ────────────────────────────────────────────────
/**
 * An annotated file is an ordinary vault document, so the note editor's
 * history modal reads it with no changes at all. A file nobody has tagged has
 * no document behind it and so no history — correct, not a gap.
 */
const showHistory = ref(false);
const hasMetadata = computed(() =>
  props.file.tags.length > 0 || (props.file.people?.length ?? 0) > 0 || !!props.file.label
);

// A different file, or this one under a new name: start again.
watch([() => props.file.id, () => props.file.filename], () => {
  isRenaming.value = false;
  isAddingTag.value = false;
  newTagInput.value = '';
  showPeopleDropdown.value = false;
  void checkReferences();
});

onMounted(() => {
  void checkReferences();
  void fetchAllPeople();
});
</script>

<template>
  <aside
    class="absolute md:relative inset-0 md:inset-auto z-40 w-full flex-shrink-0 bg-white md:bg-white/70 dark:bg-[#0a0a0a] md:dark:bg-white/[0.03] backdrop-blur-2xl md:border-l border-gray-200/50 dark:border-white/5 flex flex-col overflow-hidden"
    :class="isInfo ? 'md:w-72' : 'md:w-80 xl:w-96'"
    :aria-label="$t(heading)"
  >
    <div class="h-14 px-5 flex items-center justify-between border-b border-gray-200/50 dark:border-white/5 flex-shrink-0">
      <h2 class="font-bold text-sm text-gray-900 dark:text-white">{{ $t(heading) }}</h2>
      <button type="button" @click="emit('close')" class="btn-icon -mr-2" :aria-label="$t('file.close_panel')" :title="$t('file.close_panel')">
        <X class="w-4 h-4" />
      </button>
    </div>

    <div class="flex-1 overflow-y-auto p-5 space-y-5">
      <!-- Preview -->
      <div v-if="showPreview" class="aspect-square rounded-2xl bg-surface-hover/60 dark:bg-surface-hover-dark/40 border border-border dark:border-border-dark flex items-center justify-center overflow-hidden">
        <img v-if="['jpg','jpeg','png','gif','svg','webp','bmp','heic'].includes(ext)" :src="convertFileSrc(file.path)" class="w-full h-full object-contain" alt="" />
        <video v-else-if="['mp4','mov','webm','mkv'].includes(ext)" :src="convertFileSrc(file.path)" controls class="w-full h-full object-contain bg-black/5" />
        <audio v-else-if="['mp3','wav','ogg','m4a'].includes(ext)" :src="convertFileSrc(file.path)" controls class="w-full px-4" />
        <component v-else :is="getFileIcon(file.extension)" class="w-20 h-20 text-accent/50 dark:text-accent-dark/50" />
      </div>

      <!-- Name -->
      <div>
        <h4 v-if="isInfo" class="text-xs font-bold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-1">{{ $t('file.name_col') }}</h4>
        <input v-if="isRenaming" ref="renameInputRef" v-model="renameInput" @blur="handleRename" @keydown.enter="handleRename" @keydown.esc="isRenaming = false"
          :aria-label="$t('file.rename')"
          class="w-full break-words leading-tight text-gray-900 dark:text-white bg-transparent border-b-2 border-accent focus:outline-none"
          :class="isInfo ? 'text-sm font-semibold' : 'font-extrabold text-lg'" />
        <div v-else class="flex items-start gap-1">
          <h3 @click="startRename" class="flex-1 min-w-0 break-words leading-tight text-gray-900 dark:text-white"
            :class="[isInfo ? 'text-sm font-semibold' : 'font-extrabold text-lg', isInUse ? '' : 'cursor-text hover:underline decoration-dashed decoration-gray-400 underline-offset-4']"
            :title="isInUse ? $t('file.cannot_rename') : $t('file.click_to_rename')">{{ file.filename }}</h3>
          <!-- The same as clicking the name, for anyone who would never think to. -->
          <button type="button" @click="startRename" :disabled="isInUse"
            class="btn-icon -mt-1.5 flex-shrink-0 disabled:opacity-40 disabled:cursor-not-allowed"
            :aria-label="$t('file.rename')" :title="isInUse ? $t('file.cannot_rename') : $t('file.rename')">
            <Pencil class="w-4 h-4" />
          </button>
        </div>
      </div>

      <!-- Metadata -->
      <div class="p-4 rounded-xl bg-gray-50/50 dark:bg-black/20 border border-gray-100 dark:border-white/5 space-y-2 text-sm">
        <div class="flex justify-between"><span class="text-gray-500">{{ $t('file.type') }}</span><span class="font-medium uppercase text-gray-900 dark:text-white">{{ file.extension }}</span></div>
        <div class="flex justify-between"><span class="text-gray-500">{{ $t('file.size_col') }}</span><span class="font-medium text-gray-900 dark:text-white">{{ store.formatSize(file.size) }}</span></div>
        <div class="flex justify-between"><span class="text-gray-500">{{ $t('file.modified_col') }}</span><span class="font-medium text-gray-900 dark:text-white">{{ localDay(file.modified_at) }}</span></div>
        <div v-if="isInfo" class="flex justify-between"><span class="text-gray-500">{{ $t('file.created') }}</span><span class="font-medium text-gray-900 dark:text-white">{{ localDay(file.created_at) }}</span></div>
      </div>

      <!-- Location -->
      <div v-if="showLocation">
        <h4 class="text-xs font-bold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-1">{{ $t('file.location') }}</h4>
        <p class="text-xs font-mono text-gray-500 dark:text-gray-400 break-all p-2 bg-white dark:bg-black/40 rounded-lg border border-gray-200/50 dark:border-white/5">{{ file.path }}</p>
      </div>

      <!-- Tags -->
      <div v-if="showTags">
        <h4 class="text-xs font-bold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-2">{{ $t('file.tags') }}</h4>
        <div class="flex flex-wrap items-center gap-1.5">
          <span v-for="tag in file.tags" :key="tag" class="group relative px-2.5 py-1 bg-accent/10 text-accent dark:text-accent-dark rounded-lg text-xs font-medium border border-accent/20 flex items-center gap-1">
            #{{ tag }}
            <button v-if="!isAssetsFile" type="button" @click="handleRemoveTag(tag)" :disabled="isSaving" class="opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 pointer-coarse:opacity-100 hover:text-red-500 transition-opacity cursor-pointer" :aria-label="$t('file.remove_tag')" :title="$t('file.remove_tag')">
              <X class="w-3 h-3" />
            </button>
          </span>
          <template v-if="!isAssetsFile">
            <input v-if="isAddingTag" ref="tagInputRef" v-model="newTagInput" @keydown.enter="handleAddTag" @keydown.esc="isAddingTag = false; newTagInput = ''" @blur="handleAddTag"
              type="text" :placeholder="$t('file.tag_placeholder')" :aria-label="$t('file.tags')"
              class="px-2 py-1 bg-white dark:bg-black/40 border border-accent/40 rounded-lg text-xs font-medium focus:outline-none w-20" />
            <button v-else type="button" @click="startAddingTag" class="px-2.5 py-1 bg-white dark:bg-white/5 border border-dashed border-gray-300 dark:border-gray-600 rounded-lg text-xs font-medium text-gray-500 dark:text-gray-400 hover:text-accent dark:hover:text-accent-dark hover:border-accent/40 cursor-pointer transition-colors">
              <Plus class="w-3 h-3 inline" /> {{ $t('file.add') }}
            </button>
          </template>
        </div>
      </div>

      <!-- Linked People -->
      <div>
        <h4 class="text-xs font-bold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-2">{{ $t('file.people') }}</h4>
        <div class="flex flex-wrap items-center gap-1.5 mb-2">
          <span v-for="link in (file.people || [])" :key="link" class="group relative px-2.5 py-1 bg-emerald-50 dark:bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 rounded-lg text-xs font-medium border border-emerald-100 dark:border-emerald-500/20 flex items-center gap-1">
            @{{ getPersonName(link) }}
            <button v-if="!isAssetsFile" type="button" @click="handleRemovePerson(link)" :disabled="isSaving" class="opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 pointer-coarse:opacity-100 hover:text-red-500 transition-opacity cursor-pointer" :aria-label="$t('file.remove_person')" :title="$t('file.remove_person')">
              <X class="w-3 h-3" />
            </button>
          </span>
        </div>
        <div v-if="!isAssetsFile" class="relative">
          <input
            v-if="showPeopleDropdown"
            ref="peopleInputRef"
            v-model="searchPeopleQuery"
            type="text"
            :placeholder="$t('file.search_person_placeholder')"
            :aria-label="$t('file.link_person')"
            class="w-full px-2 py-1.5 bg-white dark:bg-black/40 border border-emerald-300 dark:border-emerald-500/50 rounded-lg text-xs font-medium focus:outline-none"
            @blur="handlePeopleDropdownBlur"
          />
          <button v-else type="button" @click="openPeopleSearch" class="px-2.5 py-1 bg-white dark:bg-white/5 border border-dashed border-gray-300 dark:border-gray-600 rounded-lg text-xs font-medium text-gray-500 dark:text-gray-400 hover:text-emerald-500 hover:border-emerald-300 cursor-pointer transition-colors">
            <Plus class="w-3 h-3 inline" /> {{ $t('file.link_person') }}
          </button>

          <div v-if="showPeopleDropdown && filteredPeople.length > 0" class="absolute z-10 w-full mt-1 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl shadow-lg max-h-40 overflow-y-auto">
            <button
              v-for="person in filteredPeople"
              :key="person.id"
              type="button"
              @click="handleSelectPerson(person)"
              class="w-full text-left px-3 py-2 text-xs font-medium hover:bg-gray-100 dark:hover:bg-gray-700 text-gray-700 dark:text-gray-300 cursor-pointer"
            >
              {{ person.title }}
            </button>
          </div>
        </div>
      </div>

      <!-- Used by -->
      <div>
        <h4 class="text-xs font-bold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-2">{{ $t('file.used_by') }}</h4>
        <div v-if="isLoadingRefs" class="text-xs text-gray-500 dark:text-gray-400">{{ $t('file.checking_refs') }}</div>
        <div v-else-if="fileRefs.length === 0" class="flex items-center gap-2 p-3 rounded-xl bg-green-50 dark:bg-green-500/10 border border-green-200 dark:border-green-500/20">
          <Check class="w-4 h-4 text-green-500 flex-shrink-0" />
          <span class="text-xs font-medium text-green-600 dark:text-green-400">{{ variant === 'duplicates' ? $t('file.safe_to_delete') : $t('file.not_used') }}</span>
        </div>
        <div v-else class="space-y-1.5">
          <div class="flex items-center gap-2 p-2 rounded-lg bg-red-50 dark:bg-red-500/10 border border-red-200 dark:border-red-500/20 mb-2">
            <AlertTriangle class="w-3.5 h-3.5 text-red-500 flex-shrink-0" />
            <span class="text-xs font-bold text-red-600 dark:text-red-400">{{ $t('file.used_in_notes', { count: fileRefs.length }, fileRefs.length) }}</span>
          </div>
          <div v-for="ref_ in fileRefs" :key="ref_.node_id" class="flex items-center gap-2 px-3 py-2 bg-white dark:bg-black/30 rounded-lg border border-gray-200/50 dark:border-white/5">
            <span class="px-1.5 py-0.5 bg-accent/10 text-accent dark:text-accent-dark rounded text-xs font-bold uppercase flex-shrink-0">{{ ref_.node_type }}</span>
            <span class="text-xs text-gray-700 dark:text-gray-300 truncate">{{ ref_.title || $t('file.untitled') }}</span>
            <!-- Showing a file and pointing at it are different relationships,
                 and only one of them breaks if the file is renamed. -->
            <span class="ml-auto text-xs text-gray-500 dark:text-gray-400 flex-shrink-0">
              {{ ref_.edge_type === 'attachment' ? $t('file.shown_in') : $t('file.linked_from') }}
            </span>
          </div>
        </div>
      </div>
    </div>

    <!-- Actions -->
    <div v-if="isInfo" class="p-3 border-t border-gray-200/50 dark:border-white/5 flex-shrink-0 space-y-2">
      <button v-if="hasMetadata" type="button" @click="showHistory = true" class="btn-secondary w-full">
        <History class="w-4 h-4" /> {{ $t('note.history') }}
      </button>
      <button type="button" @click="store.openLocalFile(file.path)" class="btn-secondary w-full">
        <ExternalLink class="w-4 h-4" /> {{ $t('file.open_externally') }}
      </button>
    </div>
    <div v-else class="p-5 border-t border-gray-200/50 dark:border-white/5 space-y-2 flex-shrink-0">
      <button type="button" @click="emit('open', file)" class="btn-primary w-full">
        {{ $t('file.open_file') }}
      </button>
      <button
        type="button"
        @click="emit('delete', file)"
        :disabled="isInUse"
        class="w-full py-2 rounded-lg font-semibold text-sm transition-colors cursor-pointer"
        :class="isInUse
          ? 'bg-gray-100 dark:bg-white/5 text-gray-500 cursor-not-allowed'
          : 'bg-red-50 dark:bg-red-500/10 text-red-600 dark:text-red-400 hover:bg-red-100 dark:hover:bg-red-500/20 border border-red-200 dark:border-red-500/20'">
        {{ fileRefs.length > 0 ? $t('file.in_use_cannot_delete') : $t('file.delete_file') }}
      </button>
    </div>

    <NoteHistoryModal
      v-if="showHistory"
      :vault-path="vaultPath"
      :note-id="file.id"
      :note-title="file.filename"
      @close="showHistory = false"
      @restored="showHistory = false"
    />
  </aside>
</template>
