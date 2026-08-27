import { describe, it, expect, vi, beforeEach } from 'vitest';
import { computed } from 'vue';
import { useEventForm } from '../composables/useEventForm';
import type { EventMetadata } from '../types';

vi.mock('@tauri-apps/plugin-dialog', () => ({ ask: vi.fn(async () => true) }));

const event = (over: Partial<EventMetadata>): EventMetadata => ({
    id: 'Events/a.md', title: 'Standup', is_all_day: false,
    start_at: '2026-03-02T09:00', end_at: '2026-03-02T09:15',
    location: '', tags: [], content: '', path: 'Events/a.md', created_at: '',
    relations: [], recurrence: 'none', recurrence_end_at: '', exceptions: [],
    series_id: '', reminders: [],
    ...over,
});

/** `Array.prototype.at` is newer than this project's `lib` target. */
const last = <T>(xs: T[]): T => xs[xs.length - 1];

function harness(events: EventMetadata[] = []) {
    const writes: any[] = [];
    const deletes: any[] = [];
    const ns = {
        writeNode: vi.fn(async (p: any) => { writes.push(p); }),
        deleteNode: vi.fn(async (p: any) => { deletes.push(p); }),
        // The series is fetched, not filtered out of what is on screen: a
        // split-off occurrence can fall outside the days being shown.
        getEventSeries: vi.fn(async (rootId: string) =>
            events.filter(e => e.id === rootId || e.series_id === rootId)),
        getNode: vi.fn(async (id: string) => events.find(e => e.id === id) ?? null),
    };
    const form = useEventForm(
        ns,
        computed(() => '2026-03-02'),
        async () => {}, async () => {}, () => {}, () => {},
    );
    return { form, ns, writes, deletes };
}

describe('reminders', () => {
    let h: ReturnType<typeof harness>;
    beforeEach(() => { h = harness(); h.form.openAddEventModal(); });

    /**
     * The regression. The select fires `update:reminderPreset` and
     * `add-reminder` in the same tick, so choosing "Custom…" ran `addReminder`
     * while the text box was still empty. It cleared the preset, the box's
     * `v-if` never became true, and custom reminders were unreachable.
     */
    it('choosing Custom keeps the box open instead of closing it', () => {
        h.form.reminderPreset.value = 'custom';
        h.form.addReminder();
        expect(h.form.reminderPreset.value).toBe('custom');
        expect(h.form.eventForm.value.reminders).toEqual([]);
        expect(h.form.reminderError.value).toBe('');
    });

    it('accepts a custom offset once one has been typed', () => {
        h.form.reminderPreset.value = 'custom';
        h.form.customReminder.value = '45m';
        h.form.addReminder();
        expect(h.form.eventForm.value.reminders).toEqual(['45m']);
        expect(h.form.reminderPreset.value).toBe('');
        expect(h.form.customReminder.value).toBe('');
    });

    it('explains a malformed offset in the form rather than an alert', () => {
        h.form.reminderPreset.value = 'custom';
        h.form.customReminder.value = 'soon';
        h.form.addReminder();
        expect(h.form.eventForm.value.reminders).toEqual([]);
        expect(h.form.reminderError.value).not.toBe('');
        expect(h.form.reminderPreset.value).toBe('custom'); // still editable
    });

    it('adds a preset straight away and does not duplicate it', () => {
        h.form.reminderPreset.value = '15m';
        h.form.addReminder();
        h.form.reminderPreset.value = '15m';
        h.form.addReminder();
        expect(h.form.eventForm.value.reminders).toEqual(['15m']);
    });
});

describe('validation', () => {
    it('refuses an event that ends before it starts, and says so', async () => {
        const h = harness();
        h.form.openAddEventModal();
        h.form.eventForm.value.title = 'Retro';
        h.form.eventForm.value.start_at = '2026-03-02T15:00';
        h.form.eventForm.value.end_at = '2026-03-02T14:00';

        expect(h.form.formError.value).not.toBe('');
        await h.form.submitEvent();
        expect(h.ns.writeNode).not.toHaveBeenCalled();
        expect(h.form.showErrors.value).toBe(true);
        expect(h.form.showEventForm.value).toBe(true);
    });

    it('refuses a repeat that ends before the event starts', () => {
        const h = harness();
        h.form.openAddEventModal();
        h.form.eventForm.value.title = 'Standup';
        h.form.eventForm.value.start_at = '2026-03-02T09:00';
        h.form.eventForm.value.end_at = '2026-03-02T09:15';
        h.form.eventForm.value.recurrence = {
            freq: 'weekly', interval: 1, byDay: ['MO'],
            endMode: 'until', until: '2026-02-01', count: 10,
        };
        expect(h.form.formError.value).not.toBe('');
    });

    /**
     * `FREQ=WEEKLY` with no `BYDAY` is legal — it means "weekly on the day it
     * starts" — and is what nearly every calendar exports. Refusing it made
     * every imported weekly event impossible to edit or drag.
     */
    it('accepts a weekly rule that names no weekday, because that is a real rule', async () => {
        const h = harness();
        h.form.openAddEventModal();
        h.form.eventForm.value.title = 'Standup';
        h.form.eventForm.value.recurrence = {
            freq: 'weekly', interval: 1, byDay: [],
            endMode: 'never', until: '', count: 10,
        };
        expect(h.form.formError.value).toBe('');
        await h.form.submitEvent();
        expect(last(h.writes).properties.rrule).toBe('FREQ=WEEKLY');
    });

    it('lets a well formed event through', async () => {
        const h = harness();
        h.form.openAddEventModal();
        h.form.eventForm.value.title = 'Standup';
        expect(h.form.formError.value).toBe('');
        await h.form.submitEvent();
        expect(h.ns.writeNode).toHaveBeenCalledOnce();
    });
});

describe('editing a whole series', () => {
    /**
     * The data-loss bug. `exceptions` records both "the user cancelled this
     * occurrence" and "a split child covers this occurrence", and the merge
     * back used to clear the array wholesale — so cancelling one standup and
     * then editing the series brought the cancelled one back.
     */
    it('keeps an occurrence the user cancelled', async () => {
        const root = event({
            recurrence: 'weekly', start_at: '2026-03-02T09:00', end_at: '2026-03-02T09:15',
            exceptions: ['2026-03-16'],
        });
        const h = harness([root]);
        await h.form.openEditEventModal(root, '2026-03-09');
        h.form.eventForm.value.title = 'Standup (30m)';
        await h.form.submitEvent();          // opens the scope modal
        h.form.scopeSelection.value = 'all';
        await h.form.confirmScopeAction();

        const write = last(h.writes);
        expect(write.properties.exceptions).toEqual(['2026-03-16']);
    });

    /** A date a split child owned is free to come back when that child goes. */
    it('drops only the exception the deleted child was covering', async () => {
        const root = event({
            recurrence: 'weekly', start_at: '2026-03-02T09:00', end_at: '2026-03-02T09:15',
            exceptions: ['2026-03-16', '2026-03-23'],
        });
        const child = event({
            id: 'Events/b.md', path: 'Events/b.md', series_id: 'Events/a.md',
            recurrence: 'none', start_at: '2026-03-23T11:00', end_at: '2026-03-23T11:30',
        });
        const h = harness([root, child]);
        await h.form.openEditEventModal(root, '2026-03-09');
        await h.form.submitEvent();
        h.form.scopeSelection.value = 'all';
        await h.form.confirmScopeAction();

        expect(h.deletes.map(d => d.relPath)).toEqual(['Events/b.md']);
        expect(last(h.writes).properties.exceptions).toEqual(['2026-03-16']);
    });
});

describe('taking a drag back', () => {
    const meeting = () => event({
        id: 'Events/m.md', path: 'Events/m.md',
        start_at: '2026-03-10T09:00', end_at: '2026-03-10T10:00',
    });

    it('remembers where a plain event came from, and puts it back there', async () => {
        const ev = meeting();
        const h = harness([ev]);
        await h.form.rescheduleEvent(ev, '2026-03-10', '2026-03-10T14:00', '2026-03-10T15:00');

        expect(last(h.writes).properties.start_at).toBe('2026-03-10T14:00');
        expect(h.form.lastMove.value).not.toBeNull();

        expect(await h.form.undoMove()).toBe(true);
        const back = last(h.writes).properties;
        expect(back.start_at).toBe('2026-03-10T09:00');
        expect(back.end_at).toBe('2026-03-10T10:00');
    });

    /**
     * A series goes through the this/following/all question and may have been
     * split into a new node on the way. Offering an undo that quietly does
     * something else is worse than not offering one.
     */
    it('offers nothing to undo after moving one occurrence of a series', async () => {
        const series = event({
            id: 'Events/s.md', path: 'Events/s.md', rrule: 'FREQ=WEEKLY',
            start_at: '2026-03-02T09:00', end_at: '2026-03-02T09:15',
        });
        const h = harness([series]);
        await h.form.rescheduleEvent(series, '2026-03-09', '2026-03-09T11:00', '2026-03-09T11:15');

        expect(h.form.showScopeModal.value).toBe(true);
        expect(h.form.lastMove.value).toBeNull();
    });

    it('can only be taken back once', async () => {
        const ev = meeting();
        const h = harness([ev]);
        await h.form.rescheduleEvent(ev, '2026-03-10', '2026-03-10T14:00', '2026-03-10T15:00');
        expect(await h.form.undoMove()).toBe(true);
        expect(await h.form.undoMove()).toBe(false);
    });

    /** Somebody else's calendar was never moved, so there is nothing to put back. */
    it('has nothing to undo for a subscribed event', async () => {
        const theirs = event({ id: 'subscription:s1/x', subscription_id: 's1' });
        const h = harness([theirs]);
        await h.form.rescheduleEvent(theirs, '2026-03-10', '2026-03-10T14:00', '2026-03-10T15:00');
        expect(h.writes).toHaveLength(0);
        expect(h.form.lastMove.value).toBeNull();
    });
});

describe('trimming a series from a given day', () => {
    /**
     * `toISOString()` on a local midnight reads as the previous day anywhere
     * east of Greenwich, so "delete this and following" set the end one day
     * early and took an extra occurrence with it.
     */
    it('ends the parent on the day before, in local time', async () => {
        const root = event({ recurrence: 'weekly', start_at: '2026-03-02T09:00', end_at: '2026-03-02T09:15' });
        const h = harness([root]);
        await h.form.deleteEvent(root, '2026-03-23');
        h.form.scopeSelection.value = 'following';
        await h.form.confirmScopeAction();

        const written = last(h.writes).properties;
        expect(written.rrule).toBe('FREQ=WEEKLY;UNTIL=20260322');
        // The legacy pair is removed on write, so nothing is left to disagree.
        expect(written.recurrence).toBeNull();
        expect(written.recurrence_end_at).toBeNull();
    });
});
