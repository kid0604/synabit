/**
 * The recurrence editor's fields, and the RFC 5545 string they mean.
 *
 * Deciding *when* a series lands is Rust's job and has exactly one
 * implementation. What lives here is narrower and unavoidable: a form has
 * checkboxes and number inputs, storage has a string, and something has to
 * turn one into the other before the write.
 *
 * That is still a second place the format is written down, so it is pinned
 * the same way the rule itself is — `contracts/rrule.json` holds the pairs,
 * and `src-tauri/src/calendar/rrule.rs` reads the same file back. A string
 * this file emits that Rust reads differently is a failing test rather than a
 * series that quietly repeats on the wrong days.
 */

export type Freq = 'none' | 'daily' | 'weekly' | 'monthly' | 'yearly';
export type EndMode = 'never' | 'until' | 'count';

export interface RecurrenceFields {
    freq: Freq;
    /** "every N days/weeks/…", at least 1. */
    interval: number;
    /** Weekday codes for a weekly rule: MO, TU, … Ignored otherwise. */
    byDay: string[];
    endMode: EndMode;
    /** YYYY-MM-DD, when endMode is 'until'. */
    until: string;
    /** Number of occurrences, when endMode is 'count'. */
    count: number;
}

export const WEEKDAY_CODES = ['MO', 'TU', 'WE', 'TH', 'FR', 'SA', 'SU'] as const;

/** RFC 5545 counts a week from Monday unless told otherwise, and so do we. */
const codeOrder = (code: string) => WEEKDAY_CODES.indexOf(code as typeof WEEKDAY_CODES[number]);

export const defaultRecurrence = (): RecurrenceFields => ({
    freq: 'none', interval: 1, byDay: [], endMode: 'never', until: '', count: 10,
});

/** The weekday code for a `YYYY-MM-DD`, for seeding a weekly rule. */
export const weekdayCodeOf = (dateStr: string): string => {
    const d = new Date(`${dateStr.split('T')[0]}T00:00:00`);
    if (Number.isNaN(d.getTime())) return 'MO';
    return WEEKDAY_CODES[(d.getDay() + 6) % 7];
};

const FREQ_WORD: Record<Exclude<Freq, 'none'>, string> = {
    daily: 'DAILY', weekly: 'WEEKLY', monthly: 'MONTHLY', yearly: 'YEARLY',
};

export const serializeRRule = (f: RecurrenceFields): string => {
    if (f.freq === 'none') return '';
    const parts = [`FREQ=${FREQ_WORD[f.freq]}`];

    const interval = Math.max(1, Math.floor(f.interval || 1));
    if (interval > 1) parts.push(`INTERVAL=${interval}`);

    if (f.freq === 'weekly' && f.byDay.length) {
        const days = [...new Set(f.byDay)].filter(c => codeOrder(c) >= 0)
            .sort((a, b) => codeOrder(a) - codeOrder(b));
        if (days.length) parts.push(`BYDAY=${days.join(',')}`);
    }

    if (f.endMode === 'count') {
        parts.push(`COUNT=${Math.max(1, Math.floor(f.count || 1))}`);
    } else if (f.endMode === 'until' && f.until) {
        parts.push(`UNTIL=${f.until.replace(/-/g, '')}`);
    }
    return parts.join(';');
};

export const parseRRule = (rule: string): RecurrenceFields => {
    const out = defaultRecurrence();
    if (!rule || !rule.trim()) return out;

    const seen: Record<string, string> = {};
    for (const part of rule.split(';')) {
        const at = part.indexOf('=');
        if (at < 0) continue;
        seen[part.slice(0, at).trim().toUpperCase()] = part.slice(at + 1).trim();
    }

    const freq = (Object.keys(FREQ_WORD) as Exclude<Freq, 'none'>[])
        .find(k => FREQ_WORD[k] === (seen.FREQ || '').toUpperCase());
    // No usable FREQ means no rule — the same answer Rust gives, so a broken
    // string shows as "does not repeat" rather than as something invented.
    if (!freq) return out;
    out.freq = freq;

    const interval = Number.parseInt(seen.INTERVAL ?? '', 10);
    if (Number.isFinite(interval) && interval >= 1) out.interval = interval;

    if (freq === 'weekly' && seen.BYDAY) {
        out.byDay = [...new Set(seen.BYDAY.split(',').map(c => c.trim().toUpperCase()))]
            .filter(c => codeOrder(c) >= 0)
            .sort((a, b) => codeOrder(a) - codeOrder(b));
    }

    const count = Number.parseInt(seen.COUNT ?? '', 10);
    if (Number.isFinite(count) && count >= 1) {
        out.endMode = 'count';
        out.count = count;
    } else if (seen.UNTIL) {
        const raw = seen.UNTIL.split('T')[0];
        const iso = raw.includes('-') ? raw
            : raw.length === 8 ? `${raw.slice(0, 4)}-${raw.slice(4, 6)}-${raw.slice(6, 8)}` : '';
        if (iso) {
            out.endMode = 'until';
            out.until = iso;
        }
    }
    return out;
};

/** What a rule reads as in one line, for a summary under the editor. */
export const describeRecurrence = (f: RecurrenceFields, t: (k: string, v?: any) => string): string => {
    if (f.freq === 'none') return t('calendar.does_not_repeat');
    const unit = t(`calendar.unit_${f.freq}`);
    let text = f.interval > 1
        ? t('calendar.every_n', { n: f.interval, unit })
        : t('calendar.every_one', { unit });
    if (f.freq === 'weekly' && f.byDay.length) {
        text += ` · ${f.byDay.map(c => t(`calendar.day_${c}`)).join(', ')}`;
    }
    if (f.endMode === 'count') text += ` · ${t('calendar.ends_after', { n: f.count })}`;
    else if (f.endMode === 'until' && f.until) text += ` · ${t('calendar.ends_by', { date: f.until })}`;
    return text;
};

/**
 * The rule an event repeats by, whichever way it was written down.
 *
 * A stored `rrule` wins outright. The legacy pair is only read when there is
 * none — reading both and merging is how two sources of truth get created,
 * and the Rust side deliberately does not do it either.
 */
export const ruleOf = (ev: {
    rrule?: string; recurrence?: string; recurrence_end_at?: string;
}): RecurrenceFields => {
    if (ev.rrule && ev.rrule.trim()) return parseRRule(ev.rrule);

    const legacy = (ev.recurrence || '').toLowerCase();
    if (!legacy || legacy === 'none') return defaultRecurrence();
    const freq = (['daily', 'weekly', 'monthly', 'yearly'] as const)
        .find(f => f === legacy);
    if (!freq) return defaultRecurrence();

    const out = defaultRecurrence();
    out.freq = freq;
    if (ev.recurrence_end_at) {
        out.endMode = 'until';
        out.until = ev.recurrence_end_at;
    }
    return out;
};

export const isSeries = (ev: { rrule?: string; recurrence?: string }): boolean =>
    ruleOf(ev).freq !== 'none';

/** The same rule, stopped on `untilDate`. Used when a series is cut short. */
export const endingOn = (rule: RecurrenceFields, untilDate: string): RecurrenceFields =>
    ({ ...rule, endMode: 'until', until: untilDate, count: rule.count });

/**
 * The frontmatter a rule writes.
 *
 * The two legacy keys are set to `null`, which a write removes — so an event
 * touched by this version comes out of it with exactly one place its
 * recurrence is recorded. Events never touched keep the old keys and keep
 * being read through the fallback above.
 */
export const recurrenceProperties = (rule: RecurrenceFields) => {
    const text = serializeRRule(rule);
    return {
        rrule: text || null,
        recurrence: null,
        recurrence_end_at: null,
    };
};
