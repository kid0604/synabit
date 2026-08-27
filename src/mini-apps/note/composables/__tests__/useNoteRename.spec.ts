import { describe, it, expect, vi } from 'vitest';
import { ref } from 'vue';
import { useNoteRename } from '../useNoteRename';
import type { NoteItem } from '../../helpers';

const note = (id: string, title: string): NoteItem => ({
  id, title, summary: '', date: '2026-01-01', path: id,
  tags: [], pinned: false, full_width: false,
});

const harness = () => {
  const notes = ref<NoteItem[]>([note('Notes/old name.md', 'old name')]);
  const currentNoteId = ref<string | null>('Notes/old name.md');
  const renameNode = vi.fn().mockResolvedValue('Notes/new name.md');
  const writeNode = vi.fn().mockResolvedValue(undefined);
  const getNode = vi.fn().mockResolvedValue({ content: 'body' });
  const scanVault = vi.fn().mockResolvedValue(undefined);
  const tabContents = ref<Record<string, string>>({ 'Notes/old name.md': 'body' });
  const activeTabs = ref<string[]>(['Notes/old name.md']);
  const recentNoteIds = ref<string[]>(['Notes/old name.md']);
  const renamedTabs = new Map<string, string>();

  const api = useNoteRename(
    notes,
    currentNoteId,
    { renameNode, writeNode, getNode },
    tabContents,
    activeTabs,
    new Map<string, number>(),
    renamedTabs,
    ref<Record<string, string>>({}),
    recentNoteIds,
    new Map(),
    vi.fn(),
    scanVault,
    ref<Record<string, any>>({}),
  );

  return {
    api, notes, currentNoteId, renameNode, writeNode, scanVault,
    tabContents, activeTabs, recentNoteIds, renamedTabs,
  };
};

/** The inline title field on the note itself, as opposed to the rename dialog. */
const typeNewTitle = (value: string) =>
  ({ type: 'keydown', key: 'Enter', target: { value } } as unknown as KeyboardEvent);

describe('useNoteRename', () => {
  it('leaves the note pointing at its new path, not the one it just left', async () => {
    // `buildNotePayload` writes to `note.id`. If the rename does not move that
    // on, the write lands back at the old path — recreating the file the
    // rename had just moved away from, and leaving the vault with both.
    const h = harness();
    h.api.renameModal.value = { show: true, noteId: 'Notes/old name.md', value: 'new name' };

    await h.api.confirmRename();

    expect(h.renameNode).toHaveBeenCalledWith({
      oldRelPath: 'Notes/old name.md',
      newName: 'new name',
    });

    const written = h.writeNode.mock.calls.map((c) => c[0]?.relPath);
    expect(written).not.toContain('Notes/old name.md');

    expect(h.notes.value[0].id).toBe('Notes/new name.md');
    expect(h.notes.value[0].path).toBe('Notes/new name.md');
    expect(h.currentNoteId.value).toBe('Notes/new name.md');
  });

  it('carries the new title', async () => {
    const h = harness();
    h.api.renameModal.value = { show: true, noteId: 'Notes/old name.md', value: 'new name' };

    await h.api.confirmRename();

    expect(h.notes.value[0].title).toBe('new name');
  });

  it('moves the open tab, the recents and the pending-save redirect across', async () => {
    const h = harness();
    h.api.renameModal.value = { show: true, noteId: 'Notes/old name.md', value: 'new name' };

    await h.api.confirmRename();

    expect(h.activeTabs.value).toEqual(['Notes/new name.md']);
    expect(h.recentNoteIds.value).toEqual(['Notes/new name.md']);
    expect(h.tabContents.value['Notes/new name.md']).toBe('body');
    expect(h.tabContents.value['Notes/old name.md']).toBeUndefined();
    // A save queued under the old path has to find its way to the new one.
    expect(h.renamedTabs.get('Notes/old name.md')).toBe('Notes/new name.md');
  });

  it('does the same when the title is edited in place, not through the dialog', async () => {
    // Two entry points, the same migration. They used to carry two copies of
    // it, and both copies forgot to move `note.id` — so the bug existed twice.
    const h = harness();

    await h.api.renameTopTitle(typeNewTitle('new name'));

    const written = h.writeNode.mock.calls.map((c) => c[0]?.relPath);
    expect(written).not.toContain('Notes/old name.md');
    expect(h.notes.value[0].id).toBe('Notes/new name.md');
    expect(h.activeTabs.value).toEqual(['Notes/new name.md']);
    expect(h.renamedTabs.get('Notes/old name.md')).toBe('Notes/new name.md');
  });

  it('leaves everything alone when the name did not actually change', async () => {
    const h = harness();
    h.api.renameModal.value = { show: true, noteId: 'Notes/old name.md', value: 'old name' };

    await h.api.confirmRename();

    expect(h.renameNode).not.toHaveBeenCalled();
    expect(h.notes.value[0].id).toBe('Notes/old name.md');
  });
});
