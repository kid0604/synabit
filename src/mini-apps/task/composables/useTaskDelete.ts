import { ref, onUnmounted, type Ref } from 'vue';
import type { TaskMetadata } from '../types';
import { taskProperties } from '../types';
import { childrenOf, descendantsOf } from '../subtasks';
import { logger } from '../../../utils/logger';

/**
 * How long a deleted task stays undoable before anything is written.
 *
 * Long enough to notice and reach the button, short enough that the task is
 * not still hanging about after the user has moved on. Matches the Notes app,
 * which is where this approach came from.
 */
export const UNDO_WINDOW_MS = 7000;

/**
 * Deleting tasks, with the delete held back long enough to take it back.
 *
 * Nothing on disk is touched until the window closes. That is not a detail —
 * it is the only reason an undo can exist at all. Sync spots a deletion by
 * noticing a tracked path no longer holds a file, so the instant a file moves,
 * a tombstone is on its way to every other device; undoing after that would be
 * a race against the tombstone, and the tombstone would sometimes win. Holding
 * a timer is not a race at all, and `commands/trash.rs` says the same thing
 * from the other side.
 *
 * The same holding pattern covers the writes a subtree delete needs, not just
 * the file moves: re-parenting a kept child is applied on screen immediately
 * and written only at commit, so undo puts the whole operation back rather
 * than most of it.
 *
 * There is no confirmation dialog for an ordinary delete, deliberately. A
 * dialog asks people to be careful beforehand, which trains them to click
 * through it; an undo lets them be careless and still be fine. Only one of
 * those two actually saves anything. A parent with subtasks still asks,
 * because "keep them" and "take them too" is a real question rather than a
 * yes/no.
 *
 * If the app quits inside the window the deletion simply never happened, which
 * is the safe direction to fail in.
 */
export function useTaskDelete(params: {
  tasks: Ref<TaskMetadata[]>;
  ns: {
    trashNode: (p: { relPath: string }) => Promise<string>;
    writeNode: (p: Record<string, unknown>) => Promise<void>;
  };
  /** Say so when the files could not be moved after all. */
  onFailed: (count: number) => void;
}) {
  const { tasks, ns, onFailed } = params;

  /** A task taken off the list, and where to put it back. */
  interface Removed {
    task: TaskMetadata;
    index: number;
  }

  /** A child whose parent is going, and the parent it inherits. */
  interface Reparented {
    task: TaskMetadata;
    from: string;
    to: string;
  }

  const pending = ref<{
    removed: Removed[];
    reparented: Reparented[];
    /** What the toast says — the one task's title, or how many there were. */
    label: string;
  } | null>(null);

  let timer: ReturnType<typeof setTimeout> | undefined;

  /**
   * Ids the list must pretend are gone.
   *
   * A pending task is still on disk, and `loadTasks` runs on every file-watcher
   * tick — without this the task reappears in the list underneath the toast
   * offering to undo its deletion.
   */
  const hiddenIds = new Set<string>();
  const isHidden = (id: string) => hiddenIds.has(id);

  /** Do the work at last. Called by the timer, or early to make way. */
  const commit = async () => {
    const held = pending.value;
    if (!held) return;
    clearTimeout(timer);
    pending.value = null;
    for (const entry of held.removed) hiddenIds.delete(entry.task.id);

    try {
      // Re-parenting first: a kept child pointing at a file that is already
      // gone is the state this is here to avoid, however briefly.
      for (const entry of held.reparented) {
        await ns.writeNode({
          relPath: entry.task.path,
          nodeType: 'task',
          title: entry.task.title,
          properties: taskProperties(entry.task),
        });
      }
      // Deepest first — see `descendantsOf`. A run that stops part way leaves
      // a tree with its top attached rather than a scatter of orphans.
      for (const entry of held.removed) {
        await ns.trashNode({ relPath: entry.task.path });
      }
    } catch (e) {
      logger.error('Could not move the tasks to the trash', e);
      // They left the list when the delete was requested, so a silent failure
      // here reads as success until the next restart brings them back.
      restore(held.removed, held.reparented);
      onFailed(held.removed.length);
    }
  };

  const restore = (removed: Removed[], reparented: Reparented[]) => {
    for (const entry of reparented) entry.task.parent_id = entry.from;

    // Back where they were rather than on top. The list has an order the
    // reader recognises, and a task that jumps position on being restored
    // looks like a different task. Shallowest first, so each index still
    // refers to the position it was taken from.
    for (const entry of [...removed].reverse()) {
      const list = [...tasks.value];
      list.splice(Math.min(entry.index, list.length), 0, entry.task);
      tasks.value = list;
    }
  };

  /**
   * Take a set of tasks off the list now and schedule the real work.
   *
   * `reparent` names the children that should survive their parent, with the
   * parent they inherit — applied on screen straight away so the list during
   * the undo window shows what the delete will actually leave behind.
   */
  const scheduleDelete = async (
    toRemove: TaskMetadata[],
    reparent: Reparented[],
    label: string,
  ) => {
    if (!toRemove.length) return;

    // One operation at a time. A second delete finishes the first rather than
    // queueing, so the toast never offers to bring back something else.
    if (pending.value) await commit();

    const removed: Removed[] = [];
    for (const task of toRemove) {
      const index = tasks.value.findIndex(t => t.id === task.id);
      if (index === -1) continue;
      removed.push({ task, index });
      hiddenIds.add(task.id);
    }
    if (!removed.length) return;

    for (const entry of reparent) entry.task.parent_id = entry.to;
    const goneIds = new Set(removed.map(entry => entry.task.id));
    tasks.value = tasks.value.filter(t => !goneIds.has(t.id));

    pending.value = { removed, reparented: reparent, label };
    timer = setTimeout(() => { void commit(); }, UNDO_WINDOW_MS);
  };

  const undo = () => {
    const held = pending.value;
    if (!held) return;
    clearTimeout(timer);
    pending.value = null;
    for (const entry of held.removed) hiddenIds.delete(entry.task.id);
    restore(held.removed, held.reparented);
  };

  /** Delete one task, taking or keeping whatever sits under it. */
  const deleteTaskTree = async (
    task: TaskMetadata,
    subtasks: 'keep' | 'all',
  ) => {
    if (subtasks === 'all') {
      await scheduleDelete([...descendantsOf(task, tasks.value), task], [], task.title);
      return;
    }
    const inherited = task.parent_id || '';
    const reparent = childrenOf(task, tasks.value).map(child => ({
      task: child,
      from: child.parent_id,
      to: inherited,
    }));
    await scheduleDelete([task], reparent, task.title);
  };

  /**
   * Delete several tasks at once.
   *
   * A selected task whose parent is also selected is not listed twice, and
   * anything left under a deleted task comes with it — a selection that took
   * the parent and left the child would leave the child pointing at nothing.
   */
  const deleteMany = async (selected: TaskMetadata[], label: string) => {
    const seen = new Set<string>();
    const ordered: TaskMetadata[] = [];
    for (const task of selected) {
      for (const descendant of descendantsOf(task, tasks.value)) {
        if (seen.has(descendant.id)) continue;
        seen.add(descendant.id);
        ordered.push(descendant);
      }
    }
    for (const task of selected) {
      if (seen.has(task.id)) continue;
      seen.add(task.id);
      ordered.push(task);
    }
    await scheduleDelete(ordered, [], label);
  };

  // Leaving the app is not taking the delete back. The work has to happen, and
  // it has to happen before this composable stops existing to do it.
  onUnmounted(() => { void commit(); });

  return { pending, isHidden, scheduleDelete, deleteTaskTree, deleteMany, undo, commit };
}
