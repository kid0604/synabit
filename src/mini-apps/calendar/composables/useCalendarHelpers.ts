import type { Ref } from 'vue';
import type { TaskMetadata, EventMetadata } from '../types';
import { formatDateString } from '../helpers';

/**
 * Reading the days the vault has already answered for.
 *
 * There is no recurrence logic here any more, and there must not be. Rust
 * decides which days a series lands on — `src-tauri/src/calendar/recurrence.rs`
 * — and hands back a day-keyed map. A second opinion in this file is exactly
 * what let the grid draw an appointment the reminder loop never fired for.
 */
export function useCalendarHelpers(
    tasksByDate: Ref<Map<string, TaskMetadata[]>>,
    eventsByDate: Ref<Map<string, EventMetadata[]>>,
) {
    const getTasksForDate = (dateStr: string): TaskMetadata[] =>
        tasksByDate.value.get(dateStr) ?? [];

    const getEventsForDate = (dateStr: string): EventMetadata[] =>
        eventsByDate.value.get(dateStr) ?? [];

    const getMonthViewItems = (dateStr: string) => {
        const events = getEventsForDate(dateStr).map(e => {
            const timePart = (e.start_at && e.start_at.includes('T')) ? e.start_at.split('T')[1].substring(0, 5) : '';
            return { id: e.id, type: 'event' as const, title: e.title, event_time: timePart, status: '', event: e };
        });
        const tasks = getTasksForDate(dateStr).map(t => ({ id: t.id, type: 'task' as const, title: t.title, event_time: '', status: t.status }));
        const all = [...events, ...tasks];
        return {
            display: all.slice(0, 3),
            moreCount: all.length > 3 ? all.length - 3 : 0
        };
    };

    const hasItemsOnDate = (date: Date) => {
        const ds = formatDateString(date);
        return getTasksForDate(ds).length > 0 || getEventsForDate(ds).length > 0;
    };

    const getSortedEventsForDate = (dateStr: string) =>
        [...getEventsForDate(dateStr)].sort((a, b) => (a.start_at || '').localeCompare(b.start_at || ''));

    return {
        getTasksForDate, getEventsForDate,
        getMonthViewItems, hasItemsOnDate, getSortedEventsForDate,
    };
}
