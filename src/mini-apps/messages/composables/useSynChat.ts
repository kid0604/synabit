import { ref, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { logger } from '../../../utils/logger';
import type { PlanStep, RunProgress, SynStreamToken, SynMessage, SynToolCallEvent, Tempo } from '../types';
import type { SynFocus } from '../../../shared/syn/focus';

/**
 * What the backend says when the switch is off.
 *
 * The literal from `commands::syn::SWITCHED_OFF`. A Rust test reads this file
 * and fails if the two stop matching, because the alternative is silently
 * showing somebody a red error for a choice they made on purpose.
 */
export const SWITCHED_OFF = 'Syn is switched off';

/** How a message is sent, beyond its words. */
export interface SendOptions {
  model?: string;
  temperature?: number;
  images?: string[];
  /**
   * Carry on with what was already asked: the id of a run that stopped for
   * permission and has now been answered. The message is empty then — the
   * backend takes the question from the conversation rather than appending an
   * empty turn, and takes the call it was about to make from that run.
   */
  resumeRun?: string;
  /** Ask again in place of this answer. See `SynChatRequest::replacing`. */
  replacing?: string;
  /**
   * Plan first: look, write the steps down, change nothing until approved.
   * See `Run::plan_only`.
   */
  planOnly?: boolean;
  /** What was on screen when the question was asked. See `focus.ts`. */
  focus?: SynFocus;
}

/**
 * One turn with Syn, from sending to the answer, for whichever screen asked.
 *
 * Messages and the ask bar both use this. The ask bar used to keep its own
 * copy of the streaming, tempo and tool listening, and that copy is why it
 * never learned about permission cards or plans: every new thing a turn could
 * say had to be taught to two places, and was taught to one.
 */
export function useSynChat() {
  const streamingContent = ref('');
  const streamingMessageId = ref<string | null>(null);
  const isStreaming = ref(false);
  const toolCalls = ref<SynToolCallEvent[]>([]);
  const activeConversationId = ref<string | null>(null);
  /**
   * How heavy this turn was judged to be, before it started.
   *
   * Emitted by the backend the moment the tempo is decided, which is the point:
   * somebody who is about to wait should be told they are about to wait. A
   * count answered from the index shows a different indicator from a question
   * that is going to take four rounds — the same dots for both is what makes a
   * fast answer feel slow and a slow one feel broken.
   */
  const tempo = ref<Tempo | null>(null);
  const error = ref<string | null>(null);
  /** Whether the last refusal was the switch rather than a fault. */
  const switchedOff = ref(false);
  /** The run's own list of steps, as it last wrote it. See `update_plan`. */
  const plan = ref<PlanStep[]>([]);
  /** How far the run has got, against its ceilings. See `syn-progress`. */
  const progress = ref<RunProgress | null>(null);

  let stops: UnlistenFn[] = [];
  const stopListening = () => {
    stops.forEach(stop => stop());
    stops = [];
  };

  const setupListener = async (conversationId: string) => {
    stopListening();

    try {
      activeConversationId.value = conversationId;
      stops.push(await listen<SynStreamToken>('syn-stream-token', (event) => {
        const token = event.payload;

        // Only process tokens for the active conversation
        if (token.conversation_id !== conversationId) return;

        if (token.done) {
          // Stream completed
          isStreaming.value = false;
          streamingMessageId.value = null;
          // Don't clear streamingContent here — the parent will handle it
          // after it picks up the final assembled message
        } else {
          streamingMessageId.value = token.message_id;
          streamingContent.value += token.token;
        }
      }));

      stops.push(await listen<{ conversation_id: string; tempo: Tempo }>(
        'syn-tempo',
        (event) => {
          if (event.payload.conversation_id !== conversationId) return;
          tempo.value = event.payload.tempo;
        },
      ));

      stops.push(await listen<SynToolCallEvent>('syn-tool-call', (event) => {
        if (event.payload.conversation_id !== conversationId) return;
        toolCalls.value.push(event.payload);
      }));

      stops.push(await listen<{ conversation_id?: string | null; plan: PlanStep[] }>('syn-plan', (event) => {
        if (event.payload.conversation_id !== conversationId) return;
        plan.value = event.payload.plan;
      }));

      stops.push(await listen<RunProgress>('syn-progress', (event) => {
        if (event.payload.conversation_id !== conversationId) return;
        progress.value = event.payload;
      }));
    } catch (e) {
      logger.error('[Syn] Failed to setup stream listener', e);
    }
  };

  /** Ask, or carry on with what was already asked. See `SendOptions`. */
  const sendMessage = async (
    vaultPath: string,
    conversationId: string,
    message: string,
    options: SendOptions = {},
  ): Promise<SynMessage | null> => {
    const { model, temperature, images, resumeRun, replacing, planOnly, focus } = options;
    error.value = null;
    isStreaming.value = true;
    streamingContent.value = '';
    toolCalls.value = [];
    streamingMessageId.value = null;
    tempo.value = null;
    plan.value = [];
    progress.value = null;

    // Setup listener before sending
    await setupListener(conversationId);

    try {
      const response = await invoke<SynMessage>('syn_send_message', {
        vaultPath,
        request: {
          conversation_id: conversationId,
          message,
          model: model || undefined,
          temperature: temperature || undefined,
          images: images?.length ? images : undefined,
          resume_run: resumeRun || undefined,
          replacing: replacing || undefined,
          plan_only: planOnly || undefined,
          focus,
        },
      });
      return response;
    } catch (e: any) {
      const said = e?.message || String(e);
      // "Syn is off" is not a failure, and logging it as one puts a chosen
      // state in the error log beside real ones. The backend refuses with a
      // fixed sentence — `commands::syn::SWITCHED_OFF`, pinned by a Rust test
      // that reads this file — so the screen can tell a decision apart from a
      // provider that is down, which look identical from here and mean
      // opposite things about whether anything is wrong.
      if (said.includes(SWITCHED_OFF)) {
        logger.info('[Syn] A message was not sent: Syn is switched off');
        switchedOff.value = true;
      } else {
        logger.error('[Syn] Failed to send message', e);
      }
      error.value = said;
      isStreaming.value = false;
      streamingContent.value = '';
      streamingMessageId.value = null;
      return null;
    }
  };

  const stopGeneration = async () => {
    try {
      await invoke('syn_stop_generation', {
        conversationId: activeConversationId.value || undefined,
      });
    } catch (e) {
      logger.error('[Syn] Failed to stop generation', e);
    } finally {
      isStreaming.value = false;
      streamingContent.value = '';
      streamingMessageId.value = null;
      activeConversationId.value = null;
      toolCalls.value = [];
      progress.value = null;
    }
  };

  const clearStreaming = () => {
    streamingContent.value = '';
    streamingMessageId.value = null;
    activeConversationId.value = null;
    isStreaming.value = false;
    toolCalls.value = [];
    plan.value = [];
    progress.value = null;
  };

  // Every listener, the tempo's included — it used to outlive the screen.
  onUnmounted(stopListening);

  return {
    switchedOff,
    streamingContent,
    streamingMessageId,
    isStreaming,
    error,
    toolCalls,
    tempo,
    plan,
    progress,
    sendMessage,
    stopGeneration,
    clearStreaming,
  };
}
