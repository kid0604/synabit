import { computed, type Ref } from 'vue';

export type HealthStatus = 'thriving' | 'on_track' | 'due_soon' | 'overdue' | 'unknown';

export interface RelationshipHealth {
    status: HealthStatus;
    label: string;
    color: string;           // Tailwind text color class
    bgColor: string;         // Tailwind bg color class
    dotColor: string;        // Tailwind bg for dot
    percent: number;         // 0-100, for progress ring
    daysSinceContact: number | null;
    nextContactDue: number | null; // days until next contact is "due"
    interactionCount: number;
    relationshipAge: string; // human-readable duration
}

/**
 * How many days each cadence allows between one contact and the next.
 *
 * The one table in the app. It used to be copied into the sidebar and the
 * reminders widget as well, and the copies had drifted: two of them fell back
 * to 60 days for a cadence they did not recognise while this one treated it as
 * untracked, so the same person could be Overdue in one list and have no
 * status at all in another.
 */
export const FREQUENCY_DAYS: Record<string, number> = {
    weekly: 7,
    biweekly: 14,
    monthly: 30,
    quarterly: 90,
    yearly: 365,
};

/**
 * Where the boundaries between statuses sit, as a fraction of the cadence.
 *
 * Named rather than written inline because the reminders widget draws its
 * Overdue and Due Soon lists from the same numbers the person's own card
 * shows. When the widget used `1.0` for overdue and the card used `1.2`, a
 * contact could sit in the Overdue list and read "Due Soon" when opened.
 */
const THRESHOLDS = { thriving: 0.5, on_track: 0.85, due_soon: 1.2 } as const;

export const STATUS_CONFIG: Record<HealthStatus, { label: string; color: string; bgColor: string; dotColor: string }> = {
    thriving:  { label: 'Thriving',    color: 'text-green-600 dark:text-green-400',  bgColor: 'bg-green-100 dark:bg-green-900/20',  dotColor: 'bg-green-500' },
    on_track:  { label: 'On Track',    color: 'text-blue-600 dark:text-blue-400',    bgColor: 'bg-blue-100 dark:bg-blue-900/20',    dotColor: 'bg-blue-500' },
    due_soon:  { label: 'Due Soon',    color: 'text-yellow-600 dark:text-yellow-400',bgColor: 'bg-yellow-100 dark:bg-yellow-900/20', dotColor: 'bg-yellow-500' },
    overdue:   { label: 'Overdue',     color: 'text-red-600 dark:text-red-400',      bgColor: 'bg-red-100 dark:bg-red-900/20',      dotColor: 'bg-red-500' },
    unknown:   { label: 'Not Tracked', color: 'text-gray-500 dark:text-gray-400',    bgColor: 'bg-gray-100 dark:bg-gray-800',       dotColor: 'bg-gray-400' },
};

// ─── Pure helpers ───────────────────────────────────────────
//
// Everything below works on a plain person object so that the sidebar, the
// reminders widget and the person's own card all reach the same answer. The
// composable at the bottom is a reactive wrapper over these, not a second
// implementation.

/** Whole days since this person was last contacted, or null if never recorded. */
export function daysSinceContact(person: any, now: number = Date.now()): number | null {
    const last = person?.properties?.last_contacted;
    if (!last) return null;
    const then = new Date(last).getTime();
    if (Number.isNaN(then)) return null;
    return Math.floor((now - then) / (1000 * 60 * 60 * 24));
}

/** The cadence in days, or null when this person is not being tracked. */
export function cadenceDays(person: any): number | null {
    const freq = person?.properties?.contact_frequency;
    return freq ? (FREQUENCY_DAYS[freq] ?? null) : null;
}

/** Days until contact is due. Negative means it is already late. */
export function daysUntilDue(person: any, now: number = Date.now()): number | null {
    const since = daysSinceContact(person, now);
    const cadence = cadenceDays(person);
    if (since === null || cadence === null) return null;
    return cadence - since;
}

export function contactStatus(person: any, now: number = Date.now()): HealthStatus {
    const since = daysSinceContact(person, now);
    const cadence = cadenceDays(person);
    if (since === null || cadence === null) return 'unknown';
    const ratio = since / cadence;
    if (ratio <= THRESHOLDS.thriving) return 'thriving';
    if (ratio <= THRESHOLDS.on_track) return 'on_track';
    if (ratio <= THRESHOLDS.due_soon) return 'due_soon';
    return 'overdue';
}

/**
 * How much of the cadence is left, as 0–100.
 *
 * An untracked person scores 100 rather than something in the middle: sorting
 * by this puts whoever is most overdue first, and somebody with no cadence set
 * is not overdue — there is nothing they are late for.
 */
export function contactPercent(person: any, now: number = Date.now()): number {
    const since = daysSinceContact(person, now);
    const cadence = cadenceDays(person);
    if (since === null || cadence === null) return 100;
    return Math.max(0, Math.min(100, Math.round((1 - since / cadence) * 100)));
}

/**
 * The dot shown beside a name, or '' for a person who is not tracked.
 *
 * Untracked deliberately shows nothing rather than the grey of
 * `STATUS_CONFIG.unknown`: a dot on every row would say "status" where there
 * is none, and drown the few rows that do mean something.
 */
export function contactDotClass(person: any, now: number = Date.now()): string {
    const status = contactStatus(person, now);
    return status === 'unknown' ? '' : STATUS_CONFIG[status].dotColor;
}

/** Human-readable age of the relationship, e.g. `2y 3mo`. */
export function relationshipAge(person: any, now: number = Date.now()): string {
    const created = person?.created_at;
    if (!created) return '';
    const days = Math.floor((now - new Date(created).getTime()) / (1000 * 60 * 60 * 24));
    if (Number.isNaN(days)) return '';
    if (days < 30) return `${days}d`;
    if (days < 365) return `${Math.floor(days / 30)}mo`;
    const years = Math.floor(days / 365);
    const months = Math.floor((days % 365) / 30);
    return months > 0 ? `${years}y ${months}mo` : `${years}y`;
}

// ─── Reactive wrapper ───────────────────────────────────────

export function useRelationshipHealth(person: Ref<any>) {
    const daysSince = computed(() => daysSinceContact(person.value));
    const nextContactDue = computed(() => daysUntilDue(person.value));
    const percent = computed(() => contactPercent(person.value));
    const status = computed<HealthStatus>(() => contactStatus(person.value));

    const interactionCount = computed(() => person.value?.properties?.interactions?.length ?? 0);

    const health = computed<RelationshipHealth>(() => {
        const config = STATUS_CONFIG[status.value];
        return {
            status: status.value,
            label: config.label,
            color: config.color,
            bgColor: config.bgColor,
            dotColor: config.dotColor,
            percent: percent.value,
            daysSinceContact: daysSince.value,
            nextContactDue: nextContactDue.value,
            interactionCount: interactionCount.value,
            relationshipAge: relationshipAge(person.value),
        };
    });

    return { health, daysSinceContact: daysSince, nextContactDue, status, percent };
}
