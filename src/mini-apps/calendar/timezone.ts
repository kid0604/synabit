/**
 * Naming the zone this machine is in, and saying so only when it matters.
 *
 * An event carries the wall clock of the place it happens, plus the name of
 * that place's zone. Everything the grid draws has already been converted to
 * the reader's zone by the time it arrives — see
 * `src-tauri/src/calendar/recurrence.rs` — so nothing here converts anything.
 * What is left is telling the reader when a time they are looking at was
 * written somewhere else.
 */

/** The IANA name for this machine's zone, or `''` if it will not say. */
export const localTimeZone = (): string => {
    try {
        return Intl.DateTimeFormat().resolvedOptions().timeZone || '';
    } catch {
        return '';
    }
};

/**
 * Is this event written in a zone other than the reader's?
 *
 * An empty `tzid` is floating — nine o'clock wherever you are — which is what
 * every event written before zones existed is, and is never "elsewhere".
 */
export const isElsewhere = (tzid: string | undefined): boolean => {
    const zone = (tzid || '').trim();
    return zone !== '' && zone !== localTimeZone();
};

/** `Asia/Ho_Chi_Minh` reads better as `Ho Chi Minh`. */
export const shortZoneName = (tzid: string): string => {
    const raw = (tzid || '').trim();
    if (!raw) return '';
    const tail = raw.split('/').pop() || raw;
    return tail.replace(/_/g, ' ');
};

/**
 * The offset a zone is at on a given day, as `GMT+7` — what a badge shows
 * next to a time so the reader can do the sum themselves.
 */
export const zoneOffsetLabel = (tzid: string, onDate?: string): string => {
    const zone = (tzid || '').trim();
    if (!zone) return '';
    try {
        const when = onDate ? new Date(`${onDate.split('T')[0]}T12:00:00`) : new Date();
        const parts = new Intl.DateTimeFormat('en-GB', {
            timeZone: zone, timeZoneName: 'shortOffset',
        }).formatToParts(Number.isNaN(when.getTime()) ? new Date() : when);
        return parts.find(p => p.type === 'timeZoneName')?.value ?? '';
    } catch {
        return '';
    }
};

/** The zones offered in the editor: everything this browser knows about. */
export const knownTimeZones = (): string[] => {
    const here = localTimeZone();
    let zones: string[] = [];
    try {
        const supported = (Intl as unknown as { supportedValuesOf?: (k: string) => string[] })
            .supportedValuesOf;
        if (supported) zones = supported('timeZone');
    } catch {
        // An engine without `Intl.supportedValuesOf` — see the browser support
        // note in CLAUDE.md. The reader's own zone is still offerable.
    }
    // `supportedValuesOf` lists canonical names only, and leaves out the
    // aliases a machine can still report as its own — `UTC` among them. A
    // reader whose clock says UTC would open the picker and not find where
    // they are, which is the one entry the list has to have.
    if (here && !zones.includes(here)) zones = [here, ...zones];
    return zones;
};
