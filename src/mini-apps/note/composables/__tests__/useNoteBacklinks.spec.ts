import { describe, it, expect, vi } from 'vitest';
import { ref, computed } from 'vue';
import { useNoteBacklinks } from '../useNoteBacklinks';
import type { NoteItem } from '../../helpers';

const note = (id: string, title = id): NoteItem => ({
  id, title, summary: '', date: '2026-01-01', path: id,
  tags: [], pinned: false, full_width: false,
});

const harness = (notesIn: NoteItem[], content: string) => {
  const notes = ref<NoteItem[]>(notesIn);
  const currentNoteId = ref<string | null>(notesIn[0]?.id ?? null);
  const currentContent = computed(() => content);
  const getLinkedNodes = vi.fn().mockResolvedValue([]);
  const getNode = vi.fn().mockResolvedValue(null);
  const writeNode = vi.fn().mockResolvedValue(undefined);

  const api = useNoteBacklinks(
    notes, currentNoteId, currentContent,
    { getLinkedNodes, getNode, writeNode },
    vi.fn().mockResolvedValue(undefined),
  );
  return { api, notes, writeNode };
};

describe('currentOutgoingLinks', () => {
  it('resolves a link to the note it points at', () => {
    const h = harness(
      [note('Notes/here.md'), note('Notes/target.md')],
      'see [target](synabit://note/Notes/target.md) for more',
    );

    expect(h.api.currentOutgoingLinks.value).toEqual(['Notes/target.md']);
  });

  it('lists a link whose target is missing under the path it names', () => {
    // A link to a note that has been deleted or never existed. Showing it as
    // the raw path is how the graph draws a "ghost" — dropping it silently
    // would hide a broken link rather than report one.
    const h = harness(
      [note('Notes/here.md')],
      'see [gone](synabit://note/Notes/gone.md)',
    );

    expect(h.api.currentOutgoingLinks.value).toEqual(['Notes/gone.md']);
  });

  it('does not repeat a note linked twice', () => {
    const h = harness(
      [note('Notes/here.md'), note('Notes/target.md')],
      '[a](synabit://note/Notes/target.md) and [b](synabit://note/Notes/target.md)',
    );

    expect(h.api.currentOutgoingLinks.value).toEqual(['Notes/target.md']);
  });

  it('is empty for a note with no links in it', () => {
    const h = harness([note('Notes/here.md')], 'just prose');
    expect(h.api.currentOutgoingLinks.value).toEqual([]);
  });

  it('picks the note the link names, not one whose path merely ends the same way', () => {
    // `Archive/Notes/target.md` ends with `Notes/target.md`. A suffix match
    // resolves the link to the wrong note, and the backlink panel then points
    // somewhere the writer never linked.
    const h = harness(
      [note('Notes/here.md'), note('Archive/Notes/target.md'), note('Notes/target.md')],
      '[target](synabit://note/Notes/target.md)',
    );

    expect(h.api.currentOutgoingLinks.value).toEqual(['Notes/target.md']);
  });

  it('resolves a link to a note whose name has spaces in it', () => {
    // Vietnamese note titles nearly all do, and the mention menu writes the
    // path into the link exactly as it stands.
    const h = harness(
      [note('Notes/here.md'), note('Notes/công ty cổ phần.md')],
      'xem [công ty](synabit://note/Notes/công ty cổ phần.md) nhé',
    );

    expect(h.api.currentOutgoingLinks.value).toEqual(['Notes/công ty cổ phần.md']);
  });
});
