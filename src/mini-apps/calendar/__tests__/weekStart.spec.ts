import { describe, it, expect } from 'vitest';
import { weekStartsOn, daysSinceWeekStart } from '../weekStart';

/**
 * The grid was wired to Sunday. Vietnamese weeks start on Monday, so every
 * week view and every month grid was shifted by a day for the language half
 * this app ships in.
 */
describe('weekStartsOn', () => {
    it('starts a Vietnamese week on Monday', () => {
        expect(weekStartsOn('vi')).toBe(1);
        expect(weekStartsOn('vi-VN')).toBe(1);
    });

    it('starts a US English week on Sunday', () => {
        expect(weekStartsOn('en')).toBe(0);
        expect(weekStartsOn('en-US')).toBe(0);
    });

    it('is case-insensitive about the tag', () => {
        expect(weekStartsOn('VI')).toBe(1);
        expect(weekStartsOn('en-us')).toBe(0);
    });

    /** An engine without `getWeekInfo` must still answer, and answer sanely. */
    it('falls back to Monday for a locale it has no opinion about', () => {
        expect([0, 1, 6]).toContain(weekStartsOn('xx-YY'));
        expect(weekStartsOn('')).toBe(1);
    });
});

describe('daysSinceWeekStart', () => {
    it('measures from Sunday when the week starts on Sunday', () => {
        expect(daysSinceWeekStart(0, 0)).toBe(0); // Sunday
        expect(daysSinceWeekStart(6, 0)).toBe(6); // Saturday
    });

    it('measures from Monday when the week starts on Monday', () => {
        expect(daysSinceWeekStart(1, 1)).toBe(0); // Monday
        expect(daysSinceWeekStart(0, 1)).toBe(6); // Sunday is the last column
        expect(daysSinceWeekStart(6, 1)).toBe(5); // Saturday
    });

    it('never leaves the seven columns of a grid', () => {
        for (let start = 0; start < 7; start++) {
            for (let day = 0; day < 7; day++) {
                const n = daysSinceWeekStart(day, start);
                expect(n).toBeGreaterThanOrEqual(0);
                expect(n).toBeLessThan(7);
            }
        }
    });
});
