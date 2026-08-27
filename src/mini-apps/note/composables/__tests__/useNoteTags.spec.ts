import { describe, it, expect, vi } from 'vitest';
import { ref, computed } from 'vue';
import { useNoteTags } from '../useNoteTags';
import type { NoteItem } from '../../helpers';

const note = (id: string, tags: string[] = []): NoteItem => ({
  id, title: 'A note', summary: '', date: '2026-01-01', path: id,
  tags, pinned: false, full_width: false,
});

/**
 * The editor hands its markdown over a fifth of a second after the last
 * keystroke, so this stands in for one with typing still unserialised: the
 * body only becomes current once `flushEditor` is called.
 */
const harness = (tags: string[] = []) => {
  const notes = ref<NoteItem[]>([note('Notes/a.md', tags)]);
  const currentNoteId = ref<string | null>('Notes/a.md');
  const pending = ref('stale body');
  const flushEditor = vi.fn(() => { pending.value = 'body including what was just typed'; });
  const writeNode = vi.fn().mockResolvedValue(undefined);

  const api = useNoteTags(
    notes,
    currentNoteId,
    computed(() => pending.value),
    { writeNode },
    vi.fn().mockResolvedValue(undefined),
    flushEditor,
  );

  return { api, notes, writeNode, flushEditor };
};

const enter = () => ({ key: 'Enter' } as KeyboardEvent);

describe('useNoteTags', () => {
  it('writes the body as it stands now, not as it stood before the last keystroke', async () => {
    // Adding a tag rewrites the whole note. Reading a body the editor has not
    // finished producing saves the note with the last few characters missing.
    const h = harness();
    h.api.newTagInput.value = 'công việc';

    await h.api.addTag(enter());

    expect(h.flushEditor).toHaveBeenCalled();
    expect(h.writeNode).toHaveBeenCalledWith(
      expect.objectContaining({ content: 'body including what was just typed' }),
    );
  });

  it('does the same when a tag is removed', async () => {
    const h = harness(['công việc']);

    await h.api.removeTag('công việc');

    expect(h.flushEditor).toHaveBeenCalled();
    expect(h.writeNode).toHaveBeenCalledWith(
      expect.objectContaining({ content: 'body including what was just typed' }),
    );
  });

  it('adds the tag to the note it writes', async () => {
    const h = harness();
    h.api.newTagInput.value = 'dự án';

    await h.api.addTag(enter());

    expect(h.notes.value[0].tags).toContain('dự án');
    expect(h.writeNode.mock.calls[0][0].properties.tags).toContain('dự án');
  });

  it('will not add the same tag twice', async () => {
    const h = harness(['dự án']);
    h.api.newTagInput.value = 'dự án';

    await h.api.addTag(enter());

    expect(h.notes.value[0].tags).toEqual(['dự án']);
    expect(h.writeNode).not.toHaveBeenCalled();
  });
});
