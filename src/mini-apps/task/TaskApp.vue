<script setup lang="ts">
import { ref, onMounted, watch, computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { ask } from '@tauri-apps/plugin-dialog';
import { CheckCircle2, Circle, Plus, Trash2, Tag, CalendarDays, List, Trello, Table2, Search, X, Inbox, Sun, Calendar, Coffee, Send, Eye, EyeOff, Menu as MenuIcon, FileText, Edit3, Settings, Palette, ChevronDown, Link, File, Unlink, User } from 'lucide-vue-next';
import TaskEditModal from './TaskEditModal.vue';
import ProjectEditModal from './ProjectEditModal.vue';
import NavButtons from '../../shared/components/NavButtons.vue';
import { useSettings } from '../../composables/useSettings';
import { logger } from '../../utils/logger';
import TransactionModal from '../finance/TransactionModal.vue';
import type { Transaction, FinanceAccount } from '../finance/types';
import { DEFAULT_INCOME_CATEGORIES, DEFAULT_EXPENSE_CATEGORIES, DEFAULT_ACCOUNTS } from '../finance/types';
import ResourceLinkModal from './ResourceLinkModal.vue';

const { taskArchiveDays } = useSettings();

const props = defineProps<{
  vaultPath: string;
}>();

const emit = defineEmits(['open-node']);



export interface TaskMetadata {
    id: string;
    title: string;
    status: string;
    is_transferred: boolean;
    transferred_to: string;
    track_progress: boolean;
    priority: string;
    start_date: string;
    due_date: string;
    comment: string;
    source_link: string;
    tags: string[];
    content: string;
    path: string;
    created_at: string;
    updated_at: string;
    completed_at: string;
    custom_fields: Record<string, any>;
    isNew?: boolean;
}

const tasks = ref<TaskMetadata[]>([]);
const projects = ref<any[]>([]);
const searchQuery = ref('');
const newProjectDraft = ref<any>(null);
const activeProjectTab = ref<'overview' | 'notes'>('overview');

const showTxModal = ref(false);
const incomeCategories = ref<string[]>([...DEFAULT_INCOME_CATEGORIES]);
const expenseCategories = ref<string[]>([...DEFAULT_EXPENSE_CATEGORIES]);
const accounts = ref<FinanceAccount[]>([...DEFAULT_ACCOUNTS]);

const activeCategory = ref<string>('today');
const isMobileSidebarOpen = ref(false);
const showProjectEditModal = ref(false);

const showEmbedPicker = ref(false);
const allNotesForPicker = ref<any[]>([]);
const isLinkingResource = ref(false);
const showAddResourceMenu = ref(false);
const showEmptyAddMenu = ref(false);

const backendSearchIds = ref<string[] | null>(null);
let taskSearchTimeout: ReturnType<typeof setTimeout>;

// Extract only the free-text portion from a task search query (strip domain-specific filters)
function extractTextQuery(query: string): string {
    return query
        .replace(/is:[^\s]+/g, '')
        .replace(/not:[^\s]+/g, '')
        .replace(/(?:p|priority):[1-4]/g, '')
        .replace(/status:[a-z_]+/g, '')
        .replace(/(?:#|tag:)[^\s]+/g, '')
        .replace(/@[^\s]+/g, '')
        .replace(/prop:[^:=\s]+(?:=[^\s]+)?/g, '')
        .trim();
}

// Debounced backend search for Tasks
watch(searchQuery, (q) => {
    clearTimeout(taskSearchTimeout);
    const textPart = extractTextQuery(q.toLowerCase());
    if (!textPart) {
        backendSearchIds.value = null;
        return;
    }
    taskSearchTimeout = setTimeout(async () => {
        try {
            const resp = await invoke<{ results: { id: string }[], total_count: number, query_time_ms: number }>('search_tasks', {
                vaultPath: props.vaultPath,
                query: textPart
            });
            if (extractTextQuery(searchQuery.value.toLowerCase()) === textPart) {
                backendSearchIds.value = resp.results.map(r => r.id);
            }
        } catch (e) {
            console.error('Task backend search error', e);
        }
    }, 200);
});

const searchedTasks = computed(() => {
    let result = tasks.value;
    
    if (searchQuery.value.trim()) {
        const query = searchQuery.value.toLowerCase();
        const textQuery = extractTextQuery(query);
        
        // Layer 1: Backend FTS5 text search (tokenized, BM25 ranked)
        if (textQuery && backendSearchIds.value !== null) {
            const idSet = new Set(backendSearchIds.value);
            result = result.filter(t => idSet.has(t.id));
            const orderMap = new Map(backendSearchIds.value.map((id, i) => [id, i]));
            result = result.sort((a, b) => (orderMap.get(a.id) ?? 999) - (orderMap.get(b.id) ?? 999));
        } else if (textQuery && backendSearchIds.value === null) {
            // Fallback: local text search while backend is loading
            result = result.filter(t =>
                t.title.toLowerCase().includes(textQuery) || 
                t.content.toLowerCase().includes(textQuery) ||
                t.tags.some(tag => tag.toLowerCase().includes(textQuery))
            );
        }
        
        // Layer 2: Local domain-specific post-filters
        const isQuery = (prop: string) => query.includes(`is:${prop}`);
        const notQuery = (prop: string) => query.includes(`not:${prop}`);
        const pQueryMatch = query.match(/(?:p|priority):([1-4])/);
        const statusQueryMatch = query.match(/status:([a-z_]+)/);
        const tagQueryMatch = query.match(/(?:#|tag:)([^\s]+)/);
        const assignQueryMatch = query.match(/@([^\s]+)/);
        const customPropMatches = [...query.matchAll(/prop:([^:=\s]+)(?:=([^\s]+))?/g)];
        
        const hasDomainFilters = isQuery('transferred') || isQuery('tracked') || isQuery('completed') || isQuery('todo') || isQuery('in_progress') ||
            notQuery('transferred') || notQuery('tracked') ||
            pQueryMatch || statusQueryMatch || tagQueryMatch || assignQueryMatch || customPropMatches.length > 0;
        
        if (hasDomainFilters) {
            result = result.filter(t => {
                if (isQuery('transferred') && !t.is_transferred) return false;
                if (notQuery('transferred') && t.is_transferred) return false;
                if (isQuery('tracked') && !t.track_progress) return false;
                if (notQuery('tracked') && t.track_progress) return false;
                
                if (isQuery('completed') && t.status !== 'done') return false;
                if (isQuery('todo') && t.status !== 'todo') return false;
                if (isQuery('in_progress') && t.status !== 'in_progress') return false;
                
                if (pQueryMatch && t.priority !== `P${pQueryMatch[1]}`) return false;
                if (statusQueryMatch && t.status !== statusQueryMatch[1]) return false;
                
                if (tagQueryMatch) {
                   const searchTag = tagQueryMatch[1];
                   if (!t.tags.some(tag => tag.toLowerCase() === searchTag || tag.toLowerCase().includes(searchTag))) return false;
                }
                
                if (assignQueryMatch) {
                   const searchName = assignQueryMatch[1];
                   if (!t.transferred_to?.toLowerCase().includes(searchName)) return false;
                }
                
                for (const match of customPropMatches) {
                    const key = match[1];
                    const expectedValue = match[2];
                    if (!t.custom_fields || t.custom_fields[key] === undefined) return false;
                    if (expectedValue && String(t.custom_fields[key]).toLowerCase() !== expectedValue) return false;
                }
                return true;
            });
        }
    }
    return result;
});

const categoryCounts = computed(() => {
    const todayStrLocal = getTodayStr();
    
    let all = 0, today = 0, upcoming = 0, someday = 0, transferred = 0;
    
    searchedTasks.value.forEach(t => {
        if (t.status === 'done') return;
        all++;
        if (t.is_transferred) {
            transferred++;
            return;
        }
        
        let isToday = false;
        if (t.due_date && t.due_date <= todayStrLocal) isToday = true;
        else if (t.start_date && t.start_date <= todayStrLocal) isToday = true;
        
        if (isToday) {
            today++;
            return;
        }
        
        let isUpcoming = false;
        if (t.start_date && t.start_date > todayStrLocal) isUpcoming = true;
        else if (t.due_date && t.due_date > todayStrLocal) isUpcoming = true;
        
        if (isUpcoming) upcoming++;
        else someday++;
    });
    
    return { all, today, upcoming, someday, transferred };
});

const getTodayStr = () => {
    const now = new Date();
    const offset = now.getTimezoneOffset() * 60000;
    const localNow = new Date(now.getTime() - offset);
    return localNow.toISOString().split('T')[0];
};

const activeCategoryTasks = computed(() => {
    const today = getTodayStr();
    
    return searchedTasks.value.filter(t => {
        if (activeCategory.value === 'all') return true;

        if (activeCategory.value.startsWith('project:')) {
            const projId = activeCategory.value.substring(8);
            return t.project_id === projId;
        }

        if (activeCategory.value === 'transferred') return t.is_transferred;
        if (t.is_transferred) return false; 
        
        // Hide completed tasks from all views except 'today' (only if completed today) and 'all'
        if (t.status === 'done') {
             if (activeCategory.value === 'today') {
                 return t.completed_at && t.completed_at.startsWith(today);
             }
             return false;
        }
        
        let isToday = false;
        if (t.due_date && t.due_date <= today) isToday = true;
        else if (t.start_date && t.start_date <= today) isToday = true;

        if (activeCategory.value === 'today') return isToday;
        
        if (isToday) return false; 
        
        let isUpcoming = false;
        if (t.start_date && t.start_date > today) isUpcoming = true;
        else if (t.due_date && t.due_date > today) isUpcoming = true;
        
        if (activeCategory.value === 'upcoming') return isUpcoming;
        
        if (activeCategory.value === 'someday') return !isUpcoming;

        return false;
    });
});

const projectProgress = computed(() => {
    if (!activeCategoryTasks.value || activeCategoryTasks.value.length === 0) return 0;
    const total = activeCategoryTasks.value.length;
    const done = activeCategoryTasks.value.filter(t => t.status === 'done').length;
    return Math.round((done / total) * 100);
});

const activeProject = computed(() => {
    if (activeCategory.value.startsWith('project:')) {
        const id = activeCategory.value.substring(8);
        return projects.value.find(p => p.id === id);
    }
    return null;
});

const formatNumber = (val: string | number | null | undefined) => {
    if (!val) return null;
    const num = String(val).replace(/[^0-9.]/g, '');
    if (!num) return null;
    const parts = num.split('.');
    parts[0] = parts[0].replace(/\B(?=(\d{3})+(?!\d))/g, ',');
    return parts.join('.');
};

const projectCurrency = computed(() => {
    if (!activeProject.value || !activeProject.value.custom_fields) return 'VND';
    const keys = Object.keys(activeProject.value.custom_fields);
    const currKey = keys.find(k => k.toLowerCase() === 'currency');
    return currKey ? activeProject.value.custom_fields[currKey] || 'VND' : 'VND';
});

const projectBudget = computed(() => {
    if (!activeProject.value || !activeProject.value.custom_fields) return null;
    const keys = Object.keys(activeProject.value.custom_fields);
    const budgetKey = keys.find(k => k.toLowerCase() === 'budget');
    if (budgetKey && activeProject.value.custom_fields[budgetKey]) {
        return formatNumber(activeProject.value.custom_fields[budgetKey]) + ' ' + projectCurrency.value;
    }
    return null;
});

const calculatedProjectSpent = ref(0);

const projectSpent = computed(() => {
    return (formatNumber(calculatedProjectSpent.value) || '0') + ' ' + projectCurrency.value;
});

const displayCustomFields = computed(() => {
    if (!activeProject.value || !activeProject.value.custom_fields) return [];
    const exclude = ['title', 'type', 'created_at', 'updated_at', 'status', 'start_date', 'due_date', 'color', 'tags', 'project_id', 'completed_at', 'order', 'budget', 'spent', 'wip_limit', 'currency', 'id', 'path', 'content'];
    
    const fields: {key: string, val: any}[] = [];
    for (const [key, val] of Object.entries(activeProject.value.custom_fields)) {
        if (!exclude.includes(key.toLowerCase())) {
            fields.push({ key, val });
        }
    }
    return fields;
});

const linkedResources = ref<any[]>([]);
let fetchNotesTimeout: any = null;

watch(activeProject, (proj, oldProj) => {
    if (proj && proj.id !== oldProj?.id) {
        activeProjectTab.value = 'overview';
    }
    clearTimeout(fetchNotesTimeout);
    if (proj) {
        fetchNotesTimeout = setTimeout(async () => {
            await loadProjectResources();
            
            // Fetch finance transactions for dynamic spent calculation
            recalculateProjectSpent(proj);
        }, 100);
    } else {
        linkedResources.value = [];
        calculatedProjectSpent.value = 0;
    }
}, { immediate: true });

const loadProjectResources = async () => {
    if (!activeProject.value) return;
    try {
        const edges = await invoke<any[]>('get_linked_nodes', { targetTitle: activeProject.value.title, targetId: activeProject.value.id });
        linkedResources.value = edges.filter((n: any) => {
            if (n.node_type === 'json' && n.id.endsWith('.whiteboard.json')) {
                n.node_type = 'whiteboard';
                return true;
            }
            return ['note', 'whiteboard', 'file'].includes(n.node_type);
        });
    } catch(e) {
        console.error('Failed to get linked resources', e);
    }
};

const recalculateProjectSpent = async (proj: any) => {
    try {
        const financeNodes = await invoke<any[]>('get_nodes', { nodeType: 'finance_month' });
        let totalSpent = 0;
        for (const node of financeNodes) {
            if (node.properties?.transactions) {
                for (const tx of node.properties.transactions) {
                    if (tx.projectId === proj.id && tx.type === 'expense') {
                        totalSpent += tx.amount;
                    }
                }
            }
        }
        calculatedProjectSpent.value = totalSpent;
    } catch (e) {
        console.error('Failed to get finance data for project spent', e);
    }
};

const viewMode = ref<'list' | 'board' | 'table' | 'gtd'>('list');

watch(activeCategory, (newCat, oldCat) => {
    const isNewProject = newCat.startsWith('project:');
    const isOldProject = oldCat.startsWith('project:');
    
    if (isNewProject && !isOldProject) {
        viewMode.value = 'board';
    } else if (!isNewProject && isOldProject) {
        viewMode.value = 'list';
    }
});

const WIP_LIMIT = computed(() => {
    if (activeProject.value && activeProject.value.custom_fields && activeProject.value.custom_fields.wip_limit) {
        const parsed = parseInt(activeProject.value.custom_fields.wip_limit);
        if (!isNaN(parsed) && parsed > 0) return parsed;
    }
    return 5;
});
const quickAddColumn = ref<string | null>(null);
const quickAddTitle = ref<string>('');

const BOARD_COLUMNS = [
  { id: 'backlog', name: 'BACKLOG', class: 'border-t-2 border-gray-400 dark:border-gray-500' },
  { id: 'todo', name: 'TO DO', class: 'border-t-2 border-gray-300 dark:border-gray-600' },
  { id: 'in_progress', name: 'IN PROGRESS', class: 'border-t-2 border-blue-400 dark:border-blue-500' },
  { id: 'done', name: 'DONE', class: 'border-t-2 border-green-400 dark:border-green-500' }
];

const getPriorityClass = (priority: string) => {
    switch (priority) {
        case 'P1': return 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400';
        case 'P2': return 'bg-orange-100 text-orange-700 dark:bg-orange-900/30 dark:text-orange-400';
        case 'P3': return 'bg-yellow-100 text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-400';
        case 'P4': return 'bg-slate-100 text-slate-700 dark:bg-slate-800/50 dark:text-slate-400';
        default: return '';
    }
};

const getOrderValueForDrop = (t: TaskMetadata) => {
    if (t.custom_fields && t.custom_fields['order'] !== undefined) {
        return Number(t.custom_fields['order']);
    }
    return -new Date(t.created_at).getTime();
};

const tasksByStatus = computed(() => {
    const sorted: Record<string, TaskMetadata[]> = { backlog: [], todo: [], in_progress: [], done: [] };
    activeCategoryTasks.value.forEach(t => {
        if (sorted[t.status]) {
            sorted[t.status].push(t);
        } else {
            sorted.todo.push(t);
        }
    });

    for (const key in sorted) {
        sorted[key].sort((a, b) => getOrderValueForDrop(a) - getOrderValueForDrop(b));
    }
    return sorted;
});

const showQuickAdd = (colId: string) => {
    quickAddColumn.value = colId;
    quickAddTitle.value = '';
    setTimeout(() => {
        const input = document.getElementById(`quick-add-input-${colId}`);
        if (input) input.focus();
    }, 50);
};

const handleQuickAdd = async (status: string) => {
    const title = quickAddTitle.value.trim();
    if (!title) {
        quickAddColumn.value = null;
        return;
    }

    const relPath = `Tasks/${crypto.randomUUID()}.md`;
    const nowStr = new Date().toISOString().replace('T', ' ').substring(0, 19);
    
    let targetStatus = status;
    if (targetStatus === 'in_progress' && tasksByStatus.value['in_progress'].length >= WIP_LIMIT.value) {
        targetStatus = 'todo';
        showToast(`⚠️ Đã đạt giới hạn WIP (${WIP_LIMIT.value} tasks). Task được đẩy về TO DO.`);
    }

    const properties: Record<string, any> = {
        status: targetStatus,
        is_transferred: false,
        track_progress: false,
        priority: '',
        start_date: '',
        due_date: '',
        tags: []
    };
    
    if (activeCategory.value.startsWith('project:')) {
        properties.project_id = activeCategory.value.substring(8);
    }
    
    try {
        await invoke('write_node_file', {
            vaultPath: props.vaultPath,
            relPath: relPath,
            nodeType: 'task',
            title: title,
            properties: properties,
            content: ''
        });
        
        const newTask: TaskMetadata = {
            id: relPath,
            path: relPath,
            title: title,
            content: '',
            created_at: nowStr,
            updated_at: nowStr,
            custom_fields: {},
            ...properties
        } as any;
        
        tasks.value.unshift(newTask);
        quickAddTitle.value = ''; 
    } catch(e) {
        console.error('Failed to quick add task', e);
    }
};

const onDragStart = (e: DragEvent, task: TaskMetadata) => {
    if (e.dataTransfer) {
        e.dataTransfer.setData('taskId', task.id);
        e.dataTransfer.effectAllowed = 'move';
    }
};

const onDrop = async (e: DragEvent, newStatus: string) => {
    const taskId = e.dataTransfer?.getData('taskId');
    if (!taskId) return;
    
    const task = tasks.value.find(t => t.id === taskId);
    if (!task) return;
    
    let targetStatus = newStatus;
    if (targetStatus === 'in_progress' && task.status !== 'in_progress' && tasksByStatus.value['in_progress'].length >= WIP_LIMIT.value) {
        targetStatus = 'todo';
        showToast(`⚠️ Đã đạt giới hạn WIP (${WIP_LIMIT.value} tasks). Task được đẩy về TO DO.`);
    }
    
    const columnElement = (e.currentTarget as HTMLElement);
    const columnContent = columnElement.querySelector('.column-content');
    let insertAfterTaskIdx = -1;
    
    if (targetStatus === newStatus && columnContent) {
        const cards = Array.from(columnContent.querySelectorAll('.task-card'));
        let filteredCardIndex = -1;
        for (let i = 0; i < cards.length; i++) {
            const card = cards[i] as HTMLElement;
            if (card.getAttribute('data-task-id') === taskId) continue;
            
            filteredCardIndex++;
            const rect = card.getBoundingClientRect();
            const cardMiddleY = rect.top + rect.height / 2;
            if (e.clientY > cardMiddleY) {
                insertAfterTaskIdx = filteredCardIndex;
            } else {
                break;
            }
        }
    }
    
    const tasksInCol = tasksByStatus.value[targetStatus].filter(t => t.id !== taskId);
    let newOrder = 0;
    
    if (tasksInCol.length === 0) {
        newOrder = new Date().getTime();
    } else if (insertAfterTaskIdx === -1) {
        newOrder = getOrderValueForDrop(tasksInCol[0]) - 100000;
    } else if (insertAfterTaskIdx >= tasksInCol.length - 1) {
        newOrder = getOrderValueForDrop(tasksInCol[tasksInCol.length - 1]) + 100000;
    } else {
        const prevOrder = getOrderValueForDrop(tasksInCol[insertAfterTaskIdx]);
        const nextOrder = getOrderValueForDrop(tasksInCol[insertAfterTaskIdx + 1]);
        newOrder = (prevOrder + nextOrder) / 2;
    }
    
    const prevStatus = task.status;
    const prevOrderFromCustomFields = task.custom_fields?.['order'];
    // Avoid API call if no change in status and order position (virtually)
    if (prevStatus === newStatus && Number(prevOrderFromCustomFields) === newOrder) return;
    
    if (!task.custom_fields) task.custom_fields = {};
    task.custom_fields['order'] = newOrder;
    task.status = targetStatus;
    
    // Track completed_at timestamp for archiving
    const nowStr = new Date().toISOString().split('T')[0];
    if (newStatus === 'done' && !task.completed_at) {
        task.completed_at = nowStr;
    } else if (newStatus !== 'done') {
        task.completed_at = '';
    }
    
    try {
        await invoke('write_node_file', {
            vaultPath: props.vaultPath,
            relPath: task.path,
            nodeType: 'task',
            title: task.title,
            properties: {
                ...task.custom_fields,
                title: task.title,
                status: targetStatus,
                is_transferred: task.is_transferred,
                transferred_to: task.transferred_to,
                track_progress: task.track_progress,
                priority: task.priority,
                start_date: task.start_date,
                due_date: task.due_date,
                comment: task.comment,
                source_link: task.source_link,
                tags: task.tags,
                completed_at: task.completed_at
            },
            content: task.content,
            existingPath: task.path
        });
    } catch (err) {
        logger.error("Drag update failed", err);
    }
};

const editingTask = ref<TaskMetadata | null>(null);
const editingTaskParams = ref({
    title: '',
    content: '',
    is_transferred: false,
    transferred_to: '',
    track_progress: false,
    priority: '',
    start_date: '',
    due_date: '',
    comment: '',
    tags: '',
    status: 'todo',
    completed_at: ''
});
const customFields = ref<{k: string, v: string}[]>([]);

const openEditModal = (task: TaskMetadata) => {
    editingTask.value = task;
    editingTaskParams.value = {
        title: task.title,
        content: task.content,
        is_transferred: task.is_transferred || false,
        transferred_to: task.transferred_to || '',
        track_progress: task.track_progress || false,
        priority: task.priority || '',
        start_date: task.start_date,
        due_date: task.due_date,
        comment: task.comment,
        tags: Array.isArray(task.tags) ? task.tags.join(', ') : '',
        status: task.status,
        project_id: task.project_id || '',
        completed_at: task.completed_at || ''
    };
    customFields.value = Object.entries(task.custom_fields || {})
        .filter(([k, _]) => k.trim() !== 'order')
        .map(([k, v]) => ({ k, v: String(v) }));
};

const openEditById = async (id: string) => {
    logger.info(`TaskApp: openEditById called with id: ${id}`);
    if (tasks.value.length === 0) {
        logger.info(`TaskApp: tasks empty, loading tasks...`);
        await loadTasks();
    }
    // Normalize path separators to ensure matching works cross-platform
    const normalizedId = id.replace(/\\/g, '/');
    const task = tasks.value.find(t => t.id.replace(/\\/g, '/') === normalizedId) 
              || tasks.value.find(t => t.id.replace(/\\/g, '/').endsWith(normalizedId));
    
    if (task) {
        logger.info(`TaskApp: Found task: ${task.title}, opening modal.`);
        // Switch view context based on task's project
        if (task.project_id) {
            activeCategory.value = 'project:' + task.project_id;
        } else {
            // Determine the GTD category for orphan tasks
            if (task.status === 'done') {
                activeCategory.value = 'all';
            } else if (task.is_transferred) {
                activeCategory.value = 'transferred';
            } else {
                const today = getTodayStr();
                let isToday = false;
                if (task.due_date && task.due_date <= today) isToday = true;
                else if (task.start_date && task.start_date <= today) isToday = true;
                
                let isUpcoming = false;
                if (task.start_date && task.start_date > today) isUpcoming = true;
                else if (task.due_date && task.due_date > today) isUpcoming = true;
                
                if (isToday) {
                    activeCategory.value = 'today';
                } else if (isUpcoming) {
                    activeCategory.value = 'upcoming';
                } else {
                    activeCategory.value = 'someday';
                }
            }
        }
        openEditModal(task);
    } else {
        logger.warn(`TaskApp: Task not found for id: ${id}`);
    }
};

const refresh = async () => {
    await loadTasks();
    if (activeProject.value) {
        await loadProjectResources();
    }
};

const openProjectById = (id: string) => {
    logger.info(`TaskApp: openProjectById called with id: ${id}`);
    const normalizedId = id.replace(/\\/g, '/');
    const proj = projects.value.find(p => p.id.replace(/\\/g, '/') === normalizedId) 
              || projects.value.find(p => p.id.replace(/\\/g, '/').endsWith(normalizedId));
              
    if (proj) {
        activeCategory.value = 'project:' + proj.id;
    } else {
        // If not found in loaded projects, fallback to the raw id
        activeCategory.value = 'project:' + id;
    }
};

defineExpose({ openEditById, openProjectById, refresh });

const openCreateModal = () => {
    editingTask.value = {
        id: '',
        title: '',
        status: 'todo',
        is_transferred: false,
        transferred_to: '',
        track_progress: false,
        priority: '',
        start_date: '',
        due_date: '',
        comment: '',
        source_link: '',
        tags: [],
        content: '',
        path: '',
        created_at: '',
        updated_at: '',
        completed_at: '',
        project_id: activeCategory.value.startsWith('project:') ? activeCategory.value.substring(8) : '',
        custom_fields: {},
        isNew: true
    };
    editingTaskParams.value = {
        title: '',
        content: '',
        is_transferred: false,
        transferred_to: '',
        track_progress: false,
        priority: '',
        start_date: '',
        due_date: '',
        comment: '',
        tags: '',
        status: 'todo',
        project_id: activeCategory.value.startsWith('project:') ? activeCategory.value.substring(8) : '',
        completed_at: ''
    };
    customFields.value = [];
};



const handleModalSave = async (payload: any) => {
    if (payload.status === 'in_progress' && editingTask.value && editingTask.value.status !== 'in_progress' && tasksByStatus.value['in_progress'].length >= WIP_LIMIT.value) {
        payload.status = 'todo';
        showToast(`⚠️ Đã đạt giới hạn WIP (${WIP_LIMIT.value} tasks). Task được đẩy về TO DO.`);
    }

    editingTaskParams.value = payload;
    if (editingTask.value) {
        if (editingTask.value.status !== payload.status) {
            if (payload.status === 'done') {
                editingTask.value.completed_at = new Date().toISOString().split('T')[0];
            } else {
                editingTask.value.completed_at = '';
            }
        }
        editingTask.value.status = payload.status;
    }
    await saveTask();
    editingTask.value = null;
};

const closeEditModal = () => {
    editingTask.value = null;
};

const toastMessage = ref('');
let toastTimeout: any = null;

const showToast = (msg: string) => {
    toastMessage.value = msg;
    if (toastTimeout) clearTimeout(toastTimeout);
    toastTimeout = setTimeout(() => {
        toastMessage.value = '';
    }, 4000);
};

const saveTask = async () => {
    if (!editingTask.value) return;
    try {
        const tagArray = editingTaskParams.value.tags.split(',').map(t => t.trim()).filter(t => t !== '');
        const updatedCustomFields: Record<string, string> = {};
        
        customFields.value.forEach(field => {
            if (field.k.trim()) {
                updatedCustomFields[field.k.trim()] = field.v;
            }
        });
        
        if (editingTask.value.custom_fields && editingTask.value.custom_fields['order'] !== undefined) {
             updatedCustomFields['order'] = editingTask.value.custom_fields['order'] as string;
        }
        
        const properties = {
            ...updatedCustomFields,
            status: editingTask.value.status || 'todo',
            is_transferred: editingTaskParams.value.is_transferred,
            transferred_to: editingTaskParams.value.transferred_to,
            track_progress: editingTaskParams.value.track_progress,
            priority: editingTaskParams.value.priority,
            start_date: editingTaskParams.value.start_date,
            due_date: editingTaskParams.value.due_date,
            comment: editingTaskParams.value.comment,
            source_link: editingTask.value.source_link || '',
            tags: tagArray,
            project_id: editingTaskParams.value.project_id,
            completed_at: editingTask.value.completed_at || ''
        };

        if (editingTask.value.isNew) {
            const relPath = `Tasks/${crypto.randomUUID()}.md`;
            
            await invoke('write_node_file', {
                vaultPath: props.vaultPath,
                relPath: relPath,
                nodeType: 'task',
                title: editingTaskParams.value.title || 'Untitled',
                properties: properties,
                content: editingTaskParams.value.content
            });
            
            const nowStr = new Date().toISOString().replace('T', ' ').substring(0, 19);
            const newTask: TaskMetadata = {
                id: relPath,
                path: relPath,
                title: editingTaskParams.value.title || 'Untitled',
                content: editingTaskParams.value.content,
                created_at: nowStr,
                updated_at: nowStr,
                custom_fields: updatedCustomFields,
                ...properties
            } as any;
            tasks.value.unshift(newTask);
        } else if (editingTask.value.path) {
            await invoke('write_node_file', {
                vaultPath: props.vaultPath,
                relPath: editingTask.value.path,
                nodeType: 'task',
                title: editingTaskParams.value.title,
                properties: properties,
                content: editingTaskParams.value.content,
                existingPath: editingTask.value.path
            });
            
            editingTask.value.title = editingTaskParams.value.title;
            editingTask.value.content = editingTaskParams.value.content;
            editingTask.value.is_transferred = editingTaskParams.value.is_transferred;
            editingTask.value.transferred_to = editingTaskParams.value.transferred_to;
            editingTask.value.track_progress = editingTaskParams.value.track_progress;
            editingTask.value.priority = editingTaskParams.value.priority;
            editingTask.value.start_date = editingTaskParams.value.start_date;
            editingTask.value.due_date = editingTaskParams.value.due_date;
            editingTask.value.comment = editingTaskParams.value.comment;
            editingTask.value.tags = tagArray;
            editingTask.value.project_id = editingTaskParams.value.project_id;
            editingTask.value.custom_fields = updatedCustomFields;
        }
        
        closeEditModal();
    } catch (e) {
        logger.error("Failed to update/create task", e);
    }
};

const mapNodeToTask = (node: any): TaskMetadata => {
    const rawTags = node.properties?.tags;
    const tagsArray = Array.isArray(rawTags) ? rawTags : (typeof rawTags === 'string' && rawTags.trim() !== '' ? [rawTags] : []);

    return {
        id: node.id,
        path: node.id, // ID is the relative path in the node system
        title: node.title,
        content: node.content,
        created_at: node.created_at,
        updated_at: node.updated_at,
        status: node.properties.status || 'todo',
        is_transferred: node.properties.is_transferred || false,
        transferred_to: node.properties.transferred_to || '',
        track_progress: node.properties.track_progress || false,
        priority: node.properties.priority || '',
        start_date: node.properties.start_date || '',
        due_date: node.properties.due_date || '',
        comment: node.properties.comment || '',
        source_link: node.properties.source_link || '',
        tags: tagsArray,
        project_id: node.properties.project_id || '',
        completed_at: node.properties.completed_at || '',
        custom_fields: node.properties || {}
    };
};

const loadTasks = async () => {
    if (!props.vaultPath) return;
    try {
        const archiveDays = taskArchiveDays.value;
        await invoke('archive_done_nodes', { vaultPath: props.vaultPath, nodeType: 'task', days: archiveDays });
        const nodes = await invoke<any[]>('get_nodes', { nodeType: 'task' });
        tasks.value = nodes.map(mapNodeToTask);
        
        const projNodes = await invoke<any[]>('get_nodes', { nodeType: 'project' });
        projects.value = projNodes.map(node => ({
            id: node.id,
            path: node.id,
            title: node.title,
            status: node.properties.status || 'active',
            start_date: node.properties.start_date || '',
            due_date: node.properties.due_date || '',
            color: node.properties.color || '',
            tags: node.properties.tags || [],
            custom_fields: (({ status, start_date, due_date, color, tags, ...rest }) => rest)(node.properties),
            content: node.content,
            created_at: node.created_at,
            updated_at: node.updated_at
        }));
        
        await loadFinanceConfig();
    } catch (e) {
        logger.error("Failed to load tasks", e);
    }
};

const loadFinanceConfig = async () => {
    try {
        const configs: any[] = await invoke('get_nodes', { nodeType: 'finance_config' });
        if (configs.length > 0) {
            const configNode = configs[0];
            if (configNode.properties) {
                if (configNode.properties.incomeCategories) {
                    incomeCategories.value = configNode.properties.incomeCategories;
                }
                if (configNode.properties.expenseCategories) {
                    expenseCategories.value = configNode.properties.expenseCategories;
                }
                if (configNode.properties.accounts) {
                    accounts.value = configNode.properties.accounts;
                }
            }
        }
    } catch (e) {
        logger.error('Failed to load finance config in TaskApp', e);
    }
};

const saveFinanceTransaction = async (tx: Transaction) => {
    const d = new Date(tx.date);
    const mm = (d.getMonth() + 1).toString().padStart(2, '0');
    const yyyy = d.getFullYear();
    const expectedId = `Finance/${yyyy}-${mm}.json`;
    
    try {
        let nodeProps: any = { transactions: [] };
        try {
            const existingNodes = await invoke<any[]>('get_nodes', { nodeType: 'finance_month' });
            const targetNode = existingNodes.find((n: any) => n.id === expectedId);
            if (targetNode && targetNode.properties) {
                nodeProps = targetNode.properties;
            }
        } catch(e) {}
        
        if (!nodeProps.transactions) nodeProps.transactions = [];
        
        const existingIdx = nodeProps.transactions.findIndex((t: Transaction) => t.id === tx.id);
        if (existingIdx >= 0) {
            nodeProps.transactions[existingIdx] = tx;
        } else {
            nodeProps.transactions.push(tx);
        }
        
        await invoke('write_node_file', {
            vaultPath: props.vaultPath,
            relPath: expectedId,
            title: `Tháng ${mm}/${yyyy}`,
            nodeType: 'finance_month',
            properties: nodeProps,
            content: ''
        });
        
        showTxModal.value = false;
        if (activeProject.value) {
            recalculateProjectSpent(activeProject.value);
        }
    } catch (e) {
        logger.error('Failed to save finance transaction from Task App', e);
    }
};

const handleCreateProjectClick = () => {
    newProjectDraft.value = {
        title: '',
        content: '',
        due_date: '',
        start_date: '',
        status: 'active',
        isNew: true
    };
    showProjectEditModal.value = true;
};

const handleProjectSave = async (updatedProject: any) => {
    try {
        if (newProjectDraft.value) {
            // Create new project
            if (!updatedProject.title.trim()) updatedProject.title = 'Untitled Project';
            const relPath = `Projects/${crypto.randomUUID()}.md`;
            await invoke('write_node_file', {
                vaultPath: props.vaultPath,
                relPath: relPath,
                nodeType: 'project',
                title: updatedProject.title,
                properties: {
                    status: updatedProject.status,
                    start_date: updatedProject.start_date,
                    due_date: updatedProject.due_date,
                    tags: updatedProject.tags,
                    color: '',
                    ...(updatedProject.custom_fields || {})
                },
                content: updatedProject.content
            });
            
            showProjectEditModal.value = false;
            newProjectDraft.value = null;
            await loadTasks();
            
            // Open the newly created project
            const newProj = projects.value.find(p => p.path === relPath);
            if (newProj) {
                activeCategory.value = 'project:' + newProj.id;
            }
        } else if (activeProject.value) {
            // Update existing project
            await invoke('write_node_file', {
                vaultPath: props.vaultPath,
                relPath: activeProject.value.path,
                nodeType: 'project',
                title: updatedProject.title,
                properties: {
                    status: updatedProject.status,
                    start_date: updatedProject.start_date,
                    due_date: updatedProject.due_date,
                    tags: updatedProject.tags,
                    color: activeProject.value.color || '',
                    ...(updatedProject.custom_fields || {})
                },
                content: updatedProject.content,
                existingPath: activeProject.value.path
            });
            showProjectEditModal.value = false;
            await loadTasks();
        }
    } catch (e) {
        logger.error("Failed to save project", e);
    }
};

const openLinkResourcePicker = async () => {
    try {
        isLinkingResource.value = true;
        const resultNotes = await invoke<any[]>('get_nodes', { nodeType: 'note' });
        const resultWhiteboards = await invoke<any[]>('scan_whiteboards', { vaultPath: props.vaultPath });
        resultWhiteboards.forEach(w => w.node_type = 'whiteboard');
        const resultFiles = await invoke<any[]>('get_nodes', { nodeType: 'file' });
        const allResources = [...resultNotes, ...resultWhiteboards, ...resultFiles];
        
        const linkedResourceIds = new Set(linkedResources.value.map(n => n.id));
        allNotesForPicker.value = allResources.filter(n => !linkedResourceIds.has(n.id));
        showEmbedPicker.value = true;
    } catch(e) {
        logger.error("Failed to load resources for picker", e);
    } finally {
        isLinkingResource.value = false;
    }
};

const createNewResourceNote = async () => {
    if (!props.vaultPath || !activeProject.value) return;
    try {
        isLinkingResource.value = true;
        // Create new node file
        const newPath = await invoke<string>('create_node_file', { 
            vaultPath: props.vaultPath, 
            directory: 'Notes', 
            nodeType: 'note' 
        });
        
        // Read it back to get default properties
        const node = await invoke<any>('get_node', { id: newPath });
        if (node) {
            const propsObj = node.properties || {};
            const projectsArray = Array.isArray(propsObj.linked_projects) ? propsObj.linked_projects : [];
            const projectLink = `[${activeProject.value.title}](synabit://project/${activeProject.value.id})`;
            
            if (!projectsArray.includes(projectLink)) {
                projectsArray.push(projectLink);
                propsObj.linked_projects = projectsArray;
                
                await invoke('write_node_file', {
                    vaultPath: props.vaultPath,
                    relPath: node.id,
                    title: node.title,
                    nodeType: 'note',
                    properties: propsObj,
                    content: node.content
                });
            }
        }
        
        // Reload linked resources
        await loadProjectResources();
        emit('open-node', newPath, 'note'); // Optionally open it immediately
    } catch(e) {
        logger.error("Failed to create resource note", e);
    } finally {
        isLinkingResource.value = false;
    }
};

const createNewResourceWhiteboard = async () => {
    if (!props.vaultPath || !activeProject.value) return;
    try {
        isLinkingResource.value = true;
        
        const projectLink = `[${activeProject.value.title}](synabit://project/${activeProject.value.id})`;
        const title = 'New Whiteboard';
        const data = {
            title: title,
            type: 'whiteboard',
            metadata: {
                linked_projects: [projectLink]
            },
            tags: [],
            created_at: new Date().toISOString(),
            viewport: { x: 0, y: 0, zoom: 1 },
            nodes: [],
            edges: [],
        };
        const content = JSON.stringify(data, null, 2);
        
        const meta = await invoke<any>('create_whiteboard', {
            vaultPath: props.vaultPath,
            title: title,
            tags: [],
            content: content
        });
        
        // Scan the new file so that its graph edges (links to project) are indexed
        await invoke('scan_specific_nodes', {
            vaultPath: props.vaultPath,
            paths: [meta.path]
        });
        
        // Reload linked resources
        await loadProjectResources();
        emit('open-node', meta.path, 'whiteboard'); // Optionally open it immediately
    } catch(e) {
        logger.error("Failed to create resource whiteboard", e);
    } finally {
        isLinkingResource.value = false;
    }
};

const unlinkResource = async (node: any) => {
    if (!activeProject.value) return;
    
    const confirmed = await ask(`"${node.title || 'This resource'}" will no longer be linked to this project.`, {
        title: 'Unlink resource?',
        kind: 'warning',
        okLabel: 'Unlink',
        cancelLabel: 'Cancel'
    });
    if (!confirmed) return;

    try {
        const projectLink = `[${activeProject.value.title}](synabit://project/${activeProject.value.id})`;
        
        if (node.node_type === 'whiteboard' && node.id.endsWith('.json')) {
            const rawContent = await invoke<string>('read_whiteboard', {
                vaultPath: props.vaultPath,
                path: node.id
            });
            const data = JSON.parse(rawContent);
            if (data.metadata?.linked_projects && Array.isArray(data.metadata.linked_projects)) {
                data.metadata.linked_projects = data.metadata.linked_projects.filter((l: string) => l !== projectLink);
                
                await invoke('update_whiteboard', {
                    vaultPath: props.vaultPath,
                    path: node.id,
                    title: data.title,
                    tags: data.tags || [],
                    content: JSON.stringify(data, null, 2)
                });
                
                await invoke('scan_specific_nodes', {
                    vaultPath: props.vaultPath,
                    paths: [node.id]
                });
            }
        } else if (node.node_type === 'file') {
            const fetchedNode = await invoke<any>('get_node', { id: node.id });
            if (fetchedNode) {
                const propsObj = fetchedNode.properties || {};
                if (Array.isArray(propsObj.linked_projects)) {
                    propsObj.linked_projects = propsObj.linked_projects.filter((l: string) => l !== projectLink);
                    await invoke('update_node_properties', {
                        id: fetchedNode.id,
                        properties: propsObj
                    });
                }
            }
        } else {
            // For notes, markdown-based nodes, and corrupted whiteboard .md files
            const fetchedNode = await invoke<any>('get_node', { id: node.id });
            if (fetchedNode) {
                const propsObj = fetchedNode.properties || {};
                if (Array.isArray(propsObj.linked_projects)) {
                    propsObj.linked_projects = propsObj.linked_projects.filter((l: string) => l !== projectLink);
                    
                    await invoke('write_node_file', {
                        vaultPath: props.vaultPath,
                        relPath: fetchedNode.id,
                        title: fetchedNode.title,
                        nodeType: fetchedNode.node_type,
                        properties: propsObj,
                        content: fetchedNode.content
                    });
                }
            }
        }
        
        await loadProjectResources();
    } catch (e) {
        logger.error('Failed to unlink resource', e);
    }
};

const handleEmbedResource = async (node: any) => {
    showEmbedPicker.value = false;
    if (!activeProject.value) return;
    try {
        isLinkingResource.value = true;
        const projectLink = `[${activeProject.value.title}](synabit://project/${activeProject.value.id})`;
        
        if (node.node_type === 'whiteboard' && node.id.endsWith('.json')) {
            const rawContent = await invoke<string>('read_whiteboard', {
                vaultPath: props.vaultPath,
                path: node.id
            });
            const data = JSON.parse(rawContent);
            if (!data.metadata) data.metadata = {};
            
            const projectsArray = Array.isArray(data.metadata.linked_projects) ? data.metadata.linked_projects : [];
            if (!projectsArray.includes(projectLink)) {
                projectsArray.push(projectLink);
                data.metadata.linked_projects = projectsArray;
                
                await invoke('update_whiteboard', {
                    vaultPath: props.vaultPath,
                    path: node.id,
                    title: data.title,
                    tags: data.tags || [],
                    content: JSON.stringify(data, null, 2)
                });
                
                await invoke('scan_specific_nodes', {
                    vaultPath: props.vaultPath,
                    paths: [node.id]
                });
            }
        } else if (node.node_type === 'file') {
            const fullNode = await invoke<any>('get_node', { id: node.id });
            if (fullNode) {
                const propsObj = fullNode.properties || {};
                const projectsArray = Array.isArray(propsObj.linked_projects) ? propsObj.linked_projects : [];
                
                if (!projectsArray.includes(projectLink)) {
                    projectsArray.push(projectLink);
                    propsObj.linked_projects = projectsArray;
                    
                    await invoke('update_node_properties', {
                        id: fullNode.id,
                        properties: propsObj
                    });
                }
            }
        } else {
            // Since we already have the node from the modal, we could use it directly
            // but we still call get_node to get fresh properties and content
            const fullNode = await invoke<any>('get_node', { id: node.id });
            if (fullNode) {
                const propsObj = fullNode.properties || {};
                const projectsArray = Array.isArray(propsObj.linked_projects) ? propsObj.linked_projects : [];
                
                if (!projectsArray.includes(projectLink)) {
                    projectsArray.push(projectLink);
                    propsObj.linked_projects = projectsArray;
                    
                    await invoke('write_node_file', {
                        vaultPath: props.vaultPath,
                        relPath: node.id,
                        title: fullNode.title,
                        nodeType: fullNode.node_type || 'note',
                        properties: propsObj,
                        content: fullNode.content
                    });
                }
            }
        }
        await loadProjectResources();
    } catch (e) {
        logger.error("Failed to link resource", e);
    } finally {
        isLinkingResource.value = false;
    }
};

const deleteProject = async () => {
    if (!activeProject.value) return;
    let isConfirmed = false;
    try {
        isConfirmed = await ask('This action cannot be undone. The project will be permanently deleted. Tasks under it will NOT be deleted.', { 
            title: 'Delete this project?', 
            kind: 'warning',
            okLabel: 'Delete',
            cancelLabel: 'Cancel'
        });
    } catch (e) {
        logger.warn("Tauri confirm failed, falling back to window.confirm", e);
        isConfirmed = window.confirm('Delete this project?');
    }
    
    if (!isConfirmed) return;
    
    try {
        await invoke('delete_node_file', { vaultPath: props.vaultPath, relPath: activeProject.value.path });
        showProjectEditModal.value = false;
        activeCategory.value = 'all';
        await loadTasks();
    } catch (e) {
        logger.error("Failed to delete project", e);
    }
};

const isOverdue = (task: TaskMetadata) => {
    if (task.status === 'done') return false;
    if (!task.due_date) return false;
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    const dueDate = new Date(task.due_date);
    dueDate.setHours(0, 0, 0, 0);
    return dueDate < today;
};

const getTransferredName = (rawStr: string | null | undefined): string => {
    if (!rawStr) return 'Unknown';
    const match = rawStr.match(/^\[(.*?)\]\(synabit:\/\/person\/.*?\)$/);
    return match ? `@${match[1]}` : rawStr;
};

const isLinkedPerson = (rawStr: string | null | undefined): boolean => {
    return /^\[(.*?)\]\(synabit:\/\/person\/.*?\)$/.test(rawStr || '');
};

const openPerson = (transferredTo: string) => {
    if (!transferredTo) return;
    const match = transferredTo.match(/^\[(.*?)\]\(synabit:\/\/person\/(.*?)\)$/);
    if (match && match[2]) {
        emit('open-node', match[2], 'person');
    }
};

const toggleTaskStatus = async (task: TaskMetadata) => {
    const newStatus = task.status === 'done' ? 'todo' : 'done';
    const nowStr = new Date().toISOString().split('T')[0];
    const newCompletedAt = newStatus === 'done' ? nowStr : '';
    
    try {
        const properties = {
            ...task.custom_fields,
            status: newStatus,
            is_transferred: task.is_transferred,
            transferred_to: task.transferred_to,
            track_progress: task.track_progress,
            priority: task.priority,
            start_date: task.start_date,
            due_date: task.due_date,
            comment: task.comment,
            source_link: task.source_link,
            tags: task.tags,
            completed_at: newCompletedAt
        };
        await invoke('write_node_file', {
            vaultPath: props.vaultPath,
            relPath: task.path,
            nodeType: 'task',
            title: task.title,
            properties: properties,
            content: task.content,
            existingPath: task.path
        });
        task.status = newStatus;
        task.completed_at = newCompletedAt;
    } catch (e) {
        logger.error("Failed to update task", e);
    }
};

const deleteTask = async (task: TaskMetadata) => {
    let isConfirmed = false;
    try {
        isConfirmed = await ask('This action cannot be undone. The task will be permanently deleted.', { 
            title: 'Delete this task?', 
            kind: 'warning',
            okLabel: 'Delete',
            cancelLabel: 'Cancel'
        });
    } catch (e) {
        logger.warn("Tauri confirm failed, falling back to window.confirm", e);
        isConfirmed = window.confirm('Delete this task?');
    }
    
    if (!isConfirmed) return;
    
    try {
        await invoke('delete_node_file', { vaultPath: props.vaultPath, relPath: task.path });
        const idx = tasks.value.findIndex(t => t.id === task.id);
        if (idx !== -1) tasks.value.splice(idx, 1);
    } catch (e) {
        logger.error("Failed to delete task", e);
    }
};

const handleModalDelete = async () => {
    if (!editingTask.value || editingTask.value.isNew) {
        editingTask.value = null;
        return;
    }
    const currentId = editingTask.value.id;
    await deleteTask(editingTask.value);
    const stillExists = tasks.value.find(t => t.id === currentId);
    if (!stillExists) {
        editingTask.value = null;
    }
};


onMounted(() => {
    loadTasks();
});

watch(() => props.vaultPath, () => {
    loadTasks();
});
</script>

<template>
  <div class="h-full flex bg-[#fdfdfc] dark:bg-[#242424] w-full overflow-hidden">
      <!-- SIDEBAR -->
      <div class="w-64 border-r border-[#e6e6e6] dark:border-[#2c2c2c] bg-gray-50/50 dark:bg-[#1a1a1a]/50 flex flex-col pt-10 shrink-0 hidden md:flex">
          <div class="flex flex-col px-3 space-y-1">
              <button @click="activeCategory = 'all'" class="flex items-center justify-between px-3 py-2 rounded-lg transition-colors cursor-pointer" :class="activeCategory === 'all' ? 'bg-white dark:bg-[#2c2c2c] text-black dark:text-white shadow-sm font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#242424]'">
                  <div class="flex items-center"><Inbox class="w-4 h-4 mr-3" />All Tasks</div>
                  <span class="text-xs bg-gray-200 dark:bg-[#333] px-1.5 py-0.5 rounded-full text-gray-600 dark:text-gray-400" v-if="categoryCounts.all">{{ categoryCounts.all }}</span>
              </button>
              <button @click="activeCategory = 'today'" class="flex items-center justify-between px-3 py-2 rounded-lg transition-colors cursor-pointer" :class="activeCategory === 'today' ? 'bg-white dark:bg-[#2c2c2c] text-blue-600 dark:text-blue-400 shadow-sm font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#242424]'">
                  <div class="flex items-center"><Sun class="w-4 h-4 mr-3" />Today</div>
                  <span class="text-xs bg-blue-100 dark:bg-blue-900/30 px-1.5 py-0.5 rounded-full text-blue-600 dark:text-blue-400" v-if="categoryCounts.today">{{ categoryCounts.today }}</span>
              </button>
              <button @click="activeCategory = 'upcoming'" class="flex items-center justify-between px-3 py-2 rounded-lg transition-colors cursor-pointer" :class="activeCategory === 'upcoming' ? 'bg-white dark:bg-[#2c2c2c] text-red-600 dark:text-red-400 shadow-sm font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#242424]'">
                  <div class="flex items-center"><Calendar class="w-4 h-4 mr-3" />Upcoming</div>
                  <span class="text-xs bg-red-100 dark:bg-red-900/30 px-1.5 py-0.5 rounded-full text-red-600 dark:text-red-400" v-if="categoryCounts.upcoming">{{ categoryCounts.upcoming }}</span>
              </button>
              <button @click="activeCategory = 'someday'" class="flex items-center justify-between px-3 py-2 rounded-lg transition-colors cursor-pointer" :class="activeCategory === 'someday' ? 'bg-white dark:bg-[#2c2c2c] text-yellow-600 dark:text-yellow-400 shadow-sm font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#242424]'">
                  <div class="flex items-center"><Coffee class="w-4 h-4 mr-3" />Someday</div>
                  <span class="text-xs bg-yellow-100 dark:bg-yellow-900/30 px-1.5 py-0.5 rounded-full text-yellow-600 dark:text-yellow-400" v-if="categoryCounts.someday">{{ categoryCounts.someday }}</span>
              </button>
              <button @click="activeCategory = 'transferred'" class="flex items-center justify-between px-3 py-2 rounded-lg transition-colors cursor-pointer" :class="activeCategory === 'transferred' ? 'bg-white dark:bg-[#2c2c2c] text-slate-600 dark:text-slate-400 shadow-sm font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#242424]'">
                  <div class="flex items-center"><Send class="w-4 h-4 mr-3" />Transferred</div>
                  <span class="text-xs bg-slate-200 dark:bg-slate-700 px-1.5 py-0.5 rounded-full text-slate-600 dark:text-slate-400" v-if="categoryCounts.transferred">{{ categoryCounts.transferred }}</span>
              </button>
              
              <div class="pt-4 pb-1 px-3 flex items-center justify-between group">
                  <span class="text-[10px] font-bold text-gray-400 uppercase tracking-wider">Projects</span>
                  <button @click="handleCreateProjectClick" class="text-gray-400 hover:text-indigo-500 opacity-0 group-hover:opacity-100 transition-opacity" title="New Project">
                      <Plus class="w-3.5 h-3.5"/>
                  </button>
              </div>
              <button v-for="proj in projects" :key="proj.id" @click="activeCategory = 'project:' + proj.id" class="flex items-center justify-between px-3 py-2 rounded-lg transition-colors cursor-pointer group" :class="activeCategory === 'project:' + proj.id ? 'bg-white dark:bg-[#2c2c2c] text-indigo-600 dark:text-indigo-400 shadow-sm font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#242424]'">
                  <div class="flex items-center truncate">
                      <svg class="w-4 h-4 mr-3 shrink-0" :class="activeCategory === 'project:' + proj.id ? 'text-indigo-500' : 'text-gray-400 group-hover:text-indigo-400'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg>
                      <span class="truncate">{{ proj.title }}</span>
                  </div>
              </button>
          </div>
      </div>

      <!-- MAIN CONTENT -->
      <div class="flex-1 flex flex-col h-full overflow-hidden">
          <!-- Header -->
          <div class="px-4 md:px-8 pt-12 md:pt-10 pb-2 md:pb-4 shrink-0 border-b border-transparent">
              <div class="flex items-center justify-between mb-4 md:mb-6">
                  <div class="flex items-center gap-3">
                      <NavButtons />
                      <button @click="isMobileSidebarOpen = true" class="md:hidden p-1 -ml-1 text-gray-500 hover:text-gray-800 dark:hover:text-gray-200 cursor-pointer">
                          <MenuIcon class="w-6 h-6" />
                      </button>
                      <h1 class="text-2xl md:text-3xl font-semibold text-[#1c1c1e] dark:text-[#f4f4f5] tracking-tight capitalize truncate max-w-[200px] sm:max-w-md lg:max-w-xl">
                          {{ activeProject ? activeProject.title : (activeCategory === 'all' ? 'All Tasks' : activeCategory) }}
                      </h1>
                  </div>
                  <div class="flex items-center gap-3">
                      <!-- New Task Button -->
                      <button 
                          @click="openCreateModal"
                          class="hidden md:flex items-center px-3 py-1.5 bg-blue-500 hover:bg-blue-600 text-white rounded-lg shadow-[0_2px_10px_rgba(59,130,246,0.3)] hover:shadow-[0_4px_14px_rgba(59,130,246,0.4)] transition-all cursor-pointer text-sm font-medium"
                      >
                          <Plus class="w-4 h-4 mr-1.5"/>
                          New
                      </button>

                      <div class="flex bg-gray-100 dark:bg-[#1a1a1a] p-1 rounded-xl">
                          <button @click="viewMode = 'list'" class="p-1.5 rounded-lg transition-colors cursor-pointer" :class="viewMode === 'list' ? 'bg-white dark:bg-[#2c2c2c] shadow-sm text-black dark:text-white' : 'text-gray-500 hover:text-black dark:hover:text-white'" title="List View">
                              <List class="w-4 h-4"/>
                          </button>
                          <button @click="viewMode = 'board'" class="p-1.5 rounded-lg transition-colors cursor-pointer" :class="viewMode === 'board' ? 'bg-white dark:bg-[#2c2c2c] shadow-sm text-black dark:text-white' : 'text-gray-500 hover:text-black dark:hover:text-white'" title="Board View">
                              <Trello class="w-4 h-4"/>
                          </button>
                          <button @click="viewMode = 'table'" class="p-1.5 rounded-lg transition-colors cursor-pointer" :class="viewMode === 'table' ? 'bg-white dark:bg-[#2c2c2c] shadow-sm text-black dark:text-white' : 'text-gray-500 hover:text-black dark:hover:text-white'" title="Table View">
                              <Table2 class="w-4 h-4"/>
                          </button>
                      </div>
                  </div>
              </div>

              <!-- Bar (Search & Properties) -->
              <div class="mt-4 flex flex-row items-center gap-3">
                  <div class="relative w-full sm:max-w-xs group">
                      <div class="absolute inset-y-0 left-0 pl-3.5 flex items-center pointer-events-none">
                          <Search class="h-4 w-4 text-gray-400 group-focus-within:text-blue-500 transition-colors" />
                      </div>
                      <input 
                          v-model="searchQuery" 
                          type="text" 
                          class="block w-full pl-10 pr-3 py-2 border border-gray-200 dark:border-[#2c2c2c] rounded-full leading-5 bg-white dark:bg-[#1e1e1e] text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-black/5 dark:focus:ring-white/10 sm:text-sm transition-all shadow-[0_2px_8px_rgba(0,0,0,0.02)]" 
                          placeholder="Search tasks or properties..." 
                      />
                      <button v-if="searchQuery" @click="searchQuery = ''" class="absolute inset-y-0 right-0 pr-3 flex items-center cursor-pointer z-10">
                          <X class="h-4 w-4 text-gray-400 hover:text-gray-600 transition-colors" />
                      </button>
                      
                      <!-- Advanced Search Tooltip/Hints -->
                      <div class="absolute top-full left-0 mt-2 p-3 bg-white dark:bg-[#1e1e1e] border border-gray-100 dark:border-[#2c2c2c] rounded-xl shadow-[0_10px_30px_rgba(0,0,0,0.1)] dark:shadow-[0_10px_30px_rgba(0,0,0,0.5)] z-20 w-72 opacity-0 invisible group-focus-within:opacity-100 group-focus-within:visible transition-all">
                          <div class="flex items-center text-[10px] font-semibold text-gray-400 dark:text-gray-500 mb-2.5 uppercase tracking-wider">
                              <Search class="w-3.5 h-3.5 mr-1" /> Quick Syntax
                          </div>
                          <div class="space-y-2 text-[11px] text-gray-600 dark:text-gray-400">
                              <div class="flex items-center gap-2"><span class="font-mono bg-blue-50/80 dark:bg-blue-900/30 px-1 border border-blue-100 dark:border-blue-900/50 rounded text-blue-600 dark:text-blue-400 font-medium whitespace-nowrap">is:transferred</span>, <span class="font-mono bg-blue-50/80 dark:bg-blue-900/30 px-1 border border-blue-100 dark:border-blue-900/50 rounded text-blue-600 dark:text-blue-400 font-medium whitespace-nowrap">is:tracked</span></div>
                              <div class="flex items-center gap-2"><span class="font-mono bg-purple-50/80 dark:bg-purple-900/30 px-1 border border-purple-100 dark:border-purple-900/50 rounded text-purple-600 dark:text-purple-400 font-medium whitespace-nowrap">p:3</span> hay <span class="font-mono bg-indigo-50/80 dark:bg-indigo-900/30 px-1 border border-indigo-100 dark:border-indigo-900/50 rounded text-indigo-600 dark:text-indigo-400 font-medium whitespace-nowrap">status:todo</span></div>
                              <div class="flex items-center gap-2"><span class="font-mono bg-emerald-50/80 dark:bg-emerald-900/30 px-1 border border-emerald-100 dark:border-emerald-900/50 rounded text-emerald-600 dark:text-emerald-400 font-medium whitespace-nowrap">@name</span> <span class="text-gray-400">(Trạng thái Assign)</span></div>
                              <div class="flex items-center gap-2"><span class="font-mono bg-amber-50/80 dark:bg-amber-900/30 px-1 border border-amber-100 dark:border-amber-900/50 rounded text-amber-600 dark:text-amber-400 font-medium whitespace-nowrap">#tag</span> hoặc <span class="font-mono bg-amber-50/80 dark:bg-amber-900/30 px-1 border border-amber-100 dark:border-amber-900/50 rounded text-amber-600 dark:text-amber-400 font-medium whitespace-nowrap">tag:urgent</span></div>
                              <div class="flex items-center gap-2"><span class="font-mono bg-slate-100 dark:bg-slate-800/50 px-1 border border-slate-200 dark:border-[#333] rounded text-slate-600 dark:text-slate-300 font-medium whitespace-nowrap">prop:cost=100</span> <span class="text-gray-400 px-1">(Custom Prop)</span></div>
                          </div>
                      </div>
                  </div>
              </div>
          </div>

      <!-- Main Content -->
      <div class="flex-1 overflow-y-auto px-4 md:px-8 pb-16">
          
          <!-- Project Header & Navigation -->
          <div v-if="activeProject" class="mb-6 mt-2 space-y-6 relative group">
              <!-- Tabs Navigation -->
              <div class="flex items-center justify-between border-b border-gray-200 dark:border-gray-800 px-2">
                  <div class="flex items-center gap-6">
                      <button @click="activeProjectTab = 'overview'" class="pb-3 text-sm font-medium transition-colors relative cursor-pointer" :class="activeProjectTab === 'overview' ? 'text-black dark:text-white' : 'text-gray-500 hover:text-gray-800 dark:hover:text-gray-300'">
                          Overview
                          <div v-if="activeProjectTab === 'overview'" class="absolute bottom-0 left-0 w-full h-0.5 bg-black dark:bg-white rounded-t-full"></div>
                      </button>
                      <button @click="activeProjectTab = 'tasks'" class="pb-3 text-sm font-medium transition-colors relative cursor-pointer" :class="activeProjectTab === 'tasks' ? 'text-black dark:text-white' : 'text-gray-500 hover:text-gray-800 dark:hover:text-gray-300'">
                          Tasks
                          <div v-if="activeProjectTab === 'tasks'" class="absolute bottom-0 left-0 w-full h-0.5 bg-black dark:bg-white rounded-t-full"></div>
                      </button>
                      <button @click="activeProjectTab = 'resources'" class="pb-3 text-sm font-medium transition-colors relative cursor-pointer" :class="activeProjectTab === 'resources' ? 'text-black dark:text-white' : 'text-gray-500 hover:text-gray-800 dark:hover:text-gray-300'">
                          Resources
                          <div v-if="activeProjectTab === 'resources'" class="absolute bottom-0 left-0 w-full h-0.5 bg-black dark:bg-white rounded-t-full"></div>
                      </button>
                  </div>
                  <button @click="showProjectEditModal = true" class="pb-3 text-gray-400 hover:text-indigo-500 transition-colors cursor-pointer" title="Project Settings">
                      <Settings class="w-4 h-4" />
                  </button>
              </div>

              <div v-if="activeProjectTab === 'overview'" class="space-y-6 animate-in fade-in slide-in-from-bottom-2 duration-300">
                  
                  <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
                      <!-- Project Description Card -->
                      <div class="md:col-span-1 bg-white dark:bg-[#1a1a1a] rounded-2xl p-5 border border-gray-100 dark:border-[#2c2c2c] shadow-sm flex flex-col">
                          <h3 class="text-sm font-semibold text-gray-900 dark:text-gray-100 mb-3">Project Description</h3>
                          <div v-if="activeProject.content" class="text-sm text-gray-600 dark:text-gray-400 prose prose-sm dark:prose-invert max-w-none mb-4 line-clamp-3">
                              <div v-html="activeProject.content"></div>
                          </div>
                          <div v-else class="text-sm text-gray-400 italic mb-4">No description provided. Click to add.</div>
                          
                          <div class="mt-auto space-y-3 pt-3 border-t border-gray-50 dark:border-[#2c2c2c]">
                              <div class="flex items-center justify-between">
                                  <div class="text-[11px] font-medium text-gray-500 uppercase tracking-wider">Status</div>
                                  <span class="inline-flex items-center px-2 py-0.5 rounded text-[10px] font-medium capitalize" 
                                      :class="{
                                          'bg-blue-50 text-blue-700 dark:bg-blue-900/30 dark:text-blue-400': activeProject.status === 'active',
                                          'bg-green-50 text-green-700 dark:bg-green-900/30 dark:text-green-400': activeProject.status === 'completed',
                                          'bg-yellow-50 text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-400': activeProject.status === 'on_hold'
                                      }">
                                      {{ activeProject.status.replace('_', ' ') }}
                                  </span>
                              </div>
                              <div v-if="activeProject.tags?.length > 0" class="flex items-center justify-between">
                                  <div class="text-[11px] font-medium text-gray-500 uppercase tracking-wider">Tags</div>
                                  <div class="flex flex-wrap items-center gap-1 justify-end">
                                      <span v-for="tag in activeProject.tags.slice(0,3)" :key="tag" class="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-medium bg-gray-100 dark:bg-[#2c2c2c] text-gray-600 dark:text-gray-400">
                                          #{{ tag }}
                                      </span>
                                      <span v-if="activeProject.tags.length > 3" class="text-[10px] text-gray-400">+{{activeProject.tags.length - 3}}</span>
                                  </div>
                              </div>
                              <div class="flex items-center justify-between">
                                  <div class="text-[11px] font-medium text-gray-500 uppercase tracking-wider">Created</div>
                                  <div class="text-xs text-gray-700 dark:text-gray-300">{{ activeProject.created_at ? activeProject.created_at.substring(0, 10) : '--' }}</div>
                              </div>
                              <div class="flex items-center justify-between">
                                  <div class="text-[11px] font-medium text-gray-500 uppercase tracking-wider">Updated</div>
                                  <div class="text-xs text-gray-700 dark:text-gray-300">{{ activeProject.updated_at ? activeProject.updated_at.substring(0, 10) : '--' }}</div>
                              </div>
                          </div>
                      </div>

                      <!-- Time & Budget Card -->
                      <div class="bg-white dark:bg-[#1a1a1a] rounded-2xl p-5 border border-gray-100 dark:border-[#2c2c2c] shadow-sm hover:shadow-md transition-shadow">
                          <h3 class="text-sm font-semibold text-gray-900 dark:text-gray-100 mb-4">Time & Budget</h3>
                          
                          <div class="space-y-5">
                              <div class="grid grid-cols-2 gap-4">
                                  <div>
                                      <div class="text-[11px] font-medium text-gray-500 uppercase tracking-wider mb-1">Start Date</div>
                                      <div class="text-sm font-semibold text-gray-900 dark:text-gray-100 flex items-center">
                                          <CalendarDays class="w-3.5 h-3.5 mr-1.5 text-gray-400" />
                                          {{ activeProject.start_date || '--/--/----' }}
                                      </div>
                                  </div>
                                  <div>
                                      <div class="text-[11px] font-medium text-gray-500 uppercase tracking-wider mb-1">End Date</div>
                                      <div class="text-sm font-semibold text-red-500 flex items-center">
                                          <CalendarDays class="w-3.5 h-3.5 mr-1.5" />
                                          {{ activeProject.due_date || '--/--/----' }}
                                      </div>
                                  </div>
                              </div>
                              
                              <div class="pt-4 border-t border-gray-50 dark:border-[#2c2c2c]">
                                  <div class="text-[11px] font-medium text-gray-500 uppercase tracking-wider mb-1">Budget</div>
                                  <div class="text-xl font-bold text-gray-900 dark:text-gray-100">
                                      {{ projectBudget || 'Not set' }}
                                  </div>
                              </div>
                              
                              <div v-if="projectSpent">
                                  <div class="flex items-center justify-between mb-1">
                                      <div class="text-[11px] font-medium text-gray-500 uppercase tracking-wider">Spent</div>
                                      <button @click="showTxModal = true" class="text-[10px] flex items-center bg-gray-100 hover:bg-gray-200 dark:bg-[#333] dark:hover:bg-[#444] text-gray-600 dark:text-gray-300 px-1.5 py-0.5 rounded transition-colors" title="Log Expense">
                                          <Plus class="w-3 h-3 mr-0.5" /> Add
                                      </button>
                                  </div>
                                  <div class="text-lg font-semibold text-orange-500">
                                      {{ projectSpent }}
                                  </div>
                              </div>
                              
                              <div v-if="displayCustomFields.length > 0">
                                  <div v-for="field in displayCustomFields" :key="field.key">
                                      <div class="text-[11px] font-medium text-gray-500 uppercase tracking-wider mb-0.5 mt-3">{{ field.key }}</div>
                                      <div class="text-sm font-medium text-gray-800 dark:text-gray-200">{{ field.val }}</div>
                                  </div>
                              </div>
                          </div>
                      </div>

                      <!-- Progress & Task Summary Card -->
                      <div class="bg-white dark:bg-[#1a1a1a] rounded-2xl p-5 border border-gray-100 dark:border-[#2c2c2c] shadow-sm hover:shadow-md transition-shadow flex flex-col">
                           <div class="mb-6">
                              <div class="flex items-center justify-between mb-2">
                                  <h3 class="text-sm font-semibold text-gray-900 dark:text-gray-100">Project Progress</h3>
                                  <span class="text-xs font-bold text-indigo-600 dark:text-indigo-400 bg-indigo-50 dark:bg-indigo-900/30 px-2 py-0.5 rounded-full">{{ projectProgress }}%</span>
                              </div>
                              <div class="w-full bg-gray-100 dark:bg-gray-800 rounded-full h-2.5 overflow-hidden">
                                  <div class="bg-gradient-to-r from-blue-400 to-indigo-500 h-2.5 rounded-full transition-all duration-500" :style="{ width: projectProgress + '%' }"></div>
                              </div>
                          </div>
                          
                          <div class="flex-1 border-t border-gray-50 dark:border-[#2c2c2c] pt-4">
                              <h3 class="text-sm font-semibold text-gray-900 dark:text-gray-100 mb-4">Task Summary</h3>
                              <div class="grid grid-cols-3 gap-3">
                                  <!-- Total -->
                                  <div class="bg-gray-50 dark:bg-[#252525] rounded-xl p-3">
                                      <div class="text-xl font-bold text-gray-900 dark:text-gray-100">{{ activeCategoryTasks.length }}</div>
                                      <div class="text-[9px] font-medium text-gray-500 uppercase tracking-wider mt-1">Total</div>
                                  </div>
                                  <!-- In Progress -->
                                  <div class="bg-blue-50 dark:bg-blue-900/20 rounded-xl p-3">
                                      <div class="text-xl font-bold text-blue-600 dark:text-blue-400">{{ activeCategoryTasks.filter(t => t.status === 'in_progress').length }}</div>
                                      <div class="text-[9px] font-medium text-blue-600/70 dark:text-blue-400/70 uppercase tracking-wider mt-1">Doing</div>
                                  </div>
                                  <!-- To Do -->
                                  <div class="bg-orange-50 dark:bg-orange-900/20 rounded-xl p-3">
                                      <div class="text-xl font-bold text-orange-600 dark:text-orange-400">{{ activeCategoryTasks.filter(t => t.status === 'todo').length }}</div>
                                      <div class="text-[9px] font-medium text-orange-600/70 dark:text-orange-400/70 uppercase tracking-wider mt-1">To Do</div>
                                  </div>
                                  <!-- Backlog -->
                                  <div class="bg-purple-50 dark:bg-purple-900/20 rounded-xl p-3">
                                      <div class="text-xl font-bold text-purple-600 dark:text-purple-400">{{ activeCategoryTasks.filter(t => t.status === 'backlog').length }}</div>
                                      <div class="text-[9px] font-medium text-purple-600/70 dark:text-purple-400/70 uppercase tracking-wider mt-1">Backlog</div>
                                  </div>
                                  <!-- Completed -->
                                  <div class="bg-green-50 dark:bg-green-900/20 rounded-xl p-3">
                                      <div class="text-xl font-bold text-green-600 dark:text-green-400">{{ activeCategoryTasks.filter(t => t.status === 'done').length }}</div>
                                      <div class="text-[9px] font-medium text-green-600/70 dark:text-green-400/70 uppercase tracking-wider mt-1">Done</div>
                                  </div>
                                  <!-- Overdue -->
                                  <div class="bg-red-50 dark:bg-red-900/20 rounded-xl p-3">
                                      <div class="text-xl font-bold text-red-600 dark:text-red-400">{{ activeCategoryTasks.filter(t => isOverdue(t)).length }}</div>
                                      <div class="text-[9px] font-medium text-red-600/70 dark:text-red-400/70 uppercase tracking-wider mt-1">Overdue</div>
                                  </div>
                              </div>
                          </div>
                      </div>
                  </div>
              </div>

              <!-- RESOURCES TAB -->
              <div v-if="activeProjectTab === 'resources'" class="animate-in fade-in slide-in-from-bottom-2 duration-300">
                  <div class="flex justify-end gap-2 mb-4 relative">
                      <button @click="showAddResourceMenu = !showAddResourceMenu" :disabled="isLinkingResource" class="px-4 py-2 flex items-center gap-2 rounded-lg bg-indigo-50 text-indigo-600 dark:bg-indigo-900/30 dark:text-indigo-400 hover:bg-indigo-100 dark:hover:bg-indigo-900/50 transition-colors text-sm font-medium cursor-pointer">
                          <Plus class="w-4 h-4" />
                          Add Resource
                          <ChevronDown class="w-4 h-4 ml-1" />
                      </button>
                      
                      <!-- Dropdown Menu -->
                      <div v-if="showAddResourceMenu" class="absolute top-full right-0 mt-2 w-56 bg-white dark:bg-[#1a1a1a] rounded-xl shadow-lg border border-gray-100 dark:border-[#2c2c2c] overflow-hidden z-20">
                          <div class="p-1">
                              <button @click="createNewResourceNote(); showAddResourceMenu = false" class="w-full px-3 py-2 text-left flex items-center gap-3 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-white/5 rounded-lg cursor-pointer transition-colors">
                                  <FileText class="w-4 h-4 text-blue-500" />
                                  New Note
                              </button>
                              <button @click="createNewResourceWhiteboard(); showAddResourceMenu = false" class="w-full px-3 py-2 text-left flex items-center gap-3 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-white/5 rounded-lg cursor-pointer transition-colors">
                                  <Palette class="w-4 h-4 text-purple-500" />
                                  New Whiteboard
                              </button>
                              <div class="h-px bg-gray-100 dark:bg-[#2c2c2c] my-1"></div>
                              <button @click="openLinkResourcePicker(); showAddResourceMenu = false" class="w-full px-3 py-2 text-left flex items-center gap-3 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-white/5 rounded-lg cursor-pointer transition-colors">
                                  <Link class="w-4 h-4 text-gray-400" />
                                  Link Existing Resource
                              </button>
                          </div>
                      </div>
                  </div>

                  <!-- Close dropdown when clicking outside -->
                  <div v-if="showAddResourceMenu" @click="showAddResourceMenu = false" class="fixed inset-0 z-10"></div>

                  <div v-if="linkedResources.length > 0">
                      <div class="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-3">
                          <div v-for="node in linkedResources" :key="node.id" @click="emit('open-node', node.id, node.node_type || 'note')" 
                               class="bg-white dark:bg-[#1a1a1a] border border-gray-200 dark:border-[#2c2c2c] rounded-xl p-4 shadow-sm hover:shadow-md cursor-pointer transition-all group"
                               :class="[
                                   node.node_type === 'whiteboard' ? 'hover:border-purple-300 dark:hover:border-purple-700' :
                                   node.node_type === 'file' ? 'hover:border-emerald-300 dark:hover:border-emerald-700' :
                                   'hover:border-blue-300 dark:hover:border-blue-700'
                               ]">
                               <div class="font-medium text-[15px] text-[#1c1c1e] dark:text-[#f4f4f5] mb-2 flex items-center justify-between">
                                  <div class="flex items-center min-w-0 pr-2">
                                      <Palette v-if="node.node_type === 'whiteboard'" class="w-4 h-4 mr-2 text-purple-400 shrink-0" />
                                      <File v-else-if="node.node_type === 'file'" class="w-4 h-4 mr-2 text-emerald-400 shrink-0" />
                                      <FileText v-else class="w-4 h-4 mr-2 text-blue-400 shrink-0" />
                                      <span class="truncate">{{ node.title || (node.node_type === 'whiteboard' ? 'Untitled Whiteboard' : node.node_type === 'file' ? 'Unnamed File' : 'Untitled Note') }}</span>
                                  </div>
                                  <button @click.stop="unlinkResource(node)" title="Unlink from Project" class="opacity-0 group-hover:opacity-100 p-1.5 hover:bg-gray-100 dark:hover:bg-white/10 rounded-md text-gray-400 hover:text-red-500 transition-all shrink-0">
                                      <Unlink class="w-3.5 h-3.5" />
                                  </button>
                               </div>
                              <div v-if="node.node_type === 'file'" class="text-xs text-gray-500 mt-2 font-mono truncate">
                                  {{ node.id }}
                              </div>
                              <div v-else class="text-xs text-gray-500 line-clamp-2 leading-relaxed">
                                  {{ node.content ? node.content.replace(/<[^>]+>/g, '').substring(0, 80) + '...' : 'Empty ' + (node.node_type === 'whiteboard' ? 'whiteboard' : 'note') }}
                              </div>
                          </div>
                      </div>
                  </div>
                  <div v-else class="flex flex-col items-center justify-center h-48 opacity-80 bg-white/50 dark:bg-black/20 rounded-2xl border border-dashed border-gray-200 dark:border-gray-800">
                      <div class="flex gap-2 mb-3">
                          <FileText class="w-10 h-10 text-gray-300" />
                          <Palette class="w-10 h-10 text-gray-300" />
                      </div>
                      <p class="text-sm font-medium text-gray-500 mb-4">No resources attached yet.</p>
                      <div class="flex gap-3 relative">
                          <button @click="showEmptyAddMenu = !showEmptyAddMenu" class="px-4 py-2 bg-indigo-500 text-white rounded-lg text-sm font-medium hover:bg-indigo-600 transition-colors flex items-center gap-2 cursor-pointer">
                              Add Resource
                              <ChevronDown class="w-4 h-4" />
                          </button>
                          
                          <!-- Dropdown Menu for empty state -->
                          <div v-if="showEmptyAddMenu" class="absolute top-full left-1/2 -translate-x-1/2 mt-2 w-56 bg-white dark:bg-[#1a1a1a] rounded-xl shadow-lg border border-gray-100 dark:border-[#2c2c2c] overflow-hidden z-20">
                              <div class="p-1">
                                  <button @click="createNewResourceNote(); showEmptyAddMenu = false" class="w-full px-3 py-2 text-left flex items-center gap-3 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-white/5 rounded-lg cursor-pointer transition-colors">
                                      <FileText class="w-4 h-4 text-blue-500" />
                                      New Note
                                  </button>
                                  <button @click="createNewResourceWhiteboard(); showEmptyAddMenu = false" class="w-full px-3 py-2 text-left flex items-center gap-3 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-white/5 rounded-lg cursor-pointer transition-colors">
                                      <Palette class="w-4 h-4 text-purple-500" />
                                      New Whiteboard
                                  </button>
                                  <div class="h-px bg-gray-100 dark:bg-[#2c2c2c] my-1"></div>
                                  <button @click="openLinkResourcePicker(); showEmptyAddMenu = false" class="w-full px-3 py-2 text-left flex items-center gap-3 text-sm text-gray-700 dark:text-gray-300 hover:bg-gray-50 dark:hover:bg-white/5 rounded-lg cursor-pointer transition-colors">
                                      <Link class="w-4 h-4 text-gray-400" />
                                      Link Existing Resource
                                  </button>
                              </div>
                          </div>
                      </div>
                      
                      <!-- Close dropdown when clicking outside -->
                      <div v-if="showEmptyAddMenu" @click="showEmptyAddMenu = false" class="fixed inset-0 z-10"></div>
                  </div>
              </div>
          </div>
          
          <div v-show="!activeProject || activeProjectTab === 'tasks'" class="h-full flex-1 flex flex-col">
              <div v-if="activeCategoryTasks.length === 0" class="flex flex-col items-center justify-center h-48 opacity-40">
              <CheckCircle2 class="w-16 h-16 mb-4"/>
              <p>You're all caught up!</p>
          </div>
          
          <div v-else class="h-full flex flex-col min-h-0">
              <!-- LIST VIEW -->
              <div v-if="viewMode === 'list'" class="space-y-2 mt-4 max-w-4xl mx-auto">
                  <div v-for="task in activeCategoryTasks" :key="task.id" 
                      class="group flex items-center p-3 rounded-xl hover:bg-gray-50 dark:hover:bg-[#1a1a1a] border transition-colors cursor-pointer"
                      :class="[
                          task.status === 'done' ? 'opacity-50 border-transparent' : 
                          isOverdue(task) ? 'border-red-200 dark:border-red-900/50 bg-red-50/20 dark:bg-red-900/5' : 'border-transparent hover:border-gray-100 dark:hover:border-gray-800'
                      ]"
                      @click="openEditModal(task)"
                  >
                      <!-- Checkbox -->
                      <button @click.stop="toggleTaskStatus(task)" class="shrink-0 mr-4 transition-colors cursor-pointer">
                          <CheckCircle2 v-if="task.status === 'done'" class="w-6 h-6 text-green-500 fill-green-50 dark:fill-green-900/30" />
                          <Circle v-else class="w-6 h-6 text-gray-300 dark:text-gray-600 hover:text-black dark:hover:text-white" />
                      </button>
                      
                      <!-- Title & Meta -->
                      <div class="flex-1 min-w-0 flex items-center justify-between">
                          <p class="text-[15px] font-medium truncate transition-all duration-300" :class="task.status === 'done' ? 'text-gray-400 line-through' : 'text-[#1c1c1e] dark:text-[#f4f4f5]'">
                              {{ task.title }}
                          </p>
                          <div class="hidden md:flex items-center gap-3 overflow-hidden ml-4 shrink-0">
                              <span v-if="task.status === 'in_progress'" class="text-[10px] px-2 py-0.5 rounded-full bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-300 font-bold tracking-wider">DOING</span>
                              
                              <span v-if="task.priority" class="text-[10px] px-2 py-0.5 rounded-full font-bold tracking-wider shrink-0" :class="getPriorityClass(task.priority)">
                                  {{ task.priority }}
                              </span>
                              
                              <div v-if="task.is_transferred && task.transferred_to" @click.stop="isLinkedPerson(task.transferred_to) ? openPerson(task.transferred_to) : null" class="flex items-center shrink-0 ml-1 px-1.5 py-0.5 rounded-md text-purple-600 dark:text-purple-400 transition-colors" :class="isLinkedPerson(task.transferred_to) ? 'hover:bg-purple-50 dark:hover:bg-purple-900/20 cursor-pointer' : 'cursor-default'" :title="'Transferred to: ' + getTransferredName(task.transferred_to)">
                                  <User v-if="isLinkedPerson(task.transferred_to)" class="w-3 h-3 mr-1" />
                                  <span class="text-[10px] font-semibold truncate max-w-[120px]">{{ getTransferredName(task.transferred_to) }}</span>
                                  <Eye v-if="task.track_progress" class="w-3.5 h-3.5 ml-1.5 text-blue-500" title="Tracking Progress" />
                              </div>
                              
                              <span v-if="task.due_date" class="text-xs flex items-center font-medium"
                                  :class="isOverdue(task) ? 'text-red-600 dark:text-red-400 bg-red-100 dark:bg-red-900/30 px-1.5 py-0.5 rounded' : 'text-red-500'">
                                  <CalendarDays class="w-3 h-3 mr-1" />
                                  {{ task.due_date }}
                              </span>
                              
                              <span v-if="task.start_date" class="text-xs flex items-center text-blue-500 font-medium">
                                  <CalendarDays class="w-3 h-3 mr-1" />
                                  {{ task.start_date }}
                              </span>
                              
                              <span v-if="task.tags.length > 0" class="text-xs flex items-center text-gray-500 max-w-[150px] truncate">
                                  <Tag class="w-3 h-3 mr-1 shrink-0" />
                                  {{ task.tags.join(', ') }}
                              </span>
                          </div>
                      </div>
                      
                      <!-- Actions -->
                      <div class="hidden md:flex shrink-0 md:opacity-0 opacity-100 group-hover:opacity-100 transition-opacity items-center gap-1 ml-4 w-[60px] justify-end">
                          <button @click.stop="deleteTask(task)" class="p-1.5 text-gray-400 hover:text-red-500 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] transition-colors cursor-pointer">
                              <Trash2 class="w-4 h-4" />
                          </button>
                      </div>
                  </div>
              </div>

              <!-- BOARD VIEW -->
              <div v-else-if="viewMode === 'board'" class="flex gap-6 flex-1 mt-6 pb-8 overflow-x-auto min-h-0 items-stretch">
                  <div v-for="col in BOARD_COLUMNS" :key="col.id" 
                       class="flex-1 min-w-[280px] flex flex-col bg-gray-50/50 dark:bg-[#161616] rounded-2xl p-4 border border-[#e6e6e6] dark:border-[#2c2c2c]"
                       @dragover.prevent 
                       @drop="onDrop($event, col.id)"
                  >
                      <div class="flex items-center justify-between mb-4 px-1" :class="col.class">
                          <h3 class="text-xs font-bold text-gray-500 pt-3 flex items-center">
                              {{ col.name }} 
                              <span class="ml-2 px-2 py-0.5 rounded-full transition-colors" 
                                    :class="(col.id === 'in_progress' && tasksByStatus[col.id].length > WIP_LIMIT) ? 'bg-red-100 text-red-600 dark:bg-red-900/30 dark:text-red-400 font-bold' : 'bg-gray-200 dark:bg-[#2a2a2a] text-gray-600 dark:text-gray-300'">
                                  {{ tasksByStatus[col.id].length }}
                              </span>
                          </h3>
                          <button @click="showQuickAdd(col.id)" class="text-gray-400 hover:text-black dark:hover:text-white pt-3"><Plus class="w-4 h-4"/></button>
                      </div>
                      <div class="flex-1 overflow-y-auto space-y-3 pb-4 column-content">
                          <div v-for="task in tasksByStatus[col.id]" :key="task.id"
                               draggable="true"
                               @dragstart="onDragStart($event, task)"
                               @click="openEditModal(task)"
                               :data-task-id="task.id"
                               class="task-card p-4 rounded-xl border hover:shadow-md transition-shadow cursor-grab active:cursor-grabbing group relative"
                               :class="isOverdue(task) ? 'border-red-300 dark:border-red-900 bg-red-50/50 dark:bg-red-900/10' : 'bg-white dark:bg-[#1e1e1e] border-[#e6e6e6] dark:border-[#2c2c2c]'"
                          >
                             <p class="text-sm font-medium text-[#1c1c1e] dark:text-[#f4f4f5] leading-snug mb-3">{{ task.title }}</p>
                             <div class="flex items-center justify-between mt-auto pt-2 border-t border-gray-50 dark:border-[#2c2c2c]">
                                 <div class="flex gap-2 items-center flex-wrap">
                                     <span v-if="task.priority" class="text-[10px] px-1.5 py-0.5 rounded font-bold" :class="getPriorityClass(task.priority)">
                                         {{ task.priority }}
                                     </span>
                                     <div v-if="task.is_transferred && task.transferred_to" @click.stop="isLinkedPerson(task.transferred_to) ? openPerson(task.transferred_to) : null" class="flex items-center shrink-0 ml-0.5 px-1.5 py-0.5 rounded-md text-purple-600 dark:text-purple-400 transition-colors" :class="isLinkedPerson(task.transferred_to) ? 'hover:bg-purple-50 dark:hover:bg-purple-900/20 cursor-pointer' : 'cursor-default'" :title="'Transferred to: ' + getTransferredName(task.transferred_to)">
                                         <User v-if="isLinkedPerson(task.transferred_to)" class="w-3 h-3 mr-1" />
                                         <span class="text-[10px] font-semibold truncate max-w-[100px]">{{ getTransferredName(task.transferred_to) }}</span>
                                         <Eye v-if="task.track_progress" class="w-3 h-3 ml-1 text-blue-500" title="Tracking Progress" />
                                     </div>
                                     <span v-if="task.start_date || task.due_date" class="text-[10px] px-1.5 py-0.5 rounded flex items-center"
                                         :class="isOverdue(task) ? 'bg-red-100 text-red-600 dark:bg-red-900/30 dark:text-red-400 font-bold' : 'text-gray-500 bg-gray-100 dark:bg-[#2a2a2a]'">
                                         <CalendarDays class="w-3 h-3 mr-1" /> {{ task.start_date ? task.start_date.substring(5) : '--' }} - {{ task.due_date ? task.due_date.substring(5) : '--' }}
                                     </span>
                                     <div v-if="task.tags.length" class="flex flex-wrap gap-1">
                                         <span v-for="tag in task.tags" :key="tag" class="text-[10px] text-gray-500 bg-gray-100 dark:bg-[#2a2a2a] px-1.5 py-0.5 rounded">
                                             {{ tag }}
                                         </span>
                                     </div>
                                 </div>
                                 <button @click.stop="deleteTask(task)" class="text-gray-300 hover:text-red-500 opacity-0 group-hover:opacity-100 transition-opacity cursor-pointer">
                                     <Trash2 class="w-3.5 h-3.5" />
                                 </button>
                             </div>
                          </div>
                          
                          <!-- Quick Add Input -->
                          <div v-if="quickAddColumn === col.id" class="mt-2 bg-white dark:bg-[#1e1e1e] p-3 rounded-xl border border-indigo-300 dark:border-indigo-500 shadow-sm animate-in fade-in zoom-in duration-200 shrink-0">
                              <input :id="'quick-add-input-' + col.id" 
                                     type="text" 
                                     v-model="quickAddTitle" 
                                     @keyup.enter="handleQuickAdd(col.id)"
                                     @keyup.esc="quickAddColumn = null"
                                     @blur="!quickAddTitle.trim() ? quickAddColumn = null : null"
                                     placeholder="Task title... (Enter to save)" 
                                     class="w-full bg-transparent text-sm font-medium text-[#1c1c1e] dark:text-[#f4f4f5] outline-none placeholder:font-normal placeholder:text-gray-400"
                              />
                          </div>
                      </div>
                  </div>
              </div>

              <!-- TABLE VIEW -->
              <div v-else-if="viewMode === 'table'" class="mt-6 border border-[#e6e6e6] dark:border-[#2c2c2c] rounded-xl overflow-hidden bg-white dark:bg-[#1e1e1e]">
                 <table class="w-full text-left text-sm">
                     <thead class="bg-gray-50 dark:bg-[#1a1a1a] text-gray-500 dark:text-gray-400 text-xs uppercase font-semibold">
                         <tr>
                             <th class="px-6 py-3 w-8">Status</th>
                             <th class="px-6 py-3">Title</th>
                             <th class="px-6 py-3 w-32">Start Date</th>
                             <th class="px-6 py-3 w-32">Due Date</th>
                             <th class="px-6 py-3 w-48">Tags</th>
                             <th class="px-6 py-3 w-16"></th>
                         </tr>
                     </thead>
                     <tbody class="divide-y divide-[#e6e6e6] dark:divide-[#2c2c2c]">
                         <tr v-for="task in activeCategoryTasks" :key="task.id" class="hover:bg-gray-50 dark:hover:bg-[#252525] group cursor-pointer" @click="openEditModal(task)">
                             <td class="px-6 py-3">
                                 <button @click.stop="toggleTaskStatus(task)" class="transition-colors cursor-pointer block mt-1">
                                      <CheckCircle2 v-if="task.status === 'done'" class="w-5 h-5 text-green-500" />
                                      <Circle v-else class="w-5 h-5 text-gray-300 dark:text-gray-600 hover:text-black dark:hover:text-white" />
                                  </button>
                             </td>
                             <td class="px-6 py-3 font-medium text-[#1c1c1e] dark:text-[#f4f4f5] flex items-center gap-2" :class="task.status === 'done' ? 'line-through text-gray-400' : ''">
                                 <span v-if="task.priority" class="text-[10px] px-1.5 py-0.5 rounded font-bold" :class="getPriorityClass(task.priority)">{{ task.priority }}</span>
                                 <div v-if="task.is_transferred && task.transferred_to" @click.stop="isLinkedPerson(task.transferred_to) ? openPerson(task.transferred_to) : null" class="flex items-center shrink-0 px-1.5 py-0.5 rounded-md text-purple-600 dark:text-purple-400 transition-colors" :class="isLinkedPerson(task.transferred_to) ? 'hover:bg-purple-50 dark:hover:bg-purple-900/20 cursor-pointer' : 'cursor-default'" :title="'Transferred to: ' + getTransferredName(task.transferred_to)">
                                     <User v-if="isLinkedPerson(task.transferred_to)" class="w-3 h-3 mr-1" />
                                     <span class="text-[10px] font-semibold truncate max-w-[120px]">{{ getTransferredName(task.transferred_to) }}</span>
                                     <Eye v-if="task.track_progress" class="w-3.5 h-3.5 ml-1.5 text-blue-500" title="Tracking Progress" />
                                 </div>
                                 {{ task.title }}
                             </td>
                             <td class="px-6 py-3 text-gray-500 font-mono text-xs">
                                 {{ task.start_date || '--/--/----' }}
                             </td>
                             <td class="px-6 py-3 text-gray-500 font-mono text-xs">
                                 {{ task.due_date || '--/--/----' }}
                             </td>
                             <td class="px-6 py-3">
                                 <div class="flex flex-wrap gap-1">
                                     <span v-for="tag in task.tags" :key="tag" class="text-[10px] text-gray-500 bg-gray-100 dark:bg-[#2a2a2a] px-1.5 py-0.5 rounded">
                                         {{ tag }}
                                     </span>
                                 </div>
                             </td>
                             <td class="px-6 py-3">
                                 <button @click.stop="deleteTask(task)" class="p-1 text-gray-400 hover:text-red-500 opacity-0 group-hover:opacity-100 transition-opacity cursor-pointer">
                                     <Trash2 class="w-4 h-4" />
                                 </button>
                             </td>
                         </tr>
                     </tbody>
                 </table>
              </div>
          </div>
          </div>
      </div>

  <!-- Edit Task Modal -->
  <TaskEditModal 
      v-if="editingTask" 
      :task="editingTaskParams" 
      :vaultPath="vaultPath"
      :projects="projects"
      @save="handleModalSave" 
      @close="editingTask = null" 
      @delete="handleModalDelete"
  />

  <!-- Edit Project Modal -->
  <ProjectEditModal 
      v-if="showProjectEditModal && (activeProject || newProjectDraft)" 
      :project="newProjectDraft || activeProject" 
      :vaultPath="vaultPath"
      :dynamic-spent="calculatedProjectSpent"
      @save="handleProjectSave" 
      @close="showProjectEditModal = false; newProjectDraft = null;" 
      @delete="deleteProject"
  />

  <!-- Mobile Floating Action Button (FAB) -->
  <button 
      @click="openCreateModal"
      class="md:hidden fixed bottom-20 right-6 z-[100] flex items-center justify-center w-14 h-14 bg-blue-500 text-white rounded-full shadow-[0_4px_20px_rgba(59,130,246,0.4)] hover:bg-blue-600 active:scale-95 transition-all"
  >
      <Plus class="w-6 h-6" />
  </button>

  <!-- Mobile Sidebar Overlay -->
  <div v-if="isMobileSidebarOpen" class="fixed inset-0 z-[120] md:hidden flex">
      <!-- Backdrop -->
      <div class="absolute inset-0 bg-black/20 dark:bg-black/60 backdrop-blur-sm transition-opacity" @click="isMobileSidebarOpen = false"></div>
      
      <!-- Sidebar Panel -->
      <div class="relative w-[75%] max-w-sm h-full bg-[#fdfdfc] dark:bg-[#1e1e1e] shadow-2xl flex flex-col transform transition-transform duration-300" style="padding-top: max(env(safe-area-inset-top), 20px);">
          <!-- Header with Close Button -->
          <div class="flex items-center justify-between px-5 pb-4 border-b border-gray-100 dark:border-[#2c2c2c] shrink-0">
              <h2 class="text-xl font-semibold text-[#1c1c1e] dark:text-[#f4f4f5]">Views</h2>
              <button @click="isMobileSidebarOpen = false" class="p-2 -mr-2 rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-[#2a2a2a] transition-colors cursor-pointer">
                  <X class="w-5 h-5" />
              </button>
          </div>
          
          <!-- Menu Items -->
          <div class="flex-1 overflow-y-auto px-3 py-6 flex flex-col space-y-1.5">
              <button @click="activeCategory = 'all'; isMobileSidebarOpen = false" class="flex items-center justify-between px-3 py-3 rounded-xl transition-colors cursor-pointer" :class="activeCategory === 'all' ? 'bg-black/5 dark:bg-white/10 text-black dark:text-white font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-[#242424]'">
                  <div class="flex items-center"><Inbox class="w-5 h-5 mr-3" />All Tasks</div>
                  <span class="text-xs bg-gray-200 dark:bg-[#333] px-2 py-0.5 rounded-full text-gray-600 dark:text-gray-400" v-if="categoryCounts.all">{{ categoryCounts.all }}</span>
              </button>
              <button @click="activeCategory = 'today'; isMobileSidebarOpen = false" class="flex items-center justify-between px-3 py-3 rounded-xl transition-colors cursor-pointer" :class="activeCategory === 'today' ? 'bg-blue-50 dark:bg-blue-900/20 text-blue-600 dark:text-blue-400 font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-[#242424]'">
                  <div class="flex items-center"><Sun class="w-5 h-5 mr-3" />Today</div>
                  <span class="text-xs bg-blue-100 dark:bg-blue-900/30 px-2 py-0.5 rounded-full text-blue-600 dark:text-blue-400" v-if="categoryCounts.today">{{ categoryCounts.today }}</span>
              </button>
              <button @click="activeCategory = 'upcoming'; isMobileSidebarOpen = false" class="flex items-center justify-between px-3 py-3 rounded-xl transition-colors cursor-pointer" :class="activeCategory === 'upcoming' ? 'bg-red-50 dark:bg-red-900/20 text-red-600 dark:text-red-400 font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-[#242424]'">
                  <div class="flex items-center"><Calendar class="w-5 h-5 mr-3" />Upcoming</div>
                  <span class="text-xs bg-red-100 dark:bg-red-900/30 px-2 py-0.5 rounded-full text-red-600 dark:text-red-400" v-if="categoryCounts.upcoming">{{ categoryCounts.upcoming }}</span>
              </button>
              <button @click="activeCategory = 'someday'; isMobileSidebarOpen = false" class="flex items-center justify-between px-3 py-3 rounded-xl transition-colors cursor-pointer" :class="activeCategory === 'someday' ? 'bg-yellow-50 dark:bg-yellow-900/20 text-yellow-600 dark:text-yellow-400 font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-[#242424]'">
                  <div class="flex items-center"><Coffee class="w-5 h-5 mr-3" />Someday</div>
                  <span class="text-xs bg-yellow-100 dark:bg-yellow-900/30 px-2 py-0.5 rounded-full text-yellow-600 dark:text-yellow-400" v-if="categoryCounts.someday">{{ categoryCounts.someday }}</span>
              </button>
              <button @click="activeCategory = 'transferred'; isMobileSidebarOpen = false" class="flex items-center justify-between px-3 py-3 rounded-xl transition-colors cursor-pointer" :class="activeCategory === 'transferred' ? 'bg-slate-100 dark:bg-slate-800 text-slate-700 dark:text-slate-300 font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-[#242424]'">
                  <div class="flex items-center"><Send class="w-5 h-5 mr-3" />Transferred</div>
                  <span class="text-xs bg-slate-200 dark:bg-slate-700 px-2 py-0.5 rounded-full text-slate-600 dark:text-slate-400" v-if="categoryCounts.transferred">{{ categoryCounts.transferred }}</span>
              </button>
              
              <div class="pt-4 pb-1 px-3 flex items-center justify-between">
                  <span class="text-[10px] font-bold text-gray-400 uppercase tracking-wider">Projects</span>
                  <button @click="handleCreateProjectClick" class="text-gray-400 hover:text-indigo-500" title="New Project">
                      <Plus class="w-4 h-4"/>
                  </button>
              </div>
              <button v-for="proj in projects" :key="proj.id" @click="activeCategory = 'project:' + proj.id; isMobileSidebarOpen = false" class="flex items-center justify-between px-3 py-3 rounded-xl transition-colors cursor-pointer" :class="activeCategory === 'project:' + proj.id ? 'bg-indigo-50 dark:bg-indigo-900/20 text-indigo-600 dark:text-indigo-400 font-medium' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-50 dark:hover:bg-[#242424]'">
                  <div class="flex items-center truncate">
                      <svg class="w-5 h-5 mr-3 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"></path></svg>
                      <span class="truncate">{{ proj.title }}</span>
                  </div>
              </button>
          </div>
      </div>
  </div>
  </div>
      <!-- WIP Notification Toast -->
      <transition enter-active-class="transition duration-300 ease-out" enter-from-class="transform translate-y-4 opacity-0" enter-to-class="transform translate-y-0 opacity-100" leave-active-class="transition duration-200 ease-in" leave-from-class="transform translate-y-0 opacity-100" leave-to-class="transform translate-y-4 opacity-0">
          <div v-if="toastMessage" class="fixed bottom-8 left-1/2 -translate-x-1/2 bg-gray-900 dark:bg-white text-white dark:text-gray-900 px-5 py-3 rounded-xl shadow-xl z-[100] text-sm font-semibold flex items-center gap-2 max-w-md w-max pointer-events-none">
              {{ toastMessage }}
          </div>
      </transition>
    <!-- Transaction Modal (Finance Integration) -->
  <TransactionModal 
      :show="showTxModal" 
      :transaction="null" 
      :income-categories="incomeCategories" 
      :expense-categories="expenseCategories" 
      :accounts="accounts" 
      :projects="projects"
      :default-project-id="activeProject?.id"
      @close="showTxModal = false" 
      @save="saveFinanceTransaction" 
  />

  <ResourceLinkModal
      :show="showEmbedPicker"
      :available-nodes="allNotesForPicker"
      @close="showEmbedPicker = false"
      @select="handleEmbedResource"
  />

</div>
</template>
