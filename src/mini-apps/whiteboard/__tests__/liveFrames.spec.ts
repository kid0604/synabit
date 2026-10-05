import { beforeEach, describe, expect, it, vi } from 'vitest';

let rows: { id: string; node_type: string; title: string; cells?: string[] }[] = [];
let asked = '';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(async (_c: string, a: any) => { asked = a?.query ?? ''; return { rows, columns: ['title', ...(asked.match(/columns:(\S+)/)?.[1] ? [asked.match(/columns:(\S+)/)![1]] : [])] }; }) }));

import { gridInFrame, useLiveFrames } from '../composables/useLiveFrames';
import { parseVaultLink } from '../vaultCards';
import { wordsOf } from '../composables/useBoardAssist';
import { newBoardData } from '../boardFile';
import type { WBNode } from '../boardFile';

const frame = (): WBNode => ({ id: 'f', type: 'frame', position: { x: 100, y: 100 }, data: { label: 'Due', width: 608, height: 200, query: 'is:task' } });

const setup = () => {
  const board = { ...newBoardData('B'), nodes: [frame()] };
  let n = 0;
  const store = { currentBoardData: { value: board }, generateId: (p: string) => `${p}-${++n}`, pushUndoState: vi.fn() };
  const refresh = vi.fn();
  const live = useLiveFrames({ store, refresh, scheduleSave: vi.fn(), onError: vi.fn() });
  return { board, live, refresh };
};

beforeEach(() => {
  rows = [];
});

describe('a live frame', () => {
  it('holds a card for each answer, in a grid inside it, and grows to fit', async () => {
    const { board, live } = setup();
    rows = [1, 2, 3].map((i) => ({ id: `Tasks/${i}.md`, node_type: 'task', title: `Task ${i}` }));
    await live.refreshAll();

    const cards = board.nodes.filter((x) => x.type === 'card');
    expect(cards.map((c) => c.data.ref)).toEqual(['Tasks/1.md', 'Tasks/2.md', 'Tasks/3.md']);
    expect(cards.every((c) => c.data.fromQuery === 'f')).toBe(true);
    // Two columns fit in 608px: the third card starts a second row.
    expect(cards[2].position.y).toBeGreaterThan(cards[0].position.y);
    expect(board.nodes[0].data.height).toBeGreaterThan(200);
  });

  it('lets go of what no longer answers, and leaves the user’s own items alone', async () => {
    const { board, live } = setup();
    rows = [{ id: 'Tasks/1.md', node_type: 'task', title: 'One' }, { id: 'Tasks/2.md', node_type: 'task', title: 'Two' }];
    await live.refreshAll();
    board.nodes.push({ id: 'mine', type: 'sticky', position: { x: 120, y: 120 }, data: { label: 'my note' } });

    rows = [{ id: 'Tasks/2.md', node_type: 'task', title: 'Two, renamed' }];
    await live.refreshAll();

    const cards = board.nodes.filter((x) => x.type === 'card');
    expect(cards.map((c) => [c.data.ref, c.data.title])).toEqual([['Tasks/2.md', 'Two, renamed']]);
    expect(board.nodes.some((x) => x.id === 'mine')).toBe(true);
  });

  it('does not redraw when nothing changed', async () => {
    const { live, refresh } = setup();
    rows = [{ id: 'Tasks/1.md', node_type: 'task', title: 'One' }];
    await live.refreshAll();
    refresh.mockClear();
    await live.refreshAll();
    expect(refresh).not.toHaveBeenCalled();
  });

  it('leaves moments from the timeline out', async () => {
    const { board, live } = setup();
    rows = [{ id: 'Notes/a.md#event#1', node_type: 'event', title: 'A moment' }];
    await live.refreshAll();
    expect(board.nodes.filter((x) => x.type === 'card')).toEqual([]);
  });

  it('lays out one column in a narrow frame', () => {
    const narrow = { ...frame(), data: { ...frame().data, width: 300 } };
    const { positions } = gridInFrame(narrow, 2);
    expect(positions[0].x).toBe(positions[1].x);
  });
});

describe('vault links and words', () => {
  it('reads a link to something in the vault, bare or in Markdown', () => {
    expect(parseVaultLink('synabit://task/Tasks%2Fa.md')).toEqual({ kind: 'task', id: 'Tasks/a.md' });
    expect(parseVaultLink('[Call](synabit://person/People%2Fan.md)')).toEqual({ kind: 'person', id: 'People/an.md' });
    expect(parseVaultLink('see synabit://task/x and more')).toBeNull();
  });

  it('gives Syn the words, not the Markdown', () => {
    expect(wordsOf({ id: 'a', type: 'text', position: { x: 0, y: 0 }, data: { label: '# Plan\n- **ship** it' } })).toBe('Plan - ship it');
    expect(wordsOf({ id: 'c', type: 'card', position: { x: 0, y: 0 }, data: { ref: 'x', title: 'Call the bank' } })).toBe('Call the bank');
  });
});

describe('a live frame on two devices', () => {
  it('makes the same card id for the same answer, and drops a duplicate sync kept', async () => {
    const a = setup();
    const b = setup();
    rows = [{ id: 'Tasks/1.md', node_type: 'task', title: 'One' }];
    await a.live.refreshAll();
    await b.live.refreshAll();
    const idA = a.board.nodes.find((x) => x.type === 'card')!.id;
    expect(b.board.nodes.find((x) => x.type === 'card')!.id).toBe(idA);

    // A copy from before ids were shared, kept by a sync.
    a.board.nodes.push({ id: 'card-old', type: 'card', position: { x: 0, y: 0 }, data: { ref: 'Tasks/1.md', kind: 'task', title: 'One', fromQuery: 'f' } });
    await a.live.refreshAll();
    expect(a.board.nodes.filter((x) => x.type === 'card').map((c) => c.id)).toEqual([idA]);
  });
});

describe('a kanban live frame', () => {
  it('asks for the field it sorts by, and a card dropped in another column changes it in the vault', async () => {
    const board = { ...newBoardData('B'), nodes: [{ id: 'f', type: 'frame', position: { x: 0, y: 0 }, data: { label: 'Week', query: 'is:task', layout: 'kanban', width: 600, height: 200 } } as WBNode] };
    const store = { currentBoardData: { value: board }, generateId: (p: string) => `${p}-1`, pushUndoState: vi.fn() };
    const writeProperty = vi.fn(async () => { rows = [{ id: 'Tasks/1.md', node_type: 'task', title: 'One', cells: ['One', 'done'] }]; });
    const live = useLiveFrames({ store, refresh: vi.fn(), scheduleSave: vi.fn(), onError: vi.fn(), writeProperty });
    rows = [{ id: 'Tasks/1.md', node_type: 'task', title: 'One', cells: ['One', 'todo'] }];
    await live.refreshAll();
    expect(asked).toContain('columns:status');
    const frame = board.nodes[0];
    const card = board.nodes.find((n) => n.type === 'card')!;
    expect(card.data.value).toBe('todo');

    const done = (frame.data.lanes as { value: string; x: number }[]).find((l) => l.value === 'done')!;
    card.position = { x: frame.position.x + done.x + 4, y: card.position.y };
    await live.moveCard(card);

    expect(writeProperty).toHaveBeenCalledWith('Tasks/1.md', 'task', 'One', expect.objectContaining({ status: 'done' }), 'todo', 'done');
    // Asked again: the card is where Done is, and says so.
    expect(card.data.value).toBe('done');
    expect(card.position.x).toBeGreaterThanOrEqual(frame.position.x + done.x);
  });

  it('puts a card dropped nowhere in particular back where it was', async () => {
    const board = { ...newBoardData('B'), nodes: [{ id: 'f', type: 'frame', position: { x: 0, y: 0 }, data: { label: 'W', query: 'is:task', layout: 'kanban', width: 600, height: 200 } } as WBNode] };
    const store = { currentBoardData: { value: board }, generateId: (p: string) => `${p}-1`, pushUndoState: vi.fn() };
    const writeProperty = vi.fn(async () => {});
    const live = useLiveFrames({ store, refresh: vi.fn(), scheduleSave: vi.fn(), onError: vi.fn(), writeProperty });
    rows = [{ id: 'Tasks/1.md', node_type: 'task', title: 'One', cells: ['One', 'todo'] }];
    await live.refreshAll();
    const card = board.nodes.find((n) => n.type === 'card')!;
    const home = { ...card.position };
    card.position = { x: home.x + 5, y: home.y + 300 };
    await live.moveCard(card);
    expect(writeProperty).not.toHaveBeenCalled();
    expect(card.position).toEqual(home);
  });

  it('offers to take a move back, which puts the field back in the vault', async () => {
    const board = { ...newBoardData('B'), nodes: [{ id: 'f', type: 'frame', position: { x: 0, y: 0 }, data: { label: 'W', query: 'is:task', layout: 'kanban', width: 600, height: 200 } } as WBNode] };
    const store = { currentBoardData: { value: board }, generateId: (p: string) => `${p}-1`, pushUndoState: vi.fn() };
    const writeProperty = vi.fn(async () => {});
    let moved: any = null;
    const live = useLiveFrames({ store, refresh: vi.fn(), scheduleSave: vi.fn(), onError: vi.fn(), writeProperty, onMoved: (m) => { moved = m; } });
    rows = [{ id: 'Tasks/1.md', node_type: 'task', title: 'One', cells: ['One', 'todo'] }];
    await live.refreshAll();
    const frame = board.nodes[0];
    const card = board.nodes.find((n) => n.type === 'card')!;
    const done = (frame.data.lanes as { value: string; x: number }[]).find((l) => l.value === 'done')!;
    card.position = { x: frame.position.x + done.x + 4, y: card.position.y };
    await live.moveCard(card);
    expect(moved).toMatchObject({ title: 'One', field: 'status', was: 'todo', now: 'done' });

    await moved.undo();
    expect(writeProperty).toHaveBeenLastCalledWith('Tasks/1.md', 'task', 'One', expect.objectContaining({ status: 'todo', completed_at: null }), 'done', 'todo');
  });

  it('says so when the item cannot be changed', async () => {
    const board = { ...newBoardData('B'), nodes: [{ id: 'f', type: 'frame', position: { x: 0, y: 0 }, data: { label: 'W', query: 'is:task', layout: 'kanban', width: 600, height: 200 } } as WBNode] };
    const store = { currentBoardData: { value: board }, generateId: (p: string) => `${p}-1`, pushUndoState: vi.fn() };
    const onMoveFailed = vi.fn();
    const onError = vi.fn();
    const live = useLiveFrames({ store, refresh: vi.fn(), scheduleSave: vi.fn(), onError, onMoveFailed, writeProperty: vi.fn(async () => { throw new Error('disk full'); }) });
    rows = [{ id: 'Tasks/1.md', node_type: 'task', title: 'One', cells: ['One', 'todo'] }];
    await live.refreshAll();
    const frame = board.nodes[0];
    const card = board.nodes.find((n) => n.type === 'card')!;
    const done = (frame.data.lanes as { value: string; x: number }[]).find((l) => l.value === 'done')!;
    card.position = { x: frame.position.x + done.x + 4, y: card.position.y };
    await live.moveCard(card);
    expect(onMoveFailed).toHaveBeenCalledWith(expect.stringContaining('disk full'));
    expect(onError).not.toHaveBeenCalled();
  });

  it('changes nothing in the vault for a card dragged out of the frame', async () => {
    const board = { ...newBoardData('B'), nodes: [{ id: 'f', type: 'frame', position: { x: 0, y: 0 }, data: { label: 'W', query: 'is:task', layout: 'kanban', width: 600, height: 200 } } as WBNode] };
    const store = { currentBoardData: { value: board }, generateId: (p: string) => `${p}-1`, pushUndoState: vi.fn() };
    const writeProperty = vi.fn(async () => {});
    const live = useLiveFrames({ store, refresh: vi.fn(), scheduleSave: vi.fn(), onError: vi.fn(), writeProperty });
    rows = [{ id: 'Tasks/1.md', node_type: 'task', title: 'One', cells: ['One', 'todo'] }];
    await live.refreshAll();
    const frame = board.nodes[0];
    const card = board.nodes.find((n) => n.type === 'card')!;
    const home = { ...card.position };
    // Far to the right of the last column, where the nearest column is Done.
    card.position = { x: frame.position.x + Number(frame.data.width) + 400, y: home.y };
    await live.moveCard(card);
    expect(writeProperty).not.toHaveBeenCalled();
    expect(card.position).toEqual(home);
  });
});
