import { describe, it, expect } from 'vitest';
import { isSubscribed, paletteFor, coloursById } from '../subscriptions';
import type { Subscription } from '../subscriptions';

const sub = (id: string, colour: string): Subscription => ({
    id, colour, url: `https://e.com/${id}.ics`, name: id, enabled: true, remind: false,
    etag: '', lastModified: '', lastFetchedAt: 0, lastError: '', eventCount: 0, createdAt: 0,
});

describe('isSubscribed', () => {
    /**
     * Every event written before subscriptions existed has no such field, and
     * treating those as somebody else's would make an entire vault read-only.
     */
    it('treats an event with no source as the user’s own', () => {
        expect(isSubscribed({})).toBe(false);
        expect(isSubscribed({ subscription_id: '' })).toBe(false);
        expect(isSubscribed({ subscription_id: '   ' })).toBe(false);
        expect(isSubscribed({ subscription_id: undefined })).toBe(false);
    });

    it('treats an event from a calendar as read-only', () => {
        expect(isSubscribed({ subscription_id: 's1' })).toBe(true);
    });
});

describe('paletteFor', () => {
    /**
     * Written out rather than assembled: Tailwind keeps the classes it can
     * see in the source, so a class name built at runtime exists in the code
     * and not in the stylesheet — it looks right here and is invisible in the
     * app.
     */
    it('gives every colour a class that is written out in full', () => {
        for (const colour of ['teal', 'amber', 'rose', 'violet', 'sky', 'lime']) {
            const style = paletteFor(colour);
            expect(style.block).toContain(colour);
            expect(style.block).toContain('dark:');
            expect(style.dot).toContain(colour);
        }
    });

    it('falls back to the user’s own colour for anything it does not know', () => {
        const own = paletteFor(undefined);
        expect(paletteFor('')).toEqual(own);
        expect(paletteFor('chartreuse')).toEqual(own);
        expect(own.block).toContain('blue');
    });

    /** Two calendars must never be drawn the same. */
    it('gives each known colour a distinct block style', () => {
        const styles = ['teal', 'amber', 'rose', 'violet', 'sky', 'lime'].map(c => paletteFor(c).block);
        expect(new Set(styles).size).toBe(styles.length);
    });
});

describe('coloursById', () => {
    it('maps each calendar to the colour it was given', () => {
        expect(coloursById([sub('s1', 'teal'), sub('s2', 'rose')]))
            .toEqual({ s1: 'teal', s2: 'rose' });
    });

    it('has nothing to say about no calendars', () => {
        expect(coloursById([])).toEqual({});
    });
});
