import { describe, it, expect } from 'vitest';
import {
    shiftDateString, daysBetween, formatDateString, indexOccurrencesByDate,
} from '../helpers';
import type { EventsInRange } from '../../../types/ipc';

/**
 * Recurrence is not tested here, and cannot be: there is no longer a
 * TypeScript implementation of it to test. `contracts/recurrence.json` is run
 * against the single implementation in
 * `src-tauri/src/calendar/recurrence.rs`. What remains on this side is the
 * date arithmetic the front end still does for itself, and the seam where the
 * vault's answer becomes something a day cell can read.
 */
describe('indexOccurrencesByDate', () => {
    const range: EventsInRange = {
        events: [
            {
                id: 'Events/standup.md', title: 'Standup', is_all_day: false,
                start_at: '2026-03-02T09:00', end_at: '2026-03-02T09:15',
                location: '', tags: [], tzid: '', colour: '', subscription_id: '', rrule: 'FREQ=WEEKLY', recurrence: '', recurrence_end_at: '',
                series_id: '', exceptions: [], reminders: [], relations: [], created_at: '',
            },
            {
                id: 'Events/launch.md', title: 'Launch', is_all_day: true,
                start_at: '2026-03-09', end_at: '2026-03-09',
                location: '', tags: [], tzid: '', colour: '', subscription_id: '', rrule: '', recurrence: '', recurrence_end_at: '',
                series_id: '', exceptions: [], reminders: [], relations: [], created_at: '',
            },
        ],
        occurrences: [
            { date: '2026-03-02', event: 0, start_at: '2026-03-02T09:00', end_at: '2026-03-02T09:15' },
            { date: '2026-03-09', event: 0, start_at: '2026-03-09T09:00', end_at: '2026-03-09T09:15' },
            { date: '2026-03-09', event: 1, start_at: '2026-03-09', end_at: '2026-03-09' },
        ],
    };

    it('groups every occurrence under the day it lands on', () => {
        const byDate = indexOccurrencesByDate(range);
        expect(byDate.get('2026-03-02')?.map(e => e.title)).toEqual(['Standup']);
        expect(byDate.get('2026-03-09')?.map(e => e.title)).toEqual(['Standup', 'Launch']);
    });

    it('gives a day with nothing on it no entry rather than an empty one', () => {
        expect(indexOccurrencesByDate(range).has('2026-03-03')).toBe(false);
    });

    /**
     * Each day gets the times of the instance landing on it, not the series'
     * first occurrence — which is what a time axis needs, and what the whole
     * grid would stack on one day without.
     */
    it('gives each occurrence its own instance times', () => {
        const byDate = indexOccurrencesByDate(range);
        expect(byDate.get('2026-03-02')![0].start_at).toBe('2026-03-02T09:00');
        expect(byDate.get('2026-03-09')![0].start_at).toBe('2026-03-09T09:00');
    });

    /** The wire still carries one summary and many references to it. */
    it('does not send an event once per day', () => {
        expect(range.events.length).toBe(2);
        expect(range.occurrences.length).toBe(3);
    });

    it('survives a reference to an event that is not in the payload', () => {
        const broken = { events: [], occurrences: [{ date: '2026-03-02', event: 7, start_at: '', end_at: '' }] };
        expect(indexOccurrencesByDate(broken as EventsInRange).size).toBe(0);
    });

    it('survives an empty answer', () => {
        expect(indexOccurrencesByDate({ events: [], occurrences: [] }).size).toBe(0);
        expect(indexOccurrencesByDate(undefined as unknown as EventsInRange).size).toBe(0);
    });
});

describe('the date helpers refuse to write nonsense', () => {
    /**
     * Both feed a vault write. Before the guards, an unset occurrence date
     * made `daysBetween` return NaN and `shiftDateString` turn a real date
     * into the string "NaN-NaN-NaN" in someone's frontmatter.
     */
    it('leaves an unparseable date alone instead of mangling it', () => {
        expect(shiftDateString('', -1)).toBe('');
        expect(shiftDateString('tomorrow', 1)).toBe('tomorrow');
        expect(shiftDateString('2026-03-01', NaN)).toBe('2026-03-01');
    });

    it('treats a missing end of the range as no movement', () => {
        expect(daysBetween('', '2026-03-01')).toBe(0);
        expect(daysBetween('2026-03-01', '')).toBe(0);
    });
});

describe('daysBetween', () => {
    it('counts whole days in both directions', () => {
        expect(daysBetween('2026-03-01', '2026-03-04')).toBe(3);
        expect(daysBetween('2026-03-04', '2026-03-01')).toBe(-3);
        expect(daysBetween('2026-03-01', '2026-03-01')).toBe(0);
    });

    it('survives a daylight saving boundary', () => {
        // 2026-03-08 is the US spring-forward date; the day is 23 hours long.
        expect(daysBetween('2026-03-07', '2026-03-09')).toBe(2);
    });
});

describe('formatDateString', () => {
    it('reads the local fields, not the UTC instant', () => {
        expect(formatDateString(new Date(2026, 0, 1))).toBe('2026-01-01');
        expect(formatDateString(new Date(2026, 11, 31))).toBe('2026-12-31');
    });
});
