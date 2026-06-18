import { ref, type Ref, type ComputedRef } from 'vue';
import { ask } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { type TaskMetadata, getTodayStr } from '../types';
import { logger } from '../../../utils/logger';

export function useTaskCrud(
  tasks: Ref<TaskMetadata[]>,
  projects: Ref<any[]>,
  vaultPath: Ref<string>,
  ns: any,
  bus: any,
  activeCategory: Ref<string>,
  activeProject: ComputedRef<any | null>,
  taskArchiveDays: Ref<number>,
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
    comment: '',
    tags: '',
    status: 'todo',
    project_id: '',
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
    if (wipCheck && payload.status === 'in_progress' && editingTask.value && editingTask.value.status !== 'in_progress' && wipCheck.tasksByStatus.value['in_progress'].length >= wipCheck.WIP_LIMIT.value) {
      payload.status = 'todo';
      showToast(`⚠️ Đã đạt giới hạn WIP (${wipCheck.WIP_LIMIT.value} tasks). Task được đẩy về TO DO.`);
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
        
        await ns.writeNode({
          relPath: relPath,
          nodeType: 'task',
          title: editingTaskParams.value.title || 'Untitled',
          properties: properties,
          content: editingTaskParams.value.content,
          eventType: 'created'
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
        await ns.writeNode({
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

  const loadTasks = async (onProjectsLoaded?: () => Promise<void>) => {
    if (!vaultPath.value) return;
    try {
      const archiveDays = taskArchiveDays.value;
      await invoke('archive_done_nodes', { vaultPath: vaultPath.value, nodeType: 'task', days: archiveDays });
      const nodes = await ns.getNodes('task');
      tasks.value = nodes.map(mapNodeToTask);
      
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
      await ns.writeNode({
        relPath: task.path,
        nodeType: 'task',
        title: task.title,
        properties: properties,
        content: task.content,
        existingPath: task.path
      });
      task.status = newStatus;
      task.completed_at = newCompletedAt;
      bus.emit('task:status-changed', { id: task.id, oldStatus: newStatus === 'done' ? 'todo' : 'done', newStatus, title: task.title });
      if (newStatus === 'done') {
        bus.emit('task:completed', { id: task.id, title: task.title, projectId: activeProject.value?.id });
      }
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
      await ns.deleteNode({ relPath: task.path });
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

  return {
    editingTask, editingTaskParams, customFields,
    toastMessage, showToast,
    loadTasks, saveTask, mapNodeToTask,
    openEditModal, openCreateModal, closeEditModal,
    handleModalSave, handleModalDelete,
    openEditById,
    toggleTaskStatus, deleteTask,
  };
}
