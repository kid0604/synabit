import { ref, computed, onUnmounted, getCurrentInstance } from 'vue';
import type { Ref } from 'vue';
import type { EventMetadata } from '../types';
import { MINUTES_PER_DAY, MIN_BLOCK_MINUTES, clockOf } from '../layout';
import { shiftDateString } from '../helpers';

export const SNAP_MINUTES = 15;

/** How far the pointer has to travel before this stops being a click. */
const DRAG_THRESHOLD_PX = 4;

/** How long a task gets when it is dragged onto the day. */
export const BLOCK_MINUTES = 60;

export interface DragDraft {
    kind: 'move' | 'resize' | 'create' | 'block';
    dateStr: string;
    startMinute: number;
    endMinute: number;
    event: EventMetadata | null;
    /** What to show while dragging something that is not an event yet. */
    label?: string;
}

interface Options {
    /** The element the day columns fill, edge to edge. */
    gridEl: Ref<HTMLElement | null>;
    /** One date per column, left to right. */
    days: Ref<string[]>;
    /** A click, not a drag. */
    onOpen: (event: EventMetadata, dateStr: string) => void;
    onCreate: (dateStr: string, startMinute: number, endMinute: number) => void;
    onReschedule: (event: EventMetadata, dateStr: string, startAt: string, endAt: string) => void;
    /** A task dropped onto the day, to be given a time. */
    onBlock: (task: { id: string; title: string }, dateStr: string, startAt: string, endAt: string) => void;
}

const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));
/**
 * Midnight belongs to the next day.
 *
 * `clockOf` clamps to 23:59 because it labels a position inside a day; an
 * event dragged to the bottom of the grid ends *at* midnight, and writing
 * 23:59 would quietly shave a minute off it every time.
 */
const stamp = (dateStr: string, minute: number) =>
    minute >= MINUTES_PER_DAY
        ? `${shiftDateString(dateStr, 1)}T00:00`
        : `${dateStr}T${clockOf(minute)}`;

/**
 * Moving, resizing and drawing events directly on the time grid.
 *
 * Owned by the grid rather than by a column, because dragging a meeting from
 * Tuesday to Thursday crosses columns — the day is read from where the pointer
 * is, not from where the drag began.
 *
 * Only a mouse or a pen drags. On a touch screen the same gesture is how the
 * grid is scrolled, and taking that over to move an appointment trades a
 * thing people do constantly for a thing they do occasionally; a tap opens the
 * editor instead.
 */
export function useTimeGridDrag(opts: Options) {
    const draft = ref<DragDraft | null>(null);
    const isDragging = computed(() => draft.value !== null);

    let origin: { x: number; y: number } | null = null;
    let moved = false;
    let pending:
        | { kind: 'move'; event: EventMetadata; dateStr: string; startMinute: number; endMinute: number; grabMinute: number }
        | { kind: 'resize'; event: EventMetadata; dateStr: string; startMinute: number }
        | { kind: 'create'; dateStr: string; originMinute: number }
        | { kind: 'block'; task: { id: string; title: string }; dateStr: string }
        | null = null;

    const snap = (minute: number) => Math.round(minute / SNAP_MINUTES) * SNAP_MINUTES;

    const readPointer = (e: PointerEvent) => {
        const el = opts.gridEl.value;
        if (!el) return null;
        const rect = el.getBoundingClientRect();
        if (rect.height <= 0 || rect.width <= 0) return null;
        const minute = clamp(((e.clientY - rect.top) / rect.height) * MINUTES_PER_DAY, 0, MINUTES_PER_DAY);
        const columns = opts.days.value.length || 1;
        const index = clamp(Math.floor(((e.clientX - rect.left) / rect.width) * columns), 0, columns - 1);
        return { minute, dateStr: opts.days.value[index] ?? opts.days.value[0] };
    };

    const teardown = () => {
        window.removeEventListener('pointermove', onPointerMove);
        window.removeEventListener('pointerup', onPointerUp);
        window.removeEventListener('pointercancel', onCancel);
        origin = null;
        pending = null;
        moved = false;
        draft.value = null;
    };

    function onPointerMove(e: PointerEvent) {
        if (!pending || !origin) return;
        if (!moved) {
            if (Math.abs(e.clientX - origin.x) < DRAG_THRESHOLD_PX
                && Math.abs(e.clientY - origin.y) < DRAG_THRESHOLD_PX) return;
            moved = true;
        }
        const at = readPointer(e);
        if (!at) return;

        if (pending.kind === 'move') {
            const duration = pending.endMinute - pending.startMinute;
            const delta = snap(at.minute - pending.grabMinute);
            const start = clamp(pending.startMinute + delta, 0, MINUTES_PER_DAY - duration);
            draft.value = {
                kind: 'move', dateStr: at.dateStr, event: pending.event,
                startMinute: start, endMinute: start + duration,
            };
        } else if (pending.kind === 'resize') {
            const end = clamp(snap(at.minute), pending.startMinute + MIN_BLOCK_MINUTES, MINUTES_PER_DAY);
            draft.value = {
                kind: 'resize', dateStr: pending.dateStr, event: pending.event,
                startMinute: pending.startMinute, endMinute: end,
            };
        } else if (pending.kind === 'block') {
            const start = clamp(snap(at.minute), 0, MINUTES_PER_DAY - BLOCK_MINUTES);
            draft.value = {
                kind: 'block', dateStr: at.dateStr, event: null,
                startMinute: start, endMinute: start + BLOCK_MINUTES,
                label: pending.task.title,
            };
        } else {
            // A drag upward is still a range, just drawn from the other end.
            const a = snap(pending.originMinute);
            const b = snap(at.minute);
            const start = clamp(Math.min(a, b), 0, MINUTES_PER_DAY - MIN_BLOCK_MINUTES);
            const end = clamp(Math.max(Math.max(a, b), start + MIN_BLOCK_MINUTES), 0, MINUTES_PER_DAY);
            draft.value = { kind: 'create', dateStr: pending.dateStr, event: null, startMinute: start, endMinute: end };
        }
    }

    function onPointerUp() {
        const current = draft.value;
        const started = pending;
        const wasDrag = moved;
        teardown();
        if (!started) return;

        if (!wasDrag) {
            // Below the threshold this was a click. A click on a block opens
            // it; a click on empty grid is handled by the view, not here.
            if (started.kind === 'move') opts.onOpen(started.event, started.dateStr);
            return;
        }
        if (!current) return;

        if (current.kind === 'create') {
            opts.onCreate(current.dateStr, current.startMinute, current.endMinute);
            return;
        }
        if (current.kind === 'block') {
            if (started.kind !== 'block') return;
            opts.onBlock(
                started.task,
                current.dateStr,
                stamp(current.dateStr, current.startMinute),
                stamp(current.dateStr, current.endMinute),
            );
            return;
        }
        if (!current.event || started.kind === 'create' || started.kind === 'block') return;

        // A drag that snapped back to where it started is not an edit, and
        // writing it would send a series through the this/following/all
        // question for no reason.
        const movedNowhere = current.dateStr === started.dateStr
            && current.startMinute === started.startMinute
            && (started.kind === 'move' ? current.endMinute === started.endMinute : false);
        if (movedNowhere) return;

        opts.onReschedule(
            current.event,
            started.dateStr,
            stamp(current.dateStr, current.startMinute),
            stamp(current.dateStr, current.endMinute),
        );
    }

    function onCancel() { teardown(); }

    const begin = (e: PointerEvent) => {
        // See the note above: touch scrolls, it does not drag.
        if (e.pointerType === 'touch') return false;
        if (e.button !== 0) return false;
        origin = { x: e.clientX, y: e.clientY };
        moved = false;
        window.addEventListener('pointermove', onPointerMove);
        window.addEventListener('pointerup', onPointerUp);
        window.addEventListener('pointercancel', onCancel);
        return true;
    };

    const startMove = (e: PointerEvent, event: EventMetadata, dateStr: string, startMinute: number, endMinute: number) => {
        const at = readPointer(e);
        if (!begin(e)) return;
        e.preventDefault();
        pending = {
            kind: 'move', event, dateStr, startMinute, endMinute,
            grabMinute: at ? at.minute : startMinute,
        };
    };

    const startResize = (e: PointerEvent, event: EventMetadata, dateStr: string, startMinute: number) => {
        if (!begin(e)) return;
        e.preventDefault();
        e.stopPropagation();
        pending = { kind: 'resize', event, dateStr, startMinute };
    };

    /**
     * Give a task a time by dragging it into the day.
     *
     * The gesture is the whole feature: a task sitting in the all-day row is
     * something to do, and pulling it down onto an hour is deciding when. The
     * task is not moved or changed — an event is created that points back at
     * it, so ticking either one still means the same thing.
     */
    const startBlock = (e: PointerEvent, task: { id: string; title: string }, dateStr: string) => {
        if (!begin(e)) return;
        e.preventDefault();
        pending = { kind: 'block', task, dateStr };
    };

    const startCreate = (e: PointerEvent, dateStr: string) => {
        const at = readPointer(e);
        if (!at || !begin(e)) return;
        e.preventDefault();
        pending = { kind: 'create', dateStr, originMinute: at.minute };
    };

    // Guarded: this is also driven directly by its own tests, where there is
    // no component to hang a lifecycle hook on.
    if (getCurrentInstance()) onUnmounted(teardown);

    return { draft, isDragging, startMove, startResize, startCreate, startBlock };
}
