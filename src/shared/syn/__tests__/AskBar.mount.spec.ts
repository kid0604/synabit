import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount, flushPromises, type VueWrapper } from '@vue/test-utils';
import { i18n } from '../../../i18n';

/**
 * The ask bar, mounted, around what the review found (§5, U2): closing it
 * mid-answer.
 */

type Handler = (event: { payload: any }) => void;

const { handlers, replies, invoke } = vi.hoisted(() => {
  const handlers = new Map<string, Set<Handler>>();
  const replies: Array<(value: unknown) => void> = [];
  let made = 0;
  const invoke = vi.fn(async (command: string, _args?: unknown): Promise<unknown> => {
    switch (command) {
      case 'syn_create_conversation':
        made += 1;
        return { id: `conv-${made}` };
      case 'syn_send_message':
        return new Promise((resolve) => replies.push(resolve));
      default:
        return [];
    }
  });
  return { handlers, replies, invoke };
});

vi.mock('@tauri-apps/api/core', () => ({ invoke, convertFileSrc: (p: string) => p }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (name: string, handler: Handler) => {
    if (!handlers.has(name)) handlers.set(name, new Set());
    handlers.get(name)!.add(handler);
    return () => handlers.get(name)?.delete(handler);
  }),
}));
vi.mock('../../../utils/logger', () => ({
  logger: { error: vi.fn(), warn: vi.fn(), info: vi.fn(), debug: vi.fn() },
}));

import AskBar from '../AskBar.vue';

let wrapper: VueWrapper<any> | null = null;

const openBar = async () => {
  wrapper = mount(AskBar, {
    props: { open: false, vaultPath: '/vault' },
    global: { plugins: [i18n] },
    attachTo: document.body,
  });
  await wrapper.setProps({ open: true });
  await flushPromises();
  return wrapper;
};

const called = (command: string) => invoke.mock.calls.filter(([c]) => c === command);
const streamListeners = () => handlers.get('syn-stream-token')?.size ?? 0;

const press = async (init: KeyboardEventInit & { keyCode?: number }) => {
  const textarea = wrapper!.find('textarea');
  const event = new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init });
  // jsdom's constructor ignores `keyCode`; WebKit sends 229 during composition.
  if (init.keyCode !== undefined) Object.defineProperty(event, 'keyCode', { value: init.keyCode });
  textarea.element.dispatchEvent(event);
  await flushPromises();
};

beforeEach(() => {
  handlers.clear();
  replies.length = 0;
  invoke.mockClear();
});

afterEach(() => {
  wrapper?.unmount();
  wrapper = null;
});

describe('closing the bar mid-answer', () => {
  it('stops that run, by name, and stops listening to it', async () => {
    await openBar();
    await wrapper!.find('textarea').setValue('what is on today?');
    await press({ key: 'Enter' });
    expect(streamListeners()).toBeGreaterThan(0);

    await wrapper!.setProps({ open: false });
    await flushPromises();

    const asked = (called('syn_send_message')[0][1] as any).request.conversation_id;
    expect(asked).toMatch(/^conv-/);
    expect(called('syn_stop_generation')).toEqual([
      ['syn_stop_generation', { conversationId: asked }],
    ]);
    expect(streamListeners(), 'the closed exchange is heard by nobody').toBe(0);
  });

  it('does not show the old answer when it opens again', async () => {
    await openBar();
    await wrapper!.find('textarea').setValue('first question');
    await press({ key: 'Enter' });

    await wrapper!.setProps({ open: false });
    await flushPromises();
    await wrapper!.setProps({ open: true });
    await flushPromises();

    // The old run's reply arrives after all.
    replies[0]({ id: 'r', role: 'assistant', content: 'THE OLD ANSWER', timestamp: '' });
    await flushPromises();

    expect(wrapper!.html()).not.toContain('THE OLD ANSWER');
    // And the bar is free for a new question, not stuck busy on the old one.
    await wrapper!.find('textarea').setValue('second question');
    await press({ key: 'Enter' });
    expect(called('syn_send_message')).toHaveLength(2);
  });
});
