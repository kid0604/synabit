<script setup lang="ts">
/**
 * What sync set aside, told in terms of notes rather than paths.
 *
 * The notice used to list each conflict as two paths, one struck through. For
 * a QuickCap both are UUIDs, and a note that lost its place several times came
 * in as one entry per loss, each a longer chain of `(conflict …)` than the
 * last. It answered "where did my file go" and left the question anyone
 * actually has — is there anything in these copies, and what do I do with
 * them — to a trip through the folder.
 *
 * So the copies are grouped under the note they were split from, named by its
 * title, and compared with it in Rust (`commands::kept_copies`). A copy that
 * is identical can go to the trash from here; one that differs is opened for
 * reading. Nothing differing is ever offered for deletion — that judgement
 * belongs to whoever wrote it.
 */
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import { Zap, FileText, File, Loader2 } from 'lucide-vue-next';
import AppDialog from './AppDialog.vue';
import { appName } from '../appRegistry';
import { appForOpenType } from '../appAccess';
import { logger } from '../../utils/logger';
import type { SyncConflictInfo } from '../../composables/useSync';

type Difference = 'none' | 'properties' | 'text' | 'only_copy';
interface KeptCopy { rel_path: string; difference: Difference }
interface KeptGroup { original: string; title: string; node_type: string; copies: KeptCopy[] }

const props = defineProps<{ show: boolean; vaultPath: string; conflicts: SyncConflictInfo[] }>();
const emit = defineEmits<{
  (e: 'close'): void;
  /** These copies are gone, or were never worth a notice: stop showing them. */
  (e: 'forget', keptAs: string[]): void;
  (e: 'open', relPath: string, nodeType: string): void;
}>();

const { t } = useI18n();

const groups = ref<KeptGroup[]>([]);
const loading = ref(false);
const error = ref('');
const trashing = ref<string | null>(null);

const describe = async () => {
  loading.value = true;
  error.value = '';
  try {
    groups.value = await invoke<KeptGroup[]>('describe_kept_copies', {
      vaultPath: props.vaultPath,
      conflicts: props.conflicts,
    });
    // A copy that is no longer on disk has been dealt with somewhere — here,
    // by hand, or on another device. Its notice has nothing left to say.
    const present = new Set(groups.value.flatMap((g) => g.copies.map((c) => c.rel_path)));
    const resolved = props.conflicts.map((c) => c.kept_as).filter((p) => !present.has(p));
    if (resolved.length) emit('forget', resolved);
  } catch (e) {
    logger.error('Could not describe the copies sync kept aside', e);
    error.value = t('shell.conflicts.describe_failed');
  } finally {
    loading.value = false;
  }
};

watch(() => props.show, (open) => { if (open) void describe(); }, { immediate: true });

const identical = (g: KeptGroup) => g.copies.filter((c) => c.difference === 'none');
const differing = (g: KeptGroup) => g.copies.filter((c) => c.difference !== 'none');
const allIdentical = computed(() => groups.value.flatMap(identical));

/** The badge says the most important thing about the group, worst first. */
const verdict = (g: KeptGroup): { label: string; tone: 'ok' | 'warn' | 'muted' } => {
  const kinds = new Set(g.copies.map((c) => c.difference));
  if (kinds.has('text')) return { label: t('shell.conflicts.differs_text'), tone: 'warn' };
  if (kinds.has('properties')) return { label: t('shell.conflicts.differs_properties'), tone: 'warn' };
  if (kinds.has('only_copy')) return { label: t('shell.conflicts.only_copy'), tone: 'muted' };
  return { label: t('shell.conflicts.identical'), tone: 'ok' };
};

const kindLabel = (g: KeptGroup) => {
  const app = appForOpenType(g.node_type);
  return app ? appName(app) : t('shell.conflicts.file');
};

const canOpen = (g: KeptGroup) => appForOpenType(g.node_type) !== undefined;

const trash = async (copies: KeptCopy[], key: string) => {
  trashing.value = key;
  error.value = '';
  try {
    const gone = await invoke<string[]>('trash_identical_kept_copies', {
      vaultPath: props.vaultPath,
      copies: copies.map((c) => c.rel_path),
    });
    emit('forget', gone);
    // Whatever was not trashed changed since it was compared; look again
    // rather than leave a badge that is no longer true.
    if (gone.length < copies.length) {
      error.value = t('shell.conflicts.changed_meanwhile');
      await describe();
    } else {
      const goneSet = new Set(gone);
      groups.value = groups.value
        .map((g) => ({ ...g, copies: g.copies.filter((c) => !goneSet.has(c.rel_path)) }))
        .filter((g) => g.copies.length > 0);
    }
    if (groups.value.length === 0) emit('close');
  } catch (e) {
    logger.error('Could not move the identical copies to the trash', e);
    error.value = t('shell.conflicts.trash_failed');
  } finally {
    trashing.value = null;
  }
};

const keepAll = () => {
  emit('forget', props.conflicts.map((c) => c.kept_as));
  emit('close');
};
</script>

<template>
  <!-- Deliberately not styled as an error: the sync worked, nothing was lost. -->
  <AppDialog :show="show" labelledby="sync-conflicts-title" unstyled @close="emit('close')">
    <div class="w-full rounded-2xl bg-white dark:bg-gray-900 shadow-xl border border-border dark:border-border-dark overflow-hidden">
      <div class="px-5 py-4 border-b border-gray-200 dark:border-gray-800">
        <h2 id="sync-conflicts-title" class="text-base font-semibold text-gray-900 dark:text-gray-100">
          {{ $t('shell.conflicts.title', { count: groups.length || conflicts.length }, groups.length || conflicts.length) }}
        </h2>
        <p class="mt-1 text-sm text-gray-600 dark:text-gray-400">{{ $t('shell.conflicts.body') }}</p>
      </div>

      <div v-if="loading && groups.length === 0" class="px-5 py-8 flex items-center justify-center gap-2 text-sm text-gray-500 dark:text-gray-400">
        <Loader2 class="w-4 h-4 animate-spin" /> {{ $t('shell.conflicts.checking') }}
      </div>

      <ul v-else class="max-h-80 overflow-y-auto px-5">
        <li
          v-for="g in groups"
          :key="g.original"
          class="flex items-start gap-3 py-3 border-b last:border-b-0 border-gray-100 dark:border-gray-800"
        >
          <Zap v-if="g.node_type === 'quickcap'" class="w-4 h-4 mt-0.5 shrink-0 text-gray-500 dark:text-gray-400" aria-hidden="true" />
          <File v-else-if="g.node_type === 'file'" class="w-4 h-4 mt-0.5 shrink-0 text-gray-500 dark:text-gray-400" aria-hidden="true" />
          <FileText v-else class="w-4 h-4 mt-0.5 shrink-0 text-gray-500 dark:text-gray-400" aria-hidden="true" />

          <div class="min-w-0 flex-1">
            <!-- The path is still there for whoever wants it, on hover. -->
            <div class="text-sm font-medium text-gray-900 dark:text-gray-100 truncate" :title="g.original">{{ g.title }}</div>
            <div class="text-xs text-gray-500 dark:text-gray-400 mt-0.5">
              {{ kindLabel(g) }} · {{ $t('shell.conflicts.copies', { count: g.copies.length }, g.copies.length) }}
            </div>
            <span
              :class="[
                'inline-block mt-1.5 px-2 py-0.5 rounded-md text-xs',
                verdict(g).tone === 'ok' && 'bg-emerald-50 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-300',
                verdict(g).tone === 'warn' && 'bg-amber-50 text-amber-700 dark:bg-amber-900/30 dark:text-amber-300',
                verdict(g).tone === 'muted' && 'bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-300',
              ]"
            >{{ verdict(g).label }}</span>
          </div>

          <div class="shrink-0 flex flex-col items-end gap-1.5">
            <button
              v-if="identical(g).length"
              class="btn-secondary !h-8 !px-3 !text-xs"
              :disabled="trashing !== null"
              @click="trash(identical(g), g.original)"
            >
              <Loader2 v-if="trashing === g.original" class="w-3.5 h-3.5 animate-spin" />
              {{ $t('shell.conflicts.trash_copies', { count: identical(g).length }, identical(g).length) }}
            </button>
            <button
              v-if="differing(g).length && canOpen(g)"
              class="btn-secondary !h-8 !px-3 !text-xs"
              @click="emit('open', differing(g)[0].rel_path, g.node_type)"
            >
              {{ $t('shell.conflicts.open_copy') }}
            </button>
          </div>
        </li>
      </ul>

      <p v-if="error" class="px-5 pb-2 text-xs text-rose-600 dark:text-rose-400">{{ error }}</p>

      <div class="px-5 py-3 bg-gray-50 dark:bg-gray-800/50 flex items-center justify-end gap-2">
        <template v-if="allIdentical.length">
          <button class="btn-secondary" :disabled="trashing !== null" @click="keepAll">{{ $t('shell.conflicts.keep_all') }}</button>
          <button class="btn-primary" :disabled="trashing !== null" @click="trash(allIdentical, '*')">
            <Loader2 v-if="trashing === '*'" class="w-4 h-4 animate-spin" />
            {{ $t('shell.conflicts.trash_all', { count: allIdentical.length }, allIdentical.length) }}
          </button>
        </template>
        <button v-else class="btn-primary" @click="keepAll">{{ $t('shell.conflicts.got_it') }}</button>
      </div>
    </div>
  </AppDialog>
</template>
