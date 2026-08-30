import { ref, type Ref, type ComputedRef } from 'vue';
import { ask } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { type TaskMetadata, getTodayStr, taskProperties, FORM_GOVERNED_KEYS } from '../types';

/**
 * A frontmatter value as one line of text.
 *
 * `String(value)` turns a list into `a,b` and an object into
 * `[object Object]`, and saving that back writes the mangling to disk. Anything
 * that is not a scalar is shown as the JSON it is, which round-trips.
 */
function stringifyFieldValue(value: unknown): string {
  if (value === null || value === undefined) return '';
  if (typeof value === 'object') return JSON.stringify(value);
  return String(value);
}

/**
 * The text from a field row, back as a value.
 *
 * Mirrors `stringifyFieldValue`: text that parses as JSON and was not a bare
 * number or word goes back as the structure it came from, so a list the user
 * did not touch is written out as a list rather than as a string that looks
 * like one.
 */
function parseFieldValue(text: string): unknown {
  const trimmed = text.trim();
  if (!trimmed.startsWith('[') && !trimmed.startsWith('{')) return text;
  try {
    return JSON.parse(trimmed);
  } catch {
    return text;
  }
}
import { advanceRecurrence, repeats } from '../recurrence';
import { descendantsOf } from '../subtasks';
import {
  taskFieldIssues, safeStatus, safePriority, safeRecurrence, safeDate, safeTime,
  isValidDuration,
} from '../validation';
import { useTaskDelete } from './useTaskDelete';
import { logger } from '../../../utils/logger';
import { i18n } from '../../../i18n';

const t = i18n.global.t;

export function useTaskCrud(
  tasks: Ref<TaskMetadata[]>,
  projects: Ref<any[]>,
  vaultPath: Ref<string>,
  ns: any,
  bus: any,
  activeCategory: Ref<string>,
  activeProject: ComputedRef<any | null>,
  taskArchiveDays: Ref<number>,
  taskDeleteConfirm: Ref<'dialog' | 'inline' | 'undo'>,
  wipCheck?: { tasksByStatus: ComputedRef<Record<string, TaskMetadata[]>>, WIP_LIMIT: ComputedRef<number> },
) {

  const editingTask = ref<TaskMetadata | null>(null);
  const editingTaskParams = ref<{
    title: string;
    content: string;
    is_transferred: boolean;
    transferred_to: string;
    track_progress: boolean;
    priority: string;
    start_date: string;
    due_date: string;
    due_time: string;
    reminders: string[];
    recurrence: string;
    recurrence_end_at: string;
    parent_id: string;
    comment: string;
    tags: string;
    status: string;
    project_id: string;
    completed_at: string;
  }>({
    title: '',
    content: '',
    is_transferred: false,
    transferred_to: '',
    track_progress: false,
    priority: '',
    start_date: '',
    due_date: '',
    due_time: '',
    reminders: [],
    recurrence: 'none',
    recurrence_end_at: '',
    parent_id: '',
    comment: '',
    tags: '',
    status: 'todo',
    project_id: '',
    completed_at: ''
  });
  const customFields = ref<{k: string, v: string}[]>([]);

  const openEditModal = async (task: TaskMetadata) => {
    // The list holds a preview, not the body — see `TaskMetadata.preview`. The
    // body is fetched for the one task the user actually opened.
    //
    // Fetched before the modal is shown, not after. Setting `editingTask`
    // first renders the form for one frame against the previous task's
    // fields, which reads as the wrong task having opened.
    let body = '';
    if (task.path) {
      try {
        body = (await ns.getNode(task.id))?.content ?? '';
      } catch (e) {
        logger.error("Failed to load the task body", e);
        showToast(t('task.save_failed'));
        return;
      }
    }
    editingTask.value = task;
    editingTaskParams.value = {
      title: task.title,
      content: body,
      is_transferred: task.is_transferred || false,
      transferred_to: task.transferred_to || '',
      track_progress: task.track_progress || false,
      priority: task.priority || '',
      start_date: task.start_date,
      due_date: task.due_date,
      due_time: task.due_time || '',
      reminders: [...(task.reminders || [])],
      recurrence: task.recurrence || 'none',
      recurrence_end_at: task.recurrence_end_at || '',
      parent_id: task.parent_id || '',
      comment: task.comment,
      tags: Array.isArray(task.tags) ? task.tags.join(', ') : '',
      status: task.status,
      project_id: task.project_id || '',
      completed_at: task.completed_at || ''
    };
    // Everything the file carries that the form has no control for. Before
    // this the list held every property including `status` and `due_date`,
    // which is why nothing could render it: the rows would have duplicated
    // half the form as raw text boxes.
    customFields.value = Object.entries(task.custom_fields || {})
      .filter(([k]) => !FORM_GOVERNED_KEYS.has(k.trim()))
      .map(([k, v]) => ({ k, v: stringifyFieldValue(v) }));
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
      await openEditModal(task);
    } else {
      logger.warn(`TaskApp: Task not found for id: ${id}`);
    }
  };

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
      due_time: '',
      reminders: [],
      recurrence: 'none',
      recurrence_end_at: '',
      parent_id: '',
      comment: '',
      source_link: '',
      tags: [],
      preview: '',
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
      due_time: '',
      reminders: [],
      recurrence: 'none',
      recurrence_end_at: '',
      parent_id: '',
      comment: '',
      tags: '',
      status: 'todo',
      project_id: activeCategory.value.startsWith('project:') ? activeCategory.value.substring(8) : '',
      completed_at: ''
    };
    customFields.value = [];
  };

  const handleModalSave = async (payload: any) => {
    if (wipCheck && payload.status === 'in_progress' && editingTask.value && editingTask.value.status !== 'in_progress' && wipCheck.tasksByStatus.value['in_progress'].length >= wipCheck.WIP_LIMIT.value) {
      payload.status = 'todo';
      showToast(t('task.wip_limit_reached', { limit: wipCheck.WIP_LIMIT.value }));
    }

    editingTaskParams.value = payload;
    if (editingTask.value) {
      // Setting a repeating task to Done in the form means the same as ticking
      // it off in the list: this occurrence is finished, and the task moves to
      // the next one. Applied to the payload before the save so the write and
      // the row on screen agree, rather than saving Done and correcting it.
      const becomingDone = editingTask.value.status !== 'done' && payload.status === 'done';
      const willRepeat = repeats({ recurrence: payload.recurrence } as TaskMetadata);
      if (becomingDone && willRepeat) {
        const outcome = advanceRecurrence(
          {
            recurrence: payload.recurrence,
            recurrence_end_at: payload.recurrence_end_at,
            start_date: payload.start_date,
            due_date: payload.due_date,
          } as TaskMetadata,
          getTodayStr(),
        );
        if (outcome.kind === 'advance') {
          payload.status = editingTask.value.status;
          payload.start_date = outcome.start_date;
          payload.due_date = outcome.due_date;
          payload.completed_at = '';
          editingTask.value.completed_at = '';
          showToast(t('task.recurrence_advanced', { date: outcome.due_date || outcome.start_date }));
        }
      }

      if (editingTask.value.status !== payload.status) {
        if (payload.status === 'done') {
          editingTask.value.completed_at = getTodayStr();
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
      const updatedCustomFields: Record<string, unknown> = {};

      customFields.value.forEach(field => {
        if (field.k.trim()) {
          updatedCustomFields[field.k.trim()] = parseFieldValue(field.v);
        }
      });

      // A write names the keys it changes and leaves the rest of the file
      // alone, so a row the user removed has to be named as `null` or it
      // simply stays at its old value and the deletion appears not to have
      // taken. Only keys this form governs are nulled: the ones filtered out
      // of the rows — `node_id`, `created_at`, the typed fields — are absent
      // because the form has nothing to say about them, not because they
      // should go.
      for (const key of Object.keys(editingTask.value.custom_fields || {})) {
        if (FORM_GOVERNED_KEYS.has(key.trim())) continue;
        if (!(key.trim() in updatedCustomFields)) {
          updatedCustomFields[key.trim()] = null;
        }
      }
      
      const edited = taskProperties({
        custom_fields: updatedCustomFields,
        status: editingTask.value.status || 'todo',
        is_transferred: editingTaskParams.value.is_transferred,
        transferred_to: editingTaskParams.value.transferred_to,
        track_progress: editingTaskParams.value.track_progress,
        priority: editingTaskParams.value.priority,
        start_date: editingTaskParams.value.start_date,
        due_date: editingTaskParams.value.due_date,
        due_time: editingTaskParams.value.due_time,
        reminders: editingTaskParams.value.reminders,
        recurrence: editingTaskParams.value.recurrence,
        recurrence_end_at: editingTaskParams.value.recurrence_end_at,
        parent_id: editingTaskParams.value.parent_id,
        comment: editingTaskParams.value.comment,
        source_link: editingTask.value.source_link || '',
        tags: tagArray,
        project_id: editingTaskParams.value.project_id,
        completed_at: editingTask.value.completed_at || '',
      });

      // Deletions are named in `updatedCustomFields` above, one key at a time.
      //
      // They used to be derived here by subtraction — anything in the file but
      // not in `edited` was taken to be a row the user removed — and the
      // comment said that was safe "here, and only here, because this form
      // loads every frontmatter key as a row". That premise is gone: the rows
      // are now only the keys the form has no control for, so subtraction
      // would null every key it deliberately does not show. `node_id` is the
      // one that matters — nulling it hands the file a fresh identity and
      // splits it into two documents on the next sync.
      const properties = edited;

      if (editingTask.value.isNew) {
        const relPath = `Tasks/${crypto.randomUUID()}.md`;
        
        await ns.writeNode({
          relPath: relPath,
          nodeType: 'task',
          title: editingTaskParams.value.title || t('task.untitled_task'),
          properties: properties,
          content: editingTaskParams.value.content,
          eventType: 'created'
        });
        
        const nowStr = new Date().toISOString().replace('T', ' ').substring(0, 19);
        const newTask: TaskMetadata = {
          id: relPath,
          path: relPath,
          title: editingTaskParams.value.title || t('task.untitled_task'),
          preview: editingTaskParams.value.content,
          created_at: nowStr,
          updated_at: nowStr,
          custom_fields: updatedCustomFields,
          ...properties
        } as any;
        tasks.value.unshift(newTask);
      } else if (editingTask.value.path) {
        await ns.writeNode({
          relPath: editingTask.value.path,
          nodeType: 'task',
          title: editingTaskParams.value.title,
          properties: properties,
          content: editingTaskParams.value.content
        });
        
        editingTask.value.title = editingTaskParams.value.title;
        editingTask.value.preview = editingTaskParams.value.content;
        editingTask.value.is_transferred = editingTaskParams.value.is_transferred;
        editingTask.value.transferred_to = editingTaskParams.value.transferred_to;
        editingTask.value.track_progress = editingTaskParams.value.track_progress;
        editingTask.value.priority = editingTaskParams.value.priority;
        editingTask.value.start_date = editingTaskParams.value.start_date;
        editingTask.value.due_date = editingTaskParams.value.due_date;
        editingTask.value.due_time = editingTaskParams.value.due_time;
        editingTask.value.reminders = [...editingTaskParams.value.reminders];
        editingTask.value.recurrence = editingTaskParams.value.recurrence;
        editingTask.value.recurrence_end_at = editingTaskParams.value.recurrence_end_at;
        editingTask.value.parent_id = editingTaskParams.value.parent_id;
        editingTask.value.comment = editingTaskParams.value.comment;
        editingTask.value.tags = tagArray;
        editingTask.value.project_id = editingTaskParams.value.project_id;
        editingTask.value.custom_fields = updatedCustomFields;
      }
      
      closeEditModal();
    } catch (e) {
      logger.error("Failed to update/create task", e);
      showToast(t('task.save_failed'));
    }
  };

  const mapNodeToTask = (node: any): TaskMetadata => {
    const rawTags = node.properties?.tags;
    const tagsArray = Array.isArray(rawTags) ? rawTags : (typeof rawTags === 'string' && rawTags.trim() !== '' ? [rawTags] : []);

    return {
      id: node.id,
      path: node.id, // ID is the relative path in the node system
      title: node.title,
      preview: node.preview ?? '',
      created_at: node.created_at,
      updated_at: node.updated_at,
      // Every field with a fixed set of legal values goes through a guard, and
      // a value outside that set is behaved-as-unset rather than acted on. See
      // `validation.ts`: merging two devices' edits to one frontmatter line
      // interleaves them character by character, and the result is valid YAML
      // that means nothing — `in_pronegress`, `2026-129-315`.
      status: safeStatus(node.properties.status),
      is_transferred: node.properties.is_transferred || false,
      transferred_to: node.properties.transferred_to || '',
      track_progress: node.properties.track_progress || false,
      priority: safePriority(node.properties.priority),
      start_date: safeDate(node.properties.start_date),
      due_date: safeDate(node.properties.due_date),
      // `start_time` is what the reminder loop read before this field existed;
      // vaults written by older versions still carry it.
      due_time: safeTime(node.properties.due_time || node.properties.start_time),
      reminders: Array.isArray(node.properties.reminders)
        ? node.properties.reminders.filter(isValidDuration)
        : [],
      recurrence: safeRecurrence(node.properties.recurrence),
      recurrence_end_at: safeDate(node.properties.recurrence_end_at),
      parent_id: node.properties.parent_id || '',
      // The raw properties, before the guards above substituted anything — by
      // the time the task is mapped the evidence is gone.
      issues: taskFieldIssues(node.properties),
      comment: node.properties.comment || '',
      source_link: node.properties.source_link || '',
      tags: tagsArray,
      project_id: node.properties.project_id || '',
      completed_at: node.properties.completed_at || '',
      custom_fields: node.properties || {}
    };
  };

  /**
   * File finished tasks away. Its own call, made once when the app opens.
   *
   * This used to run at the top of every `loadTasks`, and `loadTasks` is what
   * the file watcher, the sync-completed event and every node create/delete
   * all call — so saving one task scheduled a scan of every task in the vault
   * and, on the strength of a date, moved files. Archiving is a housekeeping
   * job on a day scale; it has no business firing 300ms after a keystroke.
   */
  const archiveDoneTasks = async (): Promise<number> => {
    if (!vaultPath.value) return 0;
    try {
      return await invoke<number>('archive_done_nodes', {
        vaultPath: vaultPath.value,
        nodeType: 'task',
        days: taskArchiveDays.value,
      });
    } catch (e) {
      logger.error("Failed to archive done tasks", e);
      return 0;
    }
  };

  const loadTasks = async (onProjectsLoaded?: () => Promise<void>) => {
    if (!vaultPath.value) return;
    try {
      // Summaries, not whole nodes: four views draw a title, some dates and a
      // few properties, and the bodies were the bulk of what was being sent.
      const nodes = await ns.getNodeSummaries('task');
      // A task waiting out its undo window is still on disk. Without this it
      // reappears in the list underneath the toast offering to bring it back.
      tasks.value = nodes.map(mapNodeToTask).filter((task: TaskMetadata) => !isHidden(task.id));
      
      const projNodes = await ns.getNodes('project');
      projects.value = projNodes.map((node: any) => ({
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
      
      if (onProjectsLoaded) {
        await onProjectsLoaded();
      }
    } catch (e) {
      logger.error("Failed to load tasks", e);
    }
  };

  /**
   * Tick off one occurrence of a repeating task.
   *
   * The task is not marked done; its dates move to the next occurrence and it
   * stays open, which is the whole point of a repeating task. When the series
   * has run out — `advanceRecurrence` says `complete` — it finishes like any
   * other task instead.
   */
  const advanceRecurringTask = async (task: TaskMetadata) => {
    const outcome = advanceRecurrence(task, getTodayStr());

    const previous = {
      status: task.status,
      completed_at: task.completed_at,
      start_date: task.start_date,
      due_date: task.due_date,
    };

    const next = outcome.kind === 'advance'
      ? { status: 'todo', completed_at: '', start_date: outcome.start_date, due_date: outcome.due_date }
      : { status: 'done', completed_at: getTodayStr(), start_date: task.start_date, due_date: task.due_date };

    Object.assign(task, next);

    try {
      await ns.writeNode({
        relPath: task.path,
        nodeType: 'task',
        title: task.title,
        properties: taskProperties(task),
      });
      if (outcome.kind === 'advance') {
        showToast(t('task.recurrence_advanced', { date: outcome.due_date || outcome.start_date }));
      } else {
        showToast(t('task.recurrence_finished'));
      }
      bus.emit('task:status-changed', {
        id: task.id, oldStatus: previous.status, newStatus: next.status, title: task.title,
      });
      if (next.status === 'done') {
        bus.emit('task:completed', { id: task.id, title: task.title, projectId: activeProject.value?.id });
      }
    } catch (e) {
      logger.error("Failed to advance a repeating task", e);
      Object.assign(task, previous);
      showToast(t('task.save_failed'));
    }
  };

  const toggleTaskStatus = async (task: TaskMetadata) => {
    const goingToDone = task.status !== 'done';

    // A repeating task that is being ticked off does not finish — it moves to
    // its next occurrence and stays open. Un-ticking one is left alone: the
    // dates have already moved on, and rolling them back would be guessing at
    // which occurrence the user meant.
    if (goingToDone && repeats(task)) {
      return advanceRecurringTask(task);
    }

    const newStatus = goingToDone ? 'done' : 'todo';
    // The local date, not the UTC one. A task ticked at half past midnight in
    // UTC+7 is stamped with yesterday by `toISOString`, and the Today view —
    // which compares against the local date — then hides the task the moment
    // the user completes it.
    const newCompletedAt = newStatus === 'done' ? getTodayStr() : '';
    const previousStatus = task.status;
    const previousCompletedAt = task.completed_at;

    try {
      await ns.writeNode({
        relPath: task.path,
        nodeType: 'task',
        title: task.title,
        properties: taskProperties(task, { status: newStatus, completed_at: newCompletedAt }),
        // No body: this write changes a property. Sending the copy loaded with
        // the list would revert an edit made since, in another window or by sync.
      });
      task.status = newStatus;
      task.completed_at = newCompletedAt;
      bus.emit('task:status-changed', { id: task.id, oldStatus: newStatus === 'done' ? 'todo' : 'done', newStatus, title: task.title });
      if (newStatus === 'done') {
        bus.emit('task:completed', { id: task.id, title: task.title, projectId: activeProject.value?.id });
      }
    } catch (e) {
      logger.error("Failed to update task", e);
      task.status = previousStatus;
      task.completed_at = previousCompletedAt;
      showToast(t('task.save_failed'));
    }
  };

  // ── Deleting ───────────────────────────────────────────────────────
  //
  // The work is held behind a timer and undone by cancelling it; see
  // `useTaskDelete` for why that is the only shape an undo can take here.
  const {
    pending: pendingDelete,
    isHidden,
    scheduleDelete,
    deleteTaskTree,
    deleteMany,
    undo: undoDelete,
    commit: commitDelete,
  } = useTaskDelete({
    tasks,
    ns,
    onFailed: () => showToast(t('task.delete_failed')),
  });

  // Cancel / keep / delete-everything is a genuine three-way choice, and the
  // platform dialog only offers two. So this one is asked in a modal the app
  // draws, with the promise resolved by whichever button is pressed.
  const pendingSubtreeDelete = ref<{ task: TaskMetadata; count: number } | null>(null);
  let resolveSubtreeChoice: ((choice: 'keep' | 'all' | null) => void) | null = null;

  const askSubtreeChoice = (task: TaskMetadata, count: number) =>
    new Promise<'keep' | 'all' | null>((resolve) => {
      pendingSubtreeDelete.value = { task, count };
      resolveSubtreeChoice = resolve;
    });

  const answerSubtreeDelete = (choice: 'keep' | 'all' | null) => {
    pendingSubtreeDelete.value = null;
    resolveSubtreeChoice?.(choice);
    resolveSubtreeChoice = null;
  };

  /**
   * Delete one task.
   *
   * How much it asks first is the user's setting; see `taskDeleteConfirm`. The
   * undo window happens either way — the setting decides how loudly the delete
   * announces itself, not whether it can be taken back.
   *
   * `inline` is handled by the views, which turn the bin into a second button
   * rather than opening anything, so by the time it reaches here the user has
   * already pressed twice and there is nothing left to ask.
   *
   * A parent with subtasks always asks, whatever the setting says: what
   * happens to the children is a real question and not a yes/no, and no undo
   * window can stand in for an answer to it.
   */
  const deleteTask = async (task: TaskMetadata) => {
    const descendants = descendantsOf(task, tasks.value);
    if (descendants.length > 0) {
      const choice = await askSubtreeChoice(task, descendants.length);
      if (!choice) return;
      await deleteTaskTree(task, choice);
      return;
    }

    if (taskDeleteConfirm.value === 'dialog') {
      let isConfirmed = false;
      try {
        isConfirmed = await ask(t('task.delete_task_body'), {
          title: t('task.delete_task_title'),
          kind: 'warning',
          okLabel: t('task.delete_confirm'),
          cancelLabel: t('task.delete_cancel'),
        });
      } catch (e) {
        logger.warn("Tauri confirm failed, falling back to window.confirm", e);
        isConfirmed = window.confirm(t('task.delete_task_title'));
      }
      if (!isConfirmed) return;
    }

    await scheduleDelete([task], [], task.title);
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

  return {
    editingTask, editingTaskParams, customFields,
    toastMessage, showToast,
    loadTasks, archiveDoneTasks, saveTask, mapNodeToTask,
    openEditModal, openCreateModal, closeEditModal,
    handleModalSave, handleModalDelete,
    openEditById,
    toggleTaskStatus, deleteTask,
    pendingSubtreeDelete, answerSubtreeDelete,
    pendingDelete, undoDelete, commitDelete, deleteMany,
    taskDeleteConfirm,
  };
}
