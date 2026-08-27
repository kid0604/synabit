import type { EventMetadata } from './types';
import type { AgendaDay } from './composables/useAgenda';
import { minutesOfDay, MINUTES_PER_DAY } from './layout';

/**
 * What a stretch of calendar actually went on.
 *
 * Counted from the occurrences already on screen rather than from anything
 * new: the expansion knows every instance of every series, converted into the
 * reader's own clock, so the sum is the same one they are looking at.
 *
 * Deliberately no interpretation. It reports hours against the tags and
 * people the events were labelled with — it does not decide that three
 * meetings named "sync" were the same kind of work, because that is a guess
 * and a guess in a summary is worse than no summary.
 */

export interface Tally {
    label: string;
    minutes: number;
    count: number;
}

export interface Review {
    events: number;
    /** All-day events have no hours to count, so they are counted separately. */
    allDayEvents: number;
    minutes: number;
    busiestDay: { date: string; minutes: number } | null;
    tags: Tally[];
    people: Tally[];
}

/** How long one occurrence takes on the day it is filed under. */
export const minutesOn = (event: EventMetadata, dateStr: string): number => {
    if (event.is_all_day) return 0;
    const startDate = event.start_at.split('T')[0];
    const endDate = (event.end_at || event.start_at).split('T')[0];

    const from = startDate < dateStr ? 0 : minutesOfDay(event.start_at);
    const to = endDate > dateStr ? MINUTES_PER_DAY : minutesOfDay(event.end_at || event.start_at);
    if (from === null || to === null) return 0;
    return Math.max(0, to - from);
};

/** `[Anh](synabit://person/People/anh.md)` → `Anh`. */
const peopleIn = (event: EventMetadata): string[] =>
    (event.relations || [])
        .map(link => /^\[([^\]]+)\]\(synabit:\/\/person\//.exec(link)?.[1])
        .filter((name): name is string => !!name);

const rank = (counts: Map<string, Tally>, limit: number): Tally[] =>
    [...counts.values()]
        .sort((a, b) => b.minutes - a.minutes || b.count - a.count || a.label.localeCompare(b.label))
        .slice(0, limit);

export function reviewOf(days: AgendaDay[], limit = 5): Review {
    const tags = new Map<string, Tally>();
    const people = new Map<string, Tally>();
    let events = 0;
    let allDayEvents = 0;
    let minutes = 0;
    let busiestDay: { date: string; minutes: number } | null = null;

    for (const day of days) {
        let dayMinutes = 0;
        for (const event of day.events) {
            events++;
            if (event.is_all_day) allDayEvents++;
            const length = minutesOn(event, day.date);
            dayMinutes += length;
            minutes += length;

            const add = (map: Map<string, Tally>, label: string) => {
                const found = map.get(label) ?? { label, minutes: 0, count: 0 };
                found.minutes += length;
                found.count += 1;
                map.set(label, found);
            };
            for (const tag of event.tags || []) add(tags, tag);
            for (const name of peopleIn(event)) add(people, name);
        }
        if (dayMinutes > 0 && (!busiestDay || dayMinutes > busiestDay.minutes)) {
            busiestDay = { date: day.date, minutes: dayMinutes };
        }
    }

    return { events, allDayEvents, minutes, busiestDay, tags: rank(tags, limit), people: rank(people, limit) };
}

/** `90` → `1h 30m`. Whole hours drop the minutes. */
export const asHours = (minutes: number): string => {
    const whole = Math.floor(minutes / 60);
    const rest = Math.round(minutes % 60);
    if (whole === 0) return `${rest}m`;
    return rest === 0 ? `${whole}h` : `${whole}h ${rest}m`;
};
