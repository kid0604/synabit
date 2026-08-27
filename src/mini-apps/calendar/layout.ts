import type { EventMetadata } from './types';

export const MINUTES_PER_DAY = 1440;

/**
 * The shortest a block is allowed to be drawn, and to be treated as occupying
 * space when deciding what overlaps what.
 *
 * A fifteen-minute stand-up and a five-minute reminder both need to be
 * readable and both need a column of their own. Laying out zero-length events
 * with their real extent puts them in the same column, stacked invisibly.
 */
export const MIN_BLOCK_MINUTES = 20;

export interface PositionedEvent {
    event: EventMetadata;
    key: string;
    /** Clipped to this day. */
    startMinute: number;
    endMinute: number;
    topPct: number;
    heightPct: number;
    leftPct: number;
    widthPct: number;
    /** The instance began before this day, or runs past the end of it. */
    continuesBefore: boolean;
    continuesAfter: boolean;
}

/** Minutes past midnight in a `YYYY-MM-DDTHH:mm` stamp, or null for a date. */
export const minutesOfDay = (stamp: string): number | null => {
    if (!stamp || !stamp.includes('T')) return null;
    const time = stamp.split('T')[1];
    const [h, m] = time.split(':');
    const hours = Number(h);
    const mins = Number(m);
    if (!Number.isFinite(hours) || !Number.isFinite(mins)) return null;
    return Math.max(0, Math.min(MINUTES_PER_DAY, hours * 60 + mins));
};

export const clockOf = (minute: number): string => {
    const m = Math.max(0, Math.min(MINUTES_PER_DAY - 1, Math.round(minute)));
    return `${String(Math.floor(m / 60)).padStart(2, '0')}:${String(m % 60).padStart(2, '0')}`;
};

interface Span {
    event: EventMetadata;
    startMinute: number;
    endMinute: number;
    continuesBefore: boolean;
    continuesAfter: boolean;
}

/** Where this instance sits inside `dateStr`, or null if it has no clock. */
const spanOnDay = (event: EventMetadata, dateStr: string): Span | null => {
    if (!event.start_at) return null;
    const startDate = event.start_at.split('T')[0];
    const endDate = event.end_at ? event.end_at.split('T')[0] : startDate;

    const continuesBefore = startDate < dateStr;
    const continuesAfter = endDate > dateStr;

    const startMinute = continuesBefore ? 0 : minutesOfDay(event.start_at);
    if (startMinute === null) return null;
    const rawEnd = continuesAfter ? MINUTES_PER_DAY : minutesOfDay(event.end_at);
    const endMinute = rawEnd === null ? startMinute : Math.max(startMinute, rawEnd);

    return { event, startMinute, endMinute, continuesBefore, continuesAfter };
};

/**
 * Which of a day's events belong above the time axis rather than on it.
 *
 * All-day events, and anything that covers this day end to end — a conference
 * entered with times still reads as "all day" on its middle days, and drawing
 * it as a full-height column would bury everything the day actually contains.
 */
export function splitDayEvents(events: EventMetadata[], dateStr: string): {
    allDay: EventMetadata[];
    timed: EventMetadata[];
} {
    const allDay: EventMetadata[] = [];
    const timed: EventMetadata[] = [];
    for (const event of events) {
        if (event.is_all_day) { allDay.push(event); continue; }
        const span = spanOnDay(event, dateStr);
        if (!span) { allDay.push(event); continue; }
        if (span.continuesBefore && span.continuesAfter) { allDay.push(event); continue; }
        timed.push(event);
    }
    return { allDay, timed };
}

/**
 * Place a day's timed events so that overlapping ones sit side by side.
 *
 * Every event in an hour used to be drawn at `absolute inset-x-0.5 top-0.5`
 * with a fixed height, so a day with two ten o'clock meetings showed one of
 * them and silently hid the other.
 *
 * The shape is the one every calendar uses: events that transitively overlap
 * form a cluster, each is given the first column that is free when it starts,
 * and a block then widens to the right for as long as nothing is in its way —
 * so a lone afternoon meeting is full width even when the morning was busy.
 */
export function layoutDay(events: EventMetadata[], dateStr: string): PositionedEvent[] {
    const spans: Span[] = [];
    for (const event of events) {
        const span = spanOnDay(event, dateStr);
        if (span) spans.push(span);
    }

    // Longest first among equal starts, so the big block takes the left
    // column and the short ones stack beside it rather than pushing it right.
    spans.sort((a, b) =>
        a.startMinute - b.startMinute
        || (b.endMinute - b.startMinute) - (a.endMinute - a.startMinute)
        || a.event.title.localeCompare(b.event.title)
        || a.event.id.localeCompare(b.event.id));

    // What a span occupies for the purpose of collisions, which is not always
    // what it lasts: see MIN_BLOCK_MINUTES.
    const blockEnd = (s: Span) => Math.min(MINUTES_PER_DAY, Math.max(s.endMinute, s.startMinute + MIN_BLOCK_MINUTES));

    const out: PositionedEvent[] = [];

    let i = 0;
    while (i < spans.length) {
        // One cluster: spans that overlap, directly or through a chain.
        let clusterEnd = blockEnd(spans[i]);
        let j = i + 1;
        while (j < spans.length && spans[j].startMinute < clusterEnd) {
            clusterEnd = Math.max(clusterEnd, blockEnd(spans[j]));
            j++;
        }
        const cluster = spans.slice(i, j);

        const columns: Span[][] = [];
        const columnOf = new Map<Span, number>();
        for (const span of cluster) {
            let placed = false;
            for (let c = 0; c < columns.length; c++) {
                const last = columns[c][columns[c].length - 1];
                if (blockEnd(last) <= span.startMinute) {
                    columns[c].push(span);
                    columnOf.set(span, c);
                    placed = true;
                    break;
                }
            }
            if (!placed) {
                columns.push([span]);
                columnOf.set(span, columns.length - 1);
            }
        }

        const total = columns.length;
        for (const span of cluster) {
            const col = columnOf.get(span)!;
            const end = blockEnd(span);

            let width = 1;
            for (let c = col + 1; c < total; c++) {
                const blocked = columns[c].some(
                    other => other.startMinute < end && blockEnd(other) > span.startMinute);
                if (blocked) break;
                width++;
            }

            out.push({
                event: span.event,
                key: `${span.event.id}@${dateStr}#${span.startMinute}`,
                startMinute: span.startMinute,
                endMinute: span.endMinute,
                topPct: (span.startMinute / MINUTES_PER_DAY) * 100,
                heightPct: ((end - span.startMinute) / MINUTES_PER_DAY) * 100,
                leftPct: (col / total) * 100,
                widthPct: (width / total) * 100,
                continuesBefore: span.continuesBefore,
                continuesAfter: span.continuesAfter,
            });
        }

        i = j;
    }

    return out;
}
