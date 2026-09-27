import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { shallowMount, flushPromises, type VueWrapper } from '@vue/test-utils';
import { i18n } from '../../../i18n';

/**
 * The Messages screen, run rather than read, around a turn that is still
 * answering.
 *
 * Review §5, U1: an answer landing in whichever conversation is open. It is
 * about what happens between calls, which is exactly what the tests that read
 * `MessagesApp.vue` as text cannot see.
 *
 * Children are stubbed: this is about the screen's own wiring. The panel is
 * driven through the events it emits and read through the props it is given.
 */

type Handler = (event: { payload: any }) => void;

// Hoisted: `shared/syn/pane` calls the backend as it is imported, before
// anything declared in this file would exist.
const { handlers, replies, invoke } = vi.hoisted(() => {
  const handlers = new Map<string, Set<Handler>>();
  /** Each conversation's pending answer, resolved by the test. */
  const replies = new Map<string, (value: unknown) => void>();
  const conversation = (id: string) => ({
    id,
    title: id,
    message_count: 0,
    created_at: '',
    updated_at: '',
    pinned: false,
  });
  const invoke = vi.fn(async (command: string, args?: any): Promise<unknown> => {
    switch (command) {
      case 'syn_list_conversations':
        return [conversation('A'), conversation('B')];
      case 'syn_get_conversation':
        return { meta: conversation(args.conversationId), messages: [] };
      case 'syn_send_message':
        return new Promise((resolve) => replies.set(args.request.conversation_id, resolve));
      case 'syn_check_status':
        return { connected: false, version: null, url: '' };
      case 'syn_get_settings':
        return { enabled: true };
      case 'syn_stop_generation':
        return undefined;
      default:
        return [];
    }
  });
  return { handlers, replies, invoke };
});

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (name: string, handler: Handler) => {
    if (!handlers.has(name)) handlers.set(name, new Set());
    handlers.get(name)!.add(handler);
    return () => handlers.get(name)?.delete(handler);
  }),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke,
  convertFileSrc: (p: string) => p,
}));
vi.mock('../../../utils/logger', () => ({
  logger: { error: vi.fn(), warn: vi.fn(), info: vi.fn(), debug: vi.fn() },
}));
vi.mock('../../../composables/useNodeService', () => ({
  useNodeService: () => ({ writeNode: vi.fn(), trashNode: vi.fn() }),
}));

import MessagesApp from '../MessagesApp.vue';
import ChatPanel from '../components/ChatPanel.vue';

let wrapper: VueWrapper<any> | null = null;

const open = async () => {
  wrapper = shallowMount(MessagesApp, {
    props: { vaultPath: '/vault' },
    global: { plugins: [i18n] },
    attachTo: document.body,
  });
  await flushPromises();
  return wrapper;
};

const panel = () => wrapper!.findComponent(ChatPanel);
const answer = (id: string, content: string) =>
  replies.get(id)!({ id: `r-${id}`, role: 'assistant', content, timestamp: '' });

const memoryStorage = (): Storage => {
  const held = new Map<string, string>();
  return {
    get length() {
      return held.size;
    },
    clear: () => held.clear(),
    getItem: (k) => held.get(k) ?? null,
    key: (i) => [...held.keys()][i] ?? null,
    removeItem: (k) => void held.delete(k),
    setItem: (k, v) => void held.set(k, String(v)),
  };
};

beforeEach(() => {
  // Node's own `localStorage` shadows jsdom's and is absent without a file.
  vi.stubGlobal('localStorage', memoryStorage());
  handlers.clear();
  replies.clear();
  invoke.mockClear();
});

afterEach(() => {
  wrapper?.unmount();
  wrapper = null;
  vi.unstubAllGlobals();
});

describe('an answer that arrives after somebody moved on', () => {
  it('lands in its own conversation, not the one on screen', async () => {
    await open();
    expect(panel().props('isStreaming')).toBe(false);

    // A is open (the latest), and a question goes into it.
    panel().vm.$emit('send', 'about A');
    await flushPromises();
    expect(panel().props('isStreaming')).toBe(true);

    // Somebody opens B before A has answered.
    await wrapper!.vm.openConversation('B');
    await flushPromises();
    expect(panel().props('isStreaming'), 'B has nothing running').toBe(false);

    for (const handler of handlers.get('syn-stream-token') ?? []) {
      handler({ payload: { conversation_id: 'A', message_id: 'm', token: 'A says', done: false } });
    }
    await flushPromises();
    expect(panel().props('streamingContent'), 'A’s stream is not drawn in B').toBe('');

    answer('A', 'the answer to A');
    await flushPromises();

    const inB = panel().props('messages') as Array<{ content: string }>;
    expect(inB.map((m) => m.content)).not.toContain('the answer to A');
  });

  it('is still running when its conversation is opened again', async () => {
    await open();
    panel().vm.$emit('send', 'about A');
    await flushPromises();

    await wrapper!.vm.openConversation('B');
    await flushPromises();
    await wrapper!.vm.openConversation('A');
    await flushPromises();

    expect(panel().props('isStreaming'), 'A shows its run, not an idle composer').toBe(true);

    // And a second turn cannot be sent into A beside the first.
    panel().vm.$emit('send', 'again');
    await flushPromises();
    const sends = invoke.mock.calls.filter(([c]) => c === 'syn_send_message');
    expect(sends).toHaveLength(1);

    answer('A', 'done');
    await flushPromises();
    expect(panel().props('isStreaming')).toBe(false);
    const inA = panel().props('messages') as Array<{ content: string }>;
    expect(inA.map((m) => m.content)).toContain('done');
  });
});
