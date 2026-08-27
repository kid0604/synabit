import type { EventMetadata } from './types';

/**
 * Calendars belonging to somebody else.
 *
 * A subscribed event is a cache of a feed: the next refresh replaces it, and
 * nothing here may offer to change it. That is enforced where it cannot be
 * forgotten — such an event is not a file in the vault, and its id is not a
 * path anything could write to — but the screen still has to *look* like it,
 * or people will try.
 */
export interface Subscription {
    id: string;
    url: string;
    name: string;
    colour: string;
    enabled: boolean;
    /** Announce these events the way the user's own are announced. */
    remind: boolean;
    etag: string;
    lastModified: string;
    lastFetchedAt: number;
    lastError: string;
    eventCount: number;
    createdAt: number;
}

export interface RefreshReport {
    id: string;
    name: string;
    unchanged: boolean;
    events: number;
    error: string;
}

/** Is this event somebody else's? */
export const isSubscribed = (event: Pick<EventMetadata, 'subscription_id'>): boolean =>
    !!(event.subscription_id && event.subscription_id.trim());

/**
 * How each calendar's events are drawn.
 *
 * Written out rather than built from the colour name, because Tailwind reads
 * the source for the classes it keeps: a string assembled at runtime is a
 * class that exists in the code and not in the stylesheet.
 */
const PALETTE: Record<string, { block: string; dot: string; swatch: string }> = {
    blue: {
        block: 'bg-blue-100/90 text-blue-900 border-blue-300/70 dark:bg-blue-900/50 dark:text-blue-100 dark:border-blue-700/60',
        dot: 'bg-blue-500',
        swatch: 'bg-blue-500',
    },
    teal: {
        block: 'bg-teal-100/90 text-teal-900 border-teal-300/70 dark:bg-teal-900/50 dark:text-teal-100 dark:border-teal-700/60',
        dot: 'bg-teal-500',
        swatch: 'bg-teal-500',
    },
    amber: {
        block: 'bg-amber-100/90 text-amber-900 border-amber-300/70 dark:bg-amber-900/50 dark:text-amber-100 dark:border-amber-700/60',
        dot: 'bg-amber-500',
        swatch: 'bg-amber-500',
    },
    rose: {
        block: 'bg-rose-100/90 text-rose-900 border-rose-300/70 dark:bg-rose-900/50 dark:text-rose-100 dark:border-rose-700/60',
        dot: 'bg-rose-500',
        swatch: 'bg-rose-500',
    },
    violet: {
        block: 'bg-violet-100/90 text-violet-900 border-violet-300/70 dark:bg-violet-900/50 dark:text-violet-100 dark:border-violet-700/60',
        dot: 'bg-violet-500',
        swatch: 'bg-violet-500',
    },
    sky: {
        block: 'bg-sky-100/90 text-sky-900 border-sky-300/70 dark:bg-sky-900/50 dark:text-sky-100 dark:border-sky-700/60',
        dot: 'bg-sky-500',
        swatch: 'bg-sky-500',
    },
    lime: {
        block: 'bg-lime-100/90 text-lime-900 border-lime-300/70 dark:bg-lime-900/50 dark:text-lime-100 dark:border-lime-700/60',
        dot: 'bg-lime-500',
        swatch: 'bg-lime-500',
    },
};

/** The colours an event of your own can be given, in the order offered. */
export const EVENT_COLOURS = ['blue', 'teal', 'amber', 'rose', 'violet', 'sky', 'lime'] as const;

/** What an event with no colour of its own looks like. */
const OWN = {
    block: 'bg-blue-100/90 text-blue-900 border-blue-300/70 dark:bg-blue-900/50 dark:text-blue-100 dark:border-blue-700/60',
    dot: 'bg-blue-500',
    swatch: 'bg-blue-500',
};

export const paletteFor = (colour: string | undefined) =>
    (colour && PALETTE[colour]) || OWN;

/**
 * How to draw one event.
 *
 * A subscribed event wears its calendar's colour — the reader did not choose
 * it and cannot, so the calendar it came from is the only useful thing the
 * colour can say. Everything else wears whatever the person picked, and blue
 * when they picked nothing, which is every event written before this existed.
 */
export const styleForEvent = (
    event: { subscription_id?: string; colour?: string },
    subscriptionColours: Record<string, string>,
) => isSubscribed(event)
    ? paletteFor(subscriptionColours[event.subscription_id || ''])
    : paletteFor(event.colour);

/** The colour name each subscription was given, by id. */
export const coloursById = (subs: Subscription[]): Record<string, string> => {
    const out: Record<string, string> = {};
    for (const sub of subs) out[sub.id] = sub.colour;
    return out;
};
