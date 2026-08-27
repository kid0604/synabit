import { ref, computed, watch, type Ref, type ComputedRef } from 'vue';
import { type TaskMetadata, URGENCY_THRESHOLD_DAYS, getTodayStr, taskProperties } from '../types';
import { advanceRecurrence, repeats } from '../recurrence';
import { logger } from '../../../utils/logger';
import { taskViewInPlatformScope } from '../../../shared/platformScope';
import { i18n } from '../../../i18n';
import { keyBetween, keysForSequence, isOrderKey } from '../ordering';

const t = i18n.global.t;

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
      // Opening a project normally lands on the board. On a phone the board
      // cannot be operated at all — its cards move by HTML5 drag-and-drop,
      // which touch never triggers — so the switch that is a convenience on the
      // desktop would be a dead end here.
      viewMode.value = taskViewInPlatformScope('board') ? 'board' : 'list';
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

  /**
   * How a column is sorted, whichever form its cards are in.
   *
   * Three generations sit in a vault at once: cards ordered by a string key,
   * cards still carrying the float the board used to write, and cards that
   * have never been dragged and have no key at all. Sorting has to put all
   * three in one sensible sequence without writing anything — a list that
   * rewrote files just to be looked at would sync on every render.
   *
   * String keys come first as a group, because they are the only ones the user
   * has actually arranged by hand; everything else follows in the order it
   * always had. Within the untouched group a float sorts by its value and a
   * card with nothing sorts newest-first, which is what the board did before.
   */
  const orderRank = (t: TaskMetadata): [number, string, number] => {
    const raw = t.custom_fields?.['order'];
    if (isOrderKey(raw)) return [0, raw, 0];
    if (raw !== undefined && raw !== null && !Number.isNaN(Number(raw))) {
      return [1, '', Number(raw)];
    }
    return [1, '', -new Date(t.created_at).getTime()];
  };

  const compareOrder = (a: TaskMetadata, b: TaskMetadata): number => {
    const [ga, ka, na] = orderRank(a);
    const [gb, kb, nb] = orderRank(b);
    if (ga !== gb) return ga - gb;
    if (ga === 0) return ka < kb ? -1 : ka > kb ? 1 : 0;
    return na - nb;
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
      sorted[key].sort(compareOrder);
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
      showToast(t('task.wip_limit_reached', { limit: WIP_LIMIT.value }));
    }

    const properties = taskProperties({
      status: targetStatus,
      is_transferred: false,
      track_progress: false,
      priority: '',
      start_date: '',
      due_date: '',
      tags: [],
      project_id: activeCategory.value.startsWith('project:')
        ? activeCategory.value.substring(8)
        : '',
    });
    
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
        preview: '',
        created_at: nowStr,
        updated_at: nowStr,
        custom_fields: {},
        ...properties
      } as any;
      
      tasks.value.unshift(newTask);
      quickAddTitle.value = ''; 
    } catch(e) {
      logger.error('Failed to quick add task', e);
      showToast(t('task.save_failed'));
    }
  };

  /**
   * Give every card in a column a string key, keeping the order it is in.
   *
   * A no-op once a column has been through it. Returns whether it succeeded:
   * a half-migrated column would have some cards ordered by key and some by
   * float, and the next drop would mint a key between two things that are not
   * comparable — better to abandon the drag than to write that down.
   */
  const migrateColumnKeys = async (column: TaskMetadata[]): Promise<boolean> => {
    const needsKeys = column.some(t => !isOrderKey(t.custom_fields?.['order']));
    if (!needsKeys) return true;

    const keys = keysForSequence(column.length);
    const previous = column.map(t => t.custom_fields?.['order']);

    try {
      for (let i = 0; i < column.length; i += 1) {
        const card = column[i];
        if (!card.custom_fields) card.custom_fields = {};
        card.custom_fields['order'] = keys[i];
        await ns.writeNode({
          relPath: card.path,
          nodeType: 'task',
          title: card.title,
          properties: taskProperties(card),
        });
      }
      return true;
    } catch (err) {
      logger.error("Could not write the column's order", err);
      column.forEach((card, i) => {
        if (previous[i] === undefined) delete card.custom_fields?.['order'];
        else card.custom_fields!['order'] = previous[i];
      });
      showToast(t('task.move_failed'));
      return false;
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
      showToast(t('task.wip_limit_reached', { limit: WIP_LIMIT.value }));
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

    // The neighbours must all carry string keys before one can be minted
    // between them, so a column still on the old float ordering is written down
    // in the order it is already showing. It happens once, on the first drag
    // into that column, and costs one write per card sitting in it — every drag
    // after that is a single write, as it always was.
    const migrated = await migrateColumnKeys(tasksInCol);
    if (!migrated) return;

    const keyAt = (index: number): string | null => {
      const neighbour = tasksInCol[index];
      const raw = neighbour?.custom_fields?.['order'];
      return isOrderKey(raw) ? raw : null;
    };

    let newOrder: string;
    if (tasksInCol.length === 0) {
      newOrder = keyBetween(null, null);
    } else if (insertAfterTaskIdx === -1) {
      newOrder = keyBetween(null, keyAt(0));
    } else if (insertAfterTaskIdx >= tasksInCol.length - 1) {
      newOrder = keyBetween(keyAt(tasksInCol.length - 1), null);
    } else {
      newOrder = keyBetween(keyAt(insertAfterTaskIdx), keyAt(insertAfterTaskIdx + 1));
    }

    const prevStatus = task.status;
    const prevOrderFromCustomFields = task.custom_fields?.['order'];
    // Avoid API call if no change in status and order position
    if (prevStatus === newStatus && prevOrderFromCustomFields === newOrder) return;

    const prevCompletedAt = task.completed_at;
    const prevStartDate = task.start_date;
    const prevDueDate = task.due_date;

    if (!task.custom_fields) task.custom_fields = {};
    task.custom_fields['order'] = newOrder;
    task.status = targetStatus;

    // Dragging a repeating task into DONE means the same as ticking it off:
    // this occurrence is finished, so the task moves on to the next one and
    // stays open. It lands back in the column it came from rather than in
    // DONE, which is what the dates now say about it.
    let advanced = false;
    if (targetStatus === 'done' && prevStatus !== 'done' && repeats(task)) {
      const outcome = advanceRecurrence(task, getTodayStr());
      if (outcome.kind === 'advance') {
        task.status = prevStatus;
        task.start_date = outcome.start_date;
        task.due_date = outcome.due_date;
        task.completed_at = '';
        advanced = true;
      }
    }

    // Track completed_at for archiving — in the local date, since that is what
    // the Today view and the archive countdown both compare against.
    if (!advanced) {
      if (task.status === 'done' && !task.completed_at) {
        task.completed_at = getTodayStr();
      } else if (task.status !== 'done') {
        task.completed_at = '';
      }
    }

    try {
      await ns.writeNode({
        relPath: task.path,
        nodeType: 'task',
        title: task.title,
        properties: taskProperties(task),
        // No body: a drag changes a property. See `writeNode`.
      });
      if (advanced) {
        showToast(t('task.recurrence_advanced', { date: task.due_date || task.start_date }));
      }
    } catch (err) {
      logger.error("Drag update failed", err);
      // Put the card back. Leaving it in the new column says the move was
      // saved, and the next reload from disk would move it again anyway.
      task.status = prevStatus;
      task.completed_at = prevCompletedAt;
      task.start_date = prevStartDate;
      task.due_date = prevDueDate;
      if (prevOrderFromCustomFields === undefined) delete task.custom_fields['order'];
      else task.custom_fields['order'] = prevOrderFromCustomFields;
      showToast(t('task.move_failed'));
    }
  };

  const onMatrixDrop = async (e: DragEvent, quadrantId: string) => {
    const taskId = e.dataTransfer?.getData('taskId');
    if (!taskId) return;
    const task = tasks.value.find(t => t.id === taskId);
    if (!task) return;
    if (getTaskQuadrant(task) === quadrantId) return;
    if (!task.custom_fields) task.custom_fields = {};
    const prevQuadrant = task.custom_fields['eisenhower_quadrant'];
    task.custom_fields['eisenhower_quadrant'] = quadrantId;
    try {
      await ns.writeNode({
        relPath: task.path,
        nodeType: 'task',
        title: task.title,
        properties: taskProperties(task),
        // No body: a drag changes a property. See `writeNode`.
      });
    } catch (err) {
      logger.error("Matrix drag update failed", err);
      if (prevQuadrant === undefined) delete task.custom_fields['eisenhower_quadrant'];
      else task.custom_fields['eisenhower_quadrant'] = prevQuadrant;
      showToast(t('task.move_failed'));
    }
  };

  return {
    viewMode, quickAddColumn, quickAddTitle,
    WIP_LIMIT, tasksByStatus, tasksByQuadrant,
    showQuickAdd, handleQuickAdd,
    onDragStart, onDrop, onMatrixDrop,
  };
}
