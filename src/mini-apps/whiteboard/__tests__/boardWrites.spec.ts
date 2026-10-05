import { beforeEach, describe, expect, it, vi } from 'vitest';

let onDisk: string | null = '';
let written: string | null = null;
/** Someone else writing between this writer's read and its write, once. */
let interloper: (() => void) | null = null;
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string, args: any) => {
    if (cmd === 'read_whiteboard') {
      if (onDisk === null) throw new Error('gone');
      const text = onDisk;
      if (interloper) { interloper(); interloper = null; }
      return text;
    }
    if (cmd === 'update_whiteboard') {
      if (onDisk === null || (await versionOf(onDisk)) !== args.expected) throw { code: 'STALE' };
      written = args.content;
      onDisk = args.content;
    }
    return null;
  }),
}));
vi.mock('@tauri-apps/api/event', () => ({ emit: vi.fn(async () => {}) }));

import { changeBoardLinks, writeItemChanges } from '../boardWrites';
import { versionOf } from '../boardDisk';
import { newBoardData } from '../boardFile';
import type { WBNode } from '../boardFile';

const item = (id: string, label: string, updated: number, x = 0, color = 'yellow'): WBNode => ({ id, type: 'sticky', position: { x, y: 0 }, data: { label, color }, updated });
const board = (nodes: WBNode[]) => JSON.stringify({ ...newBoardData('B'), nodes });
const saved = () => Object.fromEntries(JSON.parse(written!).nodes.map((n: WBNode) => [n.id, n]));

beforeEach(() => {
  written = null;
  interloper = null;
  onDisk = board([item('a', 'disk a', 10), item('b', 'disk b', 50), item('c', 'added elsewhere', 60)]);
});

describe('writeItemChanges', () => {
  it('writes the changes made here over older items, and keeps everything else on disk', async () => {
    await writeItemChanges('/v', 'W/b.whiteboard.json', [
      { id: 'a', position: { x: 300, y: 0 }, updated: 20 },
      { id: 'b', data: { label: 'stale here' }, updated: 40 },
    ]);
    const byId = saved();
    expect(byId.a.position.x).toBe(300);
    // Changed on disk later than here: disk wins.
    expect(byId.b.data.label).toBe('disk b');
    // Not touched here at all: kept.
    expect(byId.c.data.label).toBe('added elsewhere');
  });

  it('writes only the fields touched here, so a recolour elsewhere survives a drag here', async () => {
    onDisk = board([item('a', 'renamed by Syn', 10, 0, 'blue')]);
    await writeItemChanges('/v', 'W/b.whiteboard.json', [{ id: 'a', position: { x: 99, y: 0 }, updated: 20 }]);
    expect(saved().a).toMatchObject({ position: { x: 99 }, data: { label: 'renamed by Syn', color: 'blue' } });
  });

  it('reads again when someone wrote between its read and its write', async () => {
    interloper = () => { onDisk = board([item('a', 'disk a', 10), item('new', 'written meanwhile', 70)]); };
    await writeItemChanges('/v', 'W/b.whiteboard.json', [{ id: 'a', position: { x: 5, y: 0 }, updated: 20 }]);
    expect(Object.keys(saved())).toEqual(['a', 'new']);
    expect(saved().a.position.x).toBe(5);
  });

  it('writes nothing over a file a newer build saved, or one that is gone', async () => {
    onDisk = JSON.stringify({ schemaVersion: 99, nodes: [], edges: [] });
    expect(await writeItemChanges('/v', 'x.whiteboard.json', [{ id: 'a', updated: 1 }])).toBe(false);
    onDisk = null;
    expect(await writeItemChanges('/v', 'x.whiteboard.json', [{ id: 'a', updated: 1 }])).toBe(false);
    expect(written).toBeNull();
  });
});

describe('changeBoardLinks', () => {
  it('unlinks a project, stamped so a sync keeps the unlink', async () => {
    onDisk = JSON.stringify({ ...newBoardData('B'), metadata: { updated_at: 'x', linked_projects: ['[P](synabit://project/p)', '[Q](synabit://project/q)'] } });
    expect(await changeBoardLinks('/v', 'b.whiteboard.json', (l) => l.filter((x) => !x.includes('/p)')))).toBe(true);
    const meta = JSON.parse(written!).metadata;
    expect(meta.linked_projects).toEqual(['[Q](synabit://project/q)']);
    expect(meta.stamps['metadata.linked_projects']).toBeGreaterThan(0);
  });

  it('writes nothing when nothing changes, or over a newer build', async () => {
    onDisk = JSON.stringify({ ...newBoardData('B'), metadata: { linked_projects: ['a'] } });
    expect(await changeBoardLinks('/v', 'b.whiteboard.json', (l) => l)).toBe(false);
    onDisk = JSON.stringify({ schemaVersion: 99, nodes: [], edges: [], metadata: { linked_projects: ['a'] } });
    expect(await changeBoardLinks('/v', 'b.whiteboard.json', () => [])).toBe(false);
    expect(written).toBeNull();
  });
});
