/**
 * The two bits of text an article card prints about itself.
 */

/**
 * How long ago, briefly: "5 phút trước" / "5 min. ago", up to a week; after
 * that the date. In the app's language through `Intl`, where the card used to
 * print English "just now", "5m", "3h" whatever the language.
 *
 * `""` for a date that cannot be read. The engine now stores only real dates
 * (see feed_engine/dates.rs), but a card must never print "Invalid Date".
 */
export function cardTime(iso: string, locale: string, now: number = Date.now()): string {
  if (!iso) return '';
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return '';
  const minutes = Math.round((now - then) / 60000);
  const rtf = new Intl.RelativeTimeFormat(locale, { numeric: 'auto', style: 'short' });
  if (minutes < 1) return rtf.format(0, 'minute');
  if (minutes < 60) return rtf.format(-minutes, 'minute');
  const hours = Math.round(minutes / 60);
  if (hours < 24) return rtf.format(-hours, 'hour');
  const days = Math.round(hours / 24);
  if (days < 7) return rtf.format(-days, 'day');
  const sameYear = new Date(then).getFullYear() === new Date(now).getFullYear();
  return new Intl.DateTimeFormat(locale, sameYear
    ? { day: 'numeric', month: 'short' }
    : { day: 'numeric', month: 'short', year: 'numeric' }).format(then);
}

/**
 * The first `max` characters of an article's text, for the card.
 *
 * Read with the browser's HTML parser rather than a tag-stripping regex. The
 * list is sent only the start of each body, and the cut often lands inside a
 * tag — `<img src="https://…` with no `>` — which the regex could not see as a
 * tag, so the card printed the markup. The parser drops an unfinished tag, and
 * it also turns `&amp;` and friends back into characters. Parsing into a
 * detached document runs no scripts and loads no images.
 */
export function plainPreview(html: string, max = 120): string {
  if (!html) return '';
  let text: string;
  try {
    text = new DOMParser().parseFromString(html, 'text/html').body.textContent ?? '';
  } catch {
    text = html.replace(/<[^>]*>?/g, ' ');
  }
  const clean = text.replace(/\s+/g, ' ').trim();
  return clean.length > max ? `${clean.slice(0, max).trimEnd()}…` : clean;
}
