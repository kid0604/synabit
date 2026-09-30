/**
 * When something happened, as a person would say it.
 *
 * The sync tooltip used to print the stored ISO timestamp as it was —
 * "Synced 2026-09-30T08:14:03.512Z" — which is a fact for a log, not for the
 * person who wants to know whether their phone has the note yet.
 *
 * Recent times are relative ("5 minutes ago"), because that is the question
 * being asked; anything older than a day is a date and time in the reader's
 * own locale, because "3 days ago" stops being useful as soon as you need to
 * know which evening it was.
 *
 * Returns the input unchanged when it is not a date at all, so a value from an
 * older build still shows something rather than "Invalid Date".
 */
export function when(iso: string, locale: string, now: number = Date.now()): string {
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return iso;

  const seconds = Math.round((then - now) / 1000);
  const abs = Math.abs(seconds);
  const rtf = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' });

  if (abs < 60) return rtf.format(0, 'second');
  if (abs < 3600) return rtf.format(Math.round(seconds / 60), 'minute');
  if (abs < 86400) return rtf.format(Math.round(seconds / 3600), 'hour');
  return new Intl.DateTimeFormat(locale, { dateStyle: 'medium', timeStyle: 'short' }).format(then);
}
