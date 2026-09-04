import { describe, it, expect } from 'vitest';
import { layoutDay, splitDayEvents, minutesOfDay, clockOf, MIN_BLOCK_MINUTES } from '../layout';
import type { EventMetadata } from '../types';

const ev = (id: string, start: string, end: string, over: Partial<EventMetadata> = {}): EventMetadata => ({
    id, title: id, is_all_day: false, start_at: start, end_at: end,
    location: '', tags: [], content: '', created_at: '',
    relations: [], recurrence: 'none', recurrence_end_at: '', exceptions: [],
    series_id: '', reminders: [], ...over,
});

const DAY = '2026-03-10';
const at = (blocks: ReturnType<typeof layoutDay>, id: string) => blocks.find(b => b.event.id === id)!;

describe('minutesOfDay', () => {
    it('reads the clock, and refuses a bare date', () => {
        expect(minutesOfDay('2026-03-10T09:30')).toBe(570);
        expect(minutesOfDay('2026-03-10T00:00')).toBe(0);
        expect(minutesOfDay('2026-03-10T23:59')).toBe(1439);
        expect(minutesOfDay('2026-03-10')).toBeNull();
        expect(minutesOfDay('')).toBeNull();
    });

    it('round-trips through clockOf', () => {
        expect(clockOf(570)).toBe('09:30');
        expect(clockOf(0)).toBe('00:00');
        expect(clockOf(1439)).toBe('23:59');
    });
});

describe('what belongs above the axis', () => {
    it('sends all-day events to the all-day row', () => {
        const split = splitDayEvents([ev('a', DAY, DAY, { is_all_day: true })], DAY);
        expect(split.allDay.map(e => e.id)).toEqual(['a']);
        expect(split.timed).toEqual([]);
    });

    /**
     * A trip entered with times reads as "all day" on its middle days. Drawing
     * it as a full-height column would bury everything else the day holds.
     */
    it('sends a timed event that covers the whole day to the all-day row', () => {
        const trip = ev('trip', '2026-03-09T18:00', '2026-03-12T11:00');
        expect(splitDayEvents([trip], DAY).allDay.map(e => e.id)).toEqual(['trip']);
    });

    /** But its first and last days have a real edge, so they go on the axis. */
    it('keeps the first and last day of a trip on the axis', () => {
        const trip = ev('trip', '2026-03-09T18:00', '2026-03-12T11:00');
        expect(splitDayEvents([trip], '2026-03-09').timed.map(e => e.id)).toEqual(['trip']);
        expect(splitDayEvents([trip], '2026-03-12').timed.map(e => e.id)).toEqual(['trip']);
    });
});

describe('height follows duration', () => {
    /** The old grid drew every event 56px tall whatever it lasted. */
    it('draws a three-hour meeting three times a one-hour one', () => {
        const blocks = layoutDay([
            ev('long', `${DAY}T09:00`, `${DAY}T12:00`),
            ev('short', `${DAY}T14:00`, `${DAY}T15:00`),
        ], DAY);
        expect(at(blocks, 'long').heightPct / at(blocks, 'short').heightPct).toBeCloseTo(3, 5);
    });

    /** And it put a 10:45 meeting at the top of the ten o'clock cell. */
    it('places a quarter-to start three quarters through the hour', () => {
        const blocks = layoutDay([ev('a', `${DAY}T10:45`, `${DAY}T11:00`)], DAY);
        expect(at(blocks, 'a').topPct).toBeCloseTo((645 / 1440) * 100, 5);
    });

    it('gives a zero-length event a floor rather than no height', () => {
        const blocks = layoutDay([ev('a', `${DAY}T10:00`, `${DAY}T10:00`)], DAY);
        expect(at(blocks, 'a').heightPct).toBeCloseTo((MIN_BLOCK_MINUTES / 1440) * 100, 5);
    });

    it('clips an instance that starts before this day at midnight', () => {
        const blocks = layoutDay([ev('a', '2026-03-09T22:00', `${DAY}T01:30`)], DAY);
        const b = at(blocks, 'a');
        expect(b.startMinute).toBe(0);
        expect(b.endMinute).toBe(90);
        expect(b.continuesBefore).toBe(true);
        expect(b.continuesAfter).toBe(false);
    });

    it('clips an instance that runs past midnight at the end of the day', () => {
        const blocks = layoutDay([ev('a', `${DAY}T22:00`, '2026-03-11T01:30')], DAY);
        const b = at(blocks, 'a');
        expect(b.startMinute).toBe(1320);
        expect(b.endMinute).toBe(1440);
        expect(b.continuesAfter).toBe(true);
    });
});

describe('overlapping events sit side by side', () => {
    /**
     * The regression this algorithm exists for: in the old week view every
     * event in an hour got the same absolute position, so only the last one
     * drawn was visible.
     */
    it('gives two overlapping meetings half the width each', () => {
        const blocks = layoutDay([
            ev('a', `${DAY}T10:00`, `${DAY}T11:00`),
            ev('b', `${DAY}T10:30`, `${DAY}T11:30`),
        ], DAY);
        expect(at(blocks, 'a').widthPct).toBeCloseTo(50, 5);
        expect(at(blocks, 'b').widthPct).toBeCloseTo(50, 5);
        expect(at(blocks, 'a').leftPct).toBeCloseTo(0, 5);
        expect(at(blocks, 'b').leftPct).toBeCloseTo(50, 5);
    });

    it('gives three mutually overlapping meetings a third each', () => {
        const blocks = layoutDay([
            ev('a', `${DAY}T10:00`, `${DAY}T11:00`),
            ev('b', `${DAY}T10:15`, `${DAY}T11:15`),
            ev('c', `${DAY}T10:30`, `${DAY}T11:30`),
        ], DAY);
        for (const id of ['a', 'b', 'c']) {
            expect(at(blocks, id).widthPct).toBeCloseTo(100 / 3, 5);
        }
        const lefts = ['a', 'b', 'c'].map(id => at(blocks, id).leftPct).sort((x, y) => x - y);
        expect(lefts[0]).toBeCloseTo(0, 5);
        expect(lefts[1]).toBeCloseTo(100 / 3, 5);
        expect(lefts[2]).toBeCloseTo(200 / 3, 5);
    });

    it('lets two events that only touch share a column, full width each', () => {
        const blocks = layoutDay([
            ev('a', `${DAY}T09:00`, `${DAY}T10:00`),
            ev('b', `${DAY}T10:00`, `${DAY}T11:00`),
        ], DAY);
        expect(at(blocks, 'a').widthPct).toBeCloseTo(100, 5);
        expect(at(blocks, 'b').widthPct).toBeCloseTo(100, 5);
    });

    it('leaves an unrelated afternoon meeting at full width', () => {
        const blocks = layoutDay([
            ev('a', `${DAY}T10:00`, `${DAY}T11:00`),
            ev('b', `${DAY}T10:30`, `${DAY}T11:30`),
            ev('solo', `${DAY}T15:00`, `${DAY}T16:00`),
        ], DAY);
        expect(at(blocks, 'solo').widthPct).toBeCloseTo(100, 5);
        expect(at(blocks, 'solo').leftPct).toBeCloseTo(0, 5);
    });

    /**
     * The reason blocks widen to the right: a long morning meeting overlapped
     * only at its start should not stay a narrow sliver for its whole length
     * when the column beside it is free.
     */
    it('widens a block into the free space beside it', () => {
        const blocks = layoutDay([
            ev('long', `${DAY}T09:00`, `${DAY}T12:00`),
            ev('early', `${DAY}T09:00`, `${DAY}T09:30`),
            ev('late', `${DAY}T13:00`, `${DAY}T14:00`),
        ], DAY);
        expect(at(blocks, 'long').leftPct).toBeCloseTo(0, 5);
        expect(at(blocks, 'long').widthPct).toBeCloseTo(50, 5);
        expect(at(blocks, 'late').widthPct).toBeCloseTo(100, 5);
    });

    it('treats two back-to-back short events as overlapping if they are too short to draw apart', () => {
        // Five minutes apart: both need a readable block, so they share.
        const blocks = layoutDay([
            ev('a', `${DAY}T10:00`, `${DAY}T10:05`),
            ev('b', `${DAY}T10:05`, `${DAY}T10:10`),
        ], DAY);
        expect(at(blocks, 'a').widthPct).toBeCloseTo(50, 5);
        expect(at(blocks, 'b').widthPct).toBeCloseTo(50, 5);
    });

    it('is stable: the same input lays out the same way twice', () => {
        const input = [
            ev('b', `${DAY}T10:00`, `${DAY}T11:00`),
            ev('a', `${DAY}T10:00`, `${DAY}T11:00`),
        ];
        const first = layoutDay(input, DAY).map(b => [b.event.id, b.leftPct]);
        const second = layoutDay([...input].reverse(), DAY).map(b => [b.event.id, b.leftPct]);
        expect([...first].sort()).toEqual([...second].sort());
    });

    it('gives an empty day nothing to draw', () => {
        expect(layoutDay([], DAY)).toEqual([]);
        expect(layoutDay([ev('a', DAY, DAY, { is_all_day: true })], DAY)).toEqual([]);
    });

    it('never lets a block escape the day', () => {
        const blocks = layoutDay([
            ev('a', '2026-03-09T23:00', '2026-03-11T02:00'),
            ev('b', `${DAY}T23:50`, `${DAY}T23:59`),
        ], DAY);
        for (const b of blocks) {
            expect(b.topPct).toBeGreaterThanOrEqual(0);
            expect(b.topPct + b.heightPct).toBeLessThanOrEqual(100.0001);
            expect(b.leftPct + b.widthPct).toBeLessThanOrEqual(100.0001);
        }
    });
});
