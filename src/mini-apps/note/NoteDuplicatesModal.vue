<script setup lang="ts">
/**
 * Notes that exist twice, and a way to put the spare copy in the trash.
 *
 * Renaming a note used to write it back to the path it had just been moved
 * off, leaving one note under two names. That is fixed; the copies already
 * made are not, and nobody finds them by eye in a vault of any size.
 *
 * Nothing is deleted from here without being asked for, and what is asked for
 * goes to `.trash/` like every other delete — a report that quietly tidied up
 * would be making the judgement that belongs to whoever wrote the notes.
 */
import { ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import { Copy, X, Trash2, Check, AlertTriangle, Loader2 } from 'lucide-vue-next';
import { logger } from '../../utils/logger';

const props = defineProps<{ vaultPath: string }>();
const emit = defineEmits<{ (e: 'close'): void; (e: 'changed'): void }>();

const { t, locale } = useI18n();

interface DuplicateFile {
  rel_path: string;
  title: string;
  modified_at: number;
  bytes: number;
  same_body: boolean;
}
interface DuplicateGroup {
  node_id: string;
  files: DuplicateFile[];
}

const groups = ref<DuplicateGroup[]>([]);
const scanning = ref(true);
const error = ref('');
const trashing = ref<string | null>(null);

const when = (ms: number) =>
  new Date(ms).toLocaleString(locale.value, { dateStyle: 'medium', timeStyle: 'short' });

const scan = async () => {
  scanning.value = true;
  error.value = '';
  try {
    groups.value = await invoke<DuplicateGroup[]>('find_duplicate_notes', {
      vaultPath: props.vaultPath,
    });
  } catch (e) {
    logger.error('Could not scan the vault for duplicates', e);
    error.value = t('note.duplicates_failed');
  } finally {
    scanning.value = false;
  }
};

const trashCopy = async (group: DuplicateGroup, file: DuplicateFile) => {
  trashing.value = file.rel_path;
  try {
    await invoke('trash_node_file', { vaultPath: props.vaultPath, relPath: file.rel_path });
    group.files = group.files.filter((f) => f.rel_path !== file.rel_path);
    // A group with one file left is no longer a duplicate.
    if (group.files.length < 2) {
      groups.value = groups.value.filter((g) => g.node_id !== group.node_id);
    }
    emit('changed');
  } catch (e) {
    logger.error('Could not move that copy to the trash', e);
    error.value = t('note.duplicates_trash_failed');
  } finally {
    trashing.value = null;
  }
};

void scan();
</script>

<template>
  <div class="fixed inset-0 z-[999] flex items-center justify-center bg-black/40 backdrop-blur-sm" @click.self="emit('close')">
    <div class="bg-white dark:bg-[#2a2a2a] rounded-2xl shadow-2xl w-[95vw] max-w-[700px] max-h-[80vh] border border-[#e6e6e6] dark:border-[#3a3a3a] overflow-hidden flex flex-col">
      <div class="flex items-center justify-between px-5 py-4 border-b border-[#e6e6e6] dark:border-[#3a3a3a]">
        <h3 class="text-base font-semibold text-[#1c1c1e] dark:text-[#f4f4f5] flex items-center gap-2">
          <Copy class="w-4 h-4 text-gray-500" /> {{ $t('note.duplicates_title') }}
        </h3>
        <button @click="emit('close')" class="p-1 rounded-md hover:bg-gray-100 dark:hover:bg-[#333] text-gray-400 transition-colors" :aria-label="$t('note.cancel')">
          <X class="w-4 h-4" />
        </button>
      </div>

      <div class="flex-1 overflow-y-auto p-5 space-y-4">
        <div v-if="scanning" class="text-[13px] text-gray-400 text-center py-10 flex items-center justify-center gap-2">
          <Loader2 class="w-4 h-4 animate-spin" /> {{ $t('note.duplicates_scanning') }}
        </div>

        <div v-else-if="error" class="text-[13px] text-rose-500 text-center py-10">{{ error }}</div>

        <div v-else-if="groups.length === 0" class="text-center py-10">
          <Check class="w-6 h-6 text-emerald-500 mx-auto mb-2" />
          <p class="text-[13px] text-gray-500">{{ $t('note.duplicates_none') }}</p>
        </div>

        <template v-else>
          <p class="text-[12px] text-gray-500 leading-relaxed">{{ $t('note.duplicates_intro') }}</p>

          <div v-for="group in groups" :key="group.node_id" class="border border-[#e6e6e6] dark:border-[#3a3a3a] rounded-xl overflow-hidden">
            <div
              v-for="(file, i) in group.files"
              :key="file.rel_path"
              class="flex items-center gap-3 px-4 py-3 border-b last:border-b-0 border-[#f0f0f0] dark:border-[#333]"
            >
              <div class="min-w-0 flex-1">
                <div class="text-[13px] font-mono truncate text-[#1c1c1e] dark:text-[#f4f4f5]" :title="file.rel_path">{{ file.rel_path }}</div>
                <div class="text-[11px] text-gray-400 mt-0.5 flex items-center gap-2 flex-wrap">
                  <span>{{ when(file.modified_at) }}</span>
                  <span>·</span>
                  <span>{{ file.bytes.toLocaleString(locale) }} B</span>
                  <span v-if="i === 0" class="text-emerald-600 dark:text-emerald-400 font-medium">{{ $t('note.duplicates_newest') }}</span>
                  <!--
                    The distinction that decides whether trashing is safe. A
                    copy whose text differs holds writing the newest one does
                    not, and calling it "identical" would be a lie that costs
                    somebody a paragraph.
                  -->
                  <span v-else-if="file.same_body" class="text-gray-400">{{ $t('note.duplicates_identical') }}</span>
                  <span v-else class="text-amber-600 dark:text-amber-400 font-medium flex items-center gap-1">
                    <AlertTriangle class="w-3 h-3" /> {{ $t('note.duplicates_diverged') }}
                  </span>
                </div>
              </div>
              <button
                v-if="i > 0"
                @click="trashCopy(group, file)"
                :disabled="trashing === file.rel_path"
                class="shrink-0 flex items-center gap-1.5 px-2.5 py-1.5 text-[12px] rounded-md text-red-600 dark:text-red-400 hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors disabled:opacity-40"
              >
                <Trash2 class="w-3.5 h-3.5" />
                {{ $t('note.duplicates_trash') }}
              </button>
            </div>
          </div>
        </template>
      </div>

      <div class="p-4 border-t border-[#e6e6e6] dark:border-[#3a3a3a] bg-gray-50/50 dark:bg-[#242424]/50 flex items-center">
        <p class="text-[12px] text-gray-400">{{ $t('note.duplicates_hint') }}</p>
        <button @click="emit('close')" class="ml-auto px-4 py-2 text-sm rounded-lg text-gray-600 dark:text-gray-300 font-medium hover:bg-gray-200 dark:hover:bg-[#333] transition-colors">
          {{ $t('note.cancel') }}
        </button>
      </div>
    </div>
  </div>
</template>
