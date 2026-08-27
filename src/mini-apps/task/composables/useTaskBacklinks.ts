import { ref, watch, type Ref } from 'vue';
import type { TaskMetadata } from '../types';
import { logger } from '../../../utils/logger';

/**
 * What else in the vault points at this task.
 *
 * The thing a task manager built on a folder of markdown can do that a hosted
 * one structurally cannot. A task here shares a graph with notes, meetings,
 * people and boards, and `node_edges` already records every link between them
 * — the Projects panel has been reading it since before tasks had subtasks.
 * Nothing was reading it for a task.
 *
 * Edges are recorded between stable identities, not paths, so a task keeps its
 * backlinks when it is renamed, moved between folders, or archived.
 *
 * # Why this loads on opening rather than with the list
 *
 * One query per task, asked for the one task the user opened. Loading them for
 * a list of two hundred would be two hundred queries to draw a panel nobody
 * has looked at yet.
 */
export interface Backlink {
  id: string;
  node_type: string;
  title: string;
  /** The opening of the referring node, so a row says something. */
  preview: string;
  updated_at: string;
}

/** Node types worth naming in the panel, in the order they are grouped. */
const KNOWN_TYPES = ['note', 'task', 'project', 'event', 'person', 'whiteboard', 'file'];

export function useTaskBacklinks(editingTask: Ref<TaskMetadata | null>, ns: any) {
  const backlinks = ref<Backlink[]>([]);
  const loading = ref(false);

  /**
   * A request that has been overtaken is dropped rather than applied.
   *
   * Opening two tasks quickly issues two queries, and the slower one can land
   * second. Without this the panel would show the first task's backlinks under
   * the second task's title.
   */
  let latestRequest = 0;

  const load = async (task: TaskMetadata | null) => {
    const request = ++latestRequest;

    if (!task?.id) {
      backlinks.value = [];
      return;
    }

    loading.value = true;
    try {
      const linked = await ns.getLinkedNodes(task.title || '', task.id);
      if (request !== latestRequest) return;

      backlinks.value = (linked as any[])
        // A task that links to itself — a note pasted into its own body, a
        // hand-written self-reference — is not a backlink, it is noise.
        .filter((node) => node.id !== task.id)
        .map((node) => ({
          id: node.id,
          node_type: node.node_type || '',
          title: node.title || node.id.split('/').pop() || node.id,
          preview: String(node.content || '').replace(/\s+/g, ' ').trim().slice(0, 120),
          updated_at: node.updated_at || '',
        }));
    } catch (e) {
      logger.error('Failed to load task backlinks', e);
      if (request === latestRequest) backlinks.value = [];
    } finally {
      if (request === latestRequest) loading.value = false;
    }
  };

  watch(editingTask, (task) => { void load(task); }, { immediate: true });

  return { backlinks, loading, reload: () => load(editingTask.value), KNOWN_TYPES };
}
