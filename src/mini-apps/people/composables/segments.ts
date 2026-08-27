/**
 * Saved views of the address book: "colleagues in Đà Nẵng", "people to see
 * this quarter".
 *
 * Kept as `filter` nodes, which the app already has, rather than as a second
 * mechanism beside it. A segment is a file like anything else, so it syncs,
 * it is searchable, and somebody can edit it by hand.
 *
 * A segment stores a *question*, never a list of people. A list would be
 * wrong the moment somebody new matched it, and would have to be maintained
 * by hand forever.
 */

import { relationshipsOf } from './relationships';
import { contactStatus, type HealthStatus } from './useRelationshipHealth';
import { daysUntilAnnual } from './anniversaries';

export interface Segment {
    /** The vault path of the filter node. */
    id: string;
    name: string;
    /** Free text, matched the way the search box matches. */
    query: string;
    /** Any one of these relationships. Empty means any. */
    relationships: string[];
    /** Any one of these tags. Empty means any. */
    tags: string[];
    /** Any one of these states. Empty means any. */
    statuses: HealthStatus[];
    /** Only people with a birthday inside this many days. */
    birthdayWithinDays: number | null;
}

export const emptySegment = (): Omit<Segment, 'id'> => ({
    name: '',
    query: '',
    relationships: [],
    tags: [],
    statuses: [],
    birthdayWithinDays: null,
});

/** Read a filter node into a segment, tolerating anything missing. */
export function segmentFromNode(node: any): Segment {
    const p = node?.properties ?? {};
    const list = (value: unknown): string[] =>
        Array.isArray(value) ? value.filter((v): v is string => typeof v === 'string') : [];

    return {
        id: node.id,
        name: node.title || 'Untitled',
        query: typeof p.query === 'string' ? p.query : '',
        relationships: list(p.relationships),
        tags: list(p.tags),
        statuses: list(p.statuses) as HealthStatus[],
        birthdayWithinDays:
            typeof p.birthday_within_days === 'number' ? p.birthday_within_days : null,
    };
}

/** The properties to write for a segment. A patch, so `null` clears. */
export function segmentToProperties(segment: Omit<Segment, 'id'>): Record<string, unknown> {
    return {
        // What this filter is about, so a filter over people is not offered
        // as a filter over notes.
        subject: 'person',
        query: segment.query.trim() || null,
        relationships: segment.relationships.length > 0 ? segment.relationships : null,
        tags: segment.tags.length > 0 ? segment.tags : null,
        statuses: segment.statuses.length > 0 ? segment.statuses : null,
        birthday_within_days: segment.birthdayWithinDays ?? null,
    };
}

/** Whether a segment asks anything at all. An empty one matches everybody. */
export function isEmptySegment(segment: Omit<Segment, 'id'>): boolean {
    return (
        !segment.query.trim() &&
        segment.relationships.length === 0 &&
        segment.tags.length === 0 &&
        segment.statuses.length === 0 &&
        segment.birthdayWithinDays === null
    );
}

/**
 * Whether one person answers a segment's question.
 *
 * Every condition narrows: a segment naming both a relationship and a state
 * matches people who are both, not either. Within one condition, any value
 * will do — "colleague or client" is one question, not two.
 */
export function personMatches(segment: Segment, person: any, now: Date = new Date()): boolean {
    if (person?.properties?.is_owner) return false;

    const query = segment.query.trim().toLowerCase();
    if (query) {
        const haystack = [
            person.title,
            ...relationshipsOf(person),
            ...(person.properties?.tags ?? []),
            ...(person.properties?.details ?? []).flatMap((d: any) => [d.label, d.value]),
        ]
            .filter(v => typeof v === 'string')
            .join(' ')
            .toLowerCase();
        if (!haystack.includes(query)) return false;
    }

    if (segment.relationships.length > 0) {
        const theirs = relationshipsOf(person).map(r => r.toLowerCase());
        if (!segment.relationships.some(r => theirs.includes(r.toLowerCase()))) return false;
    }

    if (segment.tags.length > 0) {
        const theirs: string[] = (person.properties?.tags ?? []).map((t: string) =>
            String(t).toLowerCase()
        );
        if (!segment.tags.some(t => theirs.includes(t.toLowerCase()))) return false;
    }

    if (segment.statuses.length > 0) {
        if (!segment.statuses.includes(contactStatus(person, now.getTime()))) return false;
    }

    if (segment.birthdayWithinDays !== null) {
        const days = daysUntilAnnual(person?.properties?.birthday ?? '', now);
        if (days === null || days > segment.birthdayWithinDays) return false;
    }

    return true;
}

/** The people a segment is about. */
export function peopleIn(segment: Segment, people: any[], now: Date = new Date()): any[] {
    return people.filter(person => personMatches(segment, person, now));
}
