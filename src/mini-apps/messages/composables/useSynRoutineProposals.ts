/**
 * Routines Syn wrote from a conversation, waiting to be turned on.
 *
 * Like `useSynSkillProposals`: the run has answered; the routine it wrote is
 * switched off, and agreed to on no computer, until the person turns it on
 * from the card under that answer. See `routine::draft` in Rust.
 */
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { logger } from '../../../utils/logger';
import { errorText } from '../../../shared/errorText';
import type { Routine } from '../routines';

const waiting = ref<Record<string, Routine>>({});
let listening = false;

export const refreshRoutineProposals = async (vaultPath: string) => {
  if (!vaultPath) return;
  try {
    const found = await invoke<Routine[]>('syn_waiting_routine_proposals', { vaultPath });
    const next: Record<string, Routine> = {};
    for (const r of Array.isArray(found) ? found : []) if (r?.id) next[r.id] = r;
    waiting.value = next;
  } catch (e) {
    logger.error('[Syn] Could not read the routines waiting to be turned on', e);
  }
};

const listenOnce = () => {
  if (listening) return;
  listening = true;
  listen<{ conversation_id?: string | null; routines: Routine[] }>('syn-routines-proposed', event => {
    const next = { ...waiting.value };
    for (const r of event.payload?.routines ?? []) if (r?.id) next[r.id] = r;
    waiting.value = next;
  }).catch(e => {
    listening = false;
    logger.error('[Syn] Could not listen for routines Syn wrote', e);
  });
};

export function useSynRoutineProposals(vaultPath: () => string) {
  const first = !listening;
  listenOnce();
  if (first) void Promise.resolve().then(() => refreshRoutineProposals(vaultPath()));
  const error = ref<string | null>(null);
  const answering = ref(false);

  const proposedIn = (conversationId: string | null | undefined): Routine[] =>
    conversationId ? Object.values(waiting.value).filter(r => r.proposed_in === conversationId) : [];

  /** Turn it on (and agree to it on this device), or let it go. */
  const answer = async (routineId: string, keep: boolean): Promise<boolean> => {
    if (!waiting.value[routineId] || answering.value) return false;
    answering.value = true;
    error.value = null;
    try {
      await invoke('syn_answer_routine_proposal', { vaultPath: vaultPath(), routineId, keep });
      const { [routineId]: _done, ...rest } = waiting.value;
      waiting.value = rest;
      return true;
    } catch (e) {
      logger.error('[Syn] Could not answer a routine Syn wrote', e);
      error.value = errorText(e);
      return false;
    } finally {
      answering.value = false;
    }
  };

  return { proposedIn, answer, error, answering, refresh: () => refreshRoutineProposals(vaultPath()) };
}
