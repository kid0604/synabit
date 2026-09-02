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

  /**
   * A save the dirty guard lets through, that puts nothing in the patch.
   *
   * `save()` declines to write when nothing changed, so a test about what a
   * patch *contains* now has to contain a change. An added row is the one edit
   * that qualifies without showing up: its key is empty, and `save()` skips
   * empty keys — so every assertion below reads exactly the patch it always
   * did.
   */
  const editThenSave = async (detail: ReturnType<typeof useThingsNode>) => {
    detail.addField();
    await detail.save();
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
    await editThenSave(detail);

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

    await editThenSave(detail);
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
    await editThenSave(detail);
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

  /**
   * Hiding a field must not delete it.
   *
   * This is the sharp edge of showing fewer fields. The save loop names every
   * key that was on disk but is no longer in the form as `null`, and `null` in
   * the patch contract means delete — so the moment the panel stopped drawing
   * `pinned`, `pinned` became a key on disk that is not in the form. Without
   * the guard, opening a note in Things and blurring one field unpins it, and
   * the only sign is that it left the top of the Notes sidebar.
   *
   * The same subtraction is what makes deleting a field work at all, so it
   * cannot simply be removed.
   */
  describe('a field the panel does not draw', () => {
    const note = {
      id: 'Notes/loi-kenh-truyen.md',
      node_type: 'note',
      title: 'Lỗi kênh truyền 2025-05-26',
      content: 'Mất gói ở chặng hai.',
      properties: {
        node_id: 'kept-identity',
        title: 'Lỗi kênh truyền 2025-05-26',
        type: 'note',
        pinned: true,
        full_width: false,
        linked_projects: [],
        tags: ['mdp', 'network'],
      },
    };

    beforeEach(() => {
      getNode.mockReset();
      writeNode.mockReset();
      getNode.mockResolvedValue(note);
    });

    it('is kept out of the rows a person can edit', async () => {
      const detail = useThingsNode();
      await detail.open(note.id);

      expect(detail.fields.value.map(f => f.key)).toEqual(['tags']);
    });

    it('is offered separately rather than kept secret', async () => {
      const detail = useThingsNode();
      await detail.open(note.id);

      // `node_id` belongs here too: hidden from the editable rows, still
      // shown to anyone who asks what is in the file.
      expect(detail.appFields.value.map(f => f.key).sort()).toEqual(
        ['full_width', 'linked_projects', 'node_id', 'pinned'],
      );
    });

    it('is never named in the patch, so it is never deleted', async () => {
      const detail = useThingsNode();
      await detail.open(note.id);
      await editThenSave(detail);

      const sent = writeNode.mock.calls[0][0].properties;
      for (const key of ['pinned', 'full_width', 'linked_projects', 'node_id']) {
        expect(key in sent, `${key} must not appear in the patch at all`).toBe(false);
      }
    });

    /**
     * A save that changes nothing has to change nothing, twice over: the
     * second save reads `loadedKeys` written by the first, and a first save
     * that forgot the hidden keys would let the second one delete them.
     */
    it('survives a second save', async () => {
      const detail = useThingsNode();
      await detail.open(note.id);
      await editThenSave(detail);
      await editThenSave(detail);

      const sent = writeNode.mock.calls[1][0].properties;
      expect('pinned' in sent).toBe(false);
      expect(sent.tags).toEqual(['mdp', 'network']);
    });

    /**
     * The other half of the corruption: a visible field nobody touched has to
     * go back as the value it was, not as a re-reading of its own text.
     */
    it('writes an untouched value back unchanged', async () => {
      getNode.mockResolvedValue({
        ...note,
        node_type: 'animal',
        properties: { node_id: 'x', type: 'animal', vaccinated: false, weight: 4.2 },
      });
      const detail = useThingsNode();
      await detail.open(note.id);
      await editThenSave(detail);

      const sent = writeNode.mock.calls[0][0].properties;
      // `false`, not the string "false" — which YAML writes as 'false' and
      // JavaScript then reads as true.
      expect(sent.vaccinated).toBe(false);
      expect(typeof sent.vaccinated).toBe('boolean');
      expect(sent.weight).toBe(4.2);
      expect(typeof sent.weight).toBe('number');
    });
  });

  /**
   * Creating used to write the file first and ask for the name afterwards.
   *
   * That order is why the vault holds nodes with an empty title, and why it
   * holds a `type: abc` with one untitled member: the file landed the moment a
   * kind was named, so every abandoned create left a husk and every slip of
   * the keyboard minted a permanent kind. A draft is the same screen with
   * nothing committed.
   */
  describe('a node being composed', () => {
    beforeEach(() => {
      getNode.mockReset();
      writeNode.mockReset();
    });

    it('writes nothing when it is opened', async () => {
      const detail = useThingsNode();
      detail.startDraft('animal');

      expect(writeNode).not.toHaveBeenCalled();
      expect(detail.nodeType.value).toBe('animal');
    });

    /** The whole point: leaving without typing leaves no file behind. */
    it('writes nothing when it is abandoned', async () => {
      const detail = useThingsNode();
      detail.startDraft('animal');

      expect(await detail.commitDraft()).toBeNull();
      // A blur on an empty field is the ordinary way out, and it saves too.
      await detail.save();

      expect(writeNode).not.toHaveBeenCalled();
    });

    it('writes nothing for a title of pure whitespace', async () => {
      const detail = useThingsNode();
      detail.startDraft('animal');
      detail.title.value = '   ';

      expect(await detail.commitDraft()).toBeNull();
      expect(writeNode).not.toHaveBeenCalled();
    });

    it('commits once there is something to keep', async () => {
      const detail = useThingsNode();
      detail.startDraft('animal');
      detail.title.value = 'Mèo Mun';

      const created = await detail.commitDraft();

      expect(created).toMatch(/^Animal\/.+\.md$/);
      const sent = writeNode.mock.calls[0][0];
      expect(sent.nodeType).toBe('animal');
      expect(sent.title).toBe('Mèo Mun');
      expect(sent.eventType).toBe('created');
      expect(detail.draft.value, 'no longer a draft once it is on disk').toBeNull();
    });

    /** A body with no title is still somebody's writing. */
    it('commits on a body alone', async () => {
      const detail = useThingsNode();
      detail.startDraft('note');
      detail.body.value = 'Nhặt được ở ngõ.';

      expect(await detail.commitDraft()).not.toBeNull();
      expect(writeNode.mock.calls[0][0].content).toBe('Nhặt được ở ngõ.');
    });

    /**
     * Changing the kind is free while nothing is written, which is what lets
     * creating start with the name of the thing rather than a question about
     * its kind. The folder follows the kind, so getting this wrong after the
     * file existed would mean moving it.
     */
    it('changes kind before anything lands, folder and all', async () => {
      const detail = useThingsNode();
      detail.startDraft('note');
      detail.setDraftType('animal');
      detail.title.value = 'Mèo Mun';

      const created = await detail.commitDraft();

      expect(created).toMatch(/^Animal\//);
      expect(writeNode.mock.calls[0][0].nodeType).toBe('animal');
    });

    it('refuses to change kind once the node is real', async () => {
      getNode.mockResolvedValue(node);
      const detail = useThingsNode();
      await detail.open(node.id);

      detail.setDraftType('task');

      expect(detail.nodeType.value, 'an existing node keeps its kind').toBe('animal');
    });

    /** Fields typed into a draft belong to the file it creates. */
    it('carries the fields typed before the first save', async () => {
      const detail = useThingsNode();
      detail.startDraft('animal');
      detail.addField();
      detail.fields.value[0] = { key: 'species', value: 'Mèo', kind: 'text', original: undefined };

      await detail.commitDraft();

      expect(writeNode.mock.calls[0][0].properties).toEqual({ species: 'Mèo' });
    });
  });

  /**
   * The kind's shape, offered rather than imposed.
   *
   * Every rule here is the same rule seen from a different side: the rows are
   * an offer. They arrive filled in with nothing, they cost nothing to ignore,
   * and ignoring them has to be indistinguishable from never having opened the
   * screen.
   */
  describe('the fields a kind usually has', () => {
    beforeEach(() => {
      getNode.mockReset();
      writeNode.mockReset();
    });

    it('lays them out ready to fill in', () => {
      const detail = useThingsNode();
      detail.startDraft('animal', [{ key: 'species', kind: 'text' }, { key: 'colour', kind: 'text' }]);

      expect(detail.fields.value.map(f => f.key)).toEqual(['species', 'colour']);
      expect(detail.fields.value.every(f => f.value === '')).toBe(true);
    });

    /**
     * The one that matters. Offering four fields must not turn walking away
     * into a file — which is exactly what counting a named-but-empty row would
     * do, since a fresh draft would arrive already looking non-empty.
     */
    it('is still nothing when every offer is left alone', async () => {
      const detail = useThingsNode();
      detail.startDraft('animal', [{ key: 'species', kind: 'text' }, { key: 'colour', kind: 'text' }, { key: 'vaccinated_at', kind: 'text' }]);

      expect(await detail.commitDraft()).toBeNull();
      expect(writeNode).not.toHaveBeenCalled();
    });

    it('writes only the offers somebody filled in', async () => {
      const detail = useThingsNode();
      detail.startDraft('animal', [{ key: 'species', kind: 'text' }, { key: 'colour', kind: 'text' }, { key: 'vaccinated_at', kind: 'text' }]);
      detail.title.value = 'Vẹt vàng';
      detail.fields.value[1].value = 'vàng';

      await detail.commitDraft();

      expect(writeNode.mock.calls[0][0].properties).toEqual({ colour: 'vàng' });
    });

    /**
     * Changing the kind swaps the offer and keeps the answer: somebody who
     * typed a colour before noticing they picked the wrong kind meant that
     * colour either way.
     */
    it('keeps what was typed when the kind changes under it', () => {
      const detail = useThingsNode();
      detail.startDraft('note', [{ key: 'tags', kind: 'text' }]);
      detail.fields.value[0].value = 'mdp';

      detail.setDraftType('animal', [{ key: 'species', kind: 'text' }, { key: 'colour', kind: 'text' }]);

      const rows = detail.fields.value;
      expect(rows[0], 'the answer survives').toMatchObject({ key: 'tags', value: 'mdp' });
      expect(rows.map(f => f.key)).toEqual(['tags', 'species', 'colour']);
    });

    it('does not offer the same key twice when kinds share one', () => {
      const detail = useThingsNode();
      detail.startDraft('note', [{ key: 'tags', kind: 'text' }]);
      detail.fields.value[0].value = 'mdp';

      detail.setDraftType('animal', [{ key: 'tags', kind: 'text' }, { key: 'species', kind: 'text' }]);

      expect(detail.fields.value.map(f => f.key)).toEqual(['tags', 'species']);
    });
  });

  /**
   * Changing a kind's declared field type converts nothing.
   *
   * A field declared `date` and later declared `text`, with nodes already
   * holding dates: the declaration says how to draw an *empty* box, and every
   * node that has a value takes its kind from the value. Nothing reads the
   * schema on the way in or out — not the reader, not the writer, and nothing
   * in Rust at all — so five dates stay five dates.
   *
   * The rule is the same one `valueOf` enforces for values: the file is the
   * truth, and anything that disagrees with it is wrong about it.
   */
  describe('a declared kind that no longer matches the values', () => {
    beforeEach(() => {
      getNode.mockReset();
      writeNode.mockReset();
    });

    const dated = {
      id: 'Tasks/a.md',
      node_type: 'task',
      title: 'Chaos Testing',
      content: '',
      properties: { node_id: 'x', type: 'task', due_date: '2026-07-09' },
    };

    it('draws an existing value by what it is, not by what was declared', async () => {
      getNode.mockResolvedValue(dated);
      const detail = useThingsNode();
      await detail.open(dated.id);

      const row = detail.fields.value.find(f => f.key === 'due_date');
      expect(row?.kind, 'the value is a date whatever the schema says').toBe('date');
    });

    it('writes it back untouched', async () => {
      getNode.mockResolvedValue(dated);
      const detail = useThingsNode();
      await detail.open(dated.id);
      await editThenSave(detail);

      const sent = writeNode.mock.calls[0][0].properties;
      expect(sent.due_date).toBe('2026-07-09');
      expect(typeof sent.due_date).toBe('string');
    });

    /** Where the declaration does reach: a box with nothing in it yet. */
    it('draws an empty one the way the kind was declared', () => {
      const detail = useThingsNode();
      detail.startDraft('task', [{ key: 'due_date', kind: 'text' }]);

      expect(detail.fields.value[0].kind).toBe('text');

      detail.startDraft('task', [{ key: 'due_date', kind: 'date' }]);
      expect(detail.fields.value[0].kind).toBe('date');
    });
  });

  /**
   * The panel asks for a save every time focus leaves anything — a field, the
   * body. Most of those blurs follow no edit, and `save()` used to write the
   * file regardless: open a node, click once, and it had a new `updated_at`, a
   * new entry in the CRDT log and a sync payload, for a file nobody touched.
   */
  describe('a save that has nothing to say', () => {
    it('does not write when nothing was edited', async () => {
      const detail = await openIt();

      await detail.save();
      await detail.save();

      expect(writeNode, 'a blur is not an edit').not.toHaveBeenCalled();
    });

    it('writes once for an edit, and not again for the blur after it', async () => {
      const detail = await openIt();

      detail.body.value = 'Nhặt được ở ngõ. Sợ mưa.';
      await detail.save();
      expect(writeNode).toHaveBeenCalledTimes(1);

      await detail.save();
      expect(writeNode, 'the file already says this').toHaveBeenCalledTimes(1);
    });

    it('notices a title, a value, a new field and a removed one', async () => {
      const detail = await openIt();
      let writes = 0;

      const change = async (what: string, edit: () => void) => {
        edit();
        await detail.save();
        expect(writeNode, `${what} was not noticed`).toHaveBeenCalledTimes(++writes);
      };

      await change('the title', () => { detail.title.value = 'Mèo Mun II'; });
      await change('a value', () => {
        detail.fields.value.find(f => f.key === 'colour')!.value = 'xám';
      });
      await change('a renamed key', () => {
        detail.fields.value.find(f => f.key === 'species')!.key = 'loài';
      });
      await change('a new field', () => { detail.addField(); });
      await change('a removed field', () => { detail.removeField(0); });
    });

    /**
     * A write that threw leaves the file behind the panel. Marking it clean
     * there would make every later blur decline to try again, and the node
     * would sit unsaved with no sign of it.
     */
    it('stays dirty when the write failed', async () => {
      const detail = await openIt();
      writeNode.mockRejectedValueOnce(new Error('disk is full'));

      detail.title.value = 'Mèo Mun II';
      await detail.save();
      expect(writeNode).toHaveBeenCalledTimes(1);
      expect(detail.error.value).toContain('disk is full');

      await detail.save();
      expect(writeNode, 'a failed save must be retried').toHaveBeenCalledTimes(2);
    });

    /** Reverting by hand is a change like any other. */
    it('writes again when an edit is undone', async () => {
      const detail = await openIt();

      detail.title.value = 'Mèo Mun II';
      await detail.save();
      detail.title.value = 'Mèo Mun';
      await detail.save();

      expect(writeNode).toHaveBeenCalledTimes(2);
      expect(writeNode.mock.calls[1][0].title).toBe('Mèo Mun');
    });
  });
});
