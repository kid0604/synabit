import { ref, computed, watch, type Ref, type ComputedRef } from 'vue';
import { type TaskMetadata, URGENCY_THRESHOLD_DAYS } from '../types';
import { logger } from '../../../utils/logger';

export function useBoardLogic(
  tasks: Ref<TaskMetadata[]>,
  activeCategoryTasks: ComputedRef<TaskMetadata[]>,
  activeCategory: Ref<string>,
  activeProject: ComputedRef<any | null>,
  ns: any,
  showToast: (msg: string) => void,
) {
  const viewMode = ref<'list' | 'board' | 'table' | 'matrix'>('list');

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

  // Eisenhower Matrix logic
  const getTaskQuadrant = (task: TaskMetadata): string => {
    // Priority 1: Explicit override from drag-drop
    if (task.custom_fields?.eisenhower_quadrant) {
      return task.custom_fields.eisenhower_quadrant;
    }
    // Priority 2: Delegate = transferred tasks
    if (task.is_transferred) return 'delegate';
    // Derive importance & urgency
    const isImportant = task.priority === 'P1' || task.priority === 'P2';
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    const dueDate = task.due_date ? new Date(task.due_date) : null;
    if (dueDate) dueDate.setHours(0, 0, 0, 0);
    const daysUntilDue = dueDate ? Math.ceil((dueDate.getTime() - today.getTime()) / 86400000) : null;
    const isUrgent = daysUntilDue !== null && daysUntilDue <= URGENCY_THRESHOLD_DAYS;
    // Priority 3: Do First = Important (P1/P2) + Urgent (due ≤ 3 days)
    if (isImportant && isUrgent) return 'do_first';
    // Priority 4: Schedule = Has specific due date, not urgent yet
    if (dueDate && !isUrgent) return 'schedule';
    // Priority 5: Eliminate = Everything else
    return 'eliminate';
  };

  const tasksByQuadrant = computed(() => {
    const grouped: Record<string, TaskMetadata[]> = {
      do_first: [], schedule: [], delegate: [], eliminate: []
    };
    activeCategoryTasks.value.forEach(t => {
      if (t.status === 'done') return;
      const q = getTaskQuadrant(t);
      if (grouped[q]) grouped[q].push(t);
    });
    return grouped;
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
      await ns.writeNode({
        relPath: relPath,
        nodeType: 'task',
        title: title,
        properties: properties,
        content: '',
        eventType: 'created'
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
      await ns.writeNode({
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

  const onMatrixDrop = async (e: DragEvent, quadrantId: string) => {
    const taskId = e.dataTransfer?.getData('taskId');
    if (!taskId) return;
    const task = tasks.value.find(t => t.id === taskId);
    if (!task) return;
    if (getTaskQuadrant(task) === quadrantId) return;
    if (!task.custom_fields) task.custom_fields = {};
    task.custom_fields['eisenhower_quadrant'] = quadrantId;
    try {
      await ns.writeNode({
        relPath: task.path,
        nodeType: 'task',
        title: task.title,
        properties: {
          ...task.custom_fields,
          status: task.status,
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
      logger.error("Matrix drag update failed", err);
    }
  };

  return {
    viewMode, quickAddColumn, quickAddTitle,
    WIP_LIMIT, tasksByStatus, tasksByQuadrant,
    showQuickAdd, handleQuickAdd,
    onDragStart, onDrop, onMatrixDrop,
  };
}
