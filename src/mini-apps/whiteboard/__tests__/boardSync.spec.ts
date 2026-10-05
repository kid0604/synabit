import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ref } from 'vue';

// The board file, as the vault holds it. Every read and write goes here.
const disk = new Map<string, string>();
const writes: string[] = [];
let reads = 0;
const keeps: boolean[] = [];
/** Run once, right after the next read answers: a writer slipping in between a read and a write. */
let afterNextRead: (() => void) | null = null;
/** When set, the next read waits for this before answering. */
let holdNextRead: Promise<void> | null = null;

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(async (cmd: string, args: any) => {
    if (cmd === 'scan_whiteboards') {
      return [...disk.keys()].map((p) => ({ id: p, path: p, title: p, tags: [], created_at: '', updated_at: '' }));
    }
    if (cmd === 'read_whiteboard') {
      reads++;
      const hold = holdNextRead;
      holdNextRead = null;
      if (hold) await hold;
      if (!disk.has(args.path)) throw new Error('not found');
      const text = disk.get(args.path);
      const then = afterNextRead;
      afterNextRead = null;
      then?.();
      return text;
    }
    if (cmd === 'create_whiteboard') {
      const p = `Whiteboards/new-${disk.size}.whiteboard.json`;
      disk.set(p, args.content);
      return { id: p, path: p, title: args.title, tags: [], created_at: '', updated_at: '' };
    }
    if (cmd === 'update_whiteboard') {
      // As the command does: a write quoting the version it was made from is
      // refused when the file is no longer that version.
      if (args.expected !== undefined) {
        const now = disk.get(args.path);
        if (now === undefined || (await versionOf(now)) !== args.expected) throw { code: 'STALE', message: 'changed' };
      }
      disk.set(args.path, args.content);
      writes.push(args.content);
      keeps.push(!!args.keep);
      return null;
    }
    return null;
  }),
}));
vi.mock('@tauri-apps/api/event', () => ({ emit: vi.fn(async () => {}) }));

import { useWhiteboardStore } from '../composables/useWhiteboardStore';
import { versionOf } from '../boardDisk';
import { newBoardData } from '../boardFile';
import type { WBNode, WhiteboardData } from '../boardFile';

const PATH = 'Whiteboards/b.whiteboard.json';

const node = (id: string, label = id, updated = 1): WBNode => ({
  id,
  type: 'shape',
  position: { x: 0, y: 0 },
  data: { label },
  updated,
});

const fileWith = (nodes: WBNode[], metadata: Record<string, any> = {}) => {
  const data: WhiteboardData = {
    ...newBoardData('Board'),
    metadata: { updated_at: '2026-01-01T00:00:00.000Z', ...metadata },
    nodes,
  };
  return JSON.stringify(data, null, 2);
};

/** Someone else writes the file: Syn, a project link, a sync. */
const writeFromElsewhere = (change: (data: WhiteboardData) => void) => {
  const data = JSON.parse(disk.get(PATH)!);
  change(data);
  disk.set(PATH, JSON.stringify(data, null, 2));
};

const openBoard = async () => {
  const store = useWhiteboardStore(ref('/vault'));
  await store.loadBoards();
  await store.loadBoardData(PATH);
  return store;
};

const labels = (data: WhiteboardData | null) => Object.fromEntries((data?.nodes ?? []).map((n) => [n.id, n.data.label]));

beforeEach(() => {
  disk.clear();
  writes.length = 0;
  holdNextRead = null;
  disk.set(PATH, fileWith([node('a')]));
});

describe('a save never writes over what someone else wrote', () => {
  it('keeps an item Syn added while the board was open', async () => {
    const store = await openBoard();
    store.addNode(node('mine'));
    writeFromElsewhere((d) => d.nodes.push(node('syn', 'from Syn', Date.now())));

    await store.saveCurrentBoard();

    const saved = JSON.parse(disk.get(PATH)!);
    expect(labels(saved)).toEqual({ a: 'a', mine: 'mine', syn: 'from Syn' });
    expect(labels(store.currentBoardData.value)).toEqual(labels(saved));
  });

  it('keeps a project link written into the file meanwhile', async () => {
    const store = await openBoard();
    store.updateNodeData('a', { label: 'edited here' });
    writeFromElsewhere((d) => { d.metadata = { ...d.metadata, linked_projects: ['[P](synabit://project/p)'] }; });

    await store.saveCurrentBoard();

    const saved = JSON.parse(disk.get(PATH)!);
    expect(saved.metadata.linked_projects).toEqual(['[P](synabit://project/p)']);
    expect(saved.nodes[0].data.label).toBe('edited here');
  });

  it('tells the canvas to redraw, and drops a history that would undo their change', async () => {
    const store = await openBoard();
    store.addNode(node('mine'));
    expect(store.undoStack.value.length).toBe(1);
    writeFromElsewhere((d) => d.nodes.push(node('syn')));

    const before = store.externalRevision.value;
    await store.saveCurrentBoard();

    expect(store.externalRevision.value).toBe(before + 1);
    expect(store.undoStack.value).toEqual([]);
  });

  it('does not count its own save as a change from outside', async () => {
    const store = await openBoard();
    store.addNode(node('mine'));
    await store.saveCurrentBoard();
    const revision = store.externalRevision.value;

    expect(await store.syncWithDisk()).toBe(false);
    expect(store.externalRevision.value).toBe(revision);
  });

  it('refuses to write over a file a newer build saved', async () => {
    const store = await openBoard();
    store.addNode(node('mine'));
    writeFromElsewhere((d) => { d.schemaVersion = 99; });

    await store.saveCurrentBoard();

    expect(writes).toEqual([]);
    expect(store.currentBoardUnsupported.value).toBe(true);
    // What was added here is not lost: it is a board of its own.
    const aside = [...disk.entries()].find(([p]) => p !== PATH);
    expect(aside && labels(JSON.parse(aside[1]))).toEqual({ a: 'a', mine: 'mine' });
    expect(store.keptAside.value).toContain('Board');
  });

  it('heals a half-written file instead of refusing to save', async () => {
    const store = await openBoard();
    store.addNode(node('mine'));
    disk.set(PATH, '{"nodes": [');

    await store.saveCurrentBoard();

    expect(labels(JSON.parse(disk.get(PATH)!))).toEqual({ a: 'a', mine: 'mine' });
  });
});

describe('a save that keeps being overtaken', () => {
  it('still lands, with everyone\'s work, when other writers get in twice', async () => {
    const store = await openBoard();
    store.addNode(node('mine'));
    // Before the save: its first write is refused.
    writeFromElsewhere((d) => { d.nodes.push(node('first', 'first', Date.now())); });
    // Between its re-read and its second write: refused again.
    afterNextRead = () => writeFromElsewhere((d) => { d.nodes.push(node('second', 'second', Date.now())); });

    await store.saveCurrentBoard();

    expect(labels(JSON.parse(disk.get(PATH)!))).toEqual({ a: 'a', mine: 'mine', first: 'first', second: 'second' });
    expect(store.saveProblem.value).toBeNull();
  });
});

describe('restoring a version', () => {
  it('asks the save that puts it back to keep the board it replaces, and only that save', async () => {
    const store = await openBoard();
    keeps.length = 0;
    store.restoreVersion(JSON.parse(fileWith([node('old')])));
    await store.saveCurrentBoard();
    store.addNode(node('later'));
    await store.saveCurrentBoard();
    expect(keeps).toEqual([true, false]);
  });
});

describe('saving cheaply', () => {
  it('writes without reading first when nobody else wrote, and reads when somebody did', async () => {
    const store = await openBoard();
    store.addNode(node('one'));
    reads = 0;
    await store.saveCurrentBoard();
    expect(reads).toBe(0);

    writeFromElsewhere((d) => { d.nodes.push(node('theirs', 'theirs', Date.now())); });
    store.addNode(node('two'));
    await store.saveCurrentBoard();
    expect(reads).toBe(1);
    expect(labels(JSON.parse(disk.get(PATH)!))).toEqual({ a: 'a', one: 'one', theirs: 'theirs', two: 'two' });
  });

  it('keeps a stroke on one line per point', async () => {
    const store = await openBoard();
    store.addNode({ id: 's', type: 'stroke', position: { x: 0, y: 0 }, data: { points: [[1, 2, 0.5], [3.25, -4, 0.5]], width: 10, height: 10 } });
    await store.saveCurrentBoard();
    expect(disk.get(PATH)).toContain('[1,2,0.5]');
    expect(JSON.parse(disk.get(PATH)!).nodes[1].data.points).toEqual([[1, 2, 0.5], [3.25, -4, 0.5]]);
  });
});

describe('checking the disk', () => {
  it('takes in a change made while the board sat unedited', async () => {
    const store = await openBoard();
    writeFromElsewhere((d) => { d.nodes[0].data.label = 'renamed by Syn'; d.nodes[0].updated = Date.now(); });

    expect(await store.syncWithDisk()).toBe(true);
    expect(labels(store.currentBoardData.value)).toEqual({ a: 'renamed by Syn' });
    // Nothing of ours was waiting, so there is nothing to write back.
    expect(writes).toEqual([]);
  });

  it('runs one save at a time, so two in a row cannot both merge against the same file', async () => {
    const store = await openBoard();
    store.addNode(node('one'));
    const first = store.saveCurrentBoard();
    store.addNode(node('two'));
    const second = store.saveCurrentBoard();
    await Promise.all([first, second]);

    expect(labels(JSON.parse(disk.get(PATH)!))).toEqual({ a: 'a', one: 'one', two: 'two' });
  });
});

describe('opening another board while the disk is busy', () => {
  const OTHER = 'Whiteboards/other.whiteboard.json';

  it('finishes the save for the board being left before the next one opens', async () => {
    disk.set(OTHER, fileWith([node('b', 'other board')]));
    const store = await openBoard();
    store.addNode(node('mine'));

    let release!: () => void;
    holdNextRead = new Promise((r) => { release = r; });
    const saving = store.saveCurrentBoard();
    const opening = store.loadBoardData(OTHER);
    release();
    await Promise.all([saving, opening]);

    expect(labels(JSON.parse(disk.get(PATH)!))).toEqual({ a: 'a', mine: 'mine' });
    expect(labels(JSON.parse(disk.get(OTHER)!))).toEqual({ b: 'other board' });
    expect(store.currentBoardId.value).toBe(OTHER);
    expect(labels(store.currentBoardData.value)).toEqual({ b: 'other board' });
  });

  it('starts a new board with no history from the last one', async () => {
    const store = await openBoard();
    store.addNode(node('mine'));
    await store.createBoard('Fresh');

    expect(store.undoStack.value).toEqual([]);
    expect(store.currentBoardData.value?.nodes).toEqual([]);
  });
});

describe('a board file that is gone', () => {
  it('is not written back, and the problem is reported', async () => {
    const store = await openBoard();
    store.addNode(node('mine'));
    disk.delete(PATH);

    await store.saveCurrentBoard();

    expect(disk.has(PATH)).toBe(false);
    expect(store.saveProblem.value).toBe('missing');
  });
});
