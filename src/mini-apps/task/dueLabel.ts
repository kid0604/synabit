/**
 * A due date the way a person says it: "Today", "Tomorrow", "Mon 6 Oct".
 *
 * The phone's meta line used to print the stored `2026-10-06` as is — a
 * format for files, not for reading at a glance. The words come from `Intl`
 * in the app's language, so there are no strings to translate here.
 */

const DAY_MS = 86_400_000;

/** `YYYY-MM-DD` read as a local calendar day, or null for anything else. */
const parseDay = (value: string): Date | null => {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(value);
  if (!m) return null;
  const d = new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]));
  return Number.isNaN(d.getTime()) ? null : d;
};

const capitalise = (s: string, locale: string): string =>
  s ? s.charAt(0).toLocaleUpperCase(locale) + s.slice(1) : s;

export const formatDueDate = (value: string, locale: string, now: Date = new Date()): string => {
  const day = parseDay(value);
  if (!day) return value;
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  // Round, not floor: a daylight-saving night is 23 or 25 hours long.
  const diff = Math.round((day.getTime() - today.getTime()) / DAY_MS);

  if (diff >= -1 && diff <= 1) {
    const rtf = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' });
    return capitalise(rtf.format(diff, 'day'), locale);
  }

  const options: Intl.DateTimeFormatOptions = { weekday: 'short', day: 'numeric', month: 'short' };
  if (day.getFullYear() !== today.getFullYear()) options.year = 'numeric';
  return new Intl.DateTimeFormat(locale, options).format(day);
};
