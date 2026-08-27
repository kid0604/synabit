<script setup lang="ts">
/**
 * A note's history, read out of the CRDT log that every save already writes.
 *
 * The list is versions, newest first; the pane beside it is the note's whole
 * file — frontmatter and all — as it stood at the selected one. Frontmatter is
 * shown rather than hidden because a restore puts it back too, and a person
 * about to overwrite their note should see everything that is about to land.
 */
import { ref, watch, computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import { ask } from '@tauri-apps/plugin-dialog';
import { History, X, RotateCcw, Laptop } from 'lucide-vue-next';
import { logger } from '../../utils/logger';

const props = defineProps<{
  vaultPath: string;
  noteId: string;
  noteTitle: string;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'restored', content: string): void;
}>();

const { t, locale } = useI18n();

interface NodeVersion {
  id: string;
  /** Unix milliseconds — Loro's own unit, which is what `Date` wants. */
  timestamp: number | null;
  size: number;
  delta: number;
  is_current: boolean;
  is_local: boolean;
}

interface DiffLine {
  kind: 'equal' | 'insert' | 'delete';
  text: string;
}

interface DiffGroup {
  lines: DiffLine[];
  start_line: number;
}

interface VersionDiff {
  groups: DiffGroup[];
  added: number;
  removed: number;
  unchanged: boolean;
}

const versions = ref<NodeVersion[]>([]);
const selectedId = ref<string | null>(null);
const preview = ref('');
const diff = ref<VersionDiff | null>(null);
const loadingList = ref(true);
const loadingPreview = ref(false);
const restoring = ref(false);
const error = ref('');

/**
 * What the right-hand pane is showing.
 *
 * `changes` answers "what did I do in this sitting", `restore` answers "what
 * happens if I press the button", and `full` is the version itself for when
 * neither is what you wanted. A history gets opened for all three reasons, and
 * only the first is a sensible default — the point of a diff is that most of a
 * note is unchanged and reading it again is not why anyone came.
 */
type ViewMode = 'changes' | 'restore' | 'full';
const viewMode = ref<ViewMode>('changes');

const viewModes: { id: ViewMode; label: string }[] = [
  { id: 'changes', label: 'note.history_view_changes' },
  { id: 'restore', label: 'note.history_view_restore' },
  { id: 'full', label: 'note.history_view_full' },
];

const selected = computed(() => versions.value.find((v) => v.id === selectedId.value) || null);

const formatWhen = (version: NodeVersion) => {
  // An undated version is one written before the app recorded times. Saying so
  // is better than printing the start of 1970 and letting someone conclude
  // their history is broken.
  if (!version.timestamp) return t('note.history_undated');
  return new Date(version.timestamp).toLocaleString(locale.value, {
    dateStyle: 'medium',
    timeStyle: 'short',
  });
};

const formatDelta = (version: NodeVersion) => {
  if (version.delta === 0) return t('note.history_no_size_change');
  const sign = version.delta > 0 ? '+' : '−';
  return `${sign}${Math.abs(version.delta).toLocaleString(locale.value)}`;
};

const loadVersions = async () => {
  loadingList.value = true;
  error.value = '';
  try {
    versions.value = await invoke<NodeVersion[]>('list_node_versions', {
      vaultPath: props.vaultPath,
      relPath: props.noteId,
    });
    if (versions.value.length > 0) selectVersion(versions.value[0].id);
  } catch (e) {
    logger.error('Could not read this note history', e);
    error.value = t('note.history_failed');
  } finally {
    loadingList.value = false;
  }
};

// A slower earlier request must never paint over a later one's answer.
let previewToken = 0;

const loadPane = async () => {
  const id = selectedId.value;
  if (!id) return;
  const token = ++previewToken;
  loadingPreview.value = true;
  try {
    if (viewMode.value === 'full') {
      const text = await invoke<string>('read_node_version', {
        vaultPath: props.vaultPath,
        relPath: props.noteId,
        versionId: id,
      });
      if (token !== previewToken) return;
      preview.value = text;
      diff.value = null;
    } else {
      const result = await invoke<VersionDiff>('diff_node_version', {
        vaultPath: props.vaultPath,
        relPath: props.noteId,
        versionId: id,
        against: viewMode.value === 'restore' ? 'current' : 'previous',
      });
      if (token !== previewToken) return;
      diff.value = result;
      preview.value = '';
    }
    error.value = '';
  } catch (e) {
    if (token !== previewToken) return;
    logger.error('Could not read that version', e);
    preview.value = '';
    diff.value = null;
    error.value = t('note.history_failed');
  } finally {
    if (token === previewToken) loadingPreview.value = false;
  }
};

const selectVersion = (id: string) => {
  selectedId.value = id;
  void loadPane();
};

watch(viewMode, () => void loadPane());

const restore = async () => {
  if (!selected.value || selected.value.is_current) return;

  // The dialog describes what happens rather than warning about it, because
  // nothing here is destroyed: a restore is one more edit on top, and the
  // version being replaced stays in this same list.
  const confirmed = await ask(t('note.history_restore_body'), {
    title: t('note.history_restore_title', { when: formatWhen(selected.value) }),
    okLabel: t('note.history_restore_confirm'),
    cancelLabel: t('note.cancel'),
  });
  if (!confirmed) return;

  restoring.value = true;
  try {
    const content = await invoke<string>('restore_node_version', {
      vaultPath: props.vaultPath,
      relPath: props.noteId,
      versionId: selected.value.id,
    });
    emit('restored', content);
    emit('close');
  } catch (e) {
    logger.error('Could not restore that version', e);
    error.value = String(e);
  } finally {
    restoring.value = false;
  }
};

/** Comparing the current version against itself is always empty. */
watch(selected, (version) => {
  if (version?.is_current && viewMode.value === 'restore') viewMode.value = 'changes';
});

watch(() => props.noteId, loadVersions, { immediate: true });
</script>

<template>
  <div class="fixed inset-0 z-[999] flex items-center justify-center bg-black/40 backdrop-blur-sm" @click.self="emit('close')">
    <div class="bg-white dark:bg-[#2a2a2a] rounded-2xl shadow-2xl w-[95vw] max-w-[900px] h-[85vh] max-h-[640px] border border-[#e6e6e6] dark:border-[#3a3a3a] overflow-hidden flex flex-col">
      <!-- Header -->
      <div class="flex items-center justify-between px-5 py-4 border-b border-[#e6e6e6] dark:border-[#3a3a3a]">
        <h3 class="text-base font-semibold text-[#1c1c1e] dark:text-[#f4f4f5] flex items-center gap-2 min-w-0">
          <History class="w-4 h-4 text-gray-500 shrink-0" />
          <span class="truncate">{{ $t('note.history_title') }}</span>
          <span class="text-gray-400 font-normal truncate">— {{ noteTitle }}</span>
        </h3>
        <button @click="emit('close')" class="p-1 rounded-md hover:bg-gray-100 dark:hover:bg-[#333] text-gray-400 transition-colors shrink-0" :aria-label="$t('note.cancel')">
          <X class="w-4 h-4" />
        </button>
      </div>

      <div class="flex-1 flex min-h-0 max-md:flex-col">
        <!-- Version list -->
        <div class="w-64 max-md:w-full max-md:h-40 shrink-0 border-r max-md:border-r-0 max-md:border-b border-[#e6e6e6] dark:border-[#3a3a3a] overflow-y-auto p-2 space-y-1">
          <div v-if="loadingList" class="text-[13px] text-gray-400 text-center py-6">{{ $t('note.history_loading') }}</div>
          <div v-else-if="versions.length === 0" class="text-[13px] text-gray-400 text-center py-6 px-3 leading-relaxed">{{ $t('note.history_empty') }}</div>
          <button
            v-for="version in versions"
            :key="version.id"
            @click="selectVersion(version.id)"
            class="w-full text-left px-3 py-2 rounded-lg border transition-colors"
            :class="version.id === selectedId
              ? 'border-black/15 dark:border-white/20 bg-black/5 dark:bg-white/10'
              : 'border-transparent hover:bg-gray-50 dark:hover:bg-[#252525]'"
          >
            <div class="flex items-center gap-1.5" :title="version.is_local ? '' : $t('note.history_other_device')">
              <!--
                Marks the versions this device did *not* write. The peer id says
                only "somewhere else" — a second laptop looks exactly like a
                phone from here — so the icon must not claim to know which.
              -->
              <Laptop v-if="!version.is_local" class="w-3 h-3 text-gray-400 shrink-0" :aria-label="$t('note.history_other_device')" />
              <span class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] truncate" :class="version.is_local ? 'pl-[18px]' : ''">{{ formatWhen(version) }}</span>
            </div>
            <div class="flex items-center gap-2 mt-0.5 pl-[18px]">
              <span class="text-[11px] tabular-nums" :class="version.delta > 0 ? 'text-emerald-600 dark:text-emerald-400' : version.delta < 0 ? 'text-rose-500' : 'text-gray-400'">{{ formatDelta(version) }}</span>
              <span v-if="version.is_current" class="text-[10px] uppercase tracking-wider font-semibold text-gray-400">{{ $t('note.history_current') }}</span>
            </div>
          </button>
        </div>

        <!-- Preview -->
        <div class="flex-1 min-w-0 flex flex-col bg-gray-50/50 dark:bg-[#242424]/50">
          <!-- What the pane is showing -->
          <div v-if="selected" class="shrink-0 flex items-center gap-2 px-4 py-2 border-b border-[#e6e6e6] dark:border-[#3a3a3a]">
            <div class="flex bg-gray-100 dark:bg-[#1f1f1f] p-0.5 rounded-lg">
              <button
                v-for="mode in viewModes"
                :key="mode.id"
                @click="viewMode = mode.id"
                :disabled="mode.id === 'restore' && selected.is_current"
                class="px-2.5 py-1 text-[12px] rounded-md transition-colors font-medium disabled:opacity-40 disabled:cursor-not-allowed"
                :class="viewMode === mode.id ? 'bg-white dark:bg-[#2c2c2c] text-[#1c1c1e] dark:text-[#f4f4f5] shadow-sm' : 'text-gray-500 hover:text-gray-700 dark:hover:text-gray-300'"
              >
                {{ $t(mode.label) }}
              </button>
            </div>
            <span v-if="diff && !diff.unchanged" class="ml-auto text-[11px] tabular-nums flex items-center gap-2">
              <span class="text-emerald-600 dark:text-emerald-400">+{{ diff.added }}</span>
              <span class="text-rose-500">−{{ diff.removed }}</span>
            </span>
          </div>

          <div class="flex-1 min-h-0 overflow-y-auto">
            <div v-if="loadingPreview" class="text-[13px] text-gray-400 text-center py-10">{{ $t('note.history_loading') }}</div>

            <!-- Full text -->
            <pre v-else-if="viewMode === 'full' && preview" class="p-5 text-[13px] leading-relaxed whitespace-pre-wrap break-words font-mono text-[#1c1c1e] dark:text-[#f4f4f5]">{{ preview }}</pre>

            <!-- Diff -->
            <div v-else-if="diff && diff.unchanged" class="text-[13px] text-gray-400 text-center py-10 px-6 leading-relaxed">{{ $t('note.history_identical') }}</div>
            <div v-else-if="diff" class="py-2 text-[13px] font-mono leading-relaxed">
              <template v-for="(group, gi) in diff.groups" :key="gi">
                <!-- A fold marks the unchanged stretch this group skipped over. -->
                <div v-if="gi > 0" class="px-4 py-1 text-[11px] text-gray-400 select-none border-y border-dashed border-[#e6e6e6] dark:border-[#3a3a3a] my-1">⋯</div>
                <div
                  v-for="(line, li) in group.lines"
                  :key="`${gi}-${li}`"
                  class="px-4 flex gap-2 whitespace-pre-wrap break-words"
                  :class="{
                    'bg-emerald-50 dark:bg-emerald-900/20 text-emerald-900 dark:text-emerald-200': line.kind === 'insert',
                    'bg-rose-50 dark:bg-rose-900/20 text-rose-900 dark:text-rose-200': line.kind === 'delete',
                    'text-gray-500 dark:text-gray-400': line.kind === 'equal',
                  }"
                >
                  <!--
                    A sign, not colour alone: red and green are the whole
                    message here, and for a red-green colour-blind reader they
                    are the same grey.
                  -->
                  <span class="select-none w-3 shrink-0 opacity-60">{{ line.kind === 'insert' ? '+' : line.kind === 'delete' ? '−' : ' ' }}</span>
                  <span class="min-w-0">{{ line.text || ' ' }}</span>
                </div>
              </template>
            </div>

            <div v-else class="text-[13px] text-gray-400 text-center py-10">{{ $t('note.history_select') }}</div>
          </div>
        </div>
      </div>

      <!-- Footer -->
      <div class="p-4 border-t border-[#e6e6e6] dark:border-[#3a3a3a] bg-gray-50/50 dark:bg-[#242424]/50 flex items-center gap-2">
        <p v-if="error" class="text-[12px] text-rose-500 truncate">{{ error }}</p>
        <p v-else class="text-[12px] text-gray-400 truncate">{{ $t('note.history_hint') }}</p>
        <button @click="emit('close')" class="ml-auto shrink-0 px-4 py-2 text-sm rounded-lg text-gray-600 dark:text-gray-300 font-medium hover:bg-gray-200 dark:hover:bg-[#333] transition-colors">
          {{ $t('note.cancel') }}
        </button>
        <button
          @click="restore"
          :disabled="!selected || selected.is_current || restoring"
          class="shrink-0 px-4 py-2 text-sm rounded-lg bg-black dark:bg-white text-white dark:text-black font-medium hover:opacity-80 transition-opacity flex items-center gap-2 disabled:opacity-40 disabled:cursor-not-allowed"
        >
          <RotateCcw class="w-4 h-4" />
          {{ restoring ? $t('note.history_restoring') : $t('note.history_restore') }}
        </button>
      </div>
    </div>
  </div>
</template>
