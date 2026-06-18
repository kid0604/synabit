import type { Ref } from 'vue';
import type { TaskMetadata, EventMetadata } from '../types';
import { formatDateString } from '../helpers';

export function useCalendarHelpers(
    allTasks: Ref<TaskMetadata[]>,
    allEvents: Ref<EventMetadata[]>,
) {
    const getTasksForDate = (dateStr: string) => allTasks.value.filter(t => t.due_date === dateStr || t.start_date === dateStr);

    const getEventsForDate = (dateStr: string) => {
        return allEvents.value.filter(e => {
            if (!e.start_at) return false;
            const eStartStr = e.start_at.split('T')[0];
            const eEndStr = e.end_at ? e.end_at.split('T')[0] : eStartStr;
            
            if (e.exceptions && e.exceptions.includes(dateStr)) return false;
            
            if (!e.recurrence || e.recurrence === 'none') {
                return dateStr >= eStartStr && dateStr <= eEndStr;
            }

            if (dateStr < eStartStr) return false;
            if (e.recurrence_end_at && dateStr > e.recurrence_end_at) return false;

            const startObj = new Date(eStartStr + 'T00:00:00');
            const endObj = new Date(eEndStr + 'T00:00:00');
            const durationDays = Math.round((endObj.getTime() - startObj.getTime()) / 86400000);
            const targetObj = new Date(dateStr + 'T00:00:00');

            if (e.recurrence === 'daily') {
                return true;
            } else if (e.recurrence === 'weekly') {
                const diffDays = Math.round((targetObj.getTime() - startObj.getTime()) / 86400000);
                const rem = diffDays % 7;
                const posRem = (rem + 7) % 7;
                return posRem >= 0 && posRem <= durationDays;
            } else if (e.recurrence === 'monthly') {
                let cur = new Date(targetObj.getFullYear(), targetObj.getMonth(), startObj.getDate());
                if (cur.getMonth() !== targetObj.getMonth()) {
                    cur = new Date(targetObj.getFullYear(), targetObj.getMonth() + 1, 0); 
                }
                const diffDays = Math.round((targetObj.getTime() - cur.getTime()) / 86400000);
                return diffDays >= 0 && diffDays <= durationDays;
            } else if (e.recurrence === 'yearly') {
                let cur = new Date(targetObj.getFullYear(), startObj.getMonth(), startObj.getDate());
                if (startObj.getMonth() === 1 && startObj.getDate() === 29 && cur.getMonth() !== 1) {
                    cur = new Date(targetObj.getFullYear(), 2, 0);
                }
                const diffDays = Math.round((targetObj.getTime() - cur.getTime()) / 86400000);
                return diffDays >= 0 && diffDays <= durationDays;
            }
            return false;
        });
    };

    const getEventsForDateAndHour = (dateStr: string, hour: number) => {
        return getEventsForDate(dateStr).filter(e => {
            if (e.is_all_day) return false;
            if (!e.start_at) return false;
            
            const eStartStr = e.start_at.split('T')[0];
            const eEndStr = e.end_at ? e.end_at.split('T')[0] : eStartStr;
            if (eStartStr !== eEndStr) return false; // Multi-day events go to "All Day"
            
            const timePart = e.start_at.split('T')[1];
            if (!timePart) return false;
            const eHour = parseInt(timePart.split(':')[0]);
            return eHour === hour;
        });
    };

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

    return { getTasksForDate, getEventsForDate, getEventsForDateAndHour, getMonthViewItems, hasItemsOnDate };
}
