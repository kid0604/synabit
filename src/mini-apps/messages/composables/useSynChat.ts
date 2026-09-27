import { ref, reactive, computed, onUnmounted } from 'vue';
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
 * One turn in flight, or finished and not yet put away.
 *
 * Kept per conversation, because a turn belongs to its conversation and not to
 * whichever one is on screen. This composable used to hold one set of refs for
 * the whole screen: switch from A to B while A was answering, and A's state
 * was either shown in B or — once `clearStreaming` ran on the switch — thrown
 * away, so going back to A showed it idle and a second turn could be sent into
 * it beside the first.
 */
interface Turn {
  content: string;
  messageId: string | null;
  /** Whether the run is still going: from sending until it answers. */
  streaming: boolean;
  toolCalls: SynToolCallEvent[];
  tempo: Tempo | null;
  plan: PlanStep[];
  progress: RunProgress | null;
  error: string | null;
  /** This turn's own listeners, dropped with it. */
  stops: UnlistenFn[];
  /** Put away while its listeners were still being attached. */
  dropped: boolean;
}

const freshTurn = (): Turn => ({
  content: '',
  messageId: null,
  streaming: true,
  toolCalls: [],
  tempo: null,
  plan: [],
  progress: null,
  error: null,
  stops: [],
  dropped: false,
});

/**
 * One turn with Syn, from sending to the answer, for whichever screen asked.
 *
 * Messages and the ask bar both use this. The ask bar used to keep its own
 * copy of the streaming, tempo and tool listening, and that copy is why it
 * never learned about permission cards or plans: every new thing a turn could
 * say had to be taught to two places, and was taught to one.
 *
 * The refs it returns show **one** conversation's turn — the one named by
 * `show`. Turns in other conversations keep running and keep listening; they
 * are just not what the refs are showing.
 */
export function useSynChat() {
  /** Every conversation with a turn this screen started and has not put away. */
  const turns = reactive(new Map<string, Turn>());
  /** The conversation on screen: what the refs below show. See `show`. */
  const shown = ref<string | null>(null);
  const current = computed<Turn | undefined>(() =>
    shown.value ? turns.get(shown.value) : undefined,
  );

  const streamingContent = computed(() => current.value?.content ?? '');
  const streamingMessageId = computed(() => current.value?.messageId ?? null);
  const isStreaming = computed(() => current.value?.streaming ?? false);
  const toolCalls = computed<SynToolCallEvent[]>(() => current.value?.toolCalls ?? []);
  /**
   * How heavy this turn was judged to be, before it started.
   *
   * Emitted by the backend the moment the tempo is decided, which is the point:
   * somebody who is about to wait should be told they are about to wait. A
   * count answered from the index shows a different indicator from a question
   * that is going to take four rounds — the same dots for both is what makes a
   * fast answer feel slow and a slow one feel broken.
   */
  const tempo = computed<Tempo | null>(() => current.value?.tempo ?? null);
  const error = computed<string | null>(() => current.value?.error ?? null);
  /** Whether the last refusal was the switch rather than a fault. */
  const switchedOff = ref(false);
  /** The run's own list of steps, as it last wrote it. See `update_plan`. */
  const plan = computed<PlanStep[]>(() => current.value?.plan ?? []);
  /** How far the run has got, against its ceilings. See `syn-progress`. */
  const progress = computed<RunProgress | null>(() => current.value?.progress ?? null);

  /** Show this conversation's turn, if it has one. Nothing is stopped. */
  const show = (conversationId: string | null | undefined) => {
    shown.value = conversationId ?? null;
  };

  /** Whether a turn in this conversation is still running. */
  const isRunning = (conversationId: string | null | undefined): boolean =>
    !!conversationId && !!turns.get(conversationId)?.streaming;

  /**
   * Put one conversation's turn away: its words, its state, and its listeners.
   *
   * The listeners are the part that used to be missed. They were only dropped
   * when the next turn started or the screen unmounted, and the ask bar is
   * mounted for the whole session — so it went on hearing a turn it had closed.
   */
  const drop = (conversationId: string) => {
    const turn = turns.get(conversationId);
    if (!turn) return;
    turn.dropped = true;
    turn.stops.forEach(stop => stop());
    turn.stops = [];
    turns.delete(conversationId);
  };

  const setupListener = async (conversationId: string, turn: Turn) => {
    const mine = (id: string | null | undefined) => id === conversationId;
    const keep = (stop: UnlistenFn) => {
      // Put away while this was still being attached: nothing would ever drop
      // it later, so it goes now.
      if (turn.dropped) stop();
      else turn.stops.push(stop);
    };

    try {
      keep(await listen<SynStreamToken>('syn-stream-token', (event) => {
        const token = event.payload;
        if (!mine(token.conversation_id)) return;

        if (token.done) {
          // The words have all arrived. The content is kept: the caller puts
          // it away once it has picked up the final assembled message.
          turn.streaming = false;
          turn.messageId = null;
        } else {
          turn.messageId = token.message_id;
          turn.content += token.token;
        }
      }));

      keep(await listen<{ conversation_id: string; tempo: Tempo }>(
        'syn-tempo',
        (event) => {
          if (!mine(event.payload.conversation_id)) return;
          turn.tempo = event.payload.tempo;
        },
      ));

      keep(await listen<SynToolCallEvent>('syn-tool-call', (event) => {
        if (!mine(event.payload.conversation_id)) return;
        turn.toolCalls.push(event.payload);
      }));

      keep(await listen<{ conversation_id?: string | null; plan: PlanStep[] }>('syn-plan', (event) => {
        if (!mine(event.payload.conversation_id)) return;
        turn.plan = event.payload.plan;
      }));

      keep(await listen<RunProgress>('syn-progress', (event) => {
        if (!mine(event.payload.conversation_id)) return;
        turn.progress = event.payload;
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
    // A fresh turn, and with it a fresh tempo (`tempo: null`), no tools, no
    // plan and no progress: nothing from the last turn carries over.
    drop(conversationId);
    turns.set(conversationId, freshTurn());
    // Read back through the map, so the writes below go through the proxy.
    const turn = turns.get(conversationId)!;

    // Setup listener before sending
    await setupListener(conversationId, turn);

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
      turn.streaming = false;
      // The run is over and nothing more is coming for it. What it said stays
      // until the turn is put away; its listeners do not.
      turn.stops.forEach(stop => stop());
      turn.stops = [];
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
      turn.error = said;
      turn.streaming = false;
      turn.content = '';
      turn.messageId = null;
      // Nothing more is coming. The error stays, to be shown until the turn is
      // put away; the listeners do not.
      turn.stops.forEach(stop => stop());
      turn.stops = [];
      return null;
    }
  };

  /**
   * Stop one conversation's run — the one on screen, unless another is named.
   *
   * Never without an id. The backend reads a missing conversation as *stop
   * every run*, routines and Telegram included, which is not what pressing
   * Escape over one conversation means. No conversation, nothing to stop.
   */
  const stopGeneration = async (conversationId?: string | null) => {
    const id = conversationId ?? shown.value;
    if (!id) return;
    try {
      await invoke('syn_stop_generation', { conversationId: id });
    } catch (e) {
      logger.error('[Syn] Failed to stop generation', e);
    } finally {
      drop(id);
    }
  };

  /**
   * Put a turn away once its answer is in hand — the one on screen, unless
   * another conversation is named. Its listeners go with it.
   *
   * A turn that is still running is left alone: that is a later turn in the
   * same conversation, started after this answer's run was stopped, and it is
   * `stopGeneration` that puts a running turn away.
   */
  const clearStreaming = (conversationId?: string | null) => {
    const id = conversationId ?? shown.value;
    if (id && !isRunning(id)) drop(id);
  };

  // Every listener of every turn, the tempo's included.
  onUnmounted(() => {
    for (const id of [...turns.keys()]) drop(id);
  });

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
    show,
    isRunning,
    sendMessage,
    stopGeneration,
    clearStreaming,
  };
}
