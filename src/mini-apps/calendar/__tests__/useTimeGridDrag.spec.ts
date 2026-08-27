import { describe, it, expect, vi, beforeEach } from 'vitest';
import { ref } from 'vue';
import { useTimeGridDrag, SNAP_MINUTES } from '../composables/useTimeGridDrag';
import { MIN_BLOCK_MINUTES } from '../layout';
import type { EventMetadata } from '../types';

/**
 * The grid is 700px wide over seven days and 1440px tall over a day, so one
 * pixel is one minute and one hundred pixels is one column. That keeps the
 * arithmetic in these tests readable instead of hiding the thing being tested.
 */
const WIDTH = 700;
const HEIGHT = 1440;
const DAYS = ['2026-03-09', '2026-03-10', '2026-03-11', '2026-03-12', '2026-03-13', '2026-03-14', '2026-03-15'];

const meeting: EventMetadata = {
    id: 'Events/a.md', title: 'Sync', is_all_day: false,
    start_at: '2026-03-10T10:00', end_at: '2026-03-10T11:00',
    location: '', tags: [], content: '', path: 'Events/a.md', created_at: '',
    relations: [], recurrence: 'none', recurrence_end_at: '', exceptions: [],
    series_id: '', reminders: [],
};

/** x for a column index, y for a minute. */
const xOf = (dayIndex: number) => dayIndex * (WIDTH / DAYS.length) + 10;
const yOf = (minute: number) => minute;

function harness() {
    const el = document.createElement('div');
    el.getBoundingClientRect = () => ({
        top: 0, left: 0, width: WIDTH, height: HEIGHT,
        right: WIDTH, bottom: HEIGHT, x: 0, y: 0, toJSON: () => ({}),
    });
    const onOpen = vi.fn();
    const onCreate = vi.fn();
    const onReschedule = vi.fn();
    const onBlock = vi.fn();
    const api = useTimeGridDrag({
        gridEl: ref(el), days: ref(DAYS), onOpen, onCreate, onReschedule, onBlock,
    });
    return { api, onOpen, onCreate, onReschedule, onBlock };
}

const down = (x: number, y: number, pointerType = 'mouse') => ({
    pointerType, button: 0, clientX: x, clientY: y,
    preventDefault() {}, stopPropagation() {},
}) as unknown as PointerEvent;

const move = (x: number, y: number) =>
    window.dispatchEvent(new MouseEvent('pointermove', { clientX: x, clientY: y }));
const up = () => window.dispatchEvent(new MouseEvent('pointerup'));

describe('moving an event', () => {
    let h: ReturnType<typeof harness>;
    beforeEach(() => { h = harness(); });

    it('drags it down the day and snaps to the grid', () => {
        h.api.startMove(down(xOf(1), yOf(600)), meeting, '2026-03-10', 600, 660);
        move(xOf(1), yOf(600 + 47)); // 47 minutes down, snaps to 45
        up();

        expect(h.onReschedule).toHaveBeenCalledWith(
            meeting, '2026-03-10', '2026-03-10T10:45', '2026-03-10T11:45');
    });

    it('keeps the duration when it lands', () => {
        h.api.startMove(down(xOf(1), yOf(600)), meeting, '2026-03-10', 600, 660);
        move(xOf(1), yOf(900));
        up();
        const [, , start, end] = h.onReschedule.mock.calls[0];
        expect(start).toBe('2026-03-10T15:00');
        expect(end).toBe('2026-03-10T16:00');
    });

    /** A week view that could not move a meeting to Thursday would be a list. */
    it('carries it to the column the pointer is over', () => {
        h.api.startMove(down(xOf(1), yOf(600)), meeting, '2026-03-10', 600, 660);
        move(xOf(4), yOf(600));
        up();
        expect(h.onReschedule).toHaveBeenCalledWith(
            meeting, '2026-03-10', '2026-03-13T10:00', '2026-03-13T11:00');
    });

    /**
     * Dragged to the bottom, an event ends *at* midnight. Naming that moment
     * 23:59 — which is what a clock label clamped to — shaves a minute off it
     * on every such drag.
     */
    it('ends at midnight rather than a minute short of it', () => {
        h.api.startMove(down(xOf(1), yOf(600)), meeting, '2026-03-10', 600, 660);
        move(xOf(1), yOf(2000));
        up();
        const [, , start, end] = h.onReschedule.mock.calls[0];
        expect(start).toBe('2026-03-10T23:00');
        expect(end).toBe('2026-03-11T00:00');
    });

    /** Under the threshold it is a click, and a click opens the editor. */
    it('opens the event when the pointer barely moved', () => {
        h.api.startMove(down(xOf(1), yOf(600)), meeting, '2026-03-10', 600, 660);
        move(xOf(1) + 2, yOf(601));
        up();
        expect(h.onOpen).toHaveBeenCalledWith(meeting, '2026-03-10');
        expect(h.onReschedule).not.toHaveBeenCalled();
    });

    /**
     * A drag that snapped back where it started is not an edit. Writing it
     * would send a recurring event through the this/following/all question
     * for nothing.
     */
    it('writes nothing when the drag lands where it began', () => {
        h.api.startMove(down(xOf(1), yOf(600)), meeting, '2026-03-10', 600, 660);
        move(xOf(1), yOf(605)); // snaps back to 600
        up();
        expect(h.onReschedule).not.toHaveBeenCalled();
        expect(h.onOpen).not.toHaveBeenCalled();
    });

    /** Touch is how the grid is scrolled; taking it over would cost more than it gives. */
    it('ignores a touch pointer', () => {
        h.api.startMove(down(xOf(1), yOf(600), 'touch'), meeting, '2026-03-10', 600, 660);
        move(xOf(1), yOf(900));
        up();
        expect(h.onReschedule).not.toHaveBeenCalled();
        expect(h.api.draft.value).toBeNull();
    });
});

describe('resizing an event', () => {
    it('moves the end and leaves the start alone', () => {
        const h = harness();
        h.api.startResize(down(xOf(1), yOf(660)), meeting, '2026-03-10', 600);
        move(xOf(1), yOf(750));
        up();
        expect(h.onReschedule).toHaveBeenCalledWith(
            meeting, '2026-03-10', '2026-03-10T10:00', '2026-03-10T12:30');
    });

    it('refuses to drag the end above the start', () => {
        const h = harness();
        h.api.startResize(down(xOf(1), yOf(660)), meeting, '2026-03-10', 600);
        move(xOf(1), yOf(100));
        up();
        const [, , , end] = h.onReschedule.mock.calls[0];
        expect(end).toBe(`2026-03-10T10:${String(MIN_BLOCK_MINUTES).padStart(2, '0')}`);
    });

    it('stays on its own day even if the pointer wanders sideways', () => {
        const h = harness();
        h.api.startResize(down(xOf(1), yOf(660)), meeting, '2026-03-10', 600);
        move(xOf(5), yOf(750));
        up();
        const [, dateStr, start] = h.onReschedule.mock.calls[0];
        expect(dateStr).toBe('2026-03-10');
        expect(start).toBe('2026-03-10T10:00');
    });
});

describe('drawing a new event', () => {
    it('reports the range that was drawn', () => {
        const h = harness();
        h.api.startCreate(down(xOf(2), yOf(540)), '2026-03-11');
        move(xOf(2), yOf(660));
        up();
        expect(h.onCreate).toHaveBeenCalledWith('2026-03-11', 540, 660);
    });

    it('reads a drag upward as the same range', () => {
        const h = harness();
        h.api.startCreate(down(xOf(2), yOf(660)), '2026-03-11');
        move(xOf(2), yOf(540));
        up();
        expect(h.onCreate).toHaveBeenCalledWith('2026-03-11', 540, 660);
    });

    it('snaps both ends to the grid', () => {
        const h = harness();
        h.api.startCreate(down(xOf(2), yOf(547)), '2026-03-11');
        move(xOf(2), yOf(668));
        up();
        const [, start, end] = h.onCreate.mock.calls[0];
        expect(start % SNAP_MINUTES).toBe(0);
        expect(end % SNAP_MINUTES).toBe(0);
    });

    it('gives a flick of the wrist a usable minimum', () => {
        const h = harness();
        h.api.startCreate(down(xOf(2), yOf(600)), '2026-03-11');
        move(xOf(2), yOf(606));
        up();
        const [, start, end] = h.onCreate.mock.calls[0];
        expect(end - start).toBeGreaterThanOrEqual(MIN_BLOCK_MINUTES);
    });

    it('creates nothing on a bare click', () => {
        const h = harness();
        h.api.startCreate(down(xOf(2), yOf(600)), '2026-03-11');
        up();
        expect(h.onCreate).not.toHaveBeenCalled();
    });
});

describe('giving a task a time by dragging it into the day', () => {
    const task = { id: 'Tasks/report.md', title: 'Write the report' };

    /**
     * The gesture is the feature: a task in the all-day row is something to
     * do, and pulling it down onto an hour is deciding when.
     */
    it('reports where the task was dropped, with a usable length', () => {
        const h = harness();
        h.api.startBlock(down(xOf(1), yOf(0)), task, '2026-03-10');
        move(xOf(1), yOf(600));
        up();
        expect(h.onBlock).toHaveBeenCalledWith(
            task, '2026-03-10', '2026-03-10T10:00', '2026-03-10T11:00');
    });

    it('carries the task to whichever day the pointer ends on', () => {
        const h = harness();
        h.api.startBlock(down(xOf(1), yOf(0)), task, '2026-03-10');
        move(xOf(4), yOf(540));
        up();
        const [, dateStr] = h.onBlock.mock.calls[0];
        expect(dateStr).toBe('2026-03-13');
    });

    it('will not drop a task past the end of the day', () => {
        const h = harness();
        h.api.startBlock(down(xOf(1), yOf(0)), task, '2026-03-10');
        move(xOf(1), yOf(2000));
        up();
        const [, , start, end] = h.onBlock.mock.calls[0];
        expect(start).toBe('2026-03-10T23:00');
        expect(end).toBe('2026-03-11T00:00');
    });

    /** A tap on a task is not an attempt to schedule it. */
    it('schedules nothing when the pointer barely moved', () => {
        const h = harness();
        h.api.startBlock(down(xOf(1), yOf(300)), task, '2026-03-10');
        move(xOf(1) + 2, yOf(301));
        up();
        expect(h.onBlock).not.toHaveBeenCalled();
    });

    it('shows what is being dragged before it lands', () => {
        const h = harness();
        h.api.startBlock(down(xOf(1), yOf(0)), task, '2026-03-10');
        move(xOf(1), yOf(600));
        expect(h.api.draft.value).toMatchObject({
            kind: 'block', label: 'Write the report', startMinute: 600, endMinute: 660,
        });
        up();
        expect(h.api.draft.value).toBeNull();
    });
});

describe('the draft', () => {
    it('follows the pointer and is cleared when the drag ends', () => {
        const h = harness();
        h.api.startMove(down(xOf(1), yOf(600)), meeting, '2026-03-10', 600, 660);
        move(xOf(3), yOf(750));
        expect(h.api.draft.value).toMatchObject({
            kind: 'move', dateStr: '2026-03-12', startMinute: 750, endMinute: 810,
        });
        up();
        expect(h.api.draft.value).toBeNull();
    });

    it('is dropped when the gesture is cancelled', () => {
        const h = harness();
        h.api.startMove(down(xOf(1), yOf(600)), meeting, '2026-03-10', 600, 660);
        move(xOf(1), yOf(900));
        window.dispatchEvent(new MouseEvent('pointercancel'));
        expect(h.api.draft.value).toBeNull();
        expect(h.onReschedule).not.toHaveBeenCalled();
    });
});
