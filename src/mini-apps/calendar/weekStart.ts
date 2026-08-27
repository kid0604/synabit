/**
 * Which day the week starts on, as a JavaScript weekday (0 = Sunday).
 *
 * The grid was hard-wired to Sunday, which is wrong for most of the world and
 * for both of the languages this app ships in bar one. Three sources, in
 * order of how much they can be trusted:
 *
 *  1. What this app has decided for the locales it ships. Vietnamese weeks
 *     start on Monday; `en` here means US English, which starts on Sunday.
 *  2. `Intl.Locale.prototype.getWeekInfo`, for a locale added later. It is not
 *     available on every WebView this app runs in — see the browser support
 *     note in CLAUDE.md — so it is asked for, never relied on.
 *  3. Monday, which is what the majority of locales use and what ISO 8601
 *     says.
 */
const KNOWN: Record<string, number> = {
    en: 0,
    'en-us': 0,
    'en-gb': 1,
    vi: 1,
};

export const weekStartsOn = (locale: string): number => {
    const key = (locale || '').toLowerCase();
    if (key in KNOWN) return KNOWN[key];
    const base = key.split('-')[0];
    if (base in KNOWN) return KNOWN[base];

    try {
        const info = (new Intl.Locale(locale) as unknown as {
            getWeekInfo?: () => { firstDay?: number };
        }).getWeekInfo?.();
        // Intl counts 1 = Monday through 7 = Sunday; JavaScript counts 0 = Sunday.
        if (typeof info?.firstDay === 'number') return info.firstDay % 7;
    } catch {
        // An engine without `Intl.Locale`, or a locale string it will not take.
    }
    return 1;
};

/** How many days back from `weekday` the start of its week is. */
export const daysSinceWeekStart = (weekday: number, startsOn: number): number =>
    (weekday - startsOn + 7) % 7;
