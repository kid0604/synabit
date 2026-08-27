import { describe, it, expect, vi, afterEach } from 'vitest';
import { isElsewhere, shortZoneName, zoneOffsetLabel, localTimeZone, knownTimeZones } from '../timezone';

/**
 * Nothing here converts a time — Rust does that, once, during expansion. What
 * these cover is the other half: knowing when a time the reader is looking at
 * was written somewhere else, so the grid can say so instead of leaving a
 * seven-in-the-evening call unexplained.
 */
const pretendLocalZoneIs = (zone: string) => {
    const real = Intl.DateTimeFormat.bind(Intl) as (...a: unknown[]) => Intl.DateTimeFormat;
    vi.spyOn(Intl, 'DateTimeFormat').mockImplementation(((...args: unknown[]) => {
        const fmt = real(...args);
        // Only the no-argument call reports the machine's own zone.
        if (args.length === 0) {
            return {
                ...fmt,
                resolvedOptions: () => ({ ...fmt.resolvedOptions(), timeZone: zone }),
            };
        }
        return fmt;
    }) as never);
};

afterEach(() => { vi.restoreAllMocks(); });

describe('isElsewhere', () => {
    /**
     * An empty zone is floating — nine o'clock wherever you are — which is
     * every event written before zones existed. Badging those as "elsewhere"
     * would put a label on almost every event in an existing vault.
     */
    it('treats an event with no zone as here, not elsewhere', () => {
        expect(isElsewhere('')).toBe(false);
        expect(isElsewhere(undefined)).toBe(false);
        expect(isElsewhere('   ')).toBe(false);
    });

    it('treats the reader’s own zone as here', () => {
        pretendLocalZoneIs('Asia/Ho_Chi_Minh');
        expect(isElsewhere('Asia/Ho_Chi_Minh')).toBe(false);
    });

    it('treats any other zone as elsewhere, offset or not', () => {
        pretendLocalZoneIs('Asia/Ho_Chi_Minh');
        expect(isElsewhere('Asia/Tokyo')).toBe(true);
        // Same clock as Ho Chi Minh, but still written somewhere else — and
        // it will not stay the same clock if either place changes its rules.
        expect(isElsewhere('Asia/Bangkok')).toBe(true);
    });
});

describe('shortZoneName', () => {
    it('reads the city out of an IANA name', () => {
        expect(shortZoneName('Asia/Ho_Chi_Minh')).toBe('Ho Chi Minh');
        expect(shortZoneName('America/New_York')).toBe('New York');
        expect(shortZoneName('America/Argentina/Buenos_Aires')).toBe('Buenos Aires');
        expect(shortZoneName('UTC')).toBe('UTC');
    });

    it('has nothing to say about nothing', () => {
        expect(shortZoneName('')).toBe('');
        expect(shortZoneName('   ')).toBe('');
    });
});

describe('zoneOffsetLabel', () => {
    it('names the offset a zone is at', () => {
        expect(zoneOffsetLabel('Asia/Ho_Chi_Minh', '2026-03-10')).toMatch(/GMT\+7/);
        expect(zoneOffsetLabel('Asia/Tokyo', '2026-03-10')).toMatch(/GMT\+9/);
    });

    /** The label has to follow the date, or it lies for half the year. */
    it('follows daylight saving rather than quoting one figure all year', () => {
        expect(zoneOffsetLabel('America/New_York', '2026-01-15')).toMatch(/GMT-5/);
        expect(zoneOffsetLabel('America/New_York', '2026-07-15')).toMatch(/GMT-4/);
    });

    it('says nothing rather than guessing for a zone it does not know', () => {
        expect(zoneOffsetLabel('', '2026-03-10')).toBe('');
        expect(zoneOffsetLabel('Mars/Olympus', '2026-03-10')).toBe('');
    });

    it('does not throw on a date that is not one', () => {
        expect(() => zoneOffsetLabel('Asia/Tokyo', 'not a date')).not.toThrow();
    });
});

describe('what the picker offers', () => {
    it('offers real zones, including the reader’s own', () => {
        const zones = knownTimeZones();
        expect(zones.length).toBeGreaterThan(0);
        const here = localTimeZone();
        if (here) expect(zones).toContain(here);
    });

    it('offers names the conversion will actually accept', () => {
        for (const zone of knownTimeZones().slice(0, 25)) {
            expect(() => new Intl.DateTimeFormat('en', { timeZone: zone })).not.toThrow();
        }
    });
});
