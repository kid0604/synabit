import type { EventMetadata, EventFormData } from './types';
import type { EventsInRange } from '../../types/ipc';
import { i18n } from '../../i18n';
import { weekStartsOn, daysSinceWeekStart } from './weekStart';
import { recurrenceProperties } from './rrule';

/**
 * Month and weekday names come from the active app locale, not from a hard
 * coded English list and not from the operating system.
 *
 * Both of the alternatives were wrong in the same screen at the same time:
 * the month header read "August" from a constant while the day header read
 * its weekday through `toLocaleDateString(undefined, …)`, which follows the
 * OS. A Vietnamese user with a Vietnamese system saw "Thứ Hai" and "August"
 * side by side. `Intl` driven by `i18n.global.locale` makes one decision for
 * the whole app, and reading `.value` during render is what re-renders these
 * when the user switches language.
 */
const activeLocale = (): string => i18n.global.locale.value;

const nameOfMonth = (monthIndex: number, style: 'long' | 'short'): string =>
    new Intl.DateTimeFormat(activeLocale(), { month: style }).format(new Date(2026, monthIndex, 1));

export const monthName = (monthIndex: number): string => nameOfMonth(monthIndex, 'long');
export const monthNameShort = (monthIndex: number): string => nameOfMonth(monthIndex, 'short');

/** The current locale's first day of the week, as a JavaScript weekday. */
export const firstDayOfWeek = (): number => weekStartsOn(activeLocale());

/** Weekday names in the order this locale's grid puts them. */
export const dayNamesShort = (): string[] => {
    const fmt = new Intl.DateTimeFormat(activeLocale(), { weekday: 'short' });
    const start = firstDayOfWeek();
    // 2026-03-01 is a Sunday, so index 0 of this walk is Sunday.
    return Array.from({ length: 7 }, (_, i) => fmt.format(new Date(2026, 2, 1 + ((start + i) % 7))));
};

export const hours = Array.from({length: 24}, (_, i) => i);
export const hourOptions = Array.from({length: 24}, (_, i) => i.toString().padStart(2, '0'));
export const minuteOptions = ['00', '05', '10', '15', '20', '25', '30', '35', '40', '45', '50', '55'];

/** How many days into its week `date` falls, under the current locale. */
export const weekdayOffset = (date: Date): number =>
    daysSinceWeekStart(date.getDay(), firstDayOfWeek());

export const formatDateString = (date: Date) => {
    const y = date.getFullYear();
    const m = String(date.getMonth() + 1).padStart(2, '0');
    const d = String(date.getDate()).padStart(2, '0');
    return `${y}-${m}-${d}`;
};

/**
 * The calendar date `days` away from `dateStr`, still as a local date.
 *
 * Every place that shifted a date used to do it by way of
 * `toISOString().split('T')[0]`. `new Date('2026-08-24T00:00:00')` is local
 * midnight, and its UTC instant in UTC+7 is still 2026-08-23 — so "delete
 * this and following" set the series end one day early and took an extra
 * occurrence with it, for every user east of Greenwich. Reading the local
 * fields through `formatDateString` cannot drift that way.
 */
export const shiftDateString = (dateStr: string, days: number): string => {
    const d = new Date(dateStr + 'T00:00:00');
    // These feed straight into a vault write. A date that will not parse has
    // to come back unchanged rather than as the string "NaN-NaN-NaN".
    if (Number.isNaN(d.getTime()) || !Number.isFinite(days)) return dateStr;
    d.setDate(d.getDate() + days);
    return formatDateString(d);
};

/** Whole days from `fromStr` to `toStr`, negative when `toStr` is earlier. */
export const daysBetween = (fromStr: string, toStr: string): number => {
    const a = new Date(fromStr + 'T00:00:00');
    const b = new Date(toStr + 'T00:00:00');
    // Zero, not NaN: callers shift a stored date by this, and "do not move it"
    // is the only safe answer when one end will not parse.
    if (Number.isNaN(a.getTime()) || Number.isNaN(b.getTime())) return 0;
    return Math.round((b.getTime() - a.getTime()) / 86400000);
};

export const isSameDay = (d1: Date, d2: Date) => {
    return d1.getFullYear() === d2.getFullYear() && d1.getMonth() === d2.getMonth() && d1.getDate() === d2.getDate();
};

/*
 * `isAllDayOrMultiDay` used to live here.
 *
 * It answered "does this belong in the all-day row?" with "is it all-day, or
 * does it touch more than one day?", which sent every evening event running
 * past midnight to a row labelled All Day. `layout.splitDayEvents` asks the
 * question the time axis actually needs: an event goes above the axis when it
 * is all-day, or when it covers this particular day end to end.
 */

export const formatEventTime = (ev: EventMetadata) => {
    if (ev.is_all_day) return '';
    if (!ev.start_at || !ev.start_at.includes('T')) return '';
    const start = ev.start_at.split('T')[1].substring(0, 5);
    if (ev.end_at && ev.end_at.includes('T')) {
        const end = ev.end_at.split('T')[1].substring(0, 5);
        if (start === end) return start;
        return `${start} - ${end}`;
    }
    return start;
};

export const formatHourAMPM = (hr: number): string => {
    return hr === 0 ? '12 AM' : hr < 12 ? hr + ' AM' : hr === 12 ? '12 PM' : (hr - 12) + ' PM';
};

/*
 * `occursOnDate` used to live here.
 *
 * It was the front end's own copy of the recurrence rule, and it disagreed
 * with the one the reminder loop used. There is now a single implementation,
 * in `src-tauri/src/calendar/recurrence.rs`, and the calendar asks it for a
 * date range rather than re-deriving the answer per day cell. Do not bring a
 * copy back: `useNodeService.getEventsInRange` is the way to ask.
 */

/**
 * Turn the vault's answer into what a day cell reads.
 *
 * The payload is deliberately not a list of events per day: a daily series
 * over a year arrives as one summary and 365 references to it, so the wire
 * cost follows the days on screen rather than multiplying by them.
 */
export const indexOccurrencesByDate = (range: EventsInRange): Map<string, EventMetadata[]> => {
    const byDate = new Map<string, EventMetadata[]>();
    if (!range || !Array.isArray(range.occurrences)) return byDate;
    for (const occ of range.occurrences) {
        const summary = range.events[occ.event];
        if (!summary) continue;
        // The instance's own times win over the series'. `content` is absent
        // by design; the edit form fetches the body of the one event it opens.
        const event = {
            ...summary,
            // Already converted to the reader's zone. `tzid` rides along so
            // the editor can show the event in the zone it was written in.
            start_at: occ.start_at || summary.start_at,
            end_at: occ.end_at || summary.end_at,
        } as unknown as EventMetadata;
        const list = byDate.get(occ.date);
        if (list) list.push(event); else byDate.set(occ.date, [event]);
    }
    return byDate;
};

export const parseTags = (tagsStr: string): string[] => {
    if (!tagsStr.trim()) return [];
    return tagsStr.split(',').map(s => s.trim().replace(/^#/, '')).filter(s => s);
};

/** Build event payload for `ns.writeNode()`. */
export function buildEventPayload(
    form: EventFormData,
    overrides?: Partial<{
        relPath: string;
        relations: string[];
        eventType: string;
        silent: boolean;
    }>
) {
    const tags = parseTags(form.tagsStr);
    return {
        relPath: overrides?.relPath || form.id || `Events/${form.title}`,
        title: form.title,
        nodeType: 'event' as const,
        properties: {
            is_all_day: form.isAllDay,
            start_at: form.start_at,
            end_at: form.end_at,
            location: form.location,
            tzid: form.tzid || null,
            colour: form.colour || null,
            tags,
            relations: overrides?.relations ?? form.relations ?? [],
            ...recurrenceProperties(form.recurrence),
            series_id: form.series_id,
            exceptions: form.exceptions,
            reminders: form.reminders,
        },
        content: form.description,
        ...(overrides?.eventType ? { eventType: overrides.eventType } : {}),
        ...(overrides?.silent ? { silent: true } : {}),
    };
}
