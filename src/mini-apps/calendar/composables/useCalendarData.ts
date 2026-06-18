import { ref, watch, onMounted } from 'vue';
import type { Ref } from 'vue';
import type { TaskMetadata, EventMetadata } from '../types';
import { logger } from '../../../utils/logger';

export function useCalendarData(ns: any, bus: any, vaultPath: Ref<string>) {
    const allTasks = ref<TaskMetadata[]>([]);
    const allEvents = ref<EventMetadata[]>([]);

    const mapNodeToTask = (node: any): TaskMetadata => {
        const rawTags = node.properties?.tags;
        const tagsArray = Array.isArray(rawTags) ? rawTags : (typeof rawTags === 'string' && rawTags.trim() !== '' ? [rawTags] : []);

        return {
            id: node.id,
            path: node.id,
            title: node.title,
            content: node.content,
            created_at: node.created_at,
            updated_at: node.updated_at,
            status: node.properties?.status || 'todo',
            start_date: node.properties?.start_date || '',
            due_date: node.properties?.due_date || '',
            comment: node.properties?.comment || '',
            source_link: node.properties?.source_link || '',
            tags: tagsArray,
            custom_fields: node.properties || {}
        };
    };

    const loadData = async () => {
        if (!vaultPath.value) return;
        try {
            const rawTasks: any[] = await ns.getNodes('task');
            allTasks.value = rawTasks.map(mapNodeToTask);
        } catch(e) { logger.error("Error loading tasks:", e); }
        try {
            const rawEvents: any[] = await ns.getNodes('event');
            allEvents.value = rawEvents.map(n => {
                const props = n.properties || {};
                
                // Migration logic
                let isAllDay = props.is_all_day === true;
                let startAt = props.start_at || '';
                let endAt = props.end_at || '';
                
                // Fallback for legacy data (event_date + start_time/end_time)
                if (!startAt && props.event_date) {
                    const sTime = props.start_time || props.event_time;
                    if (sTime) {
                        startAt = `${props.event_date}T${sTime}:00`;
                        isAllDay = false;
                    } else {
                        startAt = props.event_date;
                        isAllDay = true;
                    }
                }
                if (!endAt && props.event_date && props.end_time) {
                    endAt = `${props.event_date}T${props.end_time}:00`;
                }
                if (!endAt && isAllDay) {
                     endAt = startAt; // all day event ends on same day
                }
                
                return {
                    id: n.id,
                    title: n.title,
                    is_all_day: isAllDay,
                    start_at: startAt,
                    end_at: endAt,
                    timezone: props.timezone || '',
                    location: props.location || '',
                    tags: props.tags || [],
                    content: n.content,
                    path: n.id,
                    created_at: n.created_at || '',
                    relations: props.relations || props.related_notes || [],
                    recurrence: props.recurrence || 'none',
                    recurrence_end_at: props.recurrence_end_at || '',
                    exceptions: props.exceptions || [],
                    series_id: props.series_id || '',
                    reminders: props.reminders || []
                };
            });
        } catch(e) { logger.error("Error loading events:", e); }
    };

    const toggleTaskStatus = async (partialTask: { id: string, status: string }) => {
        const task = allTasks.value.find(t => t.id === partialTask.id);
        if (!task) return;
        const newStatus = task.status === 'done' ? 'todo' : 'done';
        const nowStr = new Date().toISOString().split('T')[0];
        const newCompletedAt = newStatus === 'done' ? nowStr : '';
        
        // Optimistic UI update
        task.status = newStatus;
        
        try {
            const properties = {
                ...(task.custom_fields || {}),
                status: newStatus,
                start_date: task.start_date,
                due_date: task.due_date,
                comment: task.comment,
                source_link: task.source_link,
                tags: task.tags,
                completed_at: newCompletedAt
            };
            await ns.writeNode({
                relPath: task.path,
                nodeType: 'task',
                title: task.title,
                properties: properties,
                content: task.content,
                existingPath: task.path
            });
            
            // Reload to ensure consistency
            await loadData();
        } catch (error) {
            console.error("Failed to update task status", error);
            // Revert UI update
            task.status = task.status === 'done' ? 'todo' : 'done';
        }
    };

    // Debounce wrapper: coalesces rapid-fire events (e.g. node:updated + vault:file-modified)
    let _debounceTimer: ReturnType<typeof setTimeout> | null = null;
    const debouncedLoad = (fn: () => void, ms = 300) => {
        if (_debounceTimer) clearTimeout(_debounceTimer);
        _debounceTimer = setTimeout(fn, ms);
    };

    onMounted(() => {
        loadData();

        bus.on('vault:file-modified', () => {
            debouncedLoad(() => loadData());
        });

        bus.on('vault:file-created-deleted', () => {
            debouncedLoad(() => loadData());
        });

        bus.on('vault:sync-completed', () => {
            debouncedLoad(() => loadData());
        });

        bus.on('task:status-changed', () => {
            debouncedLoad(() => loadData());
        });

        // Cross-app: refresh when events are created from other apps (e.g., People birthday sync)
        bus.on('node:created', ({ nodeType }: { nodeType: string }) => {
            if (nodeType === 'event' || nodeType === 'task') debouncedLoad(() => loadData());
        });

        bus.on('node:deleted', ({ nodeType }: { nodeType: string }) => {
            if (nodeType === 'event' || nodeType === 'task') debouncedLoad(() => loadData());
        });
    });
    watch(vaultPath, () => { loadData(); });

    return { allTasks, allEvents, loadData, toggleTaskStatus };
}
