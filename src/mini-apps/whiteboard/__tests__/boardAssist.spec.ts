import { describe, expect, it, vi } from 'vitest';

let sent: any = null;
let reply: ((args: any) => any) | null = null;
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (_cmd: string, args: any) => {
    sent = args;
    if (reply) return reply(args);
    return {
      title: 'Payments',
      nodes: [
        { id: 'box-1', type: 'shape', position: { x: 40, y: 40 }, data: { label: 'Checkout', width: 160, height: 56 } },
        { id: 'box-2', type: 'shape', position: { x: 40, y: 168 }, data: { label: 'Bank', width: 160, height: 56 } },
      ],
      edges: [{ id: 'edge-1', source: 'box-1', target: 'box-2', data: {} }],
    };
  }),
}));

import { extentOf, useBoardAssist } from '../composables/useBoardAssist';
import { newBoardData } from '../boardFile';
import type { WBNode } from '../boardFile';

const setup = (nodes: WBNode[] = [], selected: string[] = [], t?: (k: string, v?: Record<string, unknown>) => string) => {
  const board = { ...newBoardData('B'), nodes };
  const paste = vi.fn();
  const notify = vi.fn();
  const store = { currentBoardData: { value: board }, currentBoardId: { value: 'b1' }, generateId: (p: string) => `${p}-x`, pushUndoState: vi.fn() };
  const assist = useBoardAssist({
    store,
    vaultPath: () => '/v',
    locale: () => 'vi',
    selectedIds: () => selected,
    boxOf: (n) => ({ x: n.position.x, y: n.position.y, w: Number(n.data.width) || 160, h: Number(n.data.height) || 80 }),
    paste,
    commit: vi.fn(),
    pictureOf: async () => null,
    addMindmapChildren: vi.fn(),
    createTasks: async () => [],
    notify,
    t: t ?? ((k: string, v?: Record<string, unknown>) => (v?.title ? `${k}:${v.title}` : k)),
    viewCentre: () => ({ x: 1000, y: 1000 }),
  });
  return { assist, paste, notify, store, board };
};

describe('a diagram drawn from a description', () => {
  it('sends the request and the note, and places the answer where the person is looking', async () => {
    const { assist, paste, notify } = setup();
    await assist.run('generate', { request: 'Draw the payment flow', useSelection: false, source: '# Payments\n\nCheckout calls the bank.' });
    expect(sent).toMatchObject({ action: 'generate', request: 'Draw the payment flow', source: '# Payments\n\nCheckout calls the bank.', items: [] });
    const [clip, at] = paste.mock.calls[0];
    expect(clip.nodes).toHaveLength(2);
    // Lines that keep facing each other as the boxes move.
    expect(clip.edges[0].data.sides).toBe('auto');
    expect(at.x).toBeGreaterThan(700);
    expect(notify).toHaveBeenLastCalledWith('whiteboard.syn.drawn_titled:Payments', 'info');
  });

  it('takes the selected words along only when asked to', async () => {
    const sticky: WBNode = { id: 's', type: 'sticky', position: { x: 0, y: 0 }, data: { label: 'Refunds go back to the card' } };
    const { assist } = setup([sticky], ['s']);
    await assist.run('generate', { request: 'Draw it', useSelection: true });
    expect(sent.items).toEqual([{ id: 's', kind: 'sticky', text: 'Refunds go back to the card' }]);
    await assist.run('generate', { request: 'Draw it', useSelection: false });
    expect(sent.items).toEqual([]);
  });

  it('asks nothing for an empty request', async () => {
    sent = null;
    const { assist } = setup();
    await assist.run('generate', { request: '  ', useSelection: false });
    expect(sent).toBeNull();
  });

  it('measures what it is about to place', () => {
    expect(extentOf([
      { id: 'a', type: 'shape', position: { x: 0, y: 0 }, data: { width: 100, height: 50 } },
      { id: 'b', type: 'shape', position: { x: 200, y: 100 }, data: { width: 100, height: 50 } },
    ])).toEqual({ w: 300, h: 150 });
  });
});

describe('Syn on the board, while it works', () => {
  it('can be stopped, and an answer that comes after is not used', async () => {
    let answer!: (v: any) => void;
    reply = () => new Promise((resolve) => { answer = resolve; });
    const { assist, paste, notify } = setup();
    const running = assist.run('generate', { request: 'Draw it', useSelection: false });
    expect(assist.busy.value).toBe('generate');
    assist.cancel();
    expect(assist.busy.value).toBeNull();
    expect(notify).toHaveBeenLastCalledWith('whiteboard.syn.cancelled', 'info');
    answer({ nodes: [{ id: 'n', type: 'shape', position: { x: 0, y: 0 }, data: {} }], edges: [] });
    await running;
    expect(paste).not.toHaveBeenCalled();
    reply = null;
  });

  it('says what went wrong in the app\'s words, and a provider\'s own message as it came', async () => {
    const words = (k: string, v?: Record<string, unknown>) => (k.startsWith('whiteboard.syn.err.') ? `vi:${k}${k.endsWith('failed') ? `:${v?.detail}` : ''}` : k);
    reply = () => { throw new Error('[syn:no_ideas] Syn gave no ideas.'); };
    const sticky: WBNode = { id: 's', type: 'sticky', position: { x: 0, y: 0 }, data: { label: 'One' } };
    const first = setup([sticky], ['s'], words);
    await first.assist.run('expand');
    expect(first.notify).toHaveBeenLastCalledWith('vi:whiteboard.syn.err.no_ideas', 'error');

    reply = () => { throw new Error('[syn:failed] Syn could not do that: 503 overloaded'); };
    const second = setup([sticky], ['s'], words);
    await second.assist.run('expand');
    expect(second.notify).toHaveBeenLastCalledWith('vi:whiteboard.syn.err.failed:503 overloaded', 'error');
    reply = null;
  });

  it('places its answer on the same board even when a sync replaced the board meanwhile', async () => {
    const { assist, paste, store, board } = setup();
    reply = () => {
      store.currentBoardData.value = { ...board, nodes: [...board.nodes] };
      return { nodes: [{ id: 'n', type: 'shape', position: { x: 0, y: 0 }, data: {} }], edges: [] };
    };
    await assist.run('generate', { request: 'Draw it', useSelection: false });
    expect(paste).toHaveBeenCalled();
    reply = null;
  });

  it('puts groups where nothing else is', async () => {
    const a: WBNode = { id: 'a', type: 'sticky', position: { x: 0, y: 0 }, data: { label: 'A', width: 100, height: 100 } };
    const b: WBNode = { id: 'b', type: 'sticky', position: { x: 120, y: 0 }, data: { label: 'B', width: 100, height: 100 } };
    const c: WBNode = { id: 'c', type: 'sticky', position: { x: 240, y: 0 }, data: { label: 'C', width: 100, height: 100 } };
    // Not in the selection, and right where the frames would have gone.
    const other: WBNode = { id: 'o', type: 'sticky', position: { x: 60, y: 140 }, data: { label: 'Other', width: 100, height: 100 } };
    reply = () => ({ groups: [{ name: 'X', items: ['a', 'b'] }, { name: 'Y', items: ['c'] }] });
    const { assist, board } = setup([a, b, c, other], ['a', 'b', 'c']);
    await assist.run('cluster');
    reply = null;
    const frames = board.nodes.filter((n) => n.type === 'frame');
    expect(frames).toHaveLength(2);
    const box = (n: WBNode) => ({ x: n.position.x, y: n.position.y, w: Number(n.data.width) || 100, h: Number(n.data.height) || 100 });
    const o = box(other);
    for (const f of frames.map(box)) {
      const apart = f.x + f.w <= o.x || o.x + o.w <= f.x || f.y + f.h <= o.y || o.y + o.h <= f.y;
      expect(apart, JSON.stringify({ f, o })).toBe(true);
    }
    expect(other.position).toEqual({ x: 60, y: 140 });
  });
});
