/**
 * A plan of sends Syn stopped on, and the person's answer to it.
 *
 * A sibling of `useSynConsent` and `useSynChoice`, shaped like them: a run
 * parked on disk, a card in its own conversation, read back from `syn_waiting`
 * so a plan proposed while nobody was looking is still there to answer. See
 * `syn::writes` in Rust.
 */
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { logger } from '../../../utils/logger';
import { errorText } from '../../../shared/errorText';

export type WriteState = 'proposed' | 'approved' | 'declined' | 'done' | 'failed';

export interface WriteShown {
  param: string;
  value: string;
  table?: string | null;
  role?: string | null;
}

export interface WriteRequest {
  tool: string;
  args: Record<string, unknown>;
  hash: string;
  connector: string;
  operation: string;
  effect: 'write' | 'irreversible' | 'read';
  shown: WriteShown[];
  problems?: string[];
  state: WriteState;
}

export interface WritePlan {
  why: string;
  requests: WriteRequest[];
  proposed_at: string;
}

export interface WritesEvent {
  run_id: string;
  conversation_id?: string | null;
  plan: WritePlan;
}

const plans = ref<Record<string, WritesEvent>>({});
let listening = false;

export const refreshWaitingPlans = async (vaultPath: string) => {
  if (!vaultPath) return;
  try {
    const waiting = await invoke<Array<Partial<WritesEvent> & { run_id: string; conversation_id?: string | null }>>('syn_waiting', { vaultPath });
    const next: Record<string, WritesEvent> = {};
    for (const q of Array.isArray(waiting) ? waiting : []) {
      if (q.plan && q.conversation_id) next[q.conversation_id] = { run_id: q.run_id, conversation_id: q.conversation_id, plan: q.plan };
    }
    plans.value = next;
  } catch (e) {
    logger.error('[Syn] Could not read the plans still waiting', e);
  }
};

const listenOnce = () => {
  if (listening) return;
  listening = true;
  listen<WritesEvent>('syn-writes-needed', event => {
    const id = event.payload?.conversation_id;
    if (id && event.payload.plan) plans.value = { ...plans.value, [id]: event.payload };
  }).catch(e => {
    listening = false;
    logger.error('[Syn] Could not listen for plans', e);
  });
};

export function useSynWrites(vaultPath: () => string) {
  const first = !listening;
  listenOnce();
  if (first) void Promise.resolve().then(() => refreshWaitingPlans(vaultPath()));
  const error = ref<string | null>(null);
  const answering = ref(false);

  const pendingIn = (conversationId: string | null | undefined): WritesEvent | null =>
    (conversationId && plans.value[conversationId]) || null;

  /** Approve these requests (by hash), decline the rest; say whether there was a plan to answer. */
  const answer = async (approved: string[], conversationId: string | null | undefined): Promise<boolean> => {
    const asked = pendingIn(conversationId);
    if (!asked || answering.value) return false;
    answering.value = true;
    error.value = null;
    try {
      const was = await invoke<boolean>('syn_answer_writes', { vaultPath: vaultPath(), runId: asked.run_id, approved });
      const { [asked.conversation_id as string]: _done, ...rest } = plans.value;
      plans.value = rest;
      return was;
    } catch (e) {
      logger.error('[Syn] Could not record the answer to a plan', e);
      error.value = errorText(e);
      return false;
    } finally {
      answering.value = false;
    }
  };

  return { pendingIn, answer, error, answering, refresh: () => refreshWaitingPlans(vaultPath()) };
}
