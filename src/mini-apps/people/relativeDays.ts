/**
 * "Today", "in 3 days", "2 months ago" — in the reader's language.
 *
 * These were English template strings, so a Vietnamese reader got "in 3 days"
 * in the middle of a translated card. `Intl.RelativeTimeFormat` already knows
 * both languages, and their plurals, and says "tomorrow" rather than "in 1
 * day" with `numeric: 'auto'`.
 */
export const relativeDays = (days: number, locale: string): string => {
  const rtf = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' });
  const span = Math.abs(days);
  const sign = days < 0 ? -1 : 1;
  if (span < 30) return rtf.format(days, 'day');
  if (span < 365) return rtf.format(sign * Math.floor(span / 30), 'month');
  return rtf.format(sign * Math.floor(span / 365), 'year');
};
