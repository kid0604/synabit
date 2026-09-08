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
   * Answer, put the card away, and say whether the work should carry on.
   *
   * It used to stop here, and leave the person to type *go on*. That was
   * wrong, and it is the bug this returns a value for: pressing **Just this
   * once** *is* saying go on, and being asked to then say it again in words is
   * being asked the same question twice. What it looked like from outside was
   * a question, an answer, and silence.
   *
   * True for a refusal as well. A `Never` is still an answer, and Syn carrying
   * on to say *then I cannot look that up* is better than Syn saying nothing —
   * the tool comes back refused and the sentence is the model's to write. See
   * `consent::Decision::Refuse`.
   *
   * False only when there was nothing to answer: already answered, or answered
   * in another window.
   */
  const answer = async (choice: ConsentAnswer): Promise<boolean> => {
    const asked = pending.value;
    if (!asked) return false;
    error.value = null;
    try {
      const wasAsked = await invoke<boolean>('syn_answer_consent', {
        vaultPath: vaultPath(),
        runId: asked.run_id,
        answer: choice,
      });
      pending.value = null;
      return wasAsked;
    } catch (e) {
      logger.error('[Syn] Could not record the answer', e);
      error.value = (e as { message?: string })?.message ?? String(e);
      return false;
    }
  };

  return { pending, error, answer };
}

/**
 * The i18n key for a capability, as a short label rather than a question.
 *
 * `askPhrase` returns the consent sentence — *"Syn wants to read your vault."*
 * — which is right on a card that is asking and wrong in a catalogue, where
 * nothing is being asked and twenty-five of them would read as twenty-five
 * pending questions.
 *
 * Same keying, same reason: the sentence comes from i18n rather than the
 * English `describe()` composed in Rust, because this app is bilingual.
 *
 * The unscoped net read — a `browse` that has no host yet because nothing has
 * been searched for — is handled once, in `askPhrase`, and arrives here as
 * `cap_netread_any` through the rename below. One rule, in one place: two
 * copies of it drifted once already, and the half that was missing was the
 * half a person reads before granting a permission.
 */
export const capabilityLabel = (
  capability: ConsentAsk['capability'],
): { key: string; values: Record<string, string> } => {
  const { key, values } = askPhrase(capability);
  return { key: key.replace('syn.consent_', 'syn.cap_'), values };
};

export const askPhrase = (
  capability: ConsentAsk['capability'],
): { key: string; values: Record<string, string> } => {
  // `Browse`, `VaultRead`, `Execute` — the ones with nothing to fill in. The
  // browser used to arrive here as a `NetRead` with an empty host, which is how
  // the card once read "Syn wants to read from ." What it asks now is the
  // question a person can actually answer: may Syn use the browser. See
  // `consent::Capability::Browse`.
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
