<script setup lang="ts">
/**
 * What the evidence ledger says about the file whose history is open.
 *
 * One line: when it was first recorded, and whether what was written has
 * changed since. The ledger fingerprints a note's body, not its frontmatter,
 * so pinning a note is not counted as an edit. When a device's
 * ledger has been altered, that is said instead, because a record that cannot
 * be relied on should not be read as one that can.
 * See `src-tauri/src/timeline/ledger.rs`.
 */
import { ref, watch, computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import { ShieldCheck, ShieldAlert } from 'lucide-vue-next';
import { logger } from '../../../utils/logger';

interface FileHistory {
  path: string;
  recorded: boolean;
  first_recorded_at: string | null;
  first_hash: string | null;
  current_hash: string | null;
  times_changed: number;
  last_changed_at: string | null;
  changed_since_record: boolean;
  trusted: boolean;
}

const props = defineProps<{
  vaultPath: string;
  relPath: string;
}>();

const emit = defineEmits<{
  (e: 'show-original'): void;
}>();

const { t, locale } = useI18n();
const history = ref<FileHistory | null>(null);

watch(() => [props.vaultPath, props.relPath] as const, async ([vaultPath, relPath]) => {
  try {
    history.value = await invoke<FileHistory>('ledger_history', { vaultPath, relPath });
  } catch (e) {
    logger.error('Failed to read the evidence ledger', e);
    history.value = null;
  }
}, { immediate: true });

const day = (stamp: string | null) =>
  stamp ? new Intl.DateTimeFormat(locale.value, { day: 'numeric', month: 'short', year: 'numeric' }).format(new Date(stamp)) : '';

const summary = computed(() => {
  const h = history.value;
  if (!h) return t('note.ledger_not_recorded');
  // First: a broken ledger may have lost the very record that would say this file was there.
  if (!h.trusted) return t('note.ledger_untrusted');
  if (!h.recorded) return t('note.ledger_not_recorded');
  if (h.times_changed === 0) return t('note.ledger_unchanged', { date: day(h.first_recorded_at) });
  if (!h.last_changed_at) return t('note.ledger_changed_unrecorded', { date: day(h.first_recorded_at) });
  return t('note.ledger_changed', {
    date: day(h.first_recorded_at),
    count: h.times_changed,
    last: day(h.last_changed_at),
  });
});
</script>

<template>
  <div
    v-if="history"
    class="flex items-center gap-2 px-5 py-2 text-xs border-b border-[#e6e6e6] dark:border-[#3a3a3a]"
    :class="history.trusted ? 'text-gray-500 dark:text-gray-400' : 'text-amber-700 dark:text-amber-400 bg-amber-50 dark:bg-amber-900/20'"
  >
    <component :is="history.trusted ? ShieldCheck : ShieldAlert" class="w-3.5 h-3.5 shrink-0" />
    <span class="truncate">{{ summary }}</span>
    <code
      v-if="history.first_hash"
      class="ml-auto shrink-0 font-mono text-[10px] text-gray-400"
      :title="$t('note.ledger_fingerprint')"
    >{{ history.first_hash.replace('blake3:', '').slice(0, 10) }}</code>
    <button
      v-if="history.recorded && history.times_changed > 0"
      type="button"
      class="shrink-0 font-medium text-indigo-600 dark:text-indigo-400 hover:underline"
      :class="history.first_hash ? '' : 'ml-auto'"
      @click="emit('show-original')"
    >
      {{ $t('note.ledger_show_original') }}
    </button>
  </div>
</template>
