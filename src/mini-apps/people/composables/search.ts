import { relationshipsOf } from './relationships';

/**
 * Finding somebody by typing.
 *
 * One predicate, used by the sidebar and the table alike — they had a copy
 * each, and the copies had already drifted: one searched relationships, the
 * other searched a flat `company` field that only some people have.
 *
 * # Why it is not full-text search
 *
 * The database has an FTS index over every node, and it would be the right
 * answer for searching the whole vault. It is the wrong answer here: this
 * list is already in memory, the answer has to change on every keystroke, and
 * a round trip to Rust per keystroke is slower than the scan below, not
 * faster. Measured, not assumed — see the benchmark in the tests.
 */

/** What a person can be found by, lowercased once. */
function haystack(person: any): string {
    const parts: string[] = [person.title ?? ''];

    for (const relationship of relationshipsOf(person)) parts.push(relationship);

    for (const tag of person.properties?.tags ?? []) {
        if (typeof tag === 'string') parts.push(tag);
    }
    for (const detail of person.properties?.details ?? []) {
        if (typeof detail?.label === 'string') parts.push(detail.label);
        if (typeof detail?.value === 'string') parts.push(detail.value);
    }
    // The flat copies, for people written before details existed.
    for (const key of ['email', 'phone', 'company', 'nickname']) {
        const value = person.properties?.[key];
        if (typeof value === 'string') parts.push(value);
    }

    return parts.join('').toLowerCase();
}

/**
 * The haystack for each person, worked out once and kept.
 *
 * Rebuilding it per keystroke is what made typing into a list of five
 * thousand contacts stutter: the same strings were lowercased and joined
 * again for every letter. Keyed on the person object, so a refetch — which
 * makes new objects — drops the old entries on its own.
 */
const cache = new WeakMap<object, string>();

function haystackFor(person: any): string {
    const found = cache.get(person);
    if (found !== undefined) return found;
    const built = haystack(person);
    cache.set(person, built);
    return built;
}

/** Whether this person answers to what was typed. */
export function personMatchesQuery(person: any, query: string): boolean {
    const needle = query.trim().toLowerCase();
    if (!needle) return true;
    return haystackFor(person).includes(needle);
}

/** The people who answer to what was typed, in the order they came. */
export function searchPeople(people: any[], query: string): any[] {
    const needle = query.trim().toLowerCase();
    if (!needle) return people;
    return people.filter(person => haystackFor(person).includes(needle));
}
