/**
 * The question Syn stops on, and the answer to it.
 *
 * A card in the conversation, not a modal. The roadmap is specific about this
 * and the reason is good: a modal arrives over whatever the person was reading,
 * demands an answer before they can look at anything, and trains them to click
 * the button that makes it go away. A card sits where the work is, next to the
 * thing it is about, and can be left alone.
 *
 * Only one question is held at a time. A run stops on the first thing it needs
 * permission for, so a second question can only exist if a second run asked —
 * and two cards competing for one decision is how somebody answers the wrong
 * one.
 */
import { onMounted, onUnmounted, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { logger } from '../../../utils/logger';
import type { ConsentAnswer, ConsentAsk } from '../types';

interface ConsentEvent {
  run_id: string;
  conversation_id?: string | null;
  ask: ConsentAsk;
}

export function useSynConsent(vaultPath: () => string) {
  const pending = ref<ConsentEvent | null>(null);
  const error = ref<string | null>(null);

  let stop: UnlistenFn | null = null;

  onMounted(async () => {
    try {
      stop = await listen<ConsentEvent>('syn-consent-needed', event => {
        pending.value = event.payload;
      });
    } catch (e) {
      logger.error('[Syn] Could not listen for consent questions', e);
    }
  });

  onUnmounted(() => {
    stop?.();
    stop = null;
  });

  /**
   * Answer, and put the card away.
   *
   * It does not restart the run. The person is in a conversation and the
   * natural way to say "go on" is to say it — resuming behind their back would
   * mean work starting again while they are still reading why it stopped.
   */
  const answer = async (choice: ConsentAnswer) => {
    const asked = pending.value;
    if (!asked) return;
    error.value = null;
    try {
      await invoke('syn_answer_consent', {
        vaultPath: vaultPath(),
        runId: asked.run_id,
        answer: choice,
      });
      pending.value = null;
    } catch (e) {
      logger.error('[Syn] Could not record the answer', e);
      error.value = (e as { message?: string })?.message ?? String(e);
    }
  };

  return { pending, error, answer };
}

/**
 * The i18n key for a capability, and the values its sentence needs.
 *
 * Keyed on the variant rather than shown as the English `about` string, because
 * this app is bilingual and a permission prompt is the last place to fall back
 * to the wrong language. Exported so it can be tested without mounting
 * anything.
 */
export const askPhrase = (
  capability: ConsentAsk['capability'],
): { key: string; values: Record<string, string> } => {
  if (typeof capability === 'string') {
    return { key: `syn.consent_${capability.toLowerCase()}`, values: {} };
  }
  if ('NetRead' in capability) {
    return { key: 'syn.consent_netread', values: { domain: capability.NetRead.domain } };
  }
  if ('NetWrite' in capability) {
    return {
      key: 'syn.consent_netwrite',
      values: { domain: capability.NetWrite.domain, tool: capability.NetWrite.tool },
    };
  }
  return {
    key: 'syn.consent_spend',
    values: { amount: (capability.Spend.cents_estimate / 100).toFixed(2) },
  };
};
