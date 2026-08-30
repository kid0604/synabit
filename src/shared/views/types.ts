/**
 * What a view primitive is given, and what it hands back.
 *
 * These mirror `QueryRow` and `QueryResult` in `db/node_query.rs`. They were
 * declared inline inside `QueryResultTable.vue`, which was fine while there was
 * one generic view and stops being fine at two.
 */

export interface QueryRow {
  /** The node's path, which is its id. */
  id: string;
  node_type: string;
  title: string;
  /** One entry per requested column, in the order the columns came back. */
  cells: string[];
}

export interface QueryResult {
  /** The columns actually shown, after unusable names were dropped. */
  columns: string[];
  rows: QueryRow[];
  /**
   * How many nodes match, ignoring the limit.
   *
   * A real count, not `rows.length`. It was the latter until an assistant read
   * it off a one-row page and reported two tasks out of a hundred and
   * twenty-six — so a view showing "N results" must read this, and a view
   * saying "and more" must compare it against `rows.length`.
   */
  total: number;
  query_time_ms: number;
}

/**
 * The contract every view primitive keeps.
 *
 * Two rules, and both are what makes a view reusable rather than a copy of the
 * app it was cut out of:
 *
 * 1. **It is given a result; it does not fetch one.** Whoever owns the query
 *    owns the loading state, the race between a slow answer and a fast one, and
 *    the decision about what to run. A view that invokes for itself cannot be
 *    put in a screen that already ran the query.
 * 2. **It never asks what type a row is in order to decide what to do.** It may
 *    ask in order to draw an icon. Deciding behaviour by type is how a generic
 *    view stops being generic.
 */
export interface ViewProps {
  result: QueryResult | null;
  loading?: boolean;
  /** The row currently open, so the view can mark it. */
  selectedId?: string | null;
}
