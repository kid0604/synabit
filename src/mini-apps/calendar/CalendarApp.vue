<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue';
import { useEventBus } from '../../composables/useEventBus';
import { useNodeService } from '../../composables/useNodeService';
import { ChevronLeft, ChevronRight, Plus, X, Calendar as CalendarIcon, Clock, MapPin, Hash, CheckSquare, Trash2, FileText, Check, User, Link2, Bell } from 'lucide-vue-next';
import NavButtons from '../../shared/components/NavButtons.vue';

const bus = useEventBus();
const ns = useNodeService();

const props = defineProps<{ vaultPath: string }>();
const emit = defineEmits<{ (e: 'open-node', id: string, type: string): void }>();

// --- Data Models ---
interface TaskMetadata {
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

interface EventMetadata {
    id: string;
    title: string;
    is_all_day: boolean;
    start_at: string; // ISO 8601 or YYYY-MM-DD
    end_at: string; // ISO 8601 or YYYY-MM-DD
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

type ViewMode = 'day' | 'week' | 'month' | 'year';

// --- State ---
const viewMode = ref<ViewMode>('month');
const currentDate = ref(new Date());
const selectedDate = ref<Date>(new Date());
const allTasks = ref<TaskMetadata[]>([]);
const allEvents = ref<EventMetadata[]>([]);

const showRightPanel = ref(false);
const showEventForm = ref(false);

const eventForm = ref({
    isEdit: false,
    id: '',
    path: '',
    title: '',
    isAllDay: false,
    start_at: '',
    end_at: '',
    location: '',
    description: '',
    tagsStr: '',
    relations: [] as string[],
    recurrence: 'none',
    recurrence_end_at: '',
    series_id: '',
    exceptions: [] as string[],
    reminders: [] as string[],
    _editScope: 'all' as 'occurrence_view' | 'this' | 'following' | 'all',
    _originalEvent: null as EventMetadata | null
});

// --- Methods ---
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
    if (!props.vaultPath) return;
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
    bus.on('node:created', ({ nodeType }) => {
        if (nodeType === 'event' || nodeType === 'task') debouncedLoad(() => loadData());
    });

    bus.on('node:deleted', ({ nodeType }) => {
        if (nodeType === 'event' || nodeType === 'task') debouncedLoad(() => loadData());
    });
});
watch(() => props.vaultPath, () => { loadData(); });

// --- Helpers ---
const monthNames = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const monthNamesShort = monthNames.map(m => m.substring(0, 3));
const dayNamesShort = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const hours = Array.from({length: 24}, (_, i) => i); // 0 to 23

const formatDateString = (date: Date) => {
    const y = date.getFullYear();
    const m = String(date.getMonth() + 1).padStart(2, '0');
    const d = String(date.getDate()).padStart(2, '0');
    return `${y}-${m}-${d}`;
};

const isSameDay = (d1: Date, d2: Date) => {
    return d1.getFullYear() === d2.getFullYear() && d1.getMonth() === d2.getMonth() && d1.getDate() === d2.getDate();
};

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

const isAllDayOrMultiDay = (e: EventMetadata) => {
    if (e.is_all_day) return true;
    if (!e.start_at) return false;
    const s = e.start_at.split('T')[0];
    const en = e.end_at ? e.end_at.split('T')[0] : s;
    return s !== en;
};

const deleteRelationNode = async (bl: any) => {
    const isConfirmed = await ask(`This will permanently delete the ${bl.node_type} "${bl.title}". This action cannot be undone.`, { 
        title: 'Delete Item', 
        kind: 'warning' 
    });
    if (!isConfirmed) return;
    
    try {
        await ns.deleteNode({ relPath: bl.id });
        
        if (eventForm.value.relations) {
            const originalLength = eventForm.value.relations.length;
            eventForm.value.relations = eventForm.value.relations.filter(link => !link.includes(bl.id));
            if (eventForm.value.relations.length < originalLength && eventForm.value.id) {
                // Background save without closing modal
                let finalTags: string[] = [];
                if (eventForm.value.tagsStr.trim()) {
                    finalTags = eventForm.value.tagsStr.split(',').map(s => s.trim().replace(/^#/, '')).filter(s => s);
                }
                await ns.writeNode({ 
                    relPath: eventForm.value.path,
                    title: eventForm.value.title,
                    nodeType: 'event',
                    properties: {
                        is_all_day: eventForm.value.isAllDay,
                        start_at: eventForm.value.start_at,
                        end_at: eventForm.value.end_at,
                        location: eventForm.value.location,
                        tags: finalTags,
                        relations: eventForm.value.relations,
                        recurrence: eventForm.value.recurrence,
                        recurrence_end_at: eventForm.value.recurrence_end_at,
                        series_id: eventForm.value.series_id,
                        exceptions: eventForm.value.exceptions
                    },
                    content: eventForm.value.description,
                    silent: true,
                });
            }
        }
        eventBacklinks.value = eventBacklinks.value.filter(n => n.id !== bl.id);
    } catch (e) {
        console.error(`Failed to delete ${bl.node_type}:`, e);
    }
};

const formatEventTime = (ev: EventMetadata) => {
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

// --- Navigation ---
const headerDisplayString = computed(() => {
    const year = currentDate.value.getFullYear();
    if (viewMode.value === 'year') return `${year}`;
    if (viewMode.value === 'month') return `${monthNames[currentDate.value.getMonth()]} ${year}`;
    if (viewMode.value === 'day') return `${currentDate.value.toLocaleDateString(undefined, { weekday: 'long', month: 'long', day: 'numeric'})}, ${year}`;
    if (viewMode.value === 'week') {
        const week = currentWeekDays.value;
        const first = week[0].date;
        const last = week[6].date;
        if (first.getMonth() === last.getMonth()) {
            return `${monthNames[first.getMonth()]} ${year}`;
        } else if (first.getFullYear() === last.getFullYear()) {
            return `${monthNamesShort[first.getMonth()]} - ${monthNamesShort[last.getMonth()]} ${year}`;
        } else {
            return `${monthNamesShort[first.getMonth()]} ${first.getFullYear()} - ${monthNamesShort[last.getMonth()]} ${last.getFullYear()}`;
        }
    }
    return '';
});

const navigatePrev = () => {
    const d = new Date(currentDate.value);
    if (viewMode.value === 'month') d.setMonth(d.getMonth() - 1);
    else if (viewMode.value === 'day') d.setDate(d.getDate() - 1);
    else if (viewMode.value === 'week') d.setDate(d.getDate() - 7);
    else if (viewMode.value === 'year') d.setFullYear(d.getFullYear() - 1);
    currentDate.value = d;
};

const navigateNext = () => {
    const d = new Date(currentDate.value);
    if (viewMode.value === 'month') d.setMonth(d.getMonth() + 1);
    else if (viewMode.value === 'day') d.setDate(d.getDate() + 1);
    else if (viewMode.value === 'week') d.setDate(d.getDate() + 7);
    else if (viewMode.value === 'year') d.setFullYear(d.getFullYear() + 1);
    currentDate.value = d;
};

const goToToday = () => {
    currentDate.value = new Date();
    selectedDate.value = new Date();
    if (viewMode.value === 'year') viewMode.value = 'month'; // Jump to month mode if today clicked from year
    showRightPanel.value = false;
};

// --- Computed Modes ---

// 1. Month Mode
const calendarDays = computed(() => {
    const year = currentDate.value.getFullYear();
    const month = currentDate.value.getMonth();
    const firstDay = new Date(year, month, 1);
    const startDayOfWeek = firstDay.getDay();
    const prevMonthDays = new Date(year, month, 0).getDate();
    const lastDayOfMonth = new Date(year, month + 1, 0).getDate();
    
    const days = [];
    for (let i = startDayOfWeek - 1; i >= 0; i--) {
        days.push({ date: new Date(year, month - 1, prevMonthDays - i), inMonth: false });
    }
    for (let d = 1; d <= lastDayOfMonth; d++) {
        days.push({ date: new Date(year, month, d), inMonth: true });
    }
    let nextI = 1;
    while (days.length % 7 !== 0 || days.length < 42) {
        days.push({ date: new Date(year, month + 1, nextI++), inMonth: false });
    }
    return days;
});

// 2. Week Mode
const currentWeekDays = computed(() => {
    const d = new Date(currentDate.value);
    const day = d.getDay();
    const diff = d.getDate() - day; // Sunday is 0
    const startOfWeek = new Date(d.setDate(diff));
    const week = [];
    for (let i = 0; i < 7; i++) {
        const cur = new Date(startOfWeek);
        cur.setDate(startOfWeek.getDate() + i);
        week.push({ date: cur, dateStr: formatDateString(cur) });
    }
    return week;
});

// 3. Year Mode
const yearMonths = computed(() => {
    const year = currentDate.value.getFullYear();
    return Array.from({length: 12}, (_, i) => { // i is month index (0-11)
        const daysInMonth = new Date(year, i + 1, 0).getDate();
        const startDayOfWeek = new Date(year, i, 1).getDay();
        const days = [];
        // empty paddings
        for (let p=0; p<startDayOfWeek; p++) days.push(null);
        // real days
        for (let d=1; d<=daysInMonth; d++) {
            const dt = new Date(year, i, d);
            days.push({
                date: dt,
                hasItems: hasItemsOnDate(dt),
                isToday: isSameDay(dt, new Date())
            });
        }
        return { monthIndex: i, name: monthNames[i], days };
    });
});

const clickDay = (dateObj: Date) => {
    selectedDate.value = dateObj;
    // Auto-update currentDate to follow the selection into views
    currentDate.value = new Date(dateObj);
    if (viewMode.value !== 'day' && viewMode.value !== 'week') {
        showRightPanel.value = true;
    }
};

const clickYearDay = (dt: Date) => {
    selectedDate.value = dt;
    currentDate.value = new Date(dt);
    viewMode.value = 'day';
    showRightPanel.value = false;
};

// --- Panel Computed ---
const selectedDateFormattedStr = computed(() => formatDateString(selectedDate.value));
const selectedDateDisplay = computed(() => selectedDate.value.toLocaleDateString(undefined, { weekday: 'long', year: 'numeric', month: 'long', day: 'numeric' }));
const selectedTasks = computed(() => getTasksForDate(selectedDateFormattedStr.value));
const selectedEvents = computed(() => getEventsForDate(selectedDateFormattedStr.value).sort((a,b) => (a.start_at || '').localeCompare(b.start_at || '')));

// --- Event Functions ---
const openAddEventModal = (defaultDate?: Date, hr?: number) => {
    const targetDateStr = defaultDate ? formatDateString(defaultDate) : selectedDateFormattedStr.value;
    const startHour = hr !== undefined ? hr.toString().padStart(2, '0') : '12';
    const endHour = hr !== undefined ? (hr + 1).toString().padStart(2, '0') : '13';
    eventForm.value = {
        isEdit: false, id: '', path: '', title: '',
        isAllDay: false, start_at: `${targetDateStr}T${startHour}:00`, end_at: `${targetDateStr}T${endHour}:00`,
        location: '', description: '', tagsStr: '', relations: [] as string[],
        recurrence: 'none', recurrence_end_at: '', series_id: '', exceptions: [], reminders: [], _editScope: 'all', _originalEvent: null
    };
    eventBacklinks.value = [];
    isCreatingNote.value = false;
    showEventForm.value = true;
};

const hourOptions = Array.from({length: 24}, (_, i) => i.toString().padStart(2, '0'));
const minuteOptions = ['00', '05', '10', '15', '20', '25', '30', '35', '40', '45', '50', '55'];

const startAtDate = computed({
    get: () => eventForm.value.start_at.split('T')[0],
    set: (v) => eventForm.value.start_at = `${v}T${eventForm.value.start_at.split('T')[1] || '12:00'}`
});
const startAtHour = computed({
    get: () => (eventForm.value.start_at.split('T')[1] || '12:00').split(':')[0],
    set: (v) => eventForm.value.start_at = `${eventForm.value.start_at.split('T')[0] || new Date().toISOString().split('T')[0]}T${v}:${startAtMinute.value}`
});
const startAtMinute = computed({
    get: () => (eventForm.value.start_at.split('T')[1] || '12:00').split(':')[1],
    set: (v) => eventForm.value.start_at = `${eventForm.value.start_at.split('T')[0] || new Date().toISOString().split('T')[0]}T${startAtHour.value}:${v}`
});
const startAtMinuteOptions = computed(() => {
    const opts = [...minuteOptions];
    if (startAtMinute.value && !opts.includes(startAtMinute.value)) {
        opts.push(startAtMinute.value);
        opts.sort();
    }
    return opts;
});

const endAtDate = computed({
    get: () => (eventForm.value.end_at || '').split('T')[0],
    set: (v) => eventForm.value.end_at = `${v}T${(eventForm.value.end_at || '').split('T')[1] || '13:00'}`
});
const endAtHour = computed({
    get: () => (eventForm.value.end_at || 'T13:00').split('T')[1].split(':')[0],
    set: (v) => eventForm.value.end_at = `${(eventForm.value.end_at || '').split('T')[0] || new Date().toISOString().split('T')[0]}T${v}:${endAtMinute.value}`
});
const endAtMinute = computed({
    get: () => (eventForm.value.end_at || 'T13:00').split('T')[1].split(':')[1],
    set: (v) => eventForm.value.end_at = `${(eventForm.value.end_at || '').split('T')[0] || new Date().toISOString().split('T')[0]}T${endAtHour.value}:${v}`
});
const endAtMinuteOptions = computed(() => {
    const opts = [...minuteOptions];
    if (endAtMinute.value && !opts.includes(endAtMinute.value)) {
        opts.push(endAtMinute.value);
        opts.sort();
    }
    return opts;
});

const eventBacklinks = ref<{ id: string, title: string, node_type: string }[]>([]);
const isCreatingNote = ref(false);
const newNoteTitle = ref('');

const loadEventBacklinks = async (title: string, id: string) => {
    try {
        eventBacklinks.value = await ns.getLinkedNodes(title, id);
    } catch (e) {
        console.error("Failed to load event backlinks", e);
        eventBacklinks.value = [];
    }
};

const eventRelations = computed(() => {
    const items = [...eventBacklinks.value];
    if (eventForm.value.relations && eventForm.value.relations.length > 0) {
        const mdLinkRe = /\[([^\]]+)\]\(synabit:\/\/(note|node|person|task|quickcap|event)\/([^)]+)\)/;
        for (const link of eventForm.value.relations) {
            const match = mdLinkRe.exec(link);
            if (match) {
                const title = match[1];
                const type = match[2];
                const id = match[3];
                if (!items.find(n => n.id === id)) {
                    items.push({ id, title, node_type: type });
                }
            }
        }
    }
    return items;
});

const createMeetingNote = async () => {
    if (!newNoteTitle.value.trim() || !eventForm.value.title) return;
    try {
        const relPath = `Notes/note_${Date.now()}.md`;
        await ns.writeNode({
            relPath,
            nodeType: 'note',
            title: newNoteTitle.value.trim(),
            properties: {},
            content: ``,
            eventType: 'created',
        });
        
        const noteMention = `[${newNoteTitle.value.trim()}](synabit://note/${relPath})`;
        eventForm.value.relations = eventForm.value.relations || [];
        eventForm.value.relations.push(noteMention);
        
        isCreatingNote.value = false;
        newNoteTitle.value = '';
        
        if (eventForm.value.id && eventForm.value.path) {
            // Auto-save the event so the graph edge is created immediately
            let finalTags: string[] = [];
            if (eventForm.value.tagsStr.trim()) {
                finalTags = eventForm.value.tagsStr.split(',').map(s => s.trim().replace(/^#/, '')).filter(s => s);
            }
            await ns.writeNode({ 
                relPath: eventForm.value.path,
                title: eventForm.value.title,
                nodeType: 'event',
                properties: {
                    is_all_day: eventForm.value.isAllDay,
                    start_at: eventForm.value.start_at,
                    end_at: eventForm.value.end_at,
                    location: eventForm.value.location,
                    tags: finalTags,
                    relations: eventForm.value.relations || [],
                    recurrence: eventForm.value.recurrence,
                    recurrence_end_at: eventForm.value.recurrence_end_at,
                    series_id: eventForm.value.series_id,
                    exceptions: eventForm.value.exceptions
                },
                content: eventForm.value.description,
                silent: true,
            });
            await loadEventBacklinks(eventForm.value.title, eventForm.value.id);
        }
    } catch (e) {
        console.error("Failed to create note", e);
    }
};

// --- Scope Modal State ---
const showScopeModal = ref(false);
const scopeAction = ref<'edit' | 'delete'>('edit');
const scopeSelection = ref<'this' | 'following' | 'all'>('this');
const targetOccurrenceDate = ref('');
const pendingEventAction = ref<EventMetadata | null>(null);

const confirmScopeAction = () => {
    showScopeModal.value = false;
    if (scopeAction.value === 'edit') {
        eventForm.value._editScope = scopeSelection.value as any;
        submitEventActual();
    } else {
        deleteEventActual(pendingEventAction.value!, targetOccurrenceDate.value, scopeSelection.value);
    }
};

const openEditEventModal = (ev: EventMetadata, dateStr: string) => {
    targetOccurrenceDate.value = dateStr;
    pendingEventAction.value = ev;
    openEditEventModalActual(ev, dateStr, 'occurrence_view');
};

const openEditEventModalActual = async (ev: EventMetadata, dateStr: string, scope: 'occurrence_view' | 'this' | 'following' | 'all') => {
    let startAt = ev.start_at || '';
    if (startAt.includes('T')) startAt = startAt.slice(0, 16);
    let endAt = ev.end_at || '';
    if (endAt.includes('T')) endAt = endAt.slice(0, 16);
    
    if (scope === 'occurrence_view' || scope === 'this' || scope === 'following') {
        const timePartStart = startAt.includes('T') ? startAt.split('T')[1] : '12:00';
        const timePartEnd = endAt.includes('T') ? endAt.split('T')[1] : '13:00';
        startAt = `${dateStr}T${timePartStart}`;
        endAt = `${dateStr}T${timePartEnd}`;
    }
    
    eventForm.value = {
        isEdit: true, id: ev.id, path: ev.path, title: ev.title,
        isAllDay: ev.is_all_day, start_at: startAt, end_at: endAt, location: ev.location,
        description: ev.content, tagsStr: ev.tags.join(', '),
        relations: [...(ev.relations || [])],
        recurrence: ev.recurrence || 'none',
        recurrence_end_at: ev.recurrence_end_at || '',
        series_id: ev.series_id || '',
        exceptions: [...(ev.exceptions || [])],
        reminders: [...(ev.reminders || [])],
        _editScope: scope as any,
        _originalEvent: ev
    };
    eventBacklinks.value = [];
    isCreatingNote.value = false;
    showEventForm.value = true;
    if (ev.title && ev.id) {
        loadEventBacklinks(ev.title, ev.id);
    }
};

watch(() => eventForm.value.isAllDay, (newVal) => {
    if (newVal) {
        eventForm.value.start_at = eventForm.value.start_at.split('T')[0];
        if (eventForm.value.end_at) {
            eventForm.value.end_at = eventForm.value.end_at.split('T')[0];
        }
    } else {
        if (!eventForm.value.start_at.includes('T')) {
            eventForm.value.start_at = `${eventForm.value.start_at}T12:00:00`;
        }
        if (eventForm.value.end_at && !eventForm.value.end_at.includes('T')) {
            eventForm.value.end_at = `${eventForm.value.end_at}T13:00:00`;
        }
    }
});

const closeEventForm = () => { showEventForm.value = false; };

const openLinkedNote = (id: string, type: string) => {
    closeEventForm();
    emit('open-node', id, type);
};

const reminderPreset = ref('');
const customReminder = ref('');
const addReminder = () => {
    let val = '';
    if (reminderPreset.value === 'custom') {
        if (customReminder.value) {
            val = customReminder.value.trim().toLowerCase();
            if (!val.match(/^\d+[mhd]$/)) {
                alert("Custom reminder must be a number followed by m, h, or d (e.g., 45m, 2h, 1d)");
                return;
            }
        }
    } else if (reminderPreset.value) {
        val = reminderPreset.value;
    }
    if (val && !eventForm.value.reminders.includes(val)) {
        eventForm.value.reminders.push(val);
    }
    reminderPreset.value = '';
    customReminder.value = '';
};
const removeReminder = (idx: number) => {
    eventForm.value.reminders.splice(idx, 1);
};

const submitEvent = async () => {
    if (!eventForm.value.title || !eventForm.value.start_at) return;
    
    if (eventForm.value.isEdit && eventForm.value._originalEvent && eventForm.value._originalEvent.recurrence && eventForm.value._originalEvent.recurrence !== 'none') {
        if (eventForm.value._editScope === 'occurrence_view') {
            scopeAction.value = 'edit';
            scopeSelection.value = 'this';
            showScopeModal.value = true;
            return;
        }
    }
    
    await submitEventActual();
};

const submitEventActual = async () => {
    let finalTags: string[] = [];
    if (eventForm.value.tagsStr.trim()) {
        finalTags = eventForm.value.tagsStr.split(',').map(s => s.trim().replace(/^#/, '')).filter(s => s);
    }
    
    // Normalize format to drop seconds or keep ISO consistent if desired, but HTML datetime-local uses YYYY-MM-DDTHH:mm
    
    try {
        let relPath = eventForm.value.path;
        let isCreatingNewNode = !eventForm.value.isEdit || !relPath;
        
        const properties: any = {
            is_all_day: eventForm.value.isAllDay,
            start_at: eventForm.value.start_at,
            end_at: eventForm.value.end_at,
            location: eventForm.value.location,
            tags: finalTags,
            recurrence: eventForm.value.recurrence,
            recurrence_end_at: eventForm.value.recurrence_end_at,
            series_id: eventForm.value.series_id,
            exceptions: eventForm.value.exceptions,
            reminders: eventForm.value.reminders
        };
        
        if (eventForm.value.isEdit && eventForm.value._editScope === 'all' && eventForm.value._originalEvent) {
            const parentEv = eventForm.value._originalEvent;
            const rootId = parentEv.series_id || parentEv.id;
            const rootEv = allEvents.value.find(e => e.id === rootId) || parentEv;
            
            const origStart = rootEv.start_at || '';
            const origEnd = rootEv.end_at || '';
            const origStartDate = origStart.split('T')[0];
            const origEndDate = origEnd.split('T')[0];
            
            const occurrenceStartObj = new Date(targetOccurrenceDate.value + 'T00:00:00');
            const newStartObj = new Date(eventForm.value.start_at.split('T')[0] + 'T00:00:00');
            const diffMs = newStartObj.getTime() - occurrenceStartObj.getTime();
            
            const rootStartObj = new Date(origStartDate + 'T00:00:00');
            rootStartObj.setTime(rootStartObj.getTime() + diffMs);
            const shiftedOrigStartDate = rootStartObj.toISOString().split('T')[0];
            
            const rootEndObj = new Date(origEndDate + 'T00:00:00');
            rootEndObj.setTime(rootEndObj.getTime() + diffMs);
            const shiftedOrigEndDate = rootEndObj.toISOString().split('T')[0];
            
            const newTimeStart = eventForm.value.start_at.includes('T') ? eventForm.value.start_at.split('T')[1] : '';
            const newTimeEnd = eventForm.value.end_at.includes('T') ? eventForm.value.end_at.split('T')[1] : '';
            
            properties.start_at = newTimeStart ? `${shiftedOrigStartDate}T${newTimeStart}` : shiftedOrigStartDate;
            properties.end_at = newTimeEnd ? `${shiftedOrigEndDate}T${newTimeEnd}` : shiftedOrigEndDate;
            
            properties.exceptions = [];
            properties.series_id = '';
            
            const familyEvents = allEvents.value.filter(e => e.id === rootId || e.series_id === rootId);
            let maxEndAt = '';
            let isInfinite = false;
            for (const fam of familyEvents) {
                if (fam.recurrence && fam.recurrence !== 'none') {
                    if (!fam.recurrence_end_at) {
                        isInfinite = true;
                        break;
                    } else if (fam.recurrence_end_at > maxEndAt) {
                        maxEndAt = fam.recurrence_end_at;
                    }
                }
            }
            if (!eventForm.value.recurrence_end_at || eventForm.value.recurrence_end_at === parentEv.recurrence_end_at) {
                properties.recurrence_end_at = isInfinite ? '' : maxEndAt;
            }
            
            for (const famEv of familyEvents) {
                if (famEv.path !== rootId) {
                    await ns.deleteNode({ relPath: famEv.path, silent: true });
                }
            }
            
            relPath = rootId;
        }
        
        if (eventForm.value.relations && eventForm.value.relations.length > 0) {
            properties.relations = eventForm.value.relations;
        }

        if (eventForm.value.isEdit && (eventForm.value._editScope === 'this' || eventForm.value._editScope === 'following')) {
            relPath = `Events/${crypto.randomUUID()}.md`;
            isCreatingNewNode = true;
            
            const parentEv = eventForm.value._originalEvent!;
            properties.recurrence = eventForm.value._editScope === 'this' ? 'none' : eventForm.value.recurrence;
            properties.recurrence_end_at = eventForm.value._editScope === 'this' ? '' : eventForm.value.recurrence_end_at;
            properties.series_id = parentEv.series_id || parentEv.id;
            properties.exceptions = []; // New split event should not inherit exceptions
            const parentProps = {
                is_all_day: parentEv.is_all_day,
                start_at: parentEv.start_at,
                end_at: parentEv.end_at,
                location: parentEv.location,
                tags: parentEv.tags,
                recurrence: parentEv.recurrence,
                recurrence_end_at: parentEv.recurrence_end_at,
                exceptions: [...(parentEv.exceptions || [])],
                relations: [...(parentEv.relations || [])],
                series_id: parentEv.series_id
            };
            
            if (eventForm.value._editScope === 'this') {
                if (!parentProps.exceptions.includes(targetOccurrenceDate.value)) {
                    parentProps.exceptions.push(targetOccurrenceDate.value);
                }
            } else if (eventForm.value._editScope === 'following') {
                const dt = new Date(targetOccurrenceDate.value + 'T00:00:00');
                dt.setDate(dt.getDate() - 1);
                parentProps.recurrence_end_at = dt.toISOString().split('T')[0];
            }
            
            await ns.writeNode({
                relPath: parentEv.path,
                title: parentEv.title,
                nodeType: 'event',
                properties: parentProps,
                content: parentEv.content,
                silent: true,
            });
        }
        
        if (isCreatingNewNode) {
            if (!relPath) relPath = `Events/${crypto.randomUUID()}.md`;
        }
        
        await ns.writeNode({ 
            relPath,
            title: eventForm.value.title,
            nodeType: 'event',
            properties,
            content: eventForm.value.description,
            eventType: isCreatingNewNode ? 'created' : 'updated',
        });
        closeEventForm();
        await loadData();
    } catch(e) { logger.error("Failed to save event:", e); }
};

import { ask } from '@tauri-apps/plugin-dialog';
import { logger } from '../../utils/logger';

const deleteEvent = async (ev: EventMetadata, dateStr: string) => {
    if (ev.recurrence && ev.recurrence !== 'none') {
        scopeAction.value = 'delete';
        scopeSelection.value = 'this';
        targetOccurrenceDate.value = dateStr;
        pendingEventAction.value = ev;
        showScopeModal.value = true;
    } else {
        const isConfirmed = await ask('This action cannot be undone. The event will be permanently removed from your calendar.', { 
            title: `Delete event '${ev.title}'?`, 
            kind: 'warning',
            okLabel: 'Delete',
            cancelLabel: 'Cancel'
        });
        if (isConfirmed) {
            await deleteEventActual(ev, dateStr, 'all');
        }
    }
};

const deleteEventActual = async (ev: EventMetadata, dateStr: string, scope: 'this' | 'following' | 'all') => {
    try {
        if (scope === 'all') {
            const rootId = ev.series_id || ev.id;
            const familyEvents = allEvents.value.filter(e => e.id === rootId || e.series_id === rootId);
            for (const famEv of familyEvents) {
                if (famEv.path !== ev.path) {
                    await ns.deleteNode({ relPath: famEv.path, silent: true });
                }
            }
            await ns.deleteNode({ relPath: ev.path });
        } else {
            const parentProps = {
                is_all_day: ev.is_all_day,
                start_at: ev.start_at,
                end_at: ev.end_at,
                location: ev.location,
                tags: ev.tags,
                recurrence: ev.recurrence,
                recurrence_end_at: ev.recurrence_end_at,
                exceptions: [...(ev.exceptions || [])],
                relations: [...(ev.relations || [])],
                series_id: ev.series_id
            };
            
            if (scope === 'this') {
                if (!parentProps.exceptions.includes(dateStr)) {
                    parentProps.exceptions.push(dateStr);
                }
            } else if (scope === 'following') {
                const dt = new Date(dateStr + 'T00:00:00');
                dt.setDate(dt.getDate() - 1);
                parentProps.recurrence_end_at = dt.toISOString().split('T')[0];
            }
            
            await ns.writeNode({
                relPath: ev.path,
                title: ev.title,
                nodeType: 'event',
                properties: parentProps,
                content: ev.content,
            });
        }
        await loadData();
    } catch(e) { logger.error("Failed to delete event:", e); }
};

const handleDeleteFromForm = () => {
    if (eventForm.value._originalEvent) {
        deleteEvent(eventForm.value._originalEvent, targetOccurrenceDate.value);
        closeEventForm();
    }
};
</script>

<template>
  <div class="h-full flex relative text-[#1c1c1e] dark:text-[#f4f4f5] bg-[#fdfdfc] dark:bg-[#242424]">
     
     <div class="flex-1 flex flex-col h-full overflow-hidden px-3 py-3 md:px-6 md:py-4 transition-all duration-300" :class="{ 'md:pr-96': showRightPanel }">
         <!-- Header -->
         <header class="flex flex-col md:flex-row md:items-center justify-between mb-4 md:mb-6 flex-shrink-0 gap-3" data-tauri-drag-region>
             <div class="flex items-center justify-between w-full md:w-auto">
                 <div class="flex items-center gap-3">
                     <NavButtons />
                     <CalendarIcon class="w-5 h-5 md:w-6 md:h-6 text-purple-500" />
                     <h1 class="text-xl md:text-2xl font-bold tracking-tight select-none">
                         {{ headerDisplayString }}
                     </h1>
                 </div>
             </div>
             
             <div class="flex items-center gap-2 md:gap-4 select-none w-full md:w-auto overflow-x-auto no-scrollbar">
                 <!-- View Switcher -->
                 <div class="flex bg-gray-100 dark:bg-[#1f1f1f] p-1 rounded-xl border border-gray-200 dark:border-[#333] shrink-0">
                    <button v-for="v in (['day','week','month','year'] as ViewMode[])" :key="v"
                            @click="viewMode = v; if (v === 'day' || v === 'week') showRightPanel = false;"
                            class="px-3 py-1.5 md:px-4 md:py-1.5 text-[11px] md:text-xs font-semibold rounded-lg capitalize transition-all"
                            :class="viewMode === v ? 'bg-white shadow-[0_1px_3px_rgba(0,0,0,0.1)] text-purple-600 dark:bg-[#333] dark:text-purple-400' : 'text-gray-500 hover:text-gray-900 dark:text-gray-400 dark:hover:text-white'">
                        {{ v }}
                    </button>
                 </div>

                 <!-- Nav Controls -->
                 <div class="flex items-center gap-2 shrink-0">
                     <button @click="goToToday" class="px-2 py-1.5 md:px-3 md:py-1.5 text-[11px] md:text-xs font-semibold bg-gray-100 hover:bg-gray-200 dark:bg-[#2c2c2c] dark:hover:bg-[#3a3a3a] rounded-lg transition-colors border border-transparent dark:border-gray-700">
                         Today
                     </button>
                     <div class="flex bg-gray-100 dark:bg-[#2c2c2c] rounded-lg p-0.5 border border-transparent dark:border-gray-700">
                         <button @click="navigatePrev" class="p-1 rounded-md hover:bg-white dark:hover:bg-[#444] transition-colors"><ChevronLeft class="w-4 h-4" /></button>
                         <button @click="navigateNext" class="p-1 rounded-md hover:bg-white dark:hover:bg-[#444] transition-colors"><ChevronRight class="w-4 h-4" /></button>
                     </div>
                     <button @click="openAddEventModal()" class="flex items-center gap-1.5 px-3 py-1.5 md:px-3 md:py-1.5 text-[11px] md:text-xs font-semibold bg-purple-600 hover:bg-purple-700 text-white rounded-lg transition-colors shadow-sm ml-0.5 md:ml-1">
                         <Plus class="w-3.5 h-3.5" />
                         <span class="hidden md:inline">New Event</span>
                         <span class="md:hidden">New</span>
                     </button>
                 </div>
             </div>
         </header>
         
         <div class="flex-1 min-h-0 relative w-full">
             <!-- MONTH VIEW -->
             <div v-show="viewMode === 'month'" class="h-full flex flex-col select-none">
                 <div class="grid grid-cols-7 mb-2 flex-shrink-0 border-b border-[#e6e6e6] dark:border-[#333] pb-2 px-1">
                     <div v-for="day in dayNamesShort" :key="day" class="text-center text-xs font-bold uppercase tracking-wider text-[#8b8b8b] dark:text-[#71717a]">
                         {{ day }}
                     </div>
                 </div>
                 <div class="flex-1 overflow-y-auto no-scrollbar pb-2 px-1">
                     <div class="grid grid-cols-7 grid-rows-6 gap-2 min-h-[500px] md:min-h-[650px] h-full">
                     <div v-for="(dayObj, idx) in calendarDays" :key="idx" 
                          @click="clickDay(dayObj.date)"
                          class="relative flex flex-col rounded-xl border border-[#ececeb] dark:border-[#2f2f2f] cursor-pointer transition-all duration-200 overflow-hidden group hover:border-[#d4d4d8] dark:hover:border-[#4f4f4f] hover:shadow-sm"
                          :class="[
                              dayObj.inMonth ? 'bg-white dark:bg-[#262626]' : 'bg-gray-50/50 dark:bg-[#1f1f1f]',
                              isSameDay(dayObj.date, selectedDate) ? 'ring-2 ring-purple-500 border-transparent dark:border-transparent' : '',
                              isSameDay(dayObj.date, new Date()) ? 'bg-gradient-to-br from-purple-50/50 to-transparent dark:from-purple-900/10' : ''
                          ]"
                     >
                         <div class="w-full flex justify-between items-start p-2 pointer-events-none">
                             <span class="text-sm font-medium w-6 h-6 flex items-center justify-center rounded-full"
                                   :class="[
                                       !dayObj.inMonth ? 'text-gray-400 dark:text-gray-600' : 'text-[#1c1c1e] dark:text-[#f4f4f5]',
                                       isSameDay(dayObj.date, new Date()) ? 'bg-purple-600 text-white dark:text-white' : ''
                                   ]"
                             >
                                 {{ dayObj.date.getDate() }}
                             </span>
                         </div>
                         <div class="flex-1 px-1 md:px-2 pb-1 md:pb-2 overflow-y-auto w-full no-scrollbar md:space-y-1">
                             <!-- Mobile Dots -->
                             <div class="flex flex-wrap gap-1 md:hidden pt-1 px-1">
                                 <div v-for="ev in getEventsForDate(formatDateString(dayObj.date))" :key="'evt-dot-'+ev.id" class="w-1.5 h-1.5 rounded-full bg-blue-500"></div>
                                 <div v-for="tk in getTasksForDate(formatDateString(dayObj.date))" :key="'tsk-dot-'+tk.id" class="w-1.5 h-1.5 rounded-full" :class="tk.status === 'done' ? 'bg-green-500' : 'bg-gray-400 dark:bg-gray-500'"></div>
                             </div>
                             <!-- Desktop Text -->
                             <div class="hidden md:flex flex-col gap-1 w-full" v-for="dayData in [getMonthViewItems(formatDateString(dayObj.date))]" :key="'ddata-'+dayObj.date.getTime()">
                                 <template v-for="item in dayData.display" :key="item.type + '-' + item.id">
                                     <div v-if="item.type === 'event'" class="w-full text-left truncate px-1.5 py-0.5 rounded text-[10px] font-medium bg-blue-100/80 text-blue-800 border border-blue-200/50 dark:bg-blue-900/30 dark:text-blue-300 dark:border-blue-800/30 shadow-[0_1px_2px_rgba(0,0,0,0.02)] transition-colors hover:brightness-95 cursor-pointer" @click.stop="openEditEventModal(item.event, formatDateString(dayObj.date))">
                                         <span v-if="item.event_time" class="opacity-70 mr-0.5">{{ item.event_time }}</span> {{ item.title }}
                                     </div>
                                     <div v-else class="w-full text-left truncate px-1.5 py-0.5 rounded text-[10px] font-medium border border-gray-200/80 dark:border-[#3a3a3a]/80 text-gray-700 dark:text-gray-300 flex items-center gap-1 bg-white dark:bg-[#252525] shadow-[0_1px_2px_rgba(0,0,0,0.02)] transition-colors hover:bg-gray-50 dark:hover:bg-[#2a2a2a] cursor-pointer hover:brightness-95" :class="item.status === 'done' ? 'opacity-60' : ''" @click.stop="$emit('open-node', item.id, 'task')">
                                         <CheckSquare class="w-2.5 h-2.5 shrink-0 hover:text-purple-500 transition-colors" :class="item.status === 'done' ? 'text-green-500' : 'text-gray-400'" @click.stop="toggleTaskStatus(item)" /> <span :class="item.status === 'done' ? 'line-through' : ''">{{ item.title }}</span>
                                     </div>
                                 </template>
                                 <div v-if="dayData.moreCount > 0" 
                                      class="text-[10px] font-semibold text-gray-500 hover:text-gray-800 dark:text-gray-400 dark:hover:text-gray-200 cursor-pointer px-1 py-0.5 hover:bg-gray-100 dark:hover:bg-[#333] rounded transition-colors w-max" 
                                      @click.stop="clickDay(dayObj.date)">
                                     +{{ dayData.moreCount }} more
                                 </div>
                             </div>
                         </div>
                     </div>
                 </div>
                 </div>
             </div>

             <!-- DAY VIEW -->
             <div v-if="viewMode === 'day'" class="w-full h-full flex flex-col border border-[#ececeb] dark:border-[#333] rounded-2xl bg-white dark:bg-[#1a1a1a] select-none overflow-hidden">
                <!-- All day tasks header -->
                <div class="flex border-b border-[#ececeb] dark:border-[#333] bg-gray-50/50 dark:bg-[#222]">
                    <div class="w-16 border-r border-[#ececeb] dark:border-[#333] flex items-center justify-center p-2">
                        <span class="text-[10px] font-bold text-gray-400 uppercase tracking-widest text-center writing-vertical-lr">All Day</span>
                    </div>
                    <div class="flex-1 p-2 flex flex-wrap gap-2 items-start min-h-[40px]" @dblclick="openAddEventModal(currentDate)">
                        <div v-for="tk in getTasksForDate(formatDateString(currentDate))" :key="'tsk-'+tk.id" class="max-w-[200px] truncate px-2 py-1 rounded text-[11px] font-medium border border-gray-200 dark:border-[#3a3a3a] text-gray-600 dark:text-gray-300 flex items-center gap-1 cursor-pointer bg-white dark:bg-[#2c2c2c] shadow-sm hover:brightness-95" @click.stop="$emit('open-node', tk.id, 'task')">
                            <CheckSquare class="w-3 h-3 flex-shrink-0 hover:text-purple-500 transition-colors" :class="tk.status === 'done' ? 'text-green-500' : ''" @click.stop="toggleTaskStatus(tk)" /> {{ tk.title }}
                        </div>
                        <div v-for="ev in getEventsForDate(formatDateString(currentDate)).filter(isAllDayOrMultiDay)" :key="'ad-ev-'+ev.id" class="max-w-[200px] truncate px-2 py-1 rounded text-[11px] font-medium border border-blue-200 dark:border-blue-800/50 text-blue-800 dark:text-blue-200 bg-blue-50 dark:bg-blue-900/30 flex items-center gap-1 cursor-pointer shadow-sm" @click.stop="openEditEventModal(ev, formatDateString(currentDate))">
                            <CalendarIcon class="w-3 h-3 flex-shrink-0" /> {{ ev.title }}
                        </div>
                    </div>
                </div>
                <!-- Hour grid -->
                <div class="flex-1 overflow-y-auto no-scrollbar relative">
                    <div v-for="hr in hours" :key="hr" class="flex min-h-[60px] border-b border-gray-100 dark:border-[#2f2f2f] group" @click="clickDay(currentDate)">
                        <div class="w-16 flex justify-center pt-2 border-r border-gray-100 dark:border-[#2f2f2f] text-xs font-medium text-gray-400 shrink-0 select-none">
                            {{ hr === 0 ? '12 AM' : hr < 12 ? hr + ' AM' : hr === 12 ? '12 PM' : (hr - 12) + ' PM' }}
                        </div>
                        <div class="flex-1 p-1 flex gap-2 relative" @dblclick.self="openAddEventModal(currentDate, hr)">
                            <!-- Events in this hour block -->
                            <div v-for="ev in getEventsForDateAndHour(formatDateString(currentDate), hr)" :key="'ev-'+ev.id" 
                                class="absolute top-1 left-1 right-1 lg:static lg:flex-1 p-2 rounded-lg bg-blue-100/80 text-blue-900 border border-blue-200 dark:bg-blue-900/30 dark:border-blue-800/50 dark:text-blue-200 shadow-sm transition-transform hover:scale-[1.01] cursor-pointer"
                                @click.stop="openEditEventModal(ev, formatDateString(currentDate))">
                                <div class="font-bold text-xs truncate">{{ ev.title }}</div>
                                <div class="flex gap-2 text-[10px] opacity-70 mt-0.5">
                                    <span v-if="formatEventTime(ev)">{{ formatEventTime(ev) }}</span>
                                    <span v-if="ev.location" class="truncate hidden lg:inline">{{ ev.location }}</span>
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
             </div>

             <!-- WEEK VIEW -->
             <div v-if="viewMode === 'week'" class="w-full h-full flex flex-col border border-[#ececeb] dark:border-[#333] rounded-2xl bg-white dark:bg-[#1a1a1a] overflow-hidden select-none">
                <!-- Week Days Header & All-day row -->
                <div class="flex border-b border-[#ececeb] dark:border-[#333] shadow-sm z-10 sticky top-0 bg-white dark:bg-[#1a1a1a]">
                    <div class="w-12 border-r border-[#ececeb] dark:border-[#333] flex items-center justify-center bg-gray-50/50 dark:bg-[#222]">
                        <span class="text-[9px] font-bold text-gray-400 uppercase tracking-widest writing-vertical-lr mb-2">All Day</span>
                    </div>
                    <!-- 7 Columns headers -->
                    <div v-for="dayObj in currentWeekDays" :key="dayObj.dateStr" class="flex-1 flex flex-col border-r last:border-0 border-[#ececeb] dark:border-[#333]" @click="clickDay(dayObj.date)">
                        <!-- Day Label -->
                        <div class="text-center py-2 border-b border-[#ececeb] dark:border-[#333]" 
                             :class="isSameDay(dayObj.date, new Date()) ? 'bg-purple-50 dark:bg-purple-900/20 text-purple-700 dark:text-purple-300' : 'bg-gray-50/50 dark:bg-[#222] text-gray-500 dark:text-gray-400'">
                            <span class="text-xs uppercase font-bold tracking-wider block mb-0.5">{{ dayNamesShort[dayObj.date.getDay()] }}</span>
                            <span class="text-lg font-black" :class="{'bg-purple-600 text-white rounded-full w-7 h-7 flex items-center justify-center mx-auto': isSameDay(dayObj.date, new Date())}">{{ dayObj.date.getDate() }}</span>
                        </div>
                        <!-- All Day Slots -->
                        <div class="p-1 min-h-[40px] flex flex-col gap-1 bg-gray-50/20 dark:bg-[#1d1d1d]" @dblclick="openAddEventModal(dayObj.date)">
                            <div v-for="tk in getTasksForDate(dayObj.dateStr)" :key="'wk-tsk-'+tk.id" class="truncate px-1.5 py-0.5 rounded text-[9px] font-medium border border-gray-200 dark:border-[#3a3a3a] text-gray-600 dark:text-gray-300 flex items-center gap-1 cursor-pointer bg-white dark:bg-[#2c2c2c] shadow-[0_1px_2px_rgba(0,0,0,0.05)] hover:brightness-95" @click.stop="$emit('open-node', tk.id, 'task')">
                                <CheckSquare class="w-2.5 h-2.5 flex-shrink-0 hover:text-purple-500 transition-colors" :class="tk.status === 'done' ? 'text-green-500' : ''" @click.stop="toggleTaskStatus(tk)" /> {{ tk.title }}
                            </div>
                            <div v-for="ev in getEventsForDate(dayObj.dateStr).filter(isAllDayOrMultiDay)" :key="'wk-ad-ev-'+ev.id" class="truncate px-1.5 py-0.5 rounded text-[9px] font-medium border border-blue-200 dark:border-blue-800/50 text-blue-800 dark:text-blue-200 bg-blue-50 dark:bg-blue-900/30 flex items-center gap-1 cursor-pointer shadow-[0_1px_2px_rgba(0,0,0,0.05)]" @click.stop="openEditEventModal(ev, dayObj.dateStr)">
                                <CalendarIcon class="w-2.5 h-2.5 flex-shrink-0" /> {{ ev.title }}
                            </div>
                        </div>
                    </div>
                </div>

                <!-- 24 Hour Grids for Week -->
                <div class="flex-1 overflow-y-auto no-scrollbar relative flex bg-gray-50/10 dark:bg-[#1f1f1f]">
                    <!-- Time labels col -->
                    <div class="w-12 border-r border-[#ececeb] dark:border-[#333] flex flex-col flex-shrink-0 sticky left-0 z-0 bg-white dark:bg-[#1a1a1a]">
                        <div v-for="hr in hours" :key="'lbl-'+hr" class="h-[60px] flex justify-center pt-2 text-[10px] font-medium text-gray-400 shrink-0 border-b border-gray-100 dark:border-[#2f2f2f]">
                             {{ hr === 0 ? '12 AM' : hr < 12 ? hr + ' AM' : hr === 12 ? '12 PM' : (hr - 12) + ' PM' }}
                        </div>
                    </div>
                    <!-- 7 Columns Grid -->
                    <div class="flex-1 flex w-full">
                        <div v-for="dayObj in currentWeekDays" :key="'col-'+dayObj.dateStr" class="flex-1 flex flex-col border-r last:border-0 border-gray-100 dark:border-[#2f2f2f] hover:bg-gray-50/50 dark:hover:bg-[#252525]/30 transition-colors" @click="clickDay(dayObj.date)">
                            <div v-for="hr in hours" :key="'col-'+dayObj.dateStr+'-'+hr" class="h-[60px] border-b border-gray-100/50 dark:border-[#2f2f2f]/50 p-0.5 relative group cursor-pointer" @dblclick.self="openAddEventModal(dayObj.date, hr)">
                                <div v-for="ev in getEventsForDateAndHour(dayObj.dateStr, hr)" :key="'ev-'+ev.id" 
                                    class="w-full absolute inset-x-0.5 top-0.5 p-1 rounded bg-blue-100/90 text-blue-900 border border-blue-200/50 dark:bg-blue-900/40 dark:border-blue-800/50 dark:text-blue-200 shadow-sm cursor-pointer hover:z-10 truncate text-[10px]"
                                    style="height: 56px;"
                                    @click.stop="openEditEventModal(ev, dayObj.dateStr)">
                                    <div class="font-bold truncate">{{ ev.title }}</div>
                                    <div class="opacity-70 truncate" v-if="formatEventTime(ev)">{{ formatEventTime(ev) }}</div>
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
             </div>

             <!-- YEAR VIEW -->
             <div v-if="viewMode === 'year'" class="w-full h-full overflow-y-auto no-scrollbar pb-6 pr-2 select-none">
                <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-4 md:gap-6 lg:gap-8">
                    <div v-for="monthObj in yearMonths" :key="monthObj.monthIndex" class="bg-white dark:bg-[#232323] border border-gray-100 dark:border-[#333] rounded-2xl p-4 shadow-sm">
                        <!-- Month Title -->
                        <div class="text-sm font-bold uppercase tracking-wider text-purple-600 dark:text-purple-400 mb-3 px-1 cursor-pointer hover:underline" @click="(currentDate = new Date(currentDate.getFullYear(), monthObj.monthIndex, 1)), (viewMode='month')">
                            {{ monthObj.name }}
                        </div>
                        <!-- Mini Grid -->
                        <div class="grid grid-cols-7 gap-y-1 gap-x-0.5 justify-items-center">
                            <div v-for="d in dayNamesShort" :key="'y-'+d" class="text-[9px] font-bold text-gray-400 mb-1">
                                {{ d.substring(0,1) }}
                            </div>
                            <div v-for="(day, dIdx) in monthObj.days" :key="dIdx" class="w-6 h-6 flex flex-col items-center justify-center relative group">
                                <template v-if="day">
                                    <div @click="clickYearDay(day.date)" class="w-5 h-5 rounded hover:bg-gray-200 dark:hover:bg-[#444] cursor-pointer flex items-center justify-center transition-colors relative"
                                         :class="[day.isToday ? 'bg-purple-600 text-white rounded-full font-bold hover:bg-purple-700' : 'text-xs text-gray-700 dark:text-gray-300']">
                                        {{ day.date.getDate() }}
                                    </div>
                                    <!-- Heatmap dot -->
                                    <div v-if="day.hasItems && !day.isToday" class="w-1 h-1 rounded-full bg-purple-500 absolute bottom-0"></div>
                                </template>
                            </div>
                        </div>
                    </div>
                </div>
             </div>
         </div>
     </div>
     
     <!-- RIGHT PANEL / BOTTOM SHEET: DAY DETAILS -->
     <!-- Mobile Overlay -->
     <div v-if="showRightPanel" class="md:hidden fixed inset-0 bg-black/20 dark:bg-black/40 z-30" @click="showRightPanel = false"></div>
     
     <div v-show="showRightPanel" 
          class="fixed md:absolute z-40 flex flex-col bg-white dark:bg-[#1a1a1a] shadow-[0_-10px_40px_rgba(0,0,0,0.1)] md:shadow-2xl border-[#e6e6e6] dark:border-[#2c2c2c] bottom-0 left-0 right-0 h-[75vh] rounded-t-3xl md:rounded-none md:h-auto md:top-0 md:bottom-0 md:left-auto md:w-96 md:border-l">
         <div class="h-16 flex items-center justify-between px-6 border-b border-[#e6e6e6] dark:border-[#2c2c2c] flex-shrink-0 relative" data-tauri-drag-region>
             <div class="absolute top-2 left-1/2 -translate-x-1/2 w-10 h-1.5 bg-gray-300 dark:bg-gray-600 rounded-full md:hidden"></div>
             <h2 class="font-bold text-lg text-purple-600 dark:text-purple-400 select-none mt-2 md:mt-0">{{ selectedDateDisplay }}</h2>
             <button @click="showRightPanel = false" class="mt-2 md:mt-0 p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] text-gray-500 transition-colors">
                 <X class="w-5 h-5" />
             </button>
         </div>
         
         <div class="flex-1 overflow-y-auto p-4 space-y-6">
             <!-- Add Event Button -->
             <button @click="openAddEventModal()" class="w-full py-3 border border-dashed border-gray-300 dark:border-gray-700 rounded-xl flex items-center justify-center gap-2 text-gray-500 hover:bg-gray-50 dark:hover:bg-[#2c2c2c] hover:text-black dark:hover:text-white transition-all cursor-pointer">
                 <Plus class="w-4 h-4" /> <span class="text-sm font-semibold">New Event</span>
             </button>
             
             <!-- Events Section -->
             <div>
                 <h3 class="text-xs font-bold text-gray-400 uppercase tracking-wider mb-3 px-2 flex items-center gap-2">
                     <CalendarIcon class="w-3.5 h-3.5" /> Events
                 </h3>
                 <div v-if="selectedEvents.length === 0" class="text-sm text-center text-gray-500 py-4 italic bg-gray-50 rounded-xl dark:bg-[#1e1e1e]">No events scheduled.</div>
                 <div class="space-y-2">
                     <div v-for="ev in selectedEvents" :key="ev.id" @click="openEditEventModal(ev, selectedDateFormattedStr)" class="p-3 bg-white dark:bg-[#232323] border border-[#f0f0f0] dark:border-[#333] rounded-xl shadow-sm group cursor-pointer hover:border-purple-300 dark:hover:border-purple-500/50 transition-colors">
                         <div class="flex justify-between items-start mb-1">
                             <h4 class="font-bold text-base text-gray-900 dark:text-gray-100 line-clamp-1">{{ ev.title }}</h4>
                             <div class="flex items-center gap-1 md:opacity-0 opacity-100 group-hover:opacity-100 transition-opacity">
                                <button @click.stop="deleteEvent(ev, selectedDateFormattedStr)" class="p-1 hover:bg-red-100 dark:hover:bg-red-900/30 rounded text-red-500"><Trash2 class="w-3 h-3"/></button>
                             </div>
                         </div>
                         <div class="flex items-center flex-wrap gap-x-3 gap-y-1 text-xs text-gray-500 dark:text-gray-400 mb-2">
                             <div class="flex items-center gap-1" v-if="formatEventTime(ev)"><Clock class="w-3 h-3" /> {{ formatEventTime(ev) }}</div>
                             <div class="flex items-center gap-1" v-if="ev.location"><MapPin class="w-3 h-3" /> {{ ev.location }}</div>
                         </div>
                         <p v-if="ev.content" class="text-sm text-gray-600 dark:text-gray-300 line-clamp-3 mb-2">{{ ev.content }}</p>
                         <div class="flex flex-wrap gap-1" v-if="ev.tags.length">
                             <span v-for="tag in ev.tags" :key="tag" class="text-[10px] flex items-center bg-gray-100 dark:bg-gray-800 px-1.5 py-0.5 rounded text-gray-600 dark:text-gray-300">
                                 <Hash class="w-2.5 h-2.5 opacity-50"/>{{ tag }}
                             </span>
                         </div>
                     </div>
                 </div>
             </div>
             
             <!-- Tasks Section -->
             <div>
                 <h3 class="text-xs font-bold text-gray-400 uppercase tracking-wider mb-3 px-2 flex items-center gap-2">
                     <CheckSquare class="w-3.5 h-3.5" /> Due Tasks
                 </h3>
                 <div v-if="selectedTasks.length === 0" class="text-sm text-center text-gray-500 py-4 italic bg-gray-50 rounded-xl dark:bg-[#1e1e1e]">No tasks due today.</div>
                 <div class="space-y-2">
                     <div v-for="tk in selectedTasks" :key="tk.id" class="p-3 bg-white dark:bg-[#232323] border border-[#f0f0f0] dark:border-[#333] rounded-xl shadow-sm flex gap-3 cursor-pointer hover:border-purple-300 transition-colors" @click.stop="$emit('open-node', tk.id, 'task')">
                         <div class="pt-1 select-none pointer-events-auto">
                             <div class="w-4 h-4 rounded border-2 flex items-center justify-center transition-colors border-gray-300 dark:border-gray-500 cursor-pointer hover:border-purple-400"
                                  :class="{'bg-purple-500 border-purple-500 dark:border-purple-500 hover:border-purple-600': tk.status === 'done'}"
                                  @click.stop="toggleTaskStatus(tk)">
                             </div>
                         </div>
                         <div class="flex-1">
                             <h4 class="text-sm font-semibold" :class="tk.status === 'done' ? 'line-through text-gray-400' : 'text-gray-900 dark:text-gray-100'">{{ tk.title }}</h4>
                             <p v-if="tk.comment" class="text-xs text-gray-500 mt-1 line-clamp-1">{{ tk.comment }}</p>
                         </div>
                     </div>
                 </div>
             </div>
         </div>
     </div>

     <!-- Scope Selection Modal -->
     <div v-if="showScopeModal" class="fixed inset-0 z-[60] flex items-center justify-center bg-black/40 backdrop-blur-sm p-4" @click.self="showScopeModal = false">
        <div class="bg-white dark:bg-[#1e1e1e] w-full max-w-sm rounded-2xl shadow-2xl overflow-hidden border border-[#e6e6e6] dark:border-[#333] flex flex-col">
            <div class="px-6 py-4 border-b border-[#e6e6e6] dark:border-[#333]">
                <h3 class="font-bold text-lg text-black dark:text-white">{{ scopeAction === 'edit' ? 'Edit Recurring Event' : 'Delete Recurring Event' }}</h3>
            </div>
            <div class="p-6 space-y-3">
                <label class="flex items-center gap-3 p-3 border border-gray-200 dark:border-[#444] rounded-lg cursor-pointer hover:bg-gray-50 dark:hover:bg-[#2a2a2a] transition-colors" :class="{'border-purple-500 bg-purple-50/50 dark:bg-purple-900/20': scopeSelection === 'this'}">
                    <input type="radio" v-model="scopeSelection" value="this" class="w-4 h-4 text-purple-600 focus:ring-purple-500 bg-gray-100 border-gray-300 dark:bg-[#333] dark:border-[#444]">
                    <span class="text-sm font-medium text-black dark:text-white">This event</span>
                </label>
                <label class="flex items-center gap-3 p-3 border border-gray-200 dark:border-[#444] rounded-lg cursor-pointer hover:bg-gray-50 dark:hover:bg-[#2a2a2a] transition-colors" :class="{'border-purple-500 bg-purple-50/50 dark:bg-purple-900/20': scopeSelection === 'following'}">
                    <input type="radio" v-model="scopeSelection" value="following" class="w-4 h-4 text-purple-600 focus:ring-purple-500 bg-gray-100 border-gray-300 dark:bg-[#333] dark:border-[#444]">
                    <span class="text-sm font-medium text-black dark:text-white">This and following events</span>
                </label>
                <label class="flex items-center gap-3 p-3 border border-gray-200 dark:border-[#444] rounded-lg cursor-pointer hover:bg-gray-50 dark:hover:bg-[#2a2a2a] transition-colors" :class="{'border-purple-500 bg-purple-50/50 dark:bg-purple-900/20': scopeSelection === 'all'}">
                    <input type="radio" v-model="scopeSelection" value="all" class="w-4 h-4 text-purple-600 focus:ring-purple-500 bg-gray-100 border-gray-300 dark:bg-[#333] dark:border-[#444]">
                    <span class="text-sm font-medium text-black dark:text-white">All events in series</span>
                </label>
            </div>
            <div class="px-6 py-4 bg-gray-50 dark:bg-[#1a1a1a] border-t border-[#e6e6e6] dark:border-[#333] flex justify-end gap-3 text-sm font-semibold select-none">
                <button @click="showScopeModal = false" class="px-4 py-2 rounded-lg text-gray-600 dark:text-gray-400 hover:bg-gray-200 dark:hover:bg-[#333] transition-colors">Cancel</button>
                <button @click="confirmScopeAction" class="px-4 py-2 rounded-lg text-white transition-colors" :class="scopeAction === 'delete' ? 'bg-red-500 hover:bg-red-600' : 'bg-black dark:bg-white dark:text-black hover:bg-purple-600 dark:hover:bg-purple-400'">OK</button>
            </div>
        </div>
     </div>

     <!-- Event Modal Overlay -->
     <div v-if="showEventForm" class="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm p-4" @click.self="closeEventForm">
        <div class="bg-white dark:bg-[#1e1e1e] w-full max-w-md rounded-2xl shadow-2xl overflow-hidden border border-[#e6e6e6] dark:border-[#333] flex flex-col max-h-[90vh]">
            <div class="flex items-center justify-between px-4 md:px-6 py-4 border-b border-[#e6e6e6] dark:border-[#333] select-none text-black dark:text-white">
                <h3 class="font-bold text-lg">{{ eventForm.isEdit ? 'Edit Event' : 'New Event' }}</h3>
                <button @click="closeEventForm" class="text-gray-400 hover:text-red-500"><X class="w-5 h-5"/></button>
            </div>
            <div class="p-6 space-y-4 overflow-y-auto max-h-[70vh]">
                <div>
                   <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5">Event Title *</label>
                   <input v-model="eventForm.title" type="text" class="w-full bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" placeholder="E.g., Team Meeting, John's Birthday">
                </div>
                    <div class="flex items-center gap-4 mb-4">
                        <label class="flex items-center gap-1.5 cursor-pointer">
                            <input type="checkbox" v-model="eventForm.isAllDay" class="w-3.5 h-3.5 text-purple-600 rounded focus:ring-purple-500 bg-gray-100 border-gray-300 dark:bg-[#333] dark:border-[#444]">
                            <span class="text-[10px] font-bold text-gray-400 uppercase tracking-wider mt-0.5">All Day Event</span>
                        </label>
                    </div>
                    <div class="grid grid-cols-2 gap-4">
                        <div>
                            <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5">Start</label>
                            <input v-if="eventForm.isAllDay" v-model="eventForm.start_at" type="date" class="w-full h-[38px] bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" style="color-scheme: dark;">
                            <div v-else class="flex flex-col gap-2">
                                <input v-model="startAtDate" type="date" class="w-full h-[38px] bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" style="color-scheme: dark;">
                                <div class="flex items-center gap-1 w-full">
                                    <select v-model="startAtHour" class="flex-1 h-[38px] bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white text-center appearance-none cursor-pointer" style="-webkit-appearance: none;">
                                        <option v-for="h in hourOptions" :key="h" :value="h">{{ h }}</option>
                                    </select>
                                    <span class="text-gray-400 font-bold">:</span>
                                    <select v-model="startAtMinute" class="flex-1 h-[38px] bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white text-center appearance-none cursor-pointer" style="-webkit-appearance: none;">
                                        <option v-for="m in startAtMinuteOptions" :key="m" :value="m">{{ m }}</option>
                                    </select>
                                </div>
                            </div>
                        </div>
                        <div>
                            <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5">End <span class="lowercase text-[9px] font-normal">(optional)</span></label>
                            <input v-if="eventForm.isAllDay" v-model="eventForm.end_at" type="date" class="w-full h-[38px] bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" style="color-scheme: dark;">
                            <div v-else class="flex flex-col gap-2">
                                <input v-model="endAtDate" type="date" class="w-full h-[38px] bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" style="color-scheme: dark;">
                                <div class="flex items-center gap-1 w-full">
                                    <select v-model="endAtHour" class="flex-1 h-[38px] bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white text-center appearance-none cursor-pointer" style="-webkit-appearance: none;">
                                        <option v-for="h in hourOptions" :key="h" :value="h">{{ h }}</option>
                                    </select>
                                    <span class="text-gray-400 font-bold">:</span>
                                    <select v-model="endAtMinute" class="flex-1 h-[38px] bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white text-center appearance-none cursor-pointer" style="-webkit-appearance: none;">
                                        <option v-for="m in endAtMinuteOptions" :key="m" :value="m">{{ m }}</option>
                                    </select>
                                </div>
                            </div>
                        </div>
                    </div>
                    <div class="grid grid-cols-2 gap-4">
                        <div>
                             <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5">Repeat</label>
                             <select v-model="eventForm.recurrence" class="w-full h-[38px] bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white appearance-none cursor-pointer">
                                 <option value="none">Does not repeat</option>
                                 <option value="daily">Daily</option>
                                 <option value="weekly">Weekly</option>
                                 <option value="monthly">Monthly</option>
                                 <option value="yearly">Yearly</option>
                             </select>
                        </div>
                        <div v-if="eventForm.recurrence !== 'none'">
                             <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5">Ends On <span class="lowercase text-[9px] font-normal">(optional)</span></label>
                             <input v-model="eventForm.recurrence_end_at" type="date" class="w-full h-[38px] bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" style="color-scheme: dark;">
                        </div>
                    </div>
                 <div>
                   <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5">Reminders</label>
                   <div class="flex flex-col gap-2">
                       <div class="flex items-center gap-2 flex-wrap">
                           <div v-for="(rem, idx) in eventForm.reminders" :key="idx" class="flex items-center gap-1 bg-purple-100 dark:bg-purple-900/30 text-purple-700 dark:text-purple-300 px-2 py-1 rounded-md text-xs font-medium">
                               <Bell class="w-3 h-3" />
                               {{ rem }}
                               <button @click="removeReminder(idx)" class="hover:text-purple-900 dark:hover:text-purple-100 ml-1">
                                   <X class="w-3 h-3" />
                               </button>
                           </div>
                       </div>
                       <div class="flex items-center gap-2">
                           <select v-model="reminderPreset" @change="addReminder" class="flex-1 bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white appearance-none cursor-pointer">
                               <option value="">Add Reminder...</option>
                               <option value="5m">5 minutes before</option>
                               <option value="15m">15 minutes before</option>
                               <option value="30m">30 minutes before</option>
                               <option value="1h">1 hour before</option>
                               <option value="1d">1 day before</option>
                               <option value="custom">Custom...</option>
                           </select>
                           <div v-if="reminderPreset === 'custom'" class="flex items-center gap-2 flex-1">
                               <input v-model="customReminder" @keyup.enter="addReminder" type="text" placeholder="e.g. 45m, 2h" class="w-full bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white">
                               <button @click="addReminder" class="bg-purple-600 hover:bg-purple-700 text-white p-2 rounded-lg transition-colors">
                                   <Plus class="w-4 h-4" />
                               </button>
                           </div>
                       </div>
                   </div>
                </div>
                <div>
                   <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5">Location</label>
                   <input v-model="eventForm.location" type="text" class="w-full bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" placeholder="Zoom link, Office, etc.">
                </div>
                <div>
                   <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5">Description</label>
                   <textarea v-model="eventForm.description" rows="3" class="w-full bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" placeholder="Event details..."></textarea>
                </div>
                <div>
                   <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5">Tags</label>
                   <input v-model="eventForm.tagsStr" type="text" class="w-full bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg px-3 py-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white" placeholder="meeting, urgent (comma separated)">
                </div>

                <!-- Relations Section -->
                <div v-if="eventForm.isEdit" class="pt-4 border-t border-gray-100 dark:border-[#333]">
                   <div class="flex items-center justify-between mb-2">
                       <label class="block text-xs font-bold text-gray-500 uppercase tracking-wider">Relations ({{ eventRelations.length }})</label>
                       <button v-if="!isCreatingNote" @click="isCreatingNote = true; newNoteTitle = `Meeting Note: ${eventForm.title}`" class="text-[11px] font-medium text-purple-600 hover:text-purple-700 flex items-center">
                           <Plus class="w-3 h-3 mr-0.5" /> Create Note
                       </button>
                   </div>
                   
                   <div v-if="isCreatingNote" class="mb-3 flex items-center gap-2">
                       <input v-model="newNoteTitle" type="text" class="flex-1 bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-md px-2.5 py-1.5 text-xs focus:outline-none focus:border-purple-500 text-black dark:text-white" placeholder="Note Title...">
                       <button @click="createMeetingNote" class="p-1.5 bg-purple-600 text-white rounded-md hover:bg-purple-700 transition-colors">
                           <Check class="w-3.5 h-3.5" />
                       </button>
                       <button @click="isCreatingNote = false" class="p-1.5 bg-gray-200 dark:bg-[#444] text-gray-600 dark:text-gray-300 rounded-md hover:bg-gray-300 dark:hover:bg-[#555] transition-colors">
                           <X class="w-3.5 h-3.5" />
                       </button>
                   </div>
                   
                   <div v-if="eventRelations.length === 0 && !isCreatingNote" class="text-[12px] text-gray-400 italic">No linked items yet.</div>
                   <div v-else class="space-y-1.5">
                       <div v-for="bl in eventRelations" :key="bl.id" @click="openLinkedNote(bl.id, bl.node_type)" class="flex items-center gap-2 px-2.5 py-2 bg-gray-50 dark:bg-[#252525] rounded-md border border-gray-100 dark:border-[#333] cursor-pointer hover:bg-gray-100 dark:hover:bg-[#2f2f2f] transition-colors group">
                           <FileText v-if="bl.node_type === 'note'" class="w-3.5 h-3.5 text-blue-500 shrink-0" />
                           <User v-else-if="bl.node_type === 'person'" class="w-3.5 h-3.5 text-green-500 shrink-0" />
                           <CheckSquare v-else-if="bl.node_type === 'task'" class="w-3.5 h-3.5 text-yellow-500 shrink-0" />
                           <Link2 v-else class="w-3.5 h-3.5 text-purple-500 shrink-0" />
                           <span class="text-[12px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] truncate flex-1">{{ bl.title }}</span>
                           
                           <div class="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                               <button @click.stop="deleteRelationNode(bl)" class="p-1 hover:bg-red-100 dark:hover:bg-red-900/30 rounded text-red-500" title="Delete Item"><Trash2 class="w-3 h-3" /></button>
                           </div>
                       </div>
                   </div>
                </div>

            </div>
            <div class="px-6 py-4 bg-gray-50 dark:bg-[#1a1a1a] border-t border-[#e6e6e6] dark:border-[#333] flex items-center gap-3 text-sm font-semibold select-none" :class="eventForm.isEdit ? 'justify-between' : 'justify-end'">
                <button v-if="eventForm.isEdit" @click="handleDeleteFromForm" class="px-4 py-2 rounded-lg text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors">Delete</button>
                <div class="flex items-center gap-3">
                    <button @click="closeEventForm" class="px-4 py-2 rounded-lg text-gray-600 dark:text-gray-400 hover:bg-gray-200 dark:hover:bg-[#333] transition-colors">Cancel</button>
                    <button @click="submitEvent" class="px-4 py-2 rounded-lg bg-black text-white dark:bg-white dark:text-black hover:bg-purple-600 dark:hover:bg-purple-400 transition-colors" :disabled="!eventForm.title">Save Event</button>
                </div>
            </div>
        </div>
     </div>
  </div>
</template>
