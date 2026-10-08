/**
 * The *which one* Syn stops on, and the answer to it.
 *
 * A sibling of `useSynConsent` and shaped exactly like it, because the two are
 * the same event with different meanings: a run parked on disk, a card in the
 * conversation, and an answer that does not restart anything.
 *
 * Separate from consent rather than folded into it, and that is the point.
 * Consent means a permission was never granted and something is being refused.
 * This means the work is going fine and Syn declines to guess one detail. Two
 * cards that look alike and mean opposite things about whether anything is
 * wrong would teach somebody to answer both the same way.
 */
import { computed, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { logger } from '../../../utils/logger';
import type { AmbiguousChoice } from '../types';
import { errorText } from '../../../shared/errorText';

export interface ChoiceEvent {
  run_id: string;
  conversation_id?: string | null;
  choice: AmbiguousChoice;
}

/**
 * The question on the table, held once for the whole app — for the reasons
 * `useSynConsent` gives: the ask bar has to see it as well as Messages, and
 * two copies could disagree about whether it was answered.
 */
const questions = ref<Record<string, ChoiceEvent>>({});
let listening = false;

/** Take in every *which one* still waiting on disk. See `useSynConsent`. */
export const refreshWaitingChoices = async (vaultPath: string) => {
  if (!vaultPath) return;
  try {
    const waiting = await invoke<Array<Partial<ChoiceEvent> & { run_id: string; conversation_id?: string | null }>>(
      'syn_waiting',
      { vaultPath },
    );
    const next: Record<string, ChoiceEvent> = {};
    for (const q of Array.isArray(waiting) ? waiting : []) {
      if (q.choice && q.conversation_id) next[q.conversation_id] = { run_id: q.run_id, conversation_id: q.conversation_id, choice: q.choice };
    }
    questions.value = next;
  } catch (e) {
    logger.error('[Syn] Could not read the which-one questions still waiting', e);
  }
};

const listenOnce = () => {
  if (listening) return;
  listening = true;
  listen<ChoiceEvent>('syn-choice-needed', event => {
    const id = event.payload?.conversation_id;
    if (id) questions.value = { ...questions.value, [id]: event.payload };
  }).catch(e => {
    listening = false;
    logger.error('[Syn] Could not listen for which-one questions', e);
  });
};

export function useSynChoice(vaultPath: () => string) {
  const first = !listening;
  listenOnce();
  // After setup, not during it: the path usually comes from props that are
  // not there yet while the caller is still being set up.
  if (first) void Promise.resolve().then(() => refreshWaitingChoices(vaultPath()));
  const error = ref<string | null>(null);

  /** The question, if it belongs to this conversation. See `useSynConsent`. */
  const pendingIn = (conversationId: string | null | undefined): ChoiceEvent | null =>
    (conversationId && questions.value[conversationId]) || null;

  /** The newest question anywhere; answering goes by conversation. */
  const pending = computed<ChoiceEvent | null>(() => Object.values(questions.value).slice(-1)[0] ?? null);

  /**
   * Say which one, and put the card away.
   *
   * Returns the title so the caller can put it in the composer. Answering
   * records the decision; **sending** is still the person's move, exactly as it
   * is for consent — work restarting while somebody is still reading why it
   * stopped is what the card is arranged to prevent.
   */
  const answer = async (nodeId: string, conversationId: string | null | undefined): Promise<string | null> => {
    const asked = pendingIn(conversationId);
    if (!asked) return null;
    error.value = null;
    const named =
      asked.choice.candidates.find(c => c.id === nodeId)?.title ?? nodeId;
    try {
      await invoke('syn_answer_choice', {
        vaultPath: vaultPath(),
        runId: asked.run_id,
        nodeId,
      });
    } catch (e) {
      logger.error('[Syn] Could not record which one was meant', e);
      error.value = errorText(e);
      return null;
    }
    const { [asked.conversation_id as string]: _answered, ...rest } = questions.value;
    questions.value = rest;
    return named;
  };

  return { pending, pendingIn, error, answer, refresh: () => refreshWaitingChoices(vaultPath()) };
}
