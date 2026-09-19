import { describe, it, expect } from 'vitest';
import { chosenShape, dateColumn, shapeFor } from '../shapeFor';
import type { QueryResult } from '../types';

const answer = (columns: string[], cells: string[][]): QueryResult => ({
  columns,
  rows: cells.map((row, i) => ({
    id: `Notes/${i}.md`,
    node_type: 'note',
    title: `row ${i}`,
    cells: row,
  })),
  total: cells.length,
  query_time_ms: 1,
});

describe('An answer choosing its own shape', () => {
  /// The whole reason a new question stops costing new code: the rows say
  /// what they are, so one renderer covers every question of that shape —
  /// including the ones nobody has thought of yet.
  it('draws anything with days in it down the days', () => {
    const events = answer(
      ['when', 'title', 'who'],
      [
        ['2019-11-05', 'Gặp Khánh', 'uuid-khanh'],
        ['2021-03-14', 'Cà phê sáng', 'uuid-khanh'],
      ],
    );
    expect(shapeFor(events)).toBe('dated');
    expect(dateColumn(events)).toBe(0);
  });

  /// A column called `due_date` in somebody's own schema is a date too. The
  /// app must not only recognise the columns the app itself writes.
  it('finds a date column nobody told it about', () => {
    const tasks = answer(
      ['title', 'hạn_chót'],
      [
        ['Gửi báo cáo', '2026-06-30'],
        ['Họp TCB', '2026-07-02'],
      ],
    );
    expect(dateColumn(tasks)).toBe(1);
    expect(shapeFor(tasks)).toBe('dated');
  });

  /// And a column named like a date but full of something else is not one.
  /// The cells are the evidence; the name is only a hint.
  it('believes the cells over the name', () => {
    const odd = answer(['date', 'title', 'kind'], [
      ['hôm qua', 'a', 'note'],
      ['tuần trước', 'b', 'note'],
    ]);
    expect(dateColumn(odd)).toBe(-1);
    expect(shapeFor(odd)).toBe('table');
  });

  it('puts a row on the day it began when it carries two', () => {
    const spans = answer(
      ['when', 'to', 'title'],
      [['2019-01-01', '2021-06-30', 'MDP']],
    );
    expect(dateColumn(spans)).toBe(0);
  });

  it('shows the columns when the columns were asked for', () => {
    expect(shapeFor(answer(['title', 'kind', 'shape'], [['a', 'note', 'occasion']]))).toBe('table');
  });

  it('keeps a short answer as a list', () => {
    expect(shapeFor(answer(['title'], [['a'], ['b']]))).toBe('list');
    expect(shapeFor(null)).toBe('list');
  });

  /// An empty answer has no cells to read, so the name is all there is — and
  /// the shape still has to be stable, because the bar draws it either way.
  it('reads the name when there are no rows to read', () => {
    expect(shapeFor(answer(['when', 'title', 'who'], []))).toBe('dated');
    expect(shapeFor(answer(['title', 'kind', 'note'], []))).toBe('table');
  });

  it('tolerates a gap in the dates rather than giving up on the column', () => {
    const patchy = answer(
      ['when', 'title'],
      [['2019-11-05', 'a'], ['2019-11-06', 'b'], ['', 'c'], ['2019-11-08', 'd'], ['2019-11-09', 'e']],
    );
    expect(dateColumn(patchy)).toBe(0);
  });

  // ─── What a person chose ────────────────────────────────────

  it('honours a choice, and asks the answer only when there is none', () => {
    const events = answer(['when', 'title'], [['2019-11-05', 'a']]);
    expect(chosenShape('table', events)).toBe('table');
    expect(chosenShape('auto', events)).toBe('dated');
    expect(chosenShape(undefined, events)).toBe('dated');
  });
});
