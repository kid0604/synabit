/**
 * The same matches, laid out down the days.
 *
 * # Why it is built from the results and not asked for again
 *
 * The screen answers one question two ways at once: a list on the left and a
 * picture on the right. Asking the engine a second time for the timeline would
 * mean two answers to one question, and the day they disagree is the day
 * somebody stops trusting either. So the timeline is **the rows the list is
 * already showing**, turned on their side.
 *
 * It follows that the timeline shows what the search returned, which is a page
 * of it. `total` is carried through so `DatedView` can say there is more.
 */
import { localDay } from '../../shared/localDay';
import type { QueryResult } from '../../shared/views/types';

/** What the search box hands back, as much of it as this needs. */
export interface Matched {
  id: string;
  item_type: string;
  title: string;
  date: string;
}

export function asTimeline(matched: Matched[], total: number, ms: number): QueryResult {
  const rows = matched
    .map(item => ({
      id: item.id,
      node_type: item.item_type,
      title: item.title,
      cells: [localDay(item.date), item.title, item.item_type],
    }))
    // Newest first, which is what somebody means by "show me" — the same
    // default `timeline::query` uses. A row with no day falls to the end
    // rather than the top, where an empty string would sort it above every
    // year and read as today.
    .sort((a, b) => {
      if (!a.cells[0]) return 1;
      if (!b.cells[0]) return -1;
      return b.cells[0].localeCompare(a.cells[0]);
    });

  return { columns: ['when', 'title', 'type'], rows, total, query_time_ms: ms };
}
