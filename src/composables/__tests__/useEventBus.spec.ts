import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

/**
 * The wire between the assistant's writes and the screen the user is looking at.
 *
 * Syn's tools run in Rust and emit `node:created` / `node:updated` /
 * `node:deleted` into Tauri. Every app already listens for those three on the
 * frontend bus and reloads. Nothing joined the two, so Syn would create a task,
 * report that it had, and the open Tasks list would not show it until the user
 * navigated away and back — which reads as the assistant lying.
 */

const listeners = new Map<string, (event: { payload: unknown }) => void>();

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
    listeners.set(name, handler);
    return () => listeners.delete(name);
  }),
}));

vi.mock('../../utils/logger', () => ({
  logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

let bus: typeof import('../useEventBus');

beforeEach(async () => {
  listeners.clear();
  vi.resetModules();
  bus = await import('../useEventBus');
  await bus.initEventBus();
});

afterEach(() => {
  bus.destroyEventBus();
});

/** Pretend Rust emitted something. */
const fromRust = (name: string, payload: unknown) => {
  const handler = listeners.get(name);
  if (!handler) throw new Error(`nothing bridged the Tauri event "${name}"`);
  handler({ payload });
};

describe('what the backend says reaches the apps', () => {
  it('delivers a node the assistant created to the same handlers the apps use', () => {
    const seen: unknown[] = [];
    bus.useEventBus().on('node:created', p => seen.push(p));

    fromRust('node:created', { id: 'Tasks/Buy milk.md', node_type: 'task', title: 'Buy milk' });

    expect(seen).toHaveLength(1);
    expect(seen[0]).toMatchObject({ id: 'Tasks/Buy milk.md', title: 'Buy milk' });
  });

  /**
   * The reshape, which is the whole reason a plain pass-through is not enough.
   * Rust writes `node_type`; every listener in the app destructures `nodeType`
   * and returns early when it does not match. Handed the raw payload they all
   * read `undefined`, no app reloads, and the failure looks exactly like the
   * missing bridge it was supposed to fix.
   */
  it('renames the field every app filters on', () => {
    for (const event of ['node:created', 'node:updated', 'node:deleted'] as const) {
      const reloaded: string[] = [];
      bus.useEventBus().on(event, ({ nodeType }: { nodeType: string }) => {
        if (nodeType === 'task') reloaded.push(event);
      });

      fromRust(event, { id: 'Tasks/x.md', node_type: 'task' });

      expect(reloaded, `${event} did not reach a listener filtering on nodeType`).toEqual([event]);
    }
  });

  /** A bulk edit names its type and no node; the filter still has to match. */
  it('carries the type of a bulk edit that names no single node', () => {
    const reloaded: string[] = [];
    bus.useEventBus().on('node:updated', ({ nodeType }: { nodeType: string }) => {
      reloaded.push(nodeType);
    });

    fromRust('node:changed', { node_type: 'book' });

    expect(reloaded, '`node:changed` is what the write path emits').toEqual(['book']);
  });
});
