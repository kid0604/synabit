/**
 * A connector Syn drafted and stopped on, for the person to approve.
 *
 * A sibling of `useSynWrites`, shaped like it: a run parked on disk, a card in
 * its own conversation, read back from `syn_waiting` so a proposal made while
 * nobody was looking is still there. Approving itself is done on the card with
 * `syn_instance_approve`; this only says the person answered, so the run can
 * carry on. See `connector::authoring` in Rust.
 */
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { logger } from '../../../utils/logger';
import { errorText } from '../../../shared/errorText';

export interface ConnectorProposal {
  run_id: string;
  conversation_id?: string | null;
  /** The draft's id. */
  connector: string;
}

const proposals = ref<Record<string, ConnectorProposal>>({});
let listening = false;

export const refreshWaitingConnectors = async (vaultPath: string) => {
  if (!vaultPath) return;
  try {
    const waiting = await invoke<Array<Partial<ConnectorProposal> & { run_id: string }>>('syn_waiting', { vaultPath });
    const next: Record<string, ConnectorProposal> = {};
    for (const q of Array.isArray(waiting) ? waiting : []) {
      if (q.connector && q.conversation_id) next[q.conversation_id] = { run_id: q.run_id, conversation_id: q.conversation_id, connector: q.connector };
    }
    proposals.value = next;
  } catch (e) {
    logger.error('[Syn] Could not read the connectors still waiting', e);
  }
};

const listenOnce = () => {
  if (listening) return;
  listening = true;
  listen<ConnectorProposal>('syn-connector-needed', event => {
    const id = event.payload?.conversation_id;
    if (id && event.payload.connector) proposals.value = { ...proposals.value, [id]: event.payload };
  }).catch(e => {
    listening = false;
    logger.error('[Syn] Could not listen for connector proposals', e);
  });
};

export function useSynConnectorProposal(vaultPath: () => string) {
  const first = !listening;
  listenOnce();
  if (first) void Promise.resolve().then(() => refreshWaitingConnectors(vaultPath()));
  const error = ref<string | null>(null);
  const answering = ref(false);

  const pendingIn = (conversationId: string | null | undefined): ConnectorProposal | null =>
    (conversationId && proposals.value[conversationId]) || null;

  /** Say the person answered — approved on the card, or not; whether one was waiting. */
  const answer = async (approved: boolean, conversationId: string | null | undefined): Promise<boolean> => {
    const asked = pendingIn(conversationId);
    if (!asked || answering.value) return false;
    answering.value = true;
    error.value = null;
    try {
      const was = await invoke<boolean>('syn_answer_connector', { vaultPath: vaultPath(), runId: asked.run_id, approved });
      const { [asked.conversation_id as string]: _done, ...rest } = proposals.value;
      proposals.value = rest;
      return was;
    } catch (e) {
      logger.error('[Syn] Could not record the answer to a connector', e);
      error.value = errorText(e);
      return false;
    } finally {
      answering.value = false;
    }
  };

  return { pendingIn, answer, error, answering, refresh: () => refreshWaitingConnectors(vaultPath()) };
}
