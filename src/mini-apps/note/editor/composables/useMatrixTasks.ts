import { useNodeService } from '../../../../composables/useNodeService';
import { useEventBus } from '../../../../composables/useEventBus';
import { getTodayStr, taskProperties } from '../../../task/types';
import { safeDate, safePriority, safeRecurrence, safeStatus } from '../../../task/validation';
import { advanceRecurrence, repeats } from '../../../task/recurrence';
import { taskQuadrant } from '../../../task/quadrant';
import type { EisenhowerOptions, MatrixTask } from '../../extensions/eisenhowerShared';

/** As much of a task file as a chip shows and a tick changes, guarded as Tasks guards it. */
function readTask(node: any) {
  const p = node.properties ?? {};
  return {
    id: node.id as string,
    title: node.title as string,
    status: safeStatus(p.status),
    priority: safePriority(p.priority),
    start_date: safeDate(p.start_date),
    due_date: safeDate(p.due_date),
    recurrence: safeRecurrence(p.recurrence),
    recurrence_end_at: safeDate(p.recurrence_end_at),
    is_transferred: !!p.is_transferred,
    custom_fields: p,
  };
}

/**
 * The vault, as an Eisenhower matrix in a note reaches it: a task to show,
 * to tick, or to make from a line.
 *
 * `noteId` is read when a task is made, so it is the note open then — the
 * same editor instance moves between tabs.
 */
export function useMatrixTasks(noteId: () => string | undefined): EisenhowerOptions {
  const ns = useNodeService();
  const bus = useEventBus();

  return {
    async loadTask(id) {
      const node = await ns.getNode(id);
      if (!node || node.node_type !== 'task') return null;
      const task = readTask(node);
      return {
        id: task.id,
        title: task.title,
        status: task.status,
        due_date: task.due_date,
        quadrant: taskQuadrant(task),
      } satisfies MatrixTask;
    },

    /**
     * What the checkbox in Tasks does: done or not done, and a repeating task
     * moved on to its next occurrence rather than finished.
     */
    async toggleTask(id) {
      const node = await ns.getNode(id);
      if (!node) return;
      const task = readTask(node);
      const today = getTodayStr();
      let next: Record<string, string>;
      if (task.status !== 'done' && repeats(task)) {
        const outcome = advanceRecurrence(task, today);
        next = outcome.kind === 'advance'
          ? { status: 'todo', completed_at: '', start_date: outcome.start_date, due_date: outcome.due_date }
          : { status: 'done', completed_at: today };
      } else {
        next = task.status === 'done' ? { status: 'todo', completed_at: '' } : { status: 'done', completed_at: today };
      }
      // Only the keys that change, and no body: see `WriteNodeParams`.
      await ns.writeNode({ relPath: id, nodeType: 'task', title: task.title, properties: next });
      bus.emit('task:status-changed', { id, oldStatus: task.status, newStatus: next.status, title: task.title });
      if (next.status === 'done') bus.emit('task:completed', { id, title: task.title });
    },

    async createTask(title, quadrant) {
      const relPath = `Tasks/${crypto.randomUUID()}.md`;
      await ns.writeNode({
        relPath,
        nodeType: 'task',
        title,
        properties: taskProperties({
          status: 'todo',
          is_transferred: false,
          track_progress: false,
          priority: '',
          start_date: '',
          due_date: '',
          tags: [],
          project_id: '',
          source_link: noteId() ?? '',
          // Where it was put, so Tasks shows it in the same quadrant. From
          // here on the two may part ways, and the chip says so when they do.
          custom_fields: { eisenhower_quadrant: quadrant },
        }),
        content: '',
        eventType: 'created',
      });
      return relPath;
    },
  };
}
