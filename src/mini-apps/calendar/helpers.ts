import type { EventMetadata, EventFormData } from './types';

export const monthNames = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
export const monthNamesShort = monthNames.map(m => m.substring(0, 3));
export const dayNamesShort = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
export const hours = Array.from({length: 24}, (_, i) => i);
export const hourOptions = Array.from({length: 24}, (_, i) => i.toString().padStart(2, '0'));
export const minuteOptions = ['00', '05', '10', '15', '20', '25', '30', '35', '40', '45', '50', '55'];

export const formatDateString = (date: Date) => {
    const y = date.getFullYear();
    const m = String(date.getMonth() + 1).padStart(2, '0');
    const d = String(date.getDate()).padStart(2, '0');
    return `${y}-${m}-${d}`;
};

export const isSameDay = (d1: Date, d2: Date) => {
    return d1.getFullYear() === d2.getFullYear() && d1.getMonth() === d2.getMonth() && d1.getDate() === d2.getDate();
};

export const isAllDayOrMultiDay = (e: EventMetadata) => {
    if (e.is_all_day) return true;
    if (!e.start_at) return false;
    const s = e.start_at.split('T')[0];
    const en = e.end_at ? e.end_at.split('T')[0] : s;
    return s !== en;
};

export const formatEventTime = (ev: EventMetadata) => {
    if (ev.is_all_day) return '';
    if (!ev.start_at || !ev.start_at.includes('T')) return '';
    const start = ev.start_at.split('T')[1].substring(0, 5);
    if (ev.end_at && ev.end_at.includes('T')) {
        const end = ev.end_at.split('T')[1].substring(0, 5);
        if (start === end) return start;
        return `${start} - ${end}`;
    }
    return start;
};

// NEW: Extract duplicated AM/PM format pattern from L1143, L1193
export const formatHourAMPM = (hr: number): string => {
    return hr === 0 ? '12 AM' : hr < 12 ? hr + ' AM' : hr === 12 ? '12 PM' : (hr - 12) + ' PM';
};

// NEW: Extract duplicated tag parsing from L361-362, L649-650, L800-801
export const parseTags = (tagsStr: string): string[] => {
    if (!tagsStr.trim()) return [];
    return tagsStr.split(',').map(s => s.trim().replace(/^#/, '')).filter(s => s);
};

// NEW: Build event payload for ns.writeNode(), eliminates 3x duplication
export function buildEventPayload(
    form: EventFormData,
    overrides?: Partial<{
        relPath: string;
        relations: string[];
        eventType: string;
        silent: boolean;
    }>
) {
    const tags = parseTags(form.tagsStr);
    return {
        relPath: overrides?.relPath || form.path || `Events/${form.title}`,
        title: form.title,
        nodeType: 'event' as const,
        properties: {
            is_all_day: form.isAllDay,
            start_at: form.start_at,
            end_at: form.end_at,
            location: form.location,
            tags,
            relations: overrides?.relations ?? form.relations ?? [],
            recurrence: form.recurrence,
            recurrence_end_at: form.recurrence_end_at,
            series_id: form.series_id,
            exceptions: form.exceptions,
            reminders: form.reminders,
        },
        content: form.description,
        ...(overrides?.eventType ? { eventType: overrides.eventType } : {}),
        ...(overrides?.silent ? { silent: true } : {}),
    };
}
