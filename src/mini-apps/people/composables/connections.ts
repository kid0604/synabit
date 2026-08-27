/**
 * The links between two people, and what has to change when one goes away.
 *
 * A connection is recorded on both ends — A holds one naming B, B holds one
 * naming A. Anything that breaks a link therefore has two writes to do, and
 * forgetting the second is how the vault ends up holding connections that
 * point at a person who is no longer in it.
 */

export interface Connection {
    /**
     * Who the link points at.
     *
     * The other person's *stable identity* — the `node_id` in their own
     * frontmatter — not the path their file happens to sit at. A path breaks
     * the moment somebody is moved or renamed, which is the bug the rest of
     * the app fixed for every other kind of link and this one kept.
     *
     * Links written before that still hold a path. Both are matched on read;
     * a link is rewritten as an identity the next time it is touched.
     */
    person_id: string;
    relation_type: string;
    /**
     * The name as it stood when the link was made.
     *
     * Only written by versions before names were resolved live. Read it as a
     * last resort — the person's own node is the authority on what they are
     * called, and preferring this copy is what left old names showing in
     * everybody else's graph after a rename.
     */
    name?: string;
}

/** The patch that takes `removedId` out of one person's links. */
export interface LinkPatch {
    /** The vault path of the person to write. */
    id: string;
    title: string;
    properties: {
        connections: Connection[] | null;
        relations: string[] | null;
    };
}

/**
 * Everyone who links to `removedId`, and what their links should become.
 *
 * `null` rather than an empty array when nothing is left: a write is a patch,
 * and `null` is how it says "remove this key" — an empty array would leave an
 * empty list sitting in the file forever.
 */
/** Every way a link could name this person: their identity, and their path. */
export function namesFor(person: any): string[] {
    return [person?.properties?.node_id, person?.id].filter(Boolean);
}

/** Whether this connection points at any of `names`. */
export function pointsAt(connection: Connection, names: string[]): boolean {
    return names.includes(connection.person_id);
}

export function linkRemovalPatches(people: any[], removed: any): LinkPatch[] {
    // A string is still accepted so a caller holding only a path can ask.
    const names = typeof removed === 'string' ? [removed] : namesFor(removed);
    const patches: LinkPatch[] = [];

    for (const person of people) {
        if (!person || names.includes(person.id) || names.includes(person.properties?.node_id)) continue;

        const connections: Connection[] = person.properties?.connections || [];
        if (!connections.some(c => pointsAt(c, names))) continue;

        const kept = connections.filter(c => !pointsAt(c, names));
        // `relations` was the duplicate copy of this list, kept as markdown
        // mentions so the edge index would notice them. The index reads
        // `connections` directly now, so any left behind are cleared out.
        const relations: string[] = (person.properties?.relations || [])
            .filter((r: string) => !names.some(name => r.includes(name)));

        patches.push({
            id: person.id,
            title: person.title,
            properties: {
                connections: kept.length > 0 ? kept : null,
                relations: relations.length > 0 ? relations : null,
            },
        });
    }

    return patches;
}
