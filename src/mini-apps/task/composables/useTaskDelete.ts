import { ref, onBeforeUnmount, onDeactivated, getCurrentInstance, type Ref } from 'vue';
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
export const UNDO_WINDOW_MS = 10000;

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
 * This composable asks nothing. The one yes/no a delete may have — the
 * app-wide "Ask before deleting" (`confirmDelete`) — is asked by the caller
 * before anything is scheduled here, and is off by default: the undo is the
 * way back. A parent with subtasks still asks, because "keep them" and "take
 * them too" is a real question rather than a yes/no.
 *
 * If the app is killed inside the window the deletion simply never happened,
 * which is the safe direction to fail in — but hiding or leaving the app
 * commits first, so that is rare.
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
  /** When the running timer fires, so a pause can keep what was left. */
  let deadline = 0;
  let remaining = UNDO_WINDOW_MS;

  /**
   * Ids the list must pretend are gone.
   *
   * A pending task is still on disk, and `loadTasks` runs on every file-watcher
   * tick — without this the task reappears in the list underneath the toast
   * offering to undo its deletion. An id leaves the set only once its file is
   * in the trash (or the delete is undone or has failed): cleared any earlier,
   * a reload landing mid-commit flashes the task back.
   */
  const hiddenIds = new Set<string>();
  const isHidden = (id: string) => hiddenIds.has(id);

  type Held = NonNullable<typeof pending.value>;

  /**
   * Take the waiting delete off the toast, synchronously. Everything that ends
   * a wait goes through here before any `await` — see `useUndoableAction`,
   * where an await between reading and clearing let a third delete be
   * overwritten by the second and never happen.
   */
  const take = (): Held | null => {
    clearTimeout(timer);
    timer = undefined;
    const held = pending.value;
    pending.value = null;
    return held;
  };

  const arm = (ms: number) => {
    clearTimeout(timer);
    deadline = Date.now() + ms;
    timer = setTimeout(() => { void commit(); }, ms);
  };

  /** The real work for one held delete. */
  const perform = async (held: Held) => {
    const written: Reparented[] = [];
    const trashed = new Set<string>();
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
        written.push(entry);
      }
      // Deepest first — see `descendantsOf`. A run that stops part way leaves
      // a tree with its top attached rather than a scatter of orphans.
      for (const entry of held.removed) {
        await ns.trashNode({ relPath: entry.task.path });
        trashed.add(entry.task.id);
        hiddenIds.delete(entry.task.id);
      }
    } catch (e) {
      logger.error('Could not move the tasks to the trash', e);
      const left = held.removed.filter(entry => !trashed.has(entry.task.id));
      for (const entry of left) hiddenIds.delete(entry.task.id);
      // A child re-parented on disk whose old parent survived must point back
      // at it, or the restored list shows one tree and the files another.
      const parentStays = new Set(left.map(entry => entry.task.id));
      const undoParent = held.reparented.filter(entry => parentStays.has(entry.from));
      for (const entry of undoParent) entry.task.parent_id = entry.from;
      for (const entry of written.filter(w => parentStays.has(w.from))) {
        try {
          await ns.writeNode({
            relPath: entry.task.path,
            nodeType: 'task',
            title: entry.task.title,
            properties: taskProperties(entry.task),
          });
        } catch (err) {
          logger.error('Could not put a subtask back under its parent', err);
        }
      }
      // They left the list when the delete was requested, so a silent failure
      // here reads as success until the next restart brings them back. Only
      // what is still on disk comes back.
      restore(left, []);
      onFailed(left.length);
    }
  };

  /** Do the work at last. Called by the timer, or early to make way. */
  const commit = async () => {
    const held = take();
    if (held) await perform(held);
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

    // One operation at a time. A second delete finishes the first rather than
    // queueing, so the toast never offers to bring back something else. The
    // first is taken synchronously; its write is awaited only afterwards.
    const previous = take();
    pending.value = { removed, reparented: reparent, label };
    remaining = UNDO_WINDOW_MS;
    arm(UNDO_WINDOW_MS);
    if (previous) await perform(previous);
  };

  const undo = () => {
    const held = take();
    if (!held) return;
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

  /** Hold the countdown while the pointer or focus is on the toast (WCAG 2.2.1). */
  const pause = () => {
    if (!pending.value || timer === undefined) return;
    remaining = Math.max(0, deadline - Date.now());
    clearTimeout(timer);
    timer = undefined;
  };

  const resume = () => {
    if (pending.value && timer === undefined) arm(remaining);
  };

  // Leaving the app is not taking the delete back. The work has to happen, and
  // it has to happen before this composable stops existing to do it — or can
  // no longer be seen: the apps live in a `<keep-alive>`, so switching app
  // deactivates rather than unmounts, and Android kills a hidden app without
  // warning, `visibilitychange` being the last moment JavaScript is given.
  const onHidden = () => {
    if (document.visibilityState === 'hidden') void commit();
  };
  if (getCurrentInstance()) {
    document.addEventListener('visibilitychange', onHidden);
    onDeactivated(() => { void commit(); });
    onBeforeUnmount(() => {
      document.removeEventListener('visibilitychange', onHidden);
      void commit();
    });
  }

  return { pending, isHidden, scheduleDelete, deleteTaskTree, deleteMany, undo, commit, pause, resume };
}
