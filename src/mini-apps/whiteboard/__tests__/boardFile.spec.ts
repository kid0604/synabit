import { describe, it, expect } from 'vitest';
import {
  BOARD_SCHEMA_VERSION,
  newBoardData,
  readBoardFile,
  stampElement,
} from '../boardFile';

/** A board file as builds before the format was numbered wrote one. */
const legacy = (over: Record<string, any> = {}) =>
  JSON.stringify({
    title: 'Plan',
    tags: ['work'],
    created_at: '2026-01-01T00:00:00.000Z',
    viewport: { x: 10, y: 20, zoom: 2 },
    nodes: [{ id: 'n1', type: 'text', position: { x: 0, y: 0 }, data: { label: 'hi' } }],
    edges: [],
    ...over,
  });

describe('reading a board file', () => {
  it('keeps what the file says and fills only what it does not', () => {
    const read = readBoardFile(legacy());
    expect(read.ok).toBe(true);
    if (!read.ok) return;

    expect(read.data.title).toBe('Plan');
    expect(read.data.tags).toEqual(['work']);
    expect(read.data.viewport).toEqual({ x: 10, y: 20, zoom: 2 });
    expect(read.data.nodes).toHaveLength(1);
  });

  it('hands back keys this app never wrote', () => {
    // A board created from a project carries `linked_projects`, and a save
    // rewrites the whole file — so anything dropped here is dropped for good.
    const read = readBoardFile(legacy({ metadata: { linked_projects: ['[P](synabit://project/p.md)'] } }));
    expect(read.ok).toBe(true);
    if (!read.ok) return;
    expect(read.data.metadata?.linked_projects).toEqual(['[P](synabit://project/p.md)']);
  });

  it('dates items that predate change stamps by the board they are on', () => {
    const read = readBoardFile(legacy({ metadata: { updated_at: '2026-03-04T05:06:07.000Z' } }));
    expect(read.ok).toBe(true);
    if (!read.ok) return;
    expect(read.data.nodes[0].updated).toBe(Date.parse('2026-03-04T05:06:07.000Z'));
  });

  it('leaves an unstampable board at zero rather than dating it now', () => {
    // Zero loses every comparison, which is the honest answer for an item
    // nobody can say anything about. "Now" would win them all.
    const read = readBoardFile(legacy({ created_at: 'some time last year' }));
    expect(read.ok).toBe(true);
    if (!read.ok) return;
    expect(read.data.nodes[0].updated).toBe(0);
  });

  it('drops a connection to something that is not on the board', () => {
    // The canvas reads both ends of every edge, and with off-screen items
    // left unbuilt a missing end is a crash rather than a blank.
    const read = readBoardFile(
      legacy({ edges: [{ id: 'e1', source: 'n1', target: 'gone', type: 'default' }] })
    );
    expect(read.ok).toBe(true);
    if (!read.ok) return;
    expect(read.data.edges).toEqual([]);
  });

  it('stamps the file with the version this build writes', () => {
    const read = readBoardFile(legacy());
    expect(read.ok).toBe(true);
    if (!read.ok) return;
    expect(read.data.schemaVersion).toBe(BOARD_SCHEMA_VERSION);
  });

  it('refuses a board from a newer build instead of rewriting it', () => {
    const read = readBoardFile(legacy({ schemaVersion: BOARD_SCHEMA_VERSION + 1 }));
    expect(read.ok).toBe(false);
    if (read.ok) return;
    expect(read.reason).toBe('too-new');
  });

  it('reports a file that is not a board rather than throwing', () => {
    expect(readBoardFile('{not json').ok).toBe(false);
    expect(readBoardFile('[]').ok).toBe(false);
    expect(readBoardFile('null').ok).toBe(false);
  });

  it('is safe to run twice, because a board is read on every open', () => {
    const once = readBoardFile(legacy());
    if (!once.ok) throw new Error('first read failed');
    const twice = readBoardFile(JSON.stringify(once.data));
    if (!twice.ok) throw new Error('second read failed');
    expect(twice.data).toEqual(once.data);
  });
});

describe('a new board', () => {
  it('is written at the current version and carries a save stamp', () => {
    const data = newBoardData('Untitled');
    expect(data.schemaVersion).toBe(BOARD_SCHEMA_VERSION);
    expect(typeof data.metadata?.updated_at).toBe('string');
    expect(Number.isNaN(Date.parse(data.metadata!.updated_at))).toBe(false);
  });
});

describe('change stamps', () => {
  it('records when an item changed', () => {
    const before = Date.now();
    const node = { updated: 0 };
    stampElement(node);
    expect(node.updated).toBeGreaterThanOrEqual(before);
  });
});
