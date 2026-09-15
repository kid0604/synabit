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
