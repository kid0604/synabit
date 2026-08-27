import { describe, it, expect, vi } from 'vitest';
import { ref } from 'vue';
import { useNoteTabs } from '../useNoteTabs';
import type { NoteItem } from '../../helpers';

const note = (id: string): NoteItem => ({
  id, title: id, summary: '', date: '2026-01-01', path: id,
  tags: [], pinned: false, full_width: false,
});

const harness = (ids: string[] = [], nodeType = 'note') => {
  const notes = ref<NoteItem[]>(ids.map(note));
  const currentNoteId = ref<string | null>(null);
  const getNode = vi.fn((id: string) =>
    Promise.resolve({
      id,
      node_type: nodeType,
      title: `title of ${id}`,
      content: `body of ${id}`,
      created_at: '2026-01-01',
      updated_at: '2026-01-02',
      properties: { tags: ['x'], pinned: true, full_width: false },
    }),
  );
  const touchNoteSession = vi.fn();

  const api = useNoteTabs(notes, currentNoteId, { getNode }, { touchNoteSession });
  return { api, notes, currentNoteId, getNode, touchNoteSession };
};

describe('useNoteTabs', () => {
  it('fetches a note body once and keeps it', async () => {
    const h = harness(['Notes/a.md']);

    await h.api.loadNoteFile('Notes/a.md');
    await h.api.loadNoteFile('Notes/a.md');

    expect(h.getNode).toHaveBeenCalledTimes(1);
    expect(h.api.tabContents.value['Notes/a.md']).toBe('body of Notes/a.md');
    expect(h.api.activeTabs.value).toEqual(['Notes/a.md']);
  });

  it('adds a note reached by link to the list, so the sidebar shows what is open', async () => {
    const h = harness([]);

    await h.api.loadNoteFile('Notes/linked.md');

    expect(h.notes.value.map((n) => n.id)).toEqual(['Notes/linked.md']);
    expect(h.notes.value[0].title).toBe('title of Notes/linked.md');
    expect(h.notes.value[0].pinned).toBe(true);
    expect(h.notes.value[0].tags).toEqual(['x']);
  });

  it('closes the least recently used tab once ten are open', async () => {
    const ids = Array.from({ length: 11 }, (_, i) => `Notes/${i}.md`);
    const h = harness(ids);

    for (const id of ids) await h.api.loadNoteFile(id);

    expect(h.api.activeTabs.value).toHaveLength(10);
    // The first one opened is the one that goes, and its body goes with it —
    // holding eleven note bodies is exactly what the cap exists to prevent.
    expect(h.api.activeTabs.value).not.toContain('Notes/0.md');
    expect(h.api.tabContents.value['Notes/0.md']).toBeUndefined();
    expect(h.api.activeTabs.value).toContain('Notes/10.md');
  });

  it('keeps a tab that was returned to, and drops one that was not', async () => {
    // "Least recently used", not "opened longest ago". A note kept coming back
    // to should outlive one opened later and abandoned.
    const ids = Array.from({ length: 10 }, (_, i) => `Notes/${i}.md`);
    const h = harness([...ids, 'Notes/new.md']);

    for (const id of ids) await h.api.loadNoteFile(id);
    await h.api.loadNoteFile('Notes/0.md'); // returned to
    await h.api.loadNoteFile('Notes/new.md'); // pushes one out

    expect(h.api.activeTabs.value).toContain('Notes/0.md');
    expect(h.api.activeTabs.value).not.toContain('Notes/1.md');
  });

  it('reads and writes the open note through currentContent', () => {
    const h = harness(['Notes/a.md']);
    h.currentNoteId.value = 'Notes/a.md';

    h.api.currentContent.value = 'typed';

    expect(h.api.tabContents.value['Notes/a.md']).toBe('typed');
    expect(h.api.currentContent.value).toBe('typed');
    // Typing counts as using the note, for a note kept behind a PIN.
    expect(h.touchNoteSession).toHaveBeenCalledWith('Notes/a.md');
  });

  it('reads as empty when no note is open', () => {
    const h = harness();
    expect(h.api.currentContent.value).toBe('');
  });

  it('ignores a request with no id rather than opening a blank tab', async () => {
    const h = harness();

    await h.api.loadNoteFile('');

    expect(h.api.activeTabs.value).toEqual([]);
    expect(h.getNode).not.toHaveBeenCalled();
  });
});

/**
 * The floor under a mis-routed link. A task reminder in Syn used to open the
 * task's own file here; the note editor saves what it holds as
 * `nodeType: 'note'`, so one autosave would have turned the task into a note
 * and lost it from the Tasks app.
 */
describe('useNoteTabs refuses what is not a note', () => {
  it('does not load a task into the editor', async () => {
    const h = harness([], 'task');

    await h.api.loadNoteFile('Tasks/a.md');

    expect(h.api.tabContents.value['Tasks/a.md']).toBeUndefined();
  });

  it('leaves no tab behind for it', async () => {
    const h = harness([], 'task');

    await h.api.loadNoteFile('Tasks/a.md');

    expect(h.api.activeTabs.value).toEqual([]);
  });

  it('does not put it in the note list', async () => {
    const h = harness([], 'event');

    await h.api.loadNoteFile('Events/a.md');

    expect(h.notes.value).toEqual([]);
  });

  it('still opens an actual note', async () => {
    const h = harness([], 'note');

    await h.api.loadNoteFile('Notes/a.md');

    expect(h.api.tabContents.value['Notes/a.md']).toBe('body of Notes/a.md');
    expect(h.api.activeTabs.value).toEqual(['Notes/a.md']);
  });

  /** Older index rows carry no type; refusing those would break normal notes. */
  it('opens a node whose type is not recorded', async () => {
    const h = harness([], '');

    await h.api.loadNoteFile('Notes/legacy.md');

    expect(h.api.tabContents.value['Notes/legacy.md']).toBe('body of Notes/legacy.md');
  });
});
