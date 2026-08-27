<script setup lang="ts">
/**
 * The table a `query` code block turns into.
 *
 * The query text is the block's own content, so it stays plain markdown: the
 * note is still readable, and still portable, in any editor that never heard
 * of this feature.
 *
 * Rows are notes. Clicking one opens it, through the same event a transclusion
 * uses, so there is one route from a link to a note however the link was made.
 */
import { ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import { FileText, CheckSquare, Calendar, Users, Zap, Box, AlertTriangle, Loader2 } from 'lucide-vue-next';
import { useEventBus } from '../../composables/useEventBus';
import { logger } from '../../utils/logger';

const props = defineProps<{ query: string }>();
const { t } = useI18n();

interface QueryRow {
  id: string;
  node_type: string;
  title: string;
  cells: string[];
}
interface QueryResult {
  columns: string[];
  rows: QueryRow[];
  total: number;
  query_time_ms: number;
}

const result = ref<QueryResult | null>(null);
const error = ref('');
const running = ref(false);

/** A slower earlier run must not paint over a later one's answer. */
let token = 0;
let timer: ReturnType<typeof setTimeout> | undefined;

const iconFor = (type: string) => {
  if (type === 'task') return CheckSquare;
  if (type === 'event') return Calendar;
  if (type === 'person') return Users;
  if (type === 'quickcap') return Zap;
  if (type === 'note') return FileText;
  return Box;
};

const run = async () => {
  const text = props.query.trim();
  if (!text) {
    result.value = null;
    error.value = '';
    return;
  }

  const mine = ++token;
  running.value = true;
  try {
    const got = await invoke<QueryResult>('run_node_query', { query: text });
    if (mine !== token) return;
    result.value = got;
    error.value = '';
  } catch (e) {
    if (mine !== token) return;
    logger.error('Query failed', e);
    result.value = null;
    // The backend's message says what is wrong with the query itself, which is
    // more use to whoever wrote it than anything this component could invent.
    error.value = String(e);
  } finally {
    if (mine === token) running.value = false;
  }
};

const open = (row: QueryRow) => {
  window.dispatchEvent(
    new CustomEvent('synabit-navigate', { detail: { type: row.node_type, id: row.id } }),
  );
};

const runSoon = () => {
  clearTimeout(timer);
  timer = setTimeout(run, 400);
};

// Typing a query is editing a code block, so this fires per keystroke. The
// wait is the same one the diagram blocks use, and for the same reason.
watch(() => props.query, runSoon, { immediate: true });

/**
 * Re-run when the vault changes under it.
 *
 * Without this the table answers the question as it stood when the note was
 * opened. Finish a task in another tab and the list of open ones still shows
 * it — which is worse than no table, because it looks current.
 *
 * The bus unsubscribes itself when this component goes.
 */
const bus = useEventBus();
bus.on('node:created', runSoon);
bus.on('node:updated', runSoon);
bus.on('node:deleted', runSoon);
</script>

<template>
  <div class="query-result" contenteditable="false">
    <div v-if="error" class="flex items-start gap-2 p-3 text-[12px] text-amber-700 dark:text-amber-400 bg-amber-50 dark:bg-amber-900/20 rounded-lg">
      <AlertTriangle class="w-4 h-4 shrink-0 mt-px" />
      <span>{{ error }}</span>
    </div>

    <div v-else-if="running && !result" class="flex items-center gap-2 p-3 text-[12px] text-gray-400">
      <Loader2 class="w-3.5 h-3.5 animate-spin" /> {{ $t('note.query_running') }}
    </div>

    <div v-else-if="result && result.rows.length === 0" class="p-3 text-[12px] text-gray-400">
      {{ $t('note.query_no_match') }}
    </div>

    <div v-else-if="result" class="overflow-x-auto">
      <table class="w-full text-[13px] border-collapse">
        <thead>
          <tr class="border-b border-[#e6e6e6] dark:border-[#3f3f46]">
            <th
              v-for="column in result.columns"
              :key="column"
              class="text-left font-semibold text-[11px] uppercase tracking-wider text-gray-500 py-2 px-3 whitespace-nowrap"
            >
              {{ column }}
            </th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="row in result.rows"
            :key="row.id"
            @click="open(row)"
            class="border-b last:border-b-0 border-[#f0f0f0] dark:border-[#2c2c2c] cursor-pointer hover:bg-gray-50 dark:hover:bg-[#252525] transition-colors"
          >
            <td
              v-for="(cell, i) in row.cells"
              :key="i"
              class="py-2 px-3 text-[#1c1c1e] dark:text-[#f4f4f5] align-top"
            >
              <span v-if="i === 0" class="flex items-center gap-2">
                <component :is="iconFor(row.node_type)" class="w-3.5 h-3.5 text-gray-400 shrink-0" />
                <span class="truncate">{{ cell || t('note.untitled_note') }}</span>
              </span>
              <span v-else class="text-gray-500 dark:text-gray-400">{{ cell }}</span>
            </td>
          </tr>
        </tbody>
      </table>

      <!--
        `total` counts what matched, `rows` is what came back. Saying so beats
        a table that silently stops, which reads as "that is all there is".
      -->
      <p class="text-[11px] text-gray-400 pt-2 px-3">
        {{ $t('note.query_summary', { shown: result.rows.length, ms: result.query_time_ms }) }}
        <span v-if="result.total > result.rows.length"> · {{ $t('note.query_more') }}</span>
      </p>
    </div>
  </div>
</template>

<style scoped>
.query-result {
  user-select: none;
}
</style>
