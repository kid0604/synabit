import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ref, defineComponent, h } from 'vue';
import { mount } from '@vue/test-utils';
import { useNoteDelete } from '../useNoteDelete';
import type { NoteItem } from '../../helpers';

const note = (id: string, title: string): NoteItem => ({
  id, title, summary: '', date: '2026-01-01', path: id,
  tags: [], pinned: false, full_width: false,
});

/**
 * Drive the composable inside a real component, so `onUnmounted` is wired up —
 * leaving the Notes app is one of the ways a pending delete has to resolve.
 */
const harness = (overrides: Partial<Parameters<typeof useNoteDelete>[0]> = {}) => {
  const notes = ref<NoteItem[]>([note('Notes/a.md', 'A'), note('Notes/b.md', 'B'), note('Notes/c.md', 'C')]);
  const currentNoteId = ref<string | null>('Notes/b.md');
  const trashNode = vi.fn().mockResolvedValue('.trash/Notes/b.md');
  const scanVault = vi.fn().mockResolvedValue(undefined);
  const onFailed = vi.fn();

  const params = {
    notes,
    currentNoteId,
    recentNoteIds: ref<string[]>(['Notes/b.md']),
    tabContents: ref<Record<string, string>>({ 'Notes/b.md': 'body' }),
    activeTabs: ref<string[]>(['Notes/b.md']),
    tabAccessTime: new Map<string, number>([['Notes/b.md', 1]]),
    saveTimeouts: new Map<string, ReturnType<typeof setTimeout>>(),
    ns: { trashNode },
    scanVault,
    onFailed,
    ...overrides,
  };

  let api!: ReturnType<typeof useNoteDelete>;
  const wrapper = mount(defineComponent({
    setup() { api = useNoteDelete(params); return () => h('div'); },
  }));

  return { ...params, api: api!, wrapper, trashNode, scanVault, onFailed, notes, currentNoteId };
};

describe('useNoteDelete', () => {
  beforeEach(() => { vi.useFakeTimers(); });
  afterEach(() => { vi.useRealTimers(); });

  it('takes the note out of the list without touching the file yet', async () => {
    // The whole design rests on this. Sync spots a deletion the moment the
    // file moves, so an undo after that races a tombstone already in flight.
    const h = harness();
    await h.api.deleteNote('Notes/b.md');

    expect(h.notes.value.map((n) => n.id)).toEqual(['Notes/a.md', 'Notes/c.md']);
    expect(h.trashNode).not.toHaveBeenCalled();
    expect(h.api.pending.value?.notes.map((e) => e.note.title)).toEqual(['B']);
  });

  it('puts the note back where it was, and never trashes it', async () => {
    const h = harness();
    await h.api.deleteNote('Notes/b.md');
    h.api.undoDelete();

    // Back in the middle, not on top: a note that jumps position on being
    // restored reads as a different note.
    expect(h.notes.value.map((n) => n.id)).toEqual(['Notes/a.md', 'Notes/b.md', 'Notes/c.md']);
    expect(h.currentNoteId.value).toBe('Notes/b.md');

    await vi.advanceTimersByTimeAsync(30_000);
    expect(h.trashNode).not.toHaveBeenCalled();
  });

  it('moves the file once the window closes', async () => {
    const h = harness();
    await h.api.deleteNote('Notes/b.md');

    await vi.advanceTimersByTimeAsync(30_000);

    expect(h.trashNode).toHaveBeenCalledWith({ relPath: 'Notes/b.md' });
    expect(h.api.pending.value).toBeNull();
  });

  it('hides a pending note from a rescan', async () => {
    // A pending note is still on disk and the file watcher triggers plenty of
    // rescans, so without this it reappears under the toast offering to undo it.
    const h = harness();
    await h.api.deleteNote('Notes/b.md');

    expect(h.api.isHidden('Notes/b.md')).toBe(true);

    h.api.undoDelete();
    expect(h.api.isHidden('Notes/b.md')).toBe(false);
  });

  it('finishes the first delete when a second one starts', async () => {
    // One pending at a time, so the toast never offers to bring back a note
    // other than the one it names.
    const h = harness();
    await h.api.deleteNote('Notes/b.md');
    await h.api.deleteNote('Notes/a.md');

    expect(h.trashNode).toHaveBeenCalledWith({ relPath: 'Notes/b.md' });
    expect(h.trashNode).toHaveBeenCalledTimes(1);
    expect(h.api.pending.value?.notes.map((e) => e.note.id)).toEqual(['Notes/a.md']);
  });

  it('deletes a whole batch under one undo window', async () => {
    // The case it was built for: a sync that left several copies of one day.
    const h = harness();
    await h.api.deleteNotes(['Notes/a.md', 'Notes/c.md']);

    expect(h.notes.value.map((n) => n.id)).toEqual(['Notes/b.md']);
    expect(h.trashNode).not.toHaveBeenCalled();
    expect(h.api.pending.value?.notes).toHaveLength(2);

    await vi.advanceTimersByTimeAsync(30_000);
    expect(h.trashNode).toHaveBeenCalledWith({ relPath: 'Notes/a.md' });
    expect(h.trashNode).toHaveBeenCalledWith({ relPath: 'Notes/c.md' });
    expect(h.trashNode).toHaveBeenCalledTimes(2);
  });

  it('puts every note in the batch back on its own row', async () => {
    // Reinserting out of order is how the second note lands one place along:
    // each insert shifts everything after it.
    const h = harness();
    await h.api.deleteNotes(['Notes/c.md', 'Notes/a.md']);
    h.api.undoDelete();

    expect(h.notes.value.map((n) => n.id)).toEqual(['Notes/a.md', 'Notes/b.md', 'Notes/c.md']);
    await vi.advanceTimersByTimeAsync(30_000);
    expect(h.trashNode).not.toHaveBeenCalled();
  });

  it('moves the rest of the batch when one note cannot be moved', async () => {
    // A batch that gave up halfway would leave the list and the disk
    // disagreeing about which notes still exist.
    const trashNode = vi.fn(({ relPath }: { relPath: string }) =>
      relPath === 'Notes/a.md' ? Promise.reject(new Error('locked')) : Promise.resolve('.trash/' + relPath));
    const h = harness({ ns: { trashNode } });

    await h.api.deleteNotes(['Notes/a.md', 'Notes/c.md']);
    await vi.advanceTimersByTimeAsync(30_000);

    expect(trashNode).toHaveBeenCalledTimes(2);
    expect(h.onFailed).toHaveBeenCalledTimes(1);
    expect(h.onFailed).toHaveBeenCalledWith(expect.objectContaining({ id: 'Notes/a.md' }));
    // The rescan has to run before anyone is told, or the note it puts back is
    // announced as missing while the list still says it is gone.
    expect(h.scanVault).toHaveBeenCalled();
  });

  it('moves off a note being edited only when it is in the batch', async () => {
    const h = harness();
    await h.api.deleteNotes(['Notes/a.md', 'Notes/c.md']);
    expect(h.currentNoteId.value).toBe('Notes/b.md');

    await h.api.deleteNotes(['Notes/b.md']);
    expect(h.currentNoteId.value).toBeNull();
  });

  it('cancels a queued autosave so it cannot write the note back', async () => {
    const saveTimeouts = new Map<string, ReturnType<typeof setTimeout>>();
    const write = vi.fn();
    // jsdom's `setTimeout` hands back a number; the app's Map is typed from
    // Node's, which is an object. Same value either way at run time.
    saveTimeouts.set('Notes/b.md', setTimeout(write, 600) as unknown as ReturnType<typeof setTimeout>);

    const h = harness({ saveTimeouts });
    await h.api.deleteNote('Notes/b.md');
    await vi.advanceTimersByTimeAsync(30_000);

    expect(write).not.toHaveBeenCalled();
    expect(saveTimeouts.has('Notes/b.md')).toBe(false);
  });

  it('says so when the file could not be moved after all', async () => {
    // The note left the list when the delete was asked for, so a silent
    // failure looks exactly like success until the next restart.
    const trashNode = vi.fn().mockRejectedValue(new Error('read-only vault'));
    const h = harness({ ns: { trashNode } });

    await h.api.deleteNote('Notes/b.md');
    await vi.advanceTimersByTimeAsync(30_000);

    expect(h.onFailed).toHaveBeenCalledWith(expect.objectContaining({ id: 'Notes/b.md' }));
    expect(h.scanVault).toHaveBeenCalled();
  });

  it('completes a pending delete rather than dropping it on unmount', async () => {
    // Leaving the Notes app is not taking the delete back.
    const h = harness();
    await h.api.deleteNote('Notes/b.md');

    h.wrapper.unmount();
    await vi.advanceTimersByTimeAsync(0);

    expect(h.trashNode).toHaveBeenCalledWith({ relPath: 'Notes/b.md' });
  });
});
