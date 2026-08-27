import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ref } from 'vue';
import { useNoteSave } from '../useNoteSave';
import type { NoteItem } from '../../helpers';

const note = (id: string): NoteItem => ({
  id, title: id, summary: '', date: '2026-01-01', path: id,
  tags: [], pinned: false, full_width: false,
});

const harness = (ids: string[] = ['Notes/a.md']) => {
  const notes = ref<NoteItem[]>(ids.map(note));
  const currentNoteId = ref<string | null>(ids[0] ?? null);
  const tabContents = ref<Record<string, string>>({});
  const renamedTabs = new Map<string, string>();
  const writeNode = vi.fn().mockResolvedValue(undefined);
  const emit = vi.fn();

  const api = useNoteSave(
    notes, currentNoteId, tabContents, renamedTabs,
    { writeNode }, { emit },
  );

  return { api, notes, tabContents, renamedTabs, writeNode, emit };
};

describe('useNoteSave', () => {
  beforeEach(() => { vi.useFakeTimers(); });
  afterEach(() => { vi.useRealTimers(); });

  describe('resolveTabId', () => {
    it('follows a rename to where the file actually is', () => {
      const h = harness();
      h.renamedTabs.set('Notes/old.md', 'Notes/new.md');

      expect(h.api.resolveTabId('Notes/old.md')).toBe('Notes/new.md');
    });

    it('follows a chain of renames to the end', () => {
      const h = harness();
      h.renamedTabs.set('a.md', 'b.md');
      h.renamedTabs.set('b.md', 'c.md');

      expect(h.api.resolveTabId('a.md')).toBe('c.md');
    });

    it('stops instead of walking a loop forever', () => {
      // Rename a note, then rename it back: the map holds `a → b` and `b → a`.
      // Walking that used to freeze the app the next time anything was typed.
      // `useNoteRename` no longer builds such a map, but the walk must survive
      // one however it arises.
      const h = harness();
      h.renamedTabs.set('a.md', 'b.md');
      h.renamedTabs.set('b.md', 'a.md');

      // Reaching this line at all is the assertion.
      expect(['a.md', 'b.md']).toContain(h.api.resolveTabId('a.md'));
    });

    it('leaves a path that was never renamed alone', () => {
      const h = harness();
      expect(h.api.resolveTabId('Notes/a.md')).toBe('Notes/a.md');
    });
  });

  describe('saving', () => {
    it('waits before writing, so a burst of typing is one save', async () => {
      const h = harness();

      h.api.onEditorUpdate('one', 'Notes/a.md');
      h.api.onEditorUpdate('one two', 'Notes/a.md');
      h.api.onEditorUpdate('one two three', 'Notes/a.md');
      expect(h.writeNode).not.toHaveBeenCalled();

      await vi.advanceTimersByTimeAsync(1000);

      expect(h.writeNode).toHaveBeenCalledTimes(1);
      expect(h.writeNode.mock.calls[0][0].content).toBe('one two three');
    });

    it('sends a save for a renamed tab to the new path', async () => {
      // The editor still reports the tab under the id it was opened with.
      const h = harness(['Notes/new.md']);
      h.renamedTabs.set('Notes/old.md', 'Notes/new.md');

      h.api.onEditorUpdate('body', 'Notes/old.md');
      await vi.advanceTimersByTimeAsync(1000);

      expect(h.writeNode).toHaveBeenCalledTimes(1);
      expect(h.writeNode.mock.calls[0][0].relPath).toBe('Notes/new.md');
      expect(h.tabContents.value['Notes/new.md']).toBe('body');
      expect(h.tabContents.value['Notes/old.md']).toBeUndefined();
    });

    it('asks the editor to finish serialising before reading the body', async () => {
      // A save arriving from somewhere other than typing — a rename — would
      // otherwise write the note as it stood before the last keystroke.
      const h = harness();
      const flushSerialize = vi.fn(() => { h.tabContents.value['Notes/a.md'] = 'flushed'; });
      h.api.editorRefs.value['Notes/a.md'] = { flushSerialize };

      h.api.saveNoteForTab('Notes/a.md');
      await vi.advanceTimersByTimeAsync(1000);

      expect(flushSerialize).toHaveBeenCalled();
      expect(h.writeNode.mock.calls[0][0].content).toBe('flushed');
    });

    it('does not write a note that is no longer in the list', async () => {
      // A note deleted while its undo window runs is out of the list but its
      // file is still there; writing it back would undelete it.
      const h = harness([]);

      h.api.saveNoteForTab('Notes/gone.md');
      await vi.advanceTimersByTimeAsync(1000);

      expect(h.writeNode).not.toHaveBeenCalled();
    });

    it('keeps the list preview in step with what was saved', async () => {
      const h = harness();

      h.api.onEditorUpdate('a new opening line', 'Notes/a.md');
      await vi.advanceTimersByTimeAsync(1000);

      expect(h.notes.value[0].summary).toBe('a new opening line');
    });
  });
});
