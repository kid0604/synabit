/**
 * The question Syn stops on, and the answer to it.
 *
 * A card in the conversation, not a modal. The roadmap is specific about this
 * and the reason is good: a modal arrives over whatever the person was reading,
 * demands an answer before they can look at anything, and trains them to click
 * the button that makes it go away. A card sits where the work is, next to the
 * thing it is about, and can be left alone.
 *
 * One question per conversation. A run stops on the first thing it needs
 * permission for, so a conversation has at most one; but two conversations —
 * a routine at 7:00 and the one somebody is typing in — can each be waiting,
 * and a single slot let the second question overwrite the first.
 *
 * And the questions are read back from disk (`syn_waiting`), not only heard as
 * events. An event is gone once sent: a question asked while nobody had
 * Messages open, or before the app restarted, used to be listed as "waiting for
 * you" with no card anywhere to answer it.
 */
import { computed, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { logger } from '../../../utils/logger';
import type { ConsentAnswer, ConsentAsk } from '../types';
import { errorText } from '../../../shared/errorText';

export interface ConsentEvent {
  run_id: string;
  conversation_id?: string | null;
  ask: ConsentAsk;
}

/**
 * The question on the table, held once for the whole app.
 *
 * Module-scoped rather than per component, and that is the fix for two bugs.
 * The listener used to live in whichever screen called this, which was only
 * Messages — so a question asked from the ask bar stopped for permission, the
 * run ended with an empty reply, and the card that could have answered it was
 * on a screen nobody was looking at, or on no screen at all. And each caller
 * holding its own copy would let two surfaces disagree about whether the
 * question had been answered.
 *
 * One store, one listener, many places that can show the card. Which of them
 * does is decided by `conversation_id` — see `pendingIn`.
 */
const questions = ref<Record<string, ConsentEvent>>({});
let listening = false;

/**
 * Take in every question still waiting on disk. Ones already held stay as
 * they are; ones answered elsewhere since are dropped.
 */
export const refreshWaiting = async (vaultPath: string) => {
  if (!vaultPath) return;
  try {
    const waiting = await invoke<Array<Partial<ConsentEvent> & { run_id: string; conversation_id?: string | null }>>(
      'syn_waiting',
      { vaultPath },
    );
    const next: Record<string, ConsentEvent> = {};
    for (const q of Array.isArray(waiting) ? waiting : []) {
      if (q.ask && q.conversation_id) next[q.conversation_id] = { run_id: q.run_id, conversation_id: q.conversation_id, ask: q.ask };
    }
    questions.value = next;
  } catch (e) {
    logger.error('[Syn] Could not read the questions still waiting', e);
  }
};

/**
 * Start listening, the first time anybody asks.
 *
 * Never stopped: the store outlives any one screen by design, and a question
 * that arrives while Messages is closed still has to be there when it opens.
 */
const listenOnce = () => {
  if (listening) return;
  listening = true;
  listen<ConsentEvent>('syn-consent-needed', event => {
    const id = event.payload?.conversation_id;
    if (id) questions.value = { ...questions.value, [id]: event.payload };
  }).catch(e => {
    // Allowed to try again on the next call rather than staying deaf for the
    // rest of the session.
    listening = false;
    logger.error('[Syn] Could not listen for consent questions', e);
  });
};

export function useSynConsent(vaultPath: () => string) {
  const first = !listening;
  listenOnce();
  // After setup, not during it: the path usually comes from props that are
  // not there yet while the caller is still being set up.
  if (first) void Promise.resolve().then(() => refreshWaiting(vaultPath()));
  /** Why the last answer did not go through. Shown on the card. */
  const error = ref<string | null>(null);
  /** An answer is on its way. The card's buttons wait for it. */
  const answering = ref(false);
  // A new question starts with a clean card, not the last one's failure.
  watch(questions, () => { error.value = null; });

  /** The newest question anywhere, for a badge; answering goes by conversation. */
  const pending = computed<ConsentEvent | null>(() => Object.values(questions.value).slice(-1)[0] ?? null);

  /**
   * The question, if it belongs to this conversation.
   *
   * A card belongs to the conversation whose run stopped. Shown in whichever
   * one happened to be open, it was answered there and the work carried on
   * there too — a permission granted for one exchange spent on another. A
   * question with no conversation is shown nowhere in one, because there is
   * nowhere in one to carry it on.
   */
  const pendingIn = (conversationId: string | null | undefined): ConsentEvent | null =>
    (conversationId && questions.value[conversationId]) || null;

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
  const answer = async (choice: ConsentAnswer, conversationId: string | null | undefined): Promise<boolean> => {
    const asked = pendingIn(conversationId);
    // One answer at a time: a second press while the first is on its way is
    // not a second decision.
    if (!asked || answering.value) return false;
    error.value = null;
    answering.value = true;
    try {
      const wasAsked = await invoke<boolean>('syn_answer_consent', {
        vaultPath: vaultPath(),
        runId: asked.run_id,
        answer: choice,
      });
      const { [asked.conversation_id as string]: _answered, ...rest } = questions.value;
      questions.value = rest;
      return wasAsked;
    } catch (e) {
      logger.error('[Syn] Could not record the answer', e);
      error.value = errorText(e);
      return false;
    } finally {
      answering.value = false;
    }
  };

  return { pending, pendingIn, error, answering, answer, refresh: () => refreshWaiting(vaultPath()) };
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
  if ('UseSecret' in capability) {
    const { item, destination, label } = capability.UseSecret;
    return {
      key: 'syn.consent_usesecret',
      values: { item, destination: label || destination.replace(/^connector:/, '') },
    };
  }
  return {
    key: 'syn.consent_spend',
    values: { amount: (capability.Spend.cents_estimate / 100).toFixed(2) },
  };
};
