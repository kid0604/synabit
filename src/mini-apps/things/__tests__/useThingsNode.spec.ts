import { describe, it, expect, vi, beforeEach } from 'vitest';

const writeNode = vi.fn();
const getNode = vi.fn();

vi.mock('../../../composables/useNodeService', () => ({
  useNodeService: () => ({ getNode, writeNode }),
}));

import { useThingsNode } from '../composables/useThingsNode';

/**
 * Gate T2: a screen that edits every kind of node is the most dangerous writer
 * in the app, so what it sends is worth pinning line by line.
 *
 * Each of these is a failure that has already happened somewhere in this
 * repository, and every one of them is silent — the save appears to work and
 * the damage shows up on the next read, or on another device.
 */
describe('what Things sends when it saves', () => {
  const node = {
    id: 'Animal/meo-mun.md',
    node_type: 'animal',
    title: 'Mèo Mun',
    content: 'Nhặt được ở ngõ.',
    properties: {
      title: 'Mèo Mun',
      type: 'animal',
      node_id: 'b7c4-stable-identity',
      created_at: '2026-03-01T00:00:00Z',
      updated_at: '2026-08-01T00:00:00Z',
      species: 'mèo',
      colour: 'đen',
      tags: ['nhà'],
    },
  };

  beforeEach(() => {
    writeNode.mockReset();
    getNode.mockReset();
    getNode.mockResolvedValue(structuredClone(node));
  });

  const openIt = async () => {
    const it = useThingsNode();
    await it.open(node.id);
    return it;
  };

  it('offers the fields that are the user’s, and not the ones the app owns', async () => {
    const detail = await openIt();
    const keys = detail.fields.value.map(f => f.key).sort();

    expect(keys).toEqual(['colour', 'species', 'tags']);
    for (const owned of ['node_id', 'created_at', 'updated_at', 'title', 'type']) {
      expect(keys, `${owned} is the app's, not a row to type into`).not.toContain(owned);
    }
  });

  /**
   * The one that splits a file in two.
   *
   * `node_id` is the file's identity to the sync engine. A save that names it
   * as `null` hands the file a fresh identity while every other device keeps
   * the old, and the note becomes two documents that never merge again.
   */
  it('never names an app-owned key in the patch, in either direction', async () => {
    const detail = await openIt();
    await detail.save();

    const patch = writeNode.mock.calls[0][0].properties;
    for (const owned of ['node_id', 'created_at', 'updated_at']) {
      expect(owned in patch, `${owned} must not be in the payload at all`).toBe(false);
    }
  });

  /**
   * A key merely left out means "I have nothing to say about this one" and
   * comes straight back on the next read, so a removed row has to be named.
   */
  it('names a removed field as null rather than leaving it out', async () => {
    const detail = await openIt();
    const colour = detail.fields.value.findIndex(f => f.key === 'colour');
    detail.removeField(colour);
    await detail.save();

    const patch = writeNode.mock.calls[0][0].properties;
    expect(patch.colour).toBeNull();
    expect(patch.species).toBe('mèo');
  });

  it('writes an edited value, and a field added by hand', async () => {
    const detail = await openIt();
    detail.fields.value.find(f => f.key === 'species')!.value = 'mèo mướp';
    detail.addField();
    const added = detail.fields.value[detail.fields.value.length - 1];
    added.key = 'vaccinated_at';
    added.value = '2026-08-01';
    await detail.save();

    const patch = writeNode.mock.calls[0][0].properties;
    expect(patch.species).toBe('mèo mướp');
    expect(patch.vaccinated_at).toBe('2026-08-01');
  });

  /**
   * `String(value)` turns a list into `a,b`, and saving that writes the
   * mangling to disk. A list the user never touched must come back a list.
   */
  it('round-trips a list rather than flattening it to text', async () => {
    const detail = await openIt();
    expect(detail.fields.value.find(f => f.key === 'tags')!.value).toBe('["nhà"]');

    await detail.save();
    expect(writeNode.mock.calls[0][0].properties.tags).toEqual(['nhà']);
  });

  /**
   * The type comes off the node on disk and nowhere else.
   *
   * `nodeRoutes.ts` records what happens otherwise: the note editor saved what
   * it held as `nodeType: 'note'`, so a task opened there became a note on the
   * first autosave and the task was gone. Things opens every kind of node,
   * which makes it the one screen where this cannot be got wrong once.
   */
  it('writes back the type the node already had', async () => {
    const detail = await openIt();
    await detail.save();
    expect(writeNode.mock.calls[0][0].nodeType).toBe('animal');
  });

  /**
   * Opening two nodes quickly issues two reads, and the slower one can land
   * second. Without the guard the pane shows the first node's fields under the
   * second node's title.
   */
  it('drops a read that has been overtaken', async () => {
    let releaseFirst: (v: unknown) => void = () => {};
    getNode
      .mockImplementationOnce(() => new Promise(resolve => { releaseFirst = resolve; }))
      .mockResolvedValueOnce({ ...structuredClone(node), id: 'Books/dune.md', title: 'Dune' });

    const detail = useThingsNode();
    const slow = detail.open('Animal/meo-mun.md');
    await detail.open('Books/dune.md');

    releaseFirst(structuredClone(node));
    await slow;

    expect(detail.title.value).toBe('Dune');
  });
});
