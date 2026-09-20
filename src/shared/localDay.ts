/**
 * The calendar day a stored timestamp falls on, where the reader is.
 *
 * The index used to hold two timestamp shapes, and three screens cut the day
 * out of them with `split(' ')[0]`, which only works on one of them. It now
 * holds one, RFC 3339 in UTC (`src-tauri/src/utils/timestamp.rs`), and slicing
 * that would show the UTC day: a note saved at 06:00 in Hà Nội would read as
 * the day before. So the day is read through `Date`, in the reader's zone.
 *
 * Every shape is still accepted, because a value can also come from a
 * frontmatter nobody has rewritten:
 *
 * - `2026-09-14` is already a day.
 * - `2026-09-14 06:00:00` has no offset and is local already; its day is its
 *   first ten characters. `new Date` would not help here, since WKWebView
 *   cannot parse it.
 * - anything `Date` cannot read is shown as its first ten characters rather
 *   than as nothing.
 */
export function localDay(stamp: string | null | undefined): string {
  if (!stamp) return '';
  if (/^\d{4}-\d{2}-\d{2}([ T]\d{2}:\d{2}(:\d{2}(\.\d+)?)?)?$/.test(stamp)) {
    return stamp.slice(0, 10);
  }
  const moment = new Date(stamp);
  if (Number.isNaN(moment.getTime())) return stamp.slice(0, 10);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${moment.getFullYear()}-${pad(moment.getMonth() + 1)}-${pad(moment.getDate())}`;
}

/**
 * A table cell as a person should read it.
 *
 * A query's columns are whatever was asked for, and two of the commonest —
 * `updated_at`, `created_at` — are stored as RFC 3339 in UTC. Shown raw they
 * read `2026-09-20T16:58:33.969Z`, which is a machine's way of saying
 * yesterday evening, inside an app whose whole subject is when things
 * happened.
 *
 * Only a stamp that carries a time is touched. A cell that is already a day
 * stays as it is, and anything that is not a date at all — a title, a place,
 * a count — is left exactly alone, because guessing at a cell's meaning is how
 * a table starts lying about what is in it.
 */
export function asShown(cell: string): string {
  return /^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}/.test(cell) ? localDay(cell) : cell;
}

/** Today, as the vault writes a day: `YYYY-MM-DD`, in the reader's own zone. */
export function todayIso(): string {
  const now = new Date();
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}
