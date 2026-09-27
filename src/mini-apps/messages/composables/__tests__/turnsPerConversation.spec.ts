import { describe, it, expect, vi, beforeEach } from 'vitest';
import { mount, flushPromises } from '@vue/test-utils';
import { defineComponent, h } from 'vue';

/**
 * A turn belongs to its conversation, and its listeners belong to the turn.
 *
 * Run, not read: the bugs this guards (review §5, U1–U3) were all about what
 * happens *between* calls — a switch mid-answer, a close mid-answer, a stop
 * with nothing named — and no reading of the source can see an ordering.
 */

type Handler = (event: { payload: any }) => void;
const handlers = new Map<string, Set<Handler>>();
const unlistened: string[] = [];

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (name: string, handler: Handler) => {
    if (!handlers.has(name)) handlers.set(name, new Set());
    handlers.get(name)!.add(handler);
    return () => {
      unlistened.push(name);
      handlers.get(name)?.delete(handler);
    };
  }),
}));

const replies = new Map<string, (value: unknown) => void>();
const invoke = vi.fn(async (command: string, args?: any): Promise<unknown> => {
  if (command === 'syn_send_message') {
    return new Promise((resolve) => replies.set(args.request.conversation_id, resolve));
  }
  return undefined;
});
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: [string, any]) => invoke(...a) }));
vi.mock('../../../../utils/logger', () => ({
  logger: { error: vi.fn(), warn: vi.fn(), info: vi.fn(), debug: vi.fn() },
}));

import { useSynChat } from '../useSynChat';

const emit = (name: string, payload: unknown) =>
  handlers.get(name)?.forEach((handler) => handler({ payload }));

const listening = () => [...handlers.values()].reduce((n, set) => n + set.size, 0);

const harness = () => {
  let chat!: ReturnType<typeof useSynChat>;
  const wrapper = mount(
    defineComponent({
      setup() {
        chat = useSynChat();
        return () => h('div');
      },
    }),
  );
  return { wrapper, chat };
};

beforeEach(() => {
  handlers.clear();
  replies.clear();
  unlistened.length = 0;
  invoke.mockClear();
});

describe('a turn in another conversation', () => {
  it('is not shown on this one, and is still running when shown again', async () => {
    const { chat } = harness();
    chat.show('A');
    const answer = chat.sendMessage('/vault', 'A', 'hello');
    await flushPromises();
    expect(chat.isStreaming.value).toBe(true);

    // Somebody opens B while A is answering.
    chat.show('B');
    emit('syn-stream-token', { conversation_id: 'A', message_id: 'm', token: 'Chào', done: false });
    emit('syn-tempo', { conversation_id: 'A', tempo: 'deep' });

    expect(chat.streamingContent.value, 'A’s words must not appear in B').toBe('');
    expect(chat.isStreaming.value, 'and B is not busy').toBe(false);
    expect(chat.isRunning('B')).toBe(false);

    // Back to A: the run is still going, with what it said while away.
    chat.show('A');
    expect(chat.isStreaming.value).toBe(true);
    expect(chat.isRunning('A')).toBe(true);
    expect(chat.streamingContent.value).toBe('Chào');
    expect(chat.tempo.value).toBe('deep');

    replies.get('A')!({ id: 'r', role: 'assistant', content: 'Chào', timestamp: '' });
    await answer;
    expect(chat.isRunning('A')).toBe(false);
  });

  it('keeps its error to itself', async () => {
    invoke.mockImplementationOnce(async () => {
      throw new Error('provider down');
    });
    const { chat } = harness();
    chat.show('B');
    await chat.sendMessage('/vault', 'A', 'hello');
    expect(chat.error.value).toBeNull();
    chat.show('A');
    expect(chat.error.value).toBe('provider down');
  });
});

describe('putting a turn away', () => {
  it('drops the listeners, not only the words', async () => {
    const { chat } = harness();
    chat.show('A');
    const answer = chat.sendMessage('/vault', 'A', 'hello');
    await flushPromises();
    expect(listening()).toBeGreaterThan(0);

    replies.get('A')!({ id: 'r', role: 'assistant', content: 'x', timestamp: '' });
    await answer;
    chat.clearStreaming('A');

    expect(listening(), 'every listener of the turn is gone').toBe(0);
    expect(unlistened).toContain('syn-stream-token');
    expect(unlistened).toContain('syn-tempo');
  });

  it('drops the listeners of a turn stopped mid-answer', async () => {
    const { chat } = harness();
    chat.show('A');
    void chat.sendMessage('/vault', 'A', 'hello');
    await flushPromises();
    await chat.stopGeneration('A');

    expect(listening()).toBe(0);
    // A token that arrives late is heard by nobody.
    emit('syn-stream-token', { conversation_id: 'A', message_id: 'm', token: 'late', done: false });
    expect(chat.streamingContent.value).toBe('');
  });

  it('drops everything when the screen goes', async () => {
    const { chat, wrapper } = harness();
    void chat.sendMessage('/vault', 'A', 'a');
    void chat.sendMessage('/vault', 'B', 'b');
    await flushPromises();
    wrapper.unmount();
    expect(listening()).toBe(0);
  });
});

describe('stopping', () => {
  /**
   * The backend reads a missing conversation as *stop every run* — routines
   * and Telegram included. Nothing on screen means nothing to stop.
   */
  it('never asks the backend to stop without naming a conversation', async () => {
    const { chat } = harness();
    await chat.stopGeneration();
    await chat.stopGeneration(null);
    expect(invoke.mock.calls.filter(([c]) => c === 'syn_stop_generation')).toHaveLength(0);
  });

  it('names the conversation it stops', async () => {
    const { chat } = harness();
    chat.show('A');
    await chat.stopGeneration();
    expect(invoke).toHaveBeenCalledWith('syn_stop_generation', { conversationId: 'A' });
  });
});
