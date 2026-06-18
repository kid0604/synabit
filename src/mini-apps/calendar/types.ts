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
    recurrence?: string;
    recurrence_end_at?: string;
    exceptions?: string[];
    series_id?: string;
    reminders?: string[];
}

export type ViewMode = 'day' | 'week' | 'month' | 'year';

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
    recurrence: string;
    recurrence_end_at: string;
    series_id: string;
    exceptions: string[];
    reminders: string[];
    _editScope: 'occurrence_view' | 'this' | 'following' | 'all';
    _originalEvent: EventMetadata | null;
}
