<script setup lang="ts">
/**
 * The table a `query` code block turns into.
 *
 * The query text is the block's own content, so it stays plain markdown: the
 * note is still readable, and still portable, in any editor that never heard
 * of this feature.
 *
 * What is left here is the *fetching* — the debounce for a query being typed a
 * character at a time, the vault events that make it re-run, and the guard
 * against a slow answer landing after a fast one. The table itself moved to
 * `shared/views/TableView.vue`, which is handed a result and draws it. Things
 * shows the same table from a completely different loading story, and neither
 * of those stories belongs inside a table.
 *
 * Rows are nodes. Clicking one opens it, through the same event a transclusion
 * uses, so there is one route from a link to a node however the link was made.
 */
import { ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import { AlertTriangle, Loader2 } from 'lucide-vue-next';
import { useEventBus } from '../../composables/useEventBus';
import { logger } from '../../utils/logger';
import TableView from '../../shared/views/TableView.vue';
import type { QueryResult, QueryRow } from '../../shared/views/types';

const props = defineProps<{ query: string }>();
const { t } = useI18n();

const result = ref<QueryResult | null>(null);
const error = ref('');
const running = ref(false);

/** A slower earlier run must not paint over a later one's answer. */
let token = 0;
let timer: ReturnType<typeof setTimeout> | undefined;

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
      <Loader2 class="w-3.5 h-3.5 animate-spin" /> {{ t('note.query_running') }}
    </div>

    <div v-else-if="result && result.rows.length === 0" class="p-3 text-[12px] text-gray-400">
      {{ t('note.query_no_match') }}
    </div>

    <TableView
      v-else
      :result="result"
      :untitled-label="t('note.untitled_note')"
      @open="open"
    />
  </div>
</template>

<style scoped>
.query-result {
  user-select: none;
}
</style>
