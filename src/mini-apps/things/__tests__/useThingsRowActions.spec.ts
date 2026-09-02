import { describe, it, expect, vi, beforeEach } from 'vitest';

// `vi.hoisted`, because a `vi.mock` factory is lifted above every `const` in
// the file — without it the mock runs before these exist.
const { getNode, writeNode, trashNode, invoke } = vi.hoisted(() => ({
  getNode: vi.fn(),
  writeNode: vi.fn(),
  trashNode: vi.fn(),
  invoke: vi.fn(),
}));

vi.mock('../../../composables/useNodeService', () => ({
  useNodeService: () => ({ getNode, writeNode, trashNode }),
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));

import { useThingsRowActions, UNDO_WINDOW_SECONDS } from '../composables/useThingsRowActions';

describe('what a row’s menu does to a node', () => {
  beforeEach(() => {
    getNode.mockReset();
    writeNode.mockReset();
    trashNode.mockReset();
    invoke.mockReset();
  });

  const book = {
    id: 'Book/sapiens.md',
    node_type: 'book',
    title: 'Sapiens',
    content: 'Chương về nông nghiệp.',
    properties: { node_id: 'stable-identity', author: 'Harari', rating: 5 },
  };

  /**
   * The copy is a different thing and must not claim the original's identity.
   *
   * `node_id` is what the sync engine calls a file. Two files carrying one
   * identity is one document in two places: the copy published under that id
   * comes back claiming a path that belongs to the original, and the
   * coordinator starts setting files aside as conflicts.
   */
  it('gives a duplicate everything except the original’s identity', async () => {
    getNode.mockResolvedValue(book);
    const actions = useThingsRowActions(() => '/vault');

    const made = await actions.duplicate(book.id);

    const written = writeNode.mock.calls[0][0];
    expect(written.properties.author).toBe('Harari');
    expect(written.properties.rating).toBe(5);
    expect('node_id' in written.properties).toBe(false);
    expect(written.content).toBe('Chương về nông nghiệp.');
    expect(made).toMatch(/^Book\/.+\.md$/);
  });

  /** From the node, never a constant — the rule every writer here keeps. */
  it('duplicates into the same type and the type’s own folder', async () => {
    getNode.mockResolvedValue(book);
    const actions = useThingsRowActions(() => '/vault');
    await actions.duplicate(book.id);

    expect(writeNode.mock.calls[0][0].nodeType).toBe('book');
    expect(writeNode.mock.calls[0][0].relPath.startsWith('Book/')).toBe(true);
  });

  it('names the copy so the two can be told apart', async () => {
    getNode.mockResolvedValue(book);
    const actions = useThingsRowActions(() => '/vault');
    await actions.duplicate(book.id);
    expect(writeNode.mock.calls[0][0].title).toBe('Sapiens (copy)');
  });

  /**
   * Trash, not unlink. A node is usually the only copy of something and the
   * gesture that loses it is a mis-aimed click on a small icon in a menu.
   */
  it('trashes rather than deleting, and holds on to where it went', async () => {
    trashNode.mockResolvedValue('.trash/Book/sapiens.md');
    const actions = useThingsRowActions(() => '/vault');

    await actions.remove(book.id, 'Sapiens');

    expect(trashNode).toHaveBeenCalledWith({ relPath: book.id });
    expect(actions.trashed.value?.title).toBe('Sapiens');
    expect(actions.trashed.value?.trashPath).toBe('.trash/Book/sapiens.md');
  });

  it('puts it back from where it went', async () => {
    trashNode.mockResolvedValue('.trash/Book/sapiens.md');
    const actions = useThingsRowActions(() => '/vault');
    await actions.remove(book.id, 'Sapiens');

    await actions.undoRemove();

    expect(invoke).toHaveBeenCalledWith('restore_from_trash', {
      vaultPath: '/vault',
      trashPath: '.trash/Book/sapiens.md',
    });
    expect(actions.trashed.value).toBeNull();
  });

  /** A failed delete must not leave an undo offering to restore nothing. */
  it('offers no undo when the delete did not happen', async () => {
    trashNode.mockRejectedValue(new Error('read-only vault'));
    const actions = useThingsRowActions(() => '/vault');

    await actions.remove(book.id, 'Sapiens');

    expect(actions.trashed.value).toBeNull();
  });

  /**
   * The offer to undo has to end on its own.
   *
   * It did not. `dismissUndo` existed and nothing ever called it, so the
   * countdown bar emptied and the toast stayed — telling somebody the undo was
   * still there for as long as the screen was open, hours after it meant
   * anything. The bar and the offer now run off the same number.
   */
  it('stops offering the undo once the window has passed', async () => {
    vi.useFakeTimers();
    try {
      trashNode.mockResolvedValue('.trash/sapiens.md');
      const actions = useThingsRowActions(() => '/vault');

      await actions.remove(book.id, book.title);
      expect(actions.trashed.value?.title).toBe('Sapiens');

      vi.advanceTimersByTime(UNDO_WINDOW_SECONDS * 1000 - 1);
      expect(actions.trashed.value, 'still offered a moment before the end').not.toBeNull();

      vi.advanceTimersByTime(2);
      expect(actions.trashed.value).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  /**
   * A second delete restarts the offer rather than inheriting the first one's
   * remaining time, which would cut it short by however long ago the first was.
   */
  it('gives the second deletion a full window of its own', async () => {
    vi.useFakeTimers();
    try {
      trashNode.mockResolvedValue('.trash/x.md');
      const actions = useThingsRowActions(() => '/vault');

      await actions.remove('Book/a.md', 'A');
      vi.advanceTimersByTime(UNDO_WINDOW_SECONDS * 1000 - 500);
      await actions.remove('Book/b.md', 'B');

      vi.advanceTimersByTime(600);
      expect(actions.trashed.value?.title, 'the first timer did not end the second offer').toBe('B');

      vi.advanceTimersByTime(UNDO_WINDOW_SECONDS * 1000);
      expect(actions.trashed.value).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  /** Taking the undo ends the offer too, without waiting for the timer. */
  it('clears the offer when the undo is taken', async () => {
    trashNode.mockResolvedValue('.trash/sapiens.md');
    invoke.mockResolvedValue(undefined);
    const actions = useThingsRowActions(() => '/vault');

    await actions.remove(book.id, book.title);
    await actions.undoRemove();

    expect(actions.trashed.value).toBeNull();
    expect(invoke).toHaveBeenCalledWith('restore_from_trash', {
      vaultPath: '/vault',
      trashPath: '.trash/sapiens.md',
    });
  });

  /**
   * Pinning a node of any kind, without disturbing the rest of it.
   *
   * A patch naming one key. Writing the node back whole is how a pin loses a
   * field on a kind nobody wrote code for — the writer would have to know what
   * a `book` holds, and it does not.
   */
  describe('pinning', () => {
    it('names only the key it is setting', async () => {
      getNode.mockResolvedValue(book);
      const actions = useThingsRowActions(() => '/vault');

      await actions.setPinned(book.id, true);

      const sent = writeNode.mock.calls[0][0];
      expect(sent.properties).toEqual({ pinned: true });
      expect(sent.relPath).toBe(book.id);
    });

    it('takes the kind from the node rather than assuming one', async () => {
      getNode.mockResolvedValue(book);
      const actions = useThingsRowActions(() => '/vault');

      await actions.setPinned(book.id, true);

      expect(writeNode.mock.calls[0][0].nodeType).toBe('book');
    });

    it('unpins by naming the same key the other way', async () => {
      getNode.mockResolvedValue(book);
      const actions = useThingsRowActions(() => '/vault');

      await actions.setPinned(book.id, false);

      expect(writeNode.mock.calls[0][0].properties).toEqual({ pinned: false });
    });

    /** A node that vanished between the click and the write. */
    it('writes nothing when the node is gone', async () => {
      getNode.mockResolvedValue(null);
      const actions = useThingsRowActions(() => '/vault');

      await actions.setPinned('Book/gone.md', true);

      expect(writeNode).not.toHaveBeenCalled();
    });
  });
});
