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
import { onMounted, onUnmounted, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { logger } from '../../../utils/logger';
import type { AmbiguousChoice } from '../types';

interface ChoiceEvent {
  run_id: string;
  conversation_id?: string | null;
  choice: AmbiguousChoice;
}

export function useSynChoice(vaultPath: () => string) {
  const pending = ref<ChoiceEvent | null>(null);
  const error = ref<string | null>(null);

  let stop: UnlistenFn | null = null;

  onMounted(async () => {
    try {
      stop = await listen<ChoiceEvent>('syn-choice-needed', event => {
        pending.value = event.payload;
      });
    } catch (e) {
      logger.error('[Syn] Could not listen for which-one questions', e);
    }
  });

  onUnmounted(() => {
    stop?.();
    stop = null;
  });

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

  return { pending, error, answer };
}
