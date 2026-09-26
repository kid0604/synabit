/**
 * Syn's recent work, kept fresh while anything is happening.
 *
 * One store for the whole app, like the consent card's: the sidebar badge and
 * the activity screen are two views of the same list, and two copies would
 * disagree about whether something is still waiting.
 *
 * Refreshed when the backend says something happened — a run moved, stopped
 * to ask, or answered — rather than on a timer. A timer polls the disk for a
 * list that is idle nearly all day.
 */
import { computed, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { logger } from '../../../utils/logger';
import type { RunSummary } from '../types';
import { groupRuns, needsYou } from '../activity';

const runs = ref<RunSummary[]>([]);
const loaded = ref(false);
let vault = '';
let listening = false;
let pending: ReturnType<typeof setTimeout> | null = null;

const load = async () => {
  if (!vault) return;
  try {
    runs.value = await invoke<RunSummary[]>('syn_list_runs', { vaultPath: vault });
    loaded.value = true;
  } catch (e) {
    logger.error('[Syn] Could not read recent work', e);
  }
};

/** Once the burst is over: a run emits progress after every tool. */
const soon = () => {
  if (pending) clearTimeout(pending);
  pending = setTimeout(() => {
    pending = null;
    void load();
  }, 800);
};

const listenOnce = () => {
  if (listening) return;
  listening = true;
  for (const name of ['syn-progress', 'syn-consent-needed', 'syn-choice-needed', 'syn-stream-token']) {
    listen(name, (event) => {
      // Tokens arrive by the hundred; only the last one of a turn is news.
      if (name === 'syn-stream-token' && !(event.payload as { done?: boolean })?.done) return;
      soon();
    }).catch(e => {
      listening = false;
      logger.error(`[Syn] Could not listen for ${name}`, e);
    });
  }
};

export function useSynActivity(vaultPath: () => string) {
  if (vault !== vaultPath()) {
    vault = vaultPath();
    loaded.value = false;
    void load();
  }
  listenOnce();

  const activity = computed(() => groupRuns(runs.value));
  const waitingCount = computed(() => needsYou(activity.value));
  const working = computed(() => activity.value.working.length > 0);

  const stop = async (runId: string) => {
    try {
      await invoke('syn_cancel_run', { runId });
    } catch (e) {
      logger.error('[Syn] Could not stop a run', e);
    }
    soon();
  };

  return { activity, waitingCount, working, loaded, reload: load, stop };
}
