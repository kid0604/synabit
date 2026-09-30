import { describe, it, expect, vi } from 'vitest';
import { ref } from 'vue';
import type { NoteItem } from '../../helpers';

vi.mock('@tauri-apps/api/core', () => ({
  // The backend search never answers here, so the local interim pass decides.
  invoke: vi.fn(() => new Promise(() => {})),
}));

import { useNoteSearch } from '../useNoteSearch';

const note = (id: string, title: string, pinned = false): NoteItem => ({
  id, title, summary: '', date: '2026-01-01', path: id,
  tags: [], pinned, full_width: false,
});

const harness = (notes: NoteItem[], recent: string[] = []) => {
  const api = useNoteSearch(ref(notes), ref(recent), ref(new Set<string>()), ref('/vault'));
  return api;
};

describe('useNoteSearch — templates in the Recent list', () => {
  const notes = [
    note('Notes/plan.md', 'Plan'),
    note('Templates/Meeting.md', 'Meeting template'),
    note('templates/sub/Lesson.md', 'Lesson'),
  ];

  it('leaves templates out of Recent while nobody is searching', () => {
    // Even the most recently opened one: editing a template is not writing.
    const api = harness(notes, ['Templates/Meeting.md', 'Notes/plan.md']);
    expect(api.recentNotes.value.map((n) => n.id)).toEqual(['Notes/plan.md']);
  });

  it('still finds them by search', () => {
    const api = harness(notes);
    api.searchQuery.value = 'meeting';
    expect(api.recentNotes.value.map((n) => n.id)).toEqual(['Templates/Meeting.md']);
  });

  it('keeps a template the user pinned on purpose', () => {
    const api = harness([...notes, note('Templates/Daily.md', 'Daily', true)]);
    expect(api.allPinnedNotes.value.map((n) => n.id)).toEqual(['Templates/Daily.md']);
  });
});
