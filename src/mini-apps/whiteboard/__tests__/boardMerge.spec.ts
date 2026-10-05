import { describe, expect, it } from 'vitest';
import { mergeBoards, newBoardData, stampFields } from '../boardFile';
import type { WBEdge, WBNode, WhiteboardData } from '../boardFile';

const node = (id: string, label = id, updated = 1): WBNode => ({
  id,
  type: 'shape',
  position: { x: 0, y: 0 },
  data: { label },
  updated,
});

const edge = (id: string, source: string, target: string, updated = 1): WBEdge => ({
  id,
  source,
  target,
  type: 'default',
  data: {},
  updated,
});

const board = (nodes: WBNode[], edges: WBEdge[] = [], extra: Partial<WhiteboardData> = {}): WhiteboardData => ({
  ...newBoardData('Board'),
  created_at: '2026-01-01T00:00:00.000Z',
  metadata: { updated_at: '2026-01-01T00:00:00.000Z' },
  nodes,
  edges,
  ...extra,
});

const clone = <T>(v: T): T => JSON.parse(JSON.stringify(v));
const labels = (b: WhiteboardData) => Object.fromEntries(b.nodes.map((n) => [n.id, n.data.label]));

describe('mergeBoards', () => {
  it('keeps what each side added', () => {
    const base = board([node('a')]);
    const local = board([node('a'), node('mine')]);
    const disk = board([node('a'), node('theirs')]);

    expect(labels(mergeBoards(base, local, disk))).toEqual({ a: 'a', mine: 'mine', theirs: 'theirs' });
  });

  it('takes each side’s change to different items', () => {
    const base = board([node('a'), node('b')]);
    const local = board([node('a', 'A here', 5), node('b')]);
    const disk = board([node('a'), node('b', 'B there', 6)]);

    expect(labels(mergeBoards(base, local, disk))).toEqual({ a: 'A here', b: 'B there' });
  });

  it('settles a change to the same item on both sides by the later stamp', () => {
    const base = board([node('a')]);
    const local = board([node('a', 'here', 10)]);
    const disk = board([node('a', 'there', 20)]);
    expect(labels(mergeBoards(base, local, disk)).a).toBe('there');

    const laterHere = board([node('a', 'here', 30)]);
    expect(labels(mergeBoards(base, laterHere, disk)).a).toBe('here');
  });

  it('honours a deletion on either side when the other side left the item alone', () => {
    const base = board([node('a'), node('b')]);
    const local = board([node('b')]); // a deleted here
    const disk = board([node('a')]); // b deleted there

    expect(mergeBoards(base, local, disk).nodes).toEqual([]);
  });

  it('keeps an item deleted on one side but worked on by the other', () => {
    const base = board([node('a')]);
    const local = board([]);
    const disk = board([node('a', 'renamed there', 9)]);

    expect(labels(mergeBoards(base, local, disk))).toEqual({ a: 'renamed there' });
  });

  it('drops an edge whose end did not survive', () => {
    const base = board([node('a'), node('b')], [edge('e', 'a', 'b')]);
    const local = board([node('a'), node('b')], [edge('e', 'a', 'b')]);
    const disk = board([node('a')], []);

    const merged = mergeBoards(base, local, disk);
    expect(merged.nodes.map((n) => n.id)).toEqual(['a']);
    expect(merged.edges).toEqual([]);
  });

  it('keeps metadata someone else wrote, like a project link', () => {
    const base = board([node('a')]);
    const local = board([node('a', 'moved', 5)], [], {
      metadata: { updated_at: '2026-02-01T00:00:00.000Z' },
    });
    const disk = board([node('a')], [], {
      metadata: { updated_at: '2026-01-15T00:00:00.000Z', linked_projects: ['[P](synabit://project/p)'] },
    });

    const merged = mergeBoards(base, local, disk);
    expect(merged.metadata?.linked_projects).toEqual(['[P](synabit://project/p)']);
    expect(labels(merged).a).toBe('moved');
  });

  it('takes a title changed here and tags changed there', () => {
    const base = board([], [], { title: 'Old', tags: [] });
    const local = board([], [], { title: 'New here', tags: [] });
    const disk = board([], [], { title: 'Old', tags: ['plan'] });

    const merged = mergeBoards(base, local, disk);
    expect(merged.title).toBe('New here');
    expect(merged.tags).toEqual(['plan']);
  });

  it('returns the disk copy when nothing changed here', () => {
    const base = board([node('a')]);
    const disk = board([node('a', 'there', 3), node('b')]);
    expect(mergeBoards(base, clone(base), disk)).toEqual(disk);
  });
});

describe('mergeBoards, item by item', () => {
  it('keeps a rename there and a move here of the same box', () => {
    const base = board([node('a', 'old', 1)]);
    const local = board([{ ...node('a', 'old', 5), position: { x: 300, y: 40 } }]);
    const disk = board([node('a', 'renamed by Syn', 6)]);

    const [merged] = mergeBoards(base, local, disk).nodes;
    expect(merged.data.label).toBe('renamed by Syn');
    expect(merged.position).toEqual({ x: 300, y: 40 });
    expect(merged.updated).toBe(6);
  });

  it('does not bring back an item deleted here because its stamp moved there', () => {
    // A board saved without per-item stamps gets them on every read from that
    // read's save time: the item is the same, only the stamp differs.
    const base = board([node('a', 'a', 100), node('b', 'b', 100)]);
    const local = board([node('b', 'b', 100)]);
    const disk = board([node('a', 'a', 200), node('b', 'b', 200)]);

    expect(mergeBoards(base, local, disk).nodes.map((n) => n.id)).toEqual(['b']);
  });
});

describe('mergeBoards, with a writer that orders keys differently', () => {
  /** The same item, keys alphabetical — as a Rust write leaves it. */
  const sorted = (v: any): any =>
    Array.isArray(v) ? v.map(sorted)
      : v && typeof v === 'object' ? Object.fromEntries(Object.keys(v).sort().map((k) => [k, sorted(v[k])]))
        : v;

  it('does not bring back an item deleted here after Syn or sync rewrote the file', () => {
    const base = board([node('a'), node('gone')]);
    const local = board([node('a')]);
    const disk = sorted(clone(base));
    expect(mergeBoards(base, local, disk).nodes.map((n) => n.id)).toEqual(['a']);
  });

  it('still sees a real change there through the reordering', () => {
    const base = board([node('a')]);
    const disk = sorted(board([node('a', 'renamed there', 2)]));
    expect(labels(mergeBoards(base, clone(base), disk))).toEqual({ a: 'renamed there' });
  });
});

describe('recordDeletions', () => {
  it('notes what went since the last save, and forgets what came back', async () => {
    const { recordDeletions } = await import('../boardFile');
    const before = board([node('a'), node('b')]);
    const now = board([node('a')]);
    recordDeletions(before, now, 1000);
    expect(now.metadata?.deleted).toEqual({ b: 1000 });

    const undone = board([node('a'), node('b')], [], { metadata: { deleted: { b: 1000 } } });
    recordDeletions(now, undone, 2000);
    expect(undone.metadata?.deleted).toBeUndefined();
  });

  it('keeps what either side deleted when merging', () => {
    const base = board([node('a')]);
    const local = board([node('a')], [], { metadata: { deleted: { x: 5 } } });
    const disk = board([node('a')], [], { metadata: { deleted: { x: 3, y: 4 } } });
    expect(mergeBoards(base, local, disk).metadata?.deleted).toEqual({ x: 5, y: 4 });
  });
});

describe('stampFields', () => {
  it('stamps the board fields that changed since the last save, and nothing else', () => {
    const before = board([], [], { title: 'Old', tags: ['a'], metadata: { updated_at: 'x', linked_projects: ['P'] } });
    const now = clone(before);
    now.title = 'New';
    now.metadata!.linked_projects = [];
    now.metadata!.updated_at = 'y';
    stampFields(before, now, 42);
    expect(now.metadata!.stamps).toEqual({ title: 42, 'metadata.linked_projects': 42 });
  });
});
