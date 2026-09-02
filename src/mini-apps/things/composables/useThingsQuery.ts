import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../../../utils/logger';
import type { QueryResult } from '../../../shared/views/types';

/**
 * The query Things is currently showing, and its answer.
 *
 * Owns the fetching so that the view primitives do not have to. That split is
 * the contract in `shared/views/types.ts`: a view is handed a result, and
 * whoever owns the query owns the loading state and the races.
 */
export function useThingsQuery() {
  const query = ref('');
  const result = ref<QueryResult | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);

  /**
   * A slower earlier run must not paint over a later one's answer.
   *
   * The same guard `QueryResultTable` carries. Typing in a query box issues one
   * request per keystroke's worth of debounce, and they do not come back in the
   * order they left.
   */
  let token = 0;

  /** Whether another page is being fetched, so a button can say so. */
  const loadingMore = ref(false);

  /**
   * Fetch the next page and append it.
   *
   * A separate call rather than a bigger `limit`, because the engine caps
   * every query at 500 whatever it is asked for — reaching row 501 is a
   * question of skipping, not of asking for more. The rows accumulate here so
   * sorting and grouping keep seeing one list.
   */
  const more = async () => {
    const found = result.value;
    if (!found || loadingMore.value) return;
    if (found.rows.length >= found.total) return;

    const mine = token;
    loadingMore.value = true;
    try {
      const next = await invoke<QueryResult>('run_node_query', {
        query: query.value.trim(),
        offset: found.rows.length,
      });
      // A new query started while this page was in flight; its answer is the
      // one on screen and this page belongs to a list nobody is looking at.
      if (mine !== token || !result.value) return;
      result.value = { ...next, rows: [...result.value.rows, ...next.rows] };
    } catch (e) {
      logger.error('[Things] Could not fetch the next page', e);
    } finally {
      loadingMore.value = false;
    }
  };

  const run = async (text?: string) => {
    if (text !== undefined) query.value = text;
    const q = query.value.trim();

    if (!q) {
      result.value = null;
      error.value = null;
      loading.value = false;
      return;
    }

    const mine = ++token;
    loading.value = true;
    error.value = null;

    try {
      const answer = await invoke<QueryResult>('run_node_query', { query: q, offset: 0 });
      if (mine !== token) return;
      result.value = answer;
    } catch (e) {
      if (mine !== token) return;
      logger.error('[Things] Query failed', e);
      // Shown to the user rather than swallowed. The engine says useful things
      // — an unknown sort key, a query with nothing to match on — and hiding
      // them leaves an empty list that looks like an empty vault.
      error.value = String(e);
      result.value = null;
    } finally {
      if (mine === token) loading.value = false;
    }
  };

  /** Browse one type. The plain case, and what the left rail does. */
  const showType = (nodeType: string) => run(`type:${nodeType} sort:-updated_at`);

  return { query, result, loading, error, run, showType, more, loadingMore };
}
