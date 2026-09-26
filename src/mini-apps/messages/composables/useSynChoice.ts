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
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { logger } from '../../../utils/logger';
import type { AmbiguousChoice } from '../types';

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
const pending = ref<ChoiceEvent | null>(null);
let listening = false;

const listenOnce = () => {
  if (listening) return;
  listening = true;
  listen<ChoiceEvent>('syn-choice-needed', event => {
    pending.value = event.payload;
  }).catch(e => {
    listening = false;
    logger.error('[Syn] Could not listen for which-one questions', e);
  });
};

export function useSynChoice(vaultPath: () => string) {
  listenOnce();
  const error = ref<string | null>(null);

  /** The question, if it belongs to this conversation. See `useSynConsent`. */
  const pendingIn = (conversationId: string | null | undefined): ChoiceEvent | null =>
    conversationId && pending.value?.conversation_id === conversationId ? pending.value : null;

  /**
   * Say which one, and put the card away.
   *
   * Returns the title so the caller can put it in the composer. Answering
   * records the decision; **sending** is still the person's move, exactly as it
   * is for consent — work restarting while somebody is still reading why it
   * stopped is what the card is arranged to prevent.
   */
  const answer = async (nodeId: string): Promise<string | null> => {
    const asked = pending.value;
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
      error.value = (e as { message?: string })?.message ?? String(e);
      return null;
    }
    pending.value = null;
    return named;
  };

  return { pending, pendingIn, error, answer };
}
