import { describe, it, expect } from 'vitest';

import { isStale, orderMemories } from '../useSynMemory';
import type { Memory } from '../../types';

/**
 * The order the memory screen is read in.
 *
 * Tested as a function rather than by mounting the panel, because the question
 * is which memory comes first and none of the answer involves a DOM, an i18n
 * plugin or a Tauri mock.
 */
const memory = (over: Partial<Memory> & { body: string }): Memory => ({
  id: `SynMemory/${over.body}.md`,
  title: over.body,
  kind: 'fact',
  subject: null,
  confidence: 0.8,
  source_run: null,
  source_nodes: [],
  first_seen: '2026-01-01',
  last_confirmed: '2026-01-01',
  review_after: null,
  pinned: false,
  supersedes: null,
  ...over,
});

const TODAY = '2026-09-04';

describe('what the memory screen shows first', () => {
  it('puts anything asking to be checked at the top', () => {
    const rows = [
      memory({ body: 'pinned and current', pinned: true, last_confirmed: '2026-09-03' }),
      memory({ body: 'asking to be checked', review_after: '2026-08-01' }),
      memory({ body: 'ordinary', last_confirmed: '2026-09-02' }),
    ];

    expect(orderMemories(rows, TODAY).map(m => m.body)).toEqual([
      'asking to be checked',
      'pinned and current',
      'ordinary',
    ]);
  });

  /**
   * `review_after` had a date, a label and a colour, and nothing that ever
   * brought it to anybody's attention: a memory could ask to be checked and
   * sit fourteenth in a list nobody scrolls. A reminder nobody receives is not
   * a reminder.
   */
  it('treats a review date in the future as nothing to do yet', () => {
    const later = memory({ body: 'check me next year', review_after: '2027-01-01' });
    expect(isStale(later, TODAY)).toBe(false);

    const rows = [memory({ body: 'ordinary', last_confirmed: '2026-09-03' }), later];
    expect(orderMemories(rows, TODAY)[0].body).toBe('ordinary');
  });

  it('then prefers pinned, then the most recently confirmed', () => {
    const rows = [
      memory({ body: 'old and unpinned', last_confirmed: '2026-01-01' }),
      memory({ body: 'new and unpinned', last_confirmed: '2026-09-01' }),
      memory({ body: 'old but pinned', pinned: true, last_confirmed: '2025-01-01' }),
    ];

    expect(orderMemories(rows, TODAY).map(m => m.body)).toEqual([
      'old but pinned',
      'new and unpinned',
      'old and unpinned',
    ]);
  });

  /**
   * The screen reads in the order the prompt gives memories up in. If these two
   * ever disagree, the list stops being a preview of what Syn would lose first,
   * which is the only reason to order it this way rather than alphabetically.
   */
  it('sorts a copy, leaving the caller its own list', () => {
    const rows = [
      memory({ body: 'second', last_confirmed: '2026-01-01' }),
      memory({ body: 'first', last_confirmed: '2026-09-01' }),
    ];
    const before = rows.map(m => m.body);

    orderMemories(rows, TODAY);

    expect(rows.map(m => m.body)).toEqual(before);
  });
});
