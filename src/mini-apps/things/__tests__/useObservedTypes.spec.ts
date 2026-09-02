import { describe, it, expect, vi, beforeEach } from 'vitest';

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import { useObservedTypes, isInternalType } from '../composables/useObservedTypes';

/**
 * A type's shape, worked out from the vault rather than declared anywhere.
 *
 * The rule this pins down is that the answer is an *offer*. Nothing here
 * decides what a node may contain — a key below the threshold is still real,
 * still on the file, still shown on the node. It simply is not what the app
 * puts in front of somebody making a new one.
 */
describe('what a type usually looks like', () => {
  beforeEach(() => invoke.mockReset());

  const withVault = async (types: unknown[]) => {
    invoke.mockResolvedValue(types);
    const observed = useObservedTypes();
    await observed.load();
    return observed;
  };

  /**
   * The vault this threshold was chosen against: `colour` on two animals of
   * four, `màu` on one. Two keys for one idea, because nothing showed the
   * first when the second was typed.
   */
  const animals = {
    node_type: 'animal',
    count: 4,
    fields: [
      { key: 'type', count: 4 },
      { key: 'node_id', count: 4 },
      { key: 'species', count: 2 },
      { key: 'colour', count: 2 },
      { key: 'vaccinated_at', count: 2 },
      { key: 'màu', count: 1 },
    ],
  };

  it('keeps the keys half the type carries, and drops the stray', async () => {
    const observed = await withVault([animals]);

    expect(observed.usualFieldsFor('animal')).toEqual(['species', 'colour', 'vaccinated_at']);
  });

  /**
   * A stray is hidden from the offer, never from the vault. Dropping it here
   * would make the app lie about what is in the file.
   */
  it('still reports the stray when asked what is actually there', async () => {
    const observed = await withVault([animals]);

    expect(observed.fieldsFor('animal')).toContain('màu');
    expect(observed.observedFor('animal').find(f => f.key === 'màu')?.count).toBe(1);
  });

  /**
   * `order` is on every task and belongs to a drag. Offering it as something
   * to fill in would be offering to type a sort position by hand.
   */
  it('leaves out the keys the app writes for itself', async () => {
    const observed = await withVault([{
      node_type: 'task',
      count: 100,
      fields: [
        { key: 'status', count: 99 },
        { key: 'order', count: 98 },
        { key: 'is_transferred', count: 99 },
        { key: 'due_date', count: 99 },
        { key: 'checklist', count: 20 },
      ],
    }]);

    expect(observed.usualFieldsFor('task')).toEqual(['status', 'due_date']);
  });

  /**
   * The same key on a type nobody wrote code for is nobody's business but the
   * person who put it there — `fieldRegistry` scopes its list by type, and
   * this is where that scoping pays off.
   */
  it('offers a key on one type that it hides on another', async () => {
    // `full_width` and not `pinned`: pinning became an affordance on every
    // kind, so its key is the app's everywhere and no longer an example of
    // anything scoped.
    const observed = await withVault([{
      node_type: 'animal',
      count: 2,
      fields: [{ key: 'full_width', count: 2 }],
    }]);

    expect(observed.usualFieldsFor('animal')).toEqual(['full_width']);
  });

  it('has nothing to offer for a type it has never seen', async () => {
    const observed = await withVault([animals]);

    expect(observed.usualFieldsFor('spaceship')).toEqual([]);
    expect(observed.observedFor('spaceship')).toEqual([]);
  });

  /** A type with no nodes cannot divide by its own count. */
  it('survives a type with nothing in it', async () => {
    const observed = await withVault([{ node_type: 'ghost', count: 0, fields: [] }]);

    expect(observed.usualFieldsFor('ghost')).toEqual([]);
  });

  /**
   * The kind of a field, read from the vault rather than assumed.
   *
   * Writing a shape down used to declare every field `text`, because nothing
   * carried what the values were — so `Schema/task.md` said `due_date` was
   * text on a hundred dated tasks, and a new task drew a text box where a date
   * picker belonged. `kindOf` was always the one thing that decides; it simply
   * had nothing to look at.
   */
  describe('what kind of value a key holds', () => {
    const taskVault = () => withVault([{
      node_type: 'task',
      count: 3,
      fields: [
        { key: 'due_date', count: 3, sample: '2026-07-09' },
        { key: 'priority', count: 3, sample: 3 },
        { key: 'track_progress', count: 3, sample: false },
        { key: 'tags', count: 3, sample: [] },
        { key: 'comment', count: 3, sample: 'một câu' },
      ],
    }]);

    it('reads it from the sample the vault reported', async () => {
      const observed = await taskVault();

      expect(observed.kindOfField('task', 'due_date')).toBe('date');
      expect(observed.kindOfField('task', 'priority')).toBe('number');
      expect(observed.kindOfField('task', 'track_progress')).toBe('boolean');
      expect(observed.kindOfField('task', 'tags')).toBe('list');
      expect(observed.kindOfField('task', 'comment')).toBe('text');
    });

    /** A key nobody has seen, and a vault reported by an older build. */
    /**
     * `null`, not `text`. A key nobody carries and a key that is a word are
     * different answers, and only the second one is grounds for telling
     * somebody their declaration disagrees with their files.
     */
    it('says nothing rather than guessing', async () => {
      const observed = await withVault([{
        node_type: 'task',
        count: 1,
        fields: [{ key: 'status', count: 1 }],
      }]);

      expect(observed.kindOfField('task', 'status'), 'no sample to read').toBeNull();
      expect(observed.kindOfField('task', 'nothing')).toBeNull();
      expect(observed.kindOfField('spaceship', 'warp')).toBeNull();
    });

    /**
     * The trap in warning about disagreements. `priority` is `''` on every
     * task in this vault: that is the absence of evidence, not evidence of
     * text, and treating it as text would argue with anybody who declared the
     * field something else on the strength of nothing at all.
     */
    it('treats a key that is empty everywhere as no evidence', async () => {
      const observed = await withVault([{
        node_type: 'task',
        count: 400,
        fields: [
          { key: 'priority', count: 400, sample: '' },
          { key: 'comment', count: 400, sample: '   ' },
          { key: 'status', count: 400, sample: 'todo' },
        ],
      }]);

      expect(observed.kindOfField('task', 'priority')).toBeNull();
      expect(observed.kindOfField('task', 'comment'), 'whitespace is not evidence').toBeNull();
      expect(observed.kindOfField('task', 'status'), 'this one does say something').toBe('text');
    });
  });
});

/**
 * The app's own filing cabinet, kept out of the list of what somebody keeps.
 *
 * `schema` and `view` are files Things writes to remember a kind's structure
 * and a saved query. They were missing from the internal list, so the moment
 * Things wrote its first schema the rail listed `schema` as a kind to browse —
 * and opening one showed its `fields` as a row of raw JSON.
 */
describe('the app’s own storage', () => {
  const vault = () => {
    invoke.mockResolvedValue([
      { node_type: 'note', count: 151, fields: [] },
      { node_type: 'schema', count: 4, fields: [] },
      { node_type: 'view', count: 2, fields: [] },
      { node_type: 'json', count: 399, fields: [] },
      { node_type: 'animal', count: 4, fields: [] },
    ]);
    const observed = useObservedTypes();
    return observed.load().then(() => observed);
  };

  it('keeps its own bookkeeping out of what you browse', async () => {
    const observed = await vault();
    const names = observed.browsable.value.map(t => t.node_type);

    expect(names).toEqual(['note', 'animal']);
  });

  /**
   * Hidden, not dropped. "What is in the vault" has one answer, and a folded
   * section is how a screen declines to lead with part of it.
   */
  it('still reports them where the answer has to be complete', async () => {
    const observed = await vault();

    expect(observed.internal.value.map(t => t.node_type)).toContain('schema');
    expect(observed.internal.value.map(t => t.node_type)).toContain('view');
    expect(observed.types.value).toHaveLength(5);
  });

  /**
   * Asked by the screens downstream, because opening one of these from the
   * folded section makes it the active kind — and everything after that used
   * to assume an active kind was a browsable one. The rail pinned `schema`
   * into the list of things somebody keeps, with a count of zero beside it,
   * and the page for managing a kind's structure offered to rename and delete
   * the app's own filing cabinet.
   */
  it('answers whether a kind is the app’s own storage', () => {
    expect(isInternalType('schema')).toBe(true);
    expect(isInternalType('view')).toBe(true);
    expect(isInternalType('json')).toBe(true);
    expect(isInternalType('finance_month')).toBe(true);

    expect(isInternalType('note')).toBe(false);
    expect(isInternalType('animal')).toBe(false);
    expect(isInternalType('book')).toBe(false);
  });
});
