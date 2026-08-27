import { describe, it, expect } from 'vitest';
import { resolveNoteId } from '../resolveNoteId';
import type { NoteItem } from '../helpers';

const note = (id: string): NoteItem => ({
  id, title: id, summary: '', date: '2026-01-01', path: id,
  tags: [], pinned: false, full_width: false,
});

const vault = [
  note('Archive/Notes/target.md'),
  note('Notes/target.md'),
  note('Notes/công ty cổ phần.md'),
];

describe('resolveNoteId', () => {
  it('answers a full path with that exact note', () => {
    // The one that used to go wrong: `Archive/Notes/target.md` ends with
    // `Notes/target.md` and sits earlier in the list, so a suffix match
    // opened the archived copy instead.
    expect(resolveNoteId(vault, 'Notes/target.md')?.id).toBe('Notes/target.md');
  });

  it('answers a bare filename with the note that has it', () => {
    // Older links and deep links from outside the app name only the file.
    expect(resolveNoteId(vault, 'công ty cổ phần.md')?.id).toBe('Notes/công ty cổ phần.md');
  });

  it('follows a note that has moved, when only one note carries the name', () => {
    // A link written before the note was filed elsewhere. There is only one
    // `target.md` in this vault, so it is plainly the one meant.
    expect(resolveNoteId([note('Archive/Notes/target.md')], 'Notes/target.md')?.id)
      .toBe('Archive/Notes/target.md');
  });

  it('refuses to guess when two notes share a filename', () => {
    // The folders are the only thing telling these apart, so a target naming
    // neither of them exactly is a question with no honest answer.
    expect(resolveNoteId(vault, 'Somewhere/else/target.md')).toBeUndefined();
    expect(resolveNoteId(vault, 'target.md')).toBeUndefined();
  });

  it('finds nothing rather than guessing when there is no match', () => {
    expect(resolveNoteId(vault, 'Notes/nowhere.md')).toBeUndefined();
    expect(resolveNoteId(vault, '')).toBeUndefined();
    expect(resolveNoteId([], 'Notes/target.md')).toBeUndefined();
  });
});
