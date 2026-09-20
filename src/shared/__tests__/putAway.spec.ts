import { describe, it, expect, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { putAway, putAwayFor } from '../putAway';
import type { QueryResult } from '../views/types';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => vi.mocked(invoke).mockReset());

const answer = (columns: string[], cells: string[][], open?: string): QueryResult => ({
  columns,
  rows: cells.map((row, i) => ({
    id: `row-${i}`,
    node_type: '',
    title: row[0] ?? '',
    cells: row,
    ...(open ? { open } : {}),
  })),
  total: cells.length,
  query_time_ms: 1,
});

describe('Saying no to a row', () => {
  /// Order is meaning, not preference: a `seq gaps` answer also has days in
  /// it, and refusing one of its rows is about the person.
  it('reads a gaps row as being about the person, not the day', () => {
    const gaps = answer(
      ['who', 'times', 'first', 'last', 'span', 'quiet', 'longest'],
      [['Khánh', '5', '2024-06-01', '2025-06-01', '365', '400', '92']],
    );
    expect(putAwayFor(gaps, gaps.rows[0])).toEqual({ kind: 'person', who: 'Khánh' });
  });

  it('reads an exploded sentence as being about that sentence', () => {
    const lines = answer(
      ['day', 'note', 'text'],
      [['2019-11-05', 'Notes/diary.md', 'Hôm nay gặp Khánh.']],
    );
    expect(putAwayFor(lines, lines.rows[0])).toEqual({
      kind: 'line',
      node: 'Notes/diary.md',
      text: 'Hôm nay gặp Khánh.',
    });
  });

  /// An event's id opens nothing — the note behind it is what a refusal is
  /// about. That is what `open` is for.
  it('reads a dated row as one thing on one day, by the note behind it', () => {
    const events = answer(['when', 'title', 'who'], [['2019-11-05', 'Gặp Khánh', '']], 'Notes/d.md');
    expect(putAwayFor(events, events.rows[0])).toEqual({
      kind: 'moment',
      node: 'Notes/d.md',
      day: '2019-11-05',
    });
  });

  /// A table of books affords nothing, and then no button is drawn.
  it('affords nothing when the answer is not about the timeline', () => {
    const books = answer(['title', 'author'], [['Sách hay', 'Nguyễn']]);
    expect(putAwayFor(books, books.rows[0])).toBeNull();
    expect(putAwayFor(null, books.rows[0])).toBeNull();
  });

  // ─── and what it writes ─────────────────────────────────────

  it('writes each kind through the command that already existed', async () => {
    await putAway('/vault', { kind: 'person', who: 'Khánh' });
    expect(invoke).toHaveBeenLastCalledWith('timeline_set_aside', {
      vaultPath: '/vault',
      who: 'Khánh',
    });

    await putAway('/vault', { kind: 'line', node: 'Notes/d.md', text: 'một câu' });
    expect(invoke).toHaveBeenLastCalledWith('timeline_drop_line', {
      vaultPath: '/vault',
      nodeId: 'Notes/d.md',
      text: 'một câu',
    });

    await putAway('/vault', { kind: 'moment', node: 'Notes/d.md', day: '2019-11-05' });
    expect(invoke).toHaveBeenLastCalledWith('timeline_not_again', {
      vaultPath: '/vault',
      nodeId: 'Notes/d.md',
      day: '2019-11-05',
    });
  });
});
