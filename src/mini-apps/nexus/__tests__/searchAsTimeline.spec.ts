import { describe, it, expect } from 'vitest';
import { asTimeline, type Matched } from '../searchAsTimeline';
import { dateColumn, shapeFor } from '../../../shared/views/shapeFor';

const found = (id: string, date: string, title = id): Matched => ({
  id,
  item_type: 'note',
  title,
  date,
});

describe('The same matches, laid out down the days', () => {
  /// `DatedView` decides which column holds the day by reading the cells, so
  /// this only works if the first column really is one.
  it('comes back in a shape the dated view can draw', () => {
    const answer = asTimeline([found('a', '2026-05-29'), found('b', '2026-02-16')], 2, 18);
    expect(answer.columns).toEqual(['when', 'title', 'type']);
    expect(dateColumn(answer)).toBe(0);
    expect(shapeFor(answer)).toBe('dated');
  });

  it('puts the newest first, which is what "show me" means', () => {
    const answer = asTimeline(
      [found('old', '2026-02-16'), found('new', '2026-05-29'), found('mid', '2026-04-01')],
      3,
      1,
    );
    expect(answer.rows.map(r => r.id)).toEqual(['new', 'mid', 'old']);
  });

  /// An empty string sorts above every year, so an undated row would sit at
  /// the top and read as today. It goes to the end.
  it('sends a row with no day to the end rather than the top', () => {
    const answer = asTimeline([found('undated', ''), found('dated', '2026-05-29')], 2, 1);
    expect(answer.rows.map(r => r.id)).toEqual(['dated', 'undated']);
  });

  /// A stored instant is UTC; the day shown is the reader's. A note saved at
  /// 06:00 in Hà Nội must not read as the day before.
  it('shows the day where the reader is, not where the clock is', () => {
    const answer = asTimeline([found('x', '2026-05-29T16:58:33.969Z')], 1, 1);
    expect(answer.rows[0].cells[0]).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });

  /// The list shows a page; `total` is how many matched. Carried through so
  /// the timeline can say there is more rather than stopping silently.
  it('carries how many matched, not how many it is drawing', () => {
    const answer = asTimeline([found('a', '2026-05-29')], 137, 39);
    expect(answer.rows).toHaveLength(1);
    expect(answer.total).toBe(137);
  });
});
