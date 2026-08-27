import type { RecurrenceFields } from './rrule';

export interface TaskMetadata {
    id: string;
    title: string;
    status: string;
    start_date: string;
    due_date: string;
    comment: string;
    source_link: string;
    tags: string[];
    content: string;
    path: string;
    created_at: string;
    updated_at: string;
    custom_fields: any;
}

export interface EventMetadata {
    id: string;
    title: string;
    is_all_day: boolean;
    start_at: string;
    end_at: string;
    timezone?: string;
    location: string;
    tags: string[];
    content: string;
    path: string;
    created_at: string;
    relations?: string[];
    /** The zone the stored clock belongs to; empty means floating. */
    tzid?: string;
    /**
     * The subscribed calendar this came from, or empty for the user's own.
     *
     * An event with one of these is a cache of somebody else's feed: the next
     * refresh replaces it, and nothing may offer to change it.
     */
    subscription_id?: string;
    /** A colour name from `EVENT_COLOURS`; empty means the default blue. */
    colour?: string;
    /** RFC 5545 rule. Authoritative when present; see `rrule.ts` `ruleOf`. */
    rrule?: string;
    /** What vaults written before `rrule` stored. Read only as a fallback. */
    recurrence?: string;
    recurrence_end_at?: string;
    exceptions?: string[];
    series_id?: string;
    reminders?: string[];
}

/**
 * `agenda` is the only one that is not a grid: it answers "where was that
 * meeting" rather than "what does this week look like", and those turn out to
 * be different questions with different shapes.
 */
export type ViewMode = 'day' | 'week' | 'month' | 'year' | 'agenda';

export interface EventFormData {
    isEdit: boolean;
    id: string;
    path: string;
    title: string;
    isAllDay: boolean;
    start_at: string;
    end_at: string;
    location: string;
    description: string;
    tagsStr: string;
    relations: string[];
    /** The zone the times in this form are written in; empty means floating. */
    tzid: string;
    /** A colour name from `EVENT_COLOURS`; empty means the default blue. */
    colour: string;
    /** The editor's fields; serialised to an `rrule` string on write. */
    recurrence: RecurrenceFields;
    series_id: string;
    exceptions: string[];
    reminders: string[];
    _editScope: 'occurrence_view' | 'this' | 'following' | 'all';
    _originalEvent: EventMetadata | null;
}
