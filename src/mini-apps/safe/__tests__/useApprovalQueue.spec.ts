import { describe, it, expect, vi, beforeEach } from 'vitest';
import { defineComponent, h } from 'vue';
import { mount, flushPromises } from '@vue/test-utils';

const handlers = new Map<string, (e: { payload: unknown }) => void>();
const invoke = vi.fn(async () => undefined);
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...(a as [])) }));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (name: string, handler: (e: { payload: unknown }) => void) => {
    handlers.set(name, handler);
    return () => handlers.delete(name);
  }),
}));

import { useApprovalQueue } from '../useApprovalQueue';

function host() {
  let exposed!: ReturnType<typeof useApprovalQueue<{ id: string; timeout_secs?: number }>>;
  const C = defineComponent({
    setup() {
      exposed = useApprovalQueue('safe://cli-approve');
      return () => h('div');
    },
  });
  mount(C);
  return () => exposed;
}

describe('approval cards', () => {
  beforeEach(() => {
    handlers.clear();
    invoke.mockClear();
    vi.useRealTimers();
  });

  /** Rust has turned every open question into a no by the time the Safe
   *  locks; a card left up would take an Allow that means nothing — or, if
   *  Rust were wrong, one that does. */
  it('takes every card down when the Safe locks', async () => {
    const q = host();
    await flushPromises();
    handlers.get('safe://cli-approve')!({ payload: { id: 'a' } });
    handlers.get('safe://cli-approve')!({ payload: { id: 'b' } });
    expect(q().queue.value.map((c) => c.id)).toEqual(['a', 'b']);
    handlers.get('safe://locked')!({ payload: null });
    expect(q().queue.value).toEqual([]);
  });

  it('takes a card down once its question timed out, and answers the first in line', async () => {
    vi.useFakeTimers();
    const q = host();
    await flushPromises();
    handlers.get('safe://cli-approve')!({ payload: { id: 'short', timeout_secs: 2 } });
    handlers.get('safe://cli-approve')!({ payload: { id: 'long', timeout_secs: 60 } });
    vi.advanceTimersByTime(3000);
    expect(q().queue.value.map((c) => c.id)).toEqual(['long']);
    await q().answer(false);
    expect(invoke).toHaveBeenCalledWith('safe_ssh_answer', { id: 'long', allow: false });
    expect(q().queue.value).toEqual([]);
  });
});
