import { describe, it, expect } from 'vitest';
import { reviewOf, minutesOn, asHours } from '../review';
import type { EventMetadata } from '../types';

const ev = (title: string, start: string, end: string, over: Partial<EventMetadata> = {}): EventMetadata => ({
    id: title, title, is_all_day: false, start_at: start, end_at: end,
    location: '', tags: [], content: '', path: title, created_at: '',
    relations: [], tzid: '', rrule: '', recurrence: '', recurrence_end_at: '',
    exceptions: [], series_id: '', reminders: [], subscription_id: '', ...over,
});

const D1 = '2026-03-10';
const D2 = '2026-03-11';

describe('minutesOn', () => {
    it('counts an ordinary meeting', () => {
        expect(minutesOn(ev('a', `${D1}T09:00`, `${D1}T10:30`), D1)).toBe(90);
    });

    /** An all-day event has no hours to count, and counting it as 24 would
     *  drown every real meeting in the summary. */
    it('counts an all-day event as no hours at all', () => {
        expect(minutesOn(ev('a', D1, D1, { is_all_day: true }), D1)).toBe(0);
    });

    it('counts only the part that falls on the day being asked about', () => {
        const overnight = ev('a', `${D1}T22:00`, `${D2}T01:30`);
        expect(minutesOn(overnight, D1)).toBe(120);
        expect(minutesOn(overnight, D2)).toBe(90);
    });

    it('does not go negative on nonsense', () => {
        expect(minutesOn(ev('a', `${D1}T10:00`, `${D1}T09:00`), D1)).toBe(0);
        expect(minutesOn(ev('a', '', ''), D1)).toBe(0);
    });
});

describe('reviewOf', () => {
    const anh = '[Anh](synabit://person/People/anh.md)';
    const binh = '[Bình](synabit://person/People/binh.md)';

    const days = [
        { date: D1, events: [
            ev('Standup', `${D1}T09:00`, `${D1}T09:15`, { tags: ['work'], relations: [anh] }),
            ev('Design review', `${D1}T10:00`, `${D1}T13:00`, { tags: ['work', 'design'], relations: [anh, binh] }),
            ev('Holiday', D1, D1, { is_all_day: true, tags: ['personal'] }),
        ] },
        { date: D2, events: [
            ev('One to one', `${D2}T14:00`, `${D2}T15:00`, { tags: ['work'], relations: [anh] }),
        ] },
    ];

    it('counts what happened', () => {
        const r = reviewOf(days);
        expect(r.events).toBe(4);
        expect(r.allDayEvents).toBe(1);
        expect(r.minutes).toBe(15 + 180 + 60);
    });

    it('names the day that took the most', () => {
        expect(reviewOf(days).busiestDay).toEqual({ date: D1, minutes: 195 });
    });

    /** The point of the whole thing: hours against the people they went to. */
    it('ranks people by the time spent with them', () => {
        const r = reviewOf(days);
        expect(r.people.map(p => [p.label, p.minutes, p.count])).toEqual([
            ['Anh', 255, 3],
            ['Bình', 180, 1],
        ]);
    });

    it('ranks tags the same way', () => {
        const r = reviewOf(days);
        expect(r.tags[0]).toEqual({ label: 'work', minutes: 255, count: 3 });
        // A tag only ever on an all-day event has no hours, but is still there.
        expect(r.tags.find(t => t.label === 'personal')).toEqual({ label: 'personal', minutes: 0, count: 1 });
    });

    it('keeps the list short enough to read', () => {
        const many = [{ date: D1, events: Array.from({ length: 12 }, (_, i) =>
            ev(`e${i}`, `${D1}T09:00`, `${D1}T10:00`, { tags: [`tag${i}`] })) }];
        expect(reviewOf(many).tags).toHaveLength(5);
        expect(reviewOf(many, 3).tags).toHaveLength(3);
    });

    it('has nothing to say about nothing', () => {
        const r = reviewOf([]);
        expect(r).toEqual({ events: 0, allDayEvents: 0, minutes: 0, busiestDay: null, tags: [], people: [] });
    });
});

describe('asHours', () => {
    it('reads the way people say it', () => {
        expect(asHours(0)).toBe('0m');
        expect(asHours(45)).toBe('45m');
        expect(asHours(60)).toBe('1h');
        expect(asHours(90)).toBe('1h 30m');
        expect(asHours(255)).toBe('4h 15m');
        expect(asHours(480)).toBe('8h');
    });
});
