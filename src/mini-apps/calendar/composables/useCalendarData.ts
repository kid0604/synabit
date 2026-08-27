import { ref, computed, watch, onMounted, onUnmounted } from 'vue';
import type { Ref } from 'vue';
import type { TaskMetadata, EventMetadata } from '../types';
import type { EventsInRange } from '../../../types/ipc';
import { indexOccurrencesByDate } from '../helpers';
import { logger } from '../../../utils/logger';
import { getTodayStr, taskProperties } from '../../task/types';

export function useCalendarData(
    ns: any,
    bus: any,
    vaultPath: Ref<string>,
    visibleRange: Ref<{ from: string; to: string }>,
) {
    /**
     * The tasks on screen, keyed by the day they land on.
     *
     * The other half of what was done for events. This used to hold every
     * task in the vault and run a linear filter for each day cell — forty-two
     * of them for a month, three hundred and sixty-five for a year, on every
     * render. Now the days on screen are what is asked for, and looking one
     * up is a map read.
     */
    const tasksByDate = ref<Map<string, TaskMetadata[]>>(new Map());
    /** Flat, for the few things that need to find one by id. */
    const allTasks = computed(() => {
        const out: TaskMetadata[] = [];
        tasksByDate.value.forEach(list => {
            for (const t of list) if (!out.some(x => x.id === t.id)) out.push(t);
        });
        return out;
    });

    /**
     * The events on screen, keyed by the day they land on.
     *
     * Which days a series lands on is decided once, in Rust, and arrives
     * already expanded. What used to happen here — hold every event in the
     * vault and re-run the recurrence rule for every day cell — is gone, along
     * with the second copy of that rule.
     */
    const eventsByDate = ref<Map<string, EventMetadata[]>>(new Map());

    const mapNodeToTask = (node: any): TaskMetadata => {
        const rawTags = node.properties?.tags;
        const tagsArray = Array.isArray(rawTags) ? rawTags : (typeof rawTags === 'string' && rawTags.trim() !== '' ? [rawTags] : []);

        return {
            id: node.id,
            path: node.id,
            title: node.title,
            content: node.content ?? '',
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

    const loadTasks = async () => {
        const { from, to } = visibleRange.value;
        if (!from || !to) return;
        try {
            const raw: any[] = await ns.getTasksInRange(from, to);
            const byDate = new Map<string, TaskMetadata[]>();
            for (const node of raw) {
                const task = mapNodeToTask(node);
                // A task can be pinned to both ends of a stretch of work, and
                // the calendar shows it at each.
                for (const day of [task.due_date, task.start_date]) {
                    if (!day || day < from || day > to) continue;
                    const list = byDate.get(day);
                    if (list) { if (!list.some(t => t.id === task.id)) list.push(task); }
                    else byDate.set(day, [task]);
                }
            }
            tasksByDate.value = byDate;
        } catch (e) { logger.error('Error loading tasks:', e); }
    };

    const loadEvents = async () => {
        const { from, to } = visibleRange.value;
        if (!from || !to) return;
        try {
            const range: EventsInRange = await ns.getEventsInRange(from, to);
            eventsByDate.value = indexOccurrencesByDate(range);
        } catch (e) { logger.error('Error loading events:', e); }
    };

    const loadData = async () => {
        if (!vaultPath.value) return;
        await Promise.all([loadTasks(), loadEvents()]);
    };

    const toggleTaskStatus = async (partialTask: { id: string, status: string }) => {
        const task = allTasks.value.find(t => t.id === partialTask.id);
        if (!task) return;
        const newStatus = task.status === 'done' ? 'todo' : 'done';
        // The local date. `toISOString` gives the UTC one, which in UTC+7 puts
        // a task ticked after midnight on the previous day and hides it from
        // the Today view that just showed it.
        const newCompletedAt = newStatus === 'done' ? getTodayStr() : '';

        // Optimistic UI update
        task.status = newStatus;

        try {
            await ns.writeNode({
                relPath: task.path,
                nodeType: 'task',
                // Shared with the Tasks app so the two cannot drift. The
                // Calendar's task carries no `priority` or `is_transferred`,
                // and `taskProperties` leaves a field it is not given at
                // whatever the file already says rather than blanking it.
                properties: taskProperties(task, { status: newStatus, completed_at: newCompletedAt }),
                title: task.title,
                // No body: this write changes a property. See `writeNode`.
            });

            await loadData();
        } catch (error) {
            logger.error('Failed to update task status', error);
            // Revert UI update
            task.status = task.status === 'done' ? 'todo' : 'done';
        }
    };

    // Debounce wrapper: coalesces rapid-fire events (e.g. node:updated + vault:file-modified)
    let _debounceTimer: ReturnType<typeof setTimeout> | null = null;
    const debouncedLoad = (ms = 300) => {
        if (_debounceTimer) clearTimeout(_debounceTimer);
        _debounceTimer = setTimeout(() => { loadData(); }, ms);
    };
    onUnmounted(() => { if (_debounceTimer) clearTimeout(_debounceTimer); });

    onMounted(() => {
        loadData();

        bus.on('vault:file-modified', () => debouncedLoad());
        bus.on('vault:file-created-deleted', () => debouncedLoad());
        bus.on('vault:sync-completed', () => debouncedLoad());
        bus.on('task:status-changed', () => debouncedLoad());

        // Cross-app: refresh when events are created from other apps (e.g., People birthday sync)
        bus.on('node:created', ({ nodeType }: { nodeType: string }) => {
            if (nodeType === 'event' || nodeType === 'task') debouncedLoad();
        });
        bus.on('node:deleted', ({ nodeType }: { nodeType: string }) => {
            if (nodeType === 'event' || nodeType === 'task') debouncedLoad();
        });
    });

    watch(vaultPath, () => { loadData(); });
    // Paging to another month is a different question for the vault, not a
    // different filter over an answer already held.
    watch(visibleRange, (next, prev) => {
        if (prev && next.from === prev.from && next.to === prev.to) return;
        loadEvents();
        loadTasks();
    });

    const eventCount = computed(() => {
        let n = 0;
        eventsByDate.value.forEach(list => { n += list.length; });
        return n;
    });

    return { allTasks, tasksByDate, eventsByDate, eventCount, loadData, loadEvents, toggleTaskStatus };
}
