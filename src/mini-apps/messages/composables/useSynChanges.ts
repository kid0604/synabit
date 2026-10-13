/**
 * Changes Syn held after reading something from outside, and the person's
 * answer to them.
 *
 * Not a stop like `useSynWrites`: the run has already answered. Its changes to
 * what was already in the vault wait here, one card per run, until the person
 * approves them or lets them go — and approving makes them, there and then.
 * Read back from `syn_waiting_changes`, so changes held while nobody was
 * looking are still there to answer. See `syn::changes` in Rust.
 */
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { logger } from '../../../utils/logger';
import { errorText } from '../../../shared/errorText';

export type ChangeState = 'held' | 'applied' | 'declined' | 'failed';
export type ChangeKind =
  | 'edit' | 'trash' | 'restore' | 'version' | 'memory' | 'board' | 'table'
  | 'transaction_new' | 'transaction_edit' | 'transaction_delete';

export interface ChangeField {
  name: string;
  before?: string | null;
  after?: string | null;
}

export interface HeldChange {
  hash: string;
  tool: string;
  kind: ChangeKind;
  titles: string[];
  fields: ChangeField[];
  before?: string | null;
  after?: string | null;
  state: ChangeState;
  error?: string | null;
}

export interface Changeset {
  changes: HeldChange[];
  because: string[];
}

export interface ChangesEvent {
  run_id: string;
  conversation_id?: string | null;
  changes: Changeset;
}

export interface ChangeOutcome {
  hash: string;
  state: ChangeState;
  error?: string | null;
}

/** By run: one conversation can hold several runs' changes. */
const waiting = ref<Record<string, ChangesEvent>>({});
let listening = false;

export const refreshWaitingChanges = async (vaultPath: string) => {
  if (!vaultPath) return;
  try {
    const found = await invoke<ChangesEvent[]>('syn_waiting_changes', { vaultPath });
    const next: Record<string, ChangesEvent> = {};
    for (const e of Array.isArray(found) ? found : []) if (e?.run_id && e.changes) next[e.run_id] = e;
    waiting.value = next;
  } catch (e) {
    logger.error('[Syn] Could not read the changes still waiting', e);
  }
};

const listenOnce = () => {
  if (listening) return;
  listening = true;
  listen<ChangesEvent>('syn-changes-needed', event => {
    const e = event.payload;
    if (e?.run_id && e.changes) waiting.value = { ...waiting.value, [e.run_id]: e };
  }).catch(e => {
    listening = false;
    logger.error('[Syn] Could not listen for held changes', e);
  });
};

export function useSynChanges(vaultPath: () => string) {
  const first = !listening;
  listenOnce();
  if (first) void Promise.resolve().then(() => refreshWaitingChanges(vaultPath()));
  const error = ref<string | null>(null);
  const answering = ref(false);

  /** The held changes of this conversation's runs, oldest first. */
  const heldIn = (conversationId: string | null | undefined): ChangesEvent[] =>
    conversationId ? Object.values(waiting.value).filter(e => e.conversation_id === conversationId) : [];

  /** Make the approved ones (by hash), let the rest go; what each came to. */
  const answer = async (runId: string, approved: string[]): Promise<ChangeOutcome[] | null> => {
    if (!waiting.value[runId] || answering.value) return null;
    answering.value = true;
    error.value = null;
    try {
      const outcomes = await invoke<ChangeOutcome[]>('syn_answer_changes', { vaultPath: vaultPath(), runId, approved });
      const { [runId]: _done, ...rest } = waiting.value;
      waiting.value = rest;
      return outcomes;
    } catch (e) {
      logger.error('[Syn] Could not answer the held changes', e);
      error.value = errorText(e);
      return null;
    } finally {
      answering.value = false;
    }
  };

  return { heldIn, answer, error, answering, refresh: () => refreshWaitingChanges(vaultPath()) };
}
