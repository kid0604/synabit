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

  // ─── A tally, which is what `| stats` makes ─────────────────

  /// §7: `stats count by month` answers a question about proportion, and a
  /// column of digits makes the reader do the comparing.
  it('draws a heap of labels with a number against each as bars', () => {
    const tally = answer(['month', 'count'], [['2019-11', '3'], ['2021-03', '1']]);
    expect(shapeFor(tally)).toBe('bars');
  });

  /// Read before the date test on purpose: `by month` has days down one side,
  /// and a timeline would show the labels and hide the thing that was counted.
  it('prefers bars over a timeline when the labels happen to be days', () => {
    const byDay = answer(['day', 'count'], [['2019-11-05', '3'], ['2019-11-06', '1']]);
    expect(dateColumn(byDay)).toBe(0);
    expect(shapeFor(byDay)).toBe('bars');
  });

  /// And the common case that looks just like it from a distance: two columns
  /// with numbers in the second is also `columns:title,priority`, which is a
  /// table of tasks. The name is the hint and the cells are the evidence, and
  /// here both have to agree.
  it('does not turn a table of tasks into a chart', () => {
    const tasks = answer(['title', 'priority'], [['Gửi báo cáo', '3'], ['Làm công văn', '5']]);
    expect(shapeFor(tasks)).toBe('list');
    // A column named like a tally but full of words is not one either.
    expect(shapeFor(answer(['place', 'count'], [['Hà Nội', 'nhiều']]))).toBe('list');
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
