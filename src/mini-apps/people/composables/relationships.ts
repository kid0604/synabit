/**
 * What somebody is to you: friend, colleague, the person who introduced you.
 *
 * A list, and it was stored as one comma-separated string. Six places split it
 * apart and put it back together, and a relationship whose name contains a
 * comma — "Bạn, đồng nghiệp cũ" typed as one label — came back as two.
 *
 * An array now. Reading stays tolerant of the old shape, because a vault
 * written before this is still a vault, and every screen has to keep working
 * on the day it is opened rather than after somebody remembers to migrate.
 */

/** The relationships on a person, whichever way they are stored. */
export function relationshipsOf(person: any): string[] {
    const raw = person?.properties?.relationship_type;
    return normalizeRelationships(raw);
}

/** The same, from a raw property value. */
export function normalizeRelationships(raw: unknown): string[] {
    if (Array.isArray(raw)) {
        return raw
            .filter((r): r is string => typeof r === 'string')
            .map(r => r.trim())
            .filter(Boolean);
    }
    if (typeof raw === 'string') {
        // The old shape. A comma was the separator, so a comma inside a label
        // was already lost before it got here — nothing can recover it, and
        // splitting is what the vault meant at the time it was written.
        return raw.split(',').map(r => r.trim()).filter(Boolean);
    }
    return [];
}

/** One line for a place that has room for one line. */
export function relationshipLabel(person: any): string {
    return relationshipsOf(person).join(', ');
}

/** Whether any of this person's relationships matches a search. */
export function matchesRelationship(person: any, query: string): boolean {
    const needle = query.toLowerCase().trim();
    if (!needle) return false;
    return relationshipsOf(person).some(r => r.toLowerCase().includes(needle));
}

/** Title Case, the way the form offers them back as suggestions. */
export function titleCase(value: string): string {
    return value
        .split(' ')
        .filter(Boolean)
        .map(word => word.charAt(0).toUpperCase() + word.slice(1))
        .join(' ');
}
