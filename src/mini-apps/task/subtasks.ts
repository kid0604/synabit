/**
 * Tasks that sit under other tasks.
 *
 * A subtask is an ordinary task with a `parent_id`. Nothing about it is
 * special: it has its own status, dates, priority and project, it appears in
 * Today when it is due today, and it can be found by search. The parent link
 * only decides how the list draws it, and lets a parent report how much of
 * itself is done.
 *
 * Two things this deliberately tolerates rather than prevents, because the
 * vault is files a user can edit by hand and both are recoverable:
 *
 * - a `parent_id` naming a task that no longer exists — shown at the top level
 * - a cycle, however it got there — broken by showing the tasks at the top level
 *
 * Neither may hang the renderer, and neither may hide a task. A task that
 * cannot be drawn where it belongs is drawn where it can be seen.
 */

import type { TaskMetadata } from './types';

export interface TaskNode {
  task: TaskMetadata;
  children: TaskNode[];
  depth: number;
}

/** How deep the list indents before it stops. */
export const MAX_SUBTASK_DEPTH = 4;

const normalise = (id: string | null | undefined): string =>
  (id || '').replace(/\\/g, '/');

/**
 * A task's ancestors, nearest first, stopping at a missing parent or a cycle.
 */
function ancestorsOf(task: TaskMetadata, byId: Map<string, TaskMetadata>): TaskMetadata[] {
  const chain: TaskMetadata[] = [];
  const seen = new Set<string>([normalise(task.id)]);
  let current = byId.get(normalise(task.parent_id));
  while (current && !seen.has(normalise(current.id))) {
    chain.push(current);
    seen.add(normalise(current.id));
    current = byId.get(normalise(current.parent_id));
  }
  return chain;
}

/** Whether following `parent_id` from `task` ever arrives back at `task`. */
export function isInCycle(task: TaskMetadata, byId: Map<string, TaskMetadata>): boolean {
  const seen = new Set<string>();
  let current: TaskMetadata | undefined = byId.get(normalise(task.parent_id));
  while (current) {
    const id = normalise(current.id);
    if (id === normalise(task.id)) return true;
    if (seen.has(id)) return false;
    seen.add(id);
    current = byId.get(normalise(current.parent_id));
  }
  return false;
}

/**
 * The visible tasks arranged as a forest, in the order they were given.
 *
 * A child whose parent is not in `tasks` becomes a root. That is what keeps a
 * subtask visible when the view is filtered to Today and its parent is not due
 * — hiding it would mean a task the user asked to see does not appear.
 */
export function buildTaskTree(tasks: TaskMetadata[]): TaskNode[] {
  const byId = new Map<string, TaskMetadata>();
  for (const task of tasks) byId.set(normalise(task.id), task);

  const nodes = new Map<string, TaskNode>();
  for (const task of tasks) {
    nodes.set(normalise(task.id), { task, children: [], depth: 0 });
  }

  const roots: TaskNode[] = [];
  for (const task of tasks) {
    const node = nodes.get(normalise(task.id))!;
    const parentId = normalise(task.parent_id);
    const parent = parentId && parentId !== normalise(task.id) ? nodes.get(parentId) : undefined;
    if (parent && !isInCycle(task, byId)) {
      parent.children.push(node);
    } else {
      roots.push(node);
    }
  }

  const setDepth = (node: TaskNode, depth: number) => {
    node.depth = depth;
    for (const child of node.children) setDepth(child, depth + 1);
  };
  for (const root of roots) setDepth(root, 0);

  return roots;
}

/** A forest flattened back into a list, parents immediately before their children. */
export function flattenTaskTree(roots: TaskNode[]): TaskNode[] {
  const out: TaskNode[] = [];
  const walk = (node: TaskNode) => {
    out.push(node);
    for (const child of node.children) walk(child);
  };
  for (const root of roots) walk(root);
  return out;
}

const indexByParent = (tasks: TaskMetadata[]): Map<string, TaskMetadata[]> => {
  const byParent = new Map<string, TaskMetadata[]>();
  for (const task of tasks) {
    const parent = normalise(task.parent_id);
    if (!parent) continue;
    const siblings = byParent.get(parent);
    if (siblings) siblings.push(task);
    else byParent.set(parent, [task]);
  }
  return byParent;
};

/** The tasks whose `parent_id` names this one. Not their children. */
export function childrenOf(task: TaskMetadata, tasks: TaskMetadata[]): TaskMetadata[] {
  return indexByParent(tasks).get(normalise(task.id)) ?? [];
}

/**
 * Everything beneath a task, deepest first.
 *
 * Deepest first because the caller deleting a subtree wants to remove the
 * leaves before the branches: if it stops halfway, what is left is a tree with
 * its top intact rather than a set of orphans with nothing to hang from.
 *
 * A cycle cannot make this loop, and cannot make a task appear twice.
 */
export function descendantsOf(task: TaskMetadata, tasks: TaskMetadata[]): TaskMetadata[] {
  const byParent = indexByParent(tasks);
  const seen = new Set<string>([normalise(task.id)]);
  const out: Array<{ task: TaskMetadata; depth: number }> = [];

  const walk = (parent: TaskMetadata, depth: number) => {
    for (const child of byParent.get(normalise(parent.id)) ?? []) {
      const id = normalise(child.id);
      if (seen.has(id)) continue;
      seen.add(id);
      out.push({ task: child, depth });
      walk(child, depth + 1);
    }
  };
  walk(task, 1);

  return out.sort((a, b) => b.depth - a.depth).map((entry) => entry.task);
}

/**
 * How much of a task's subtree is finished.
 *
 * Counts every descendant, not just the immediate children: a parent showing
 * 1/2 while one of those two has three unfinished children of its own is
 * reporting something nobody asked about. The parent itself is not counted —
 * it is the thing being reported on.
 */
export function subtaskProgress(
  task: TaskMetadata,
  tasks: TaskMetadata[],
): { done: number; total: number } {
  const descendants = descendantsOf(task, tasks);
  return {
    done: descendants.filter((t) => t.status === 'done').length,
    total: descendants.length,
  };
}

/**
 * The same figures for every task, in one pass over the list.
 *
 * `subtaskProgress` rebuilds its parent index each time it is called, which is
 * fine for one task and quadratic for a list — the row renderer was calling it
 * once per parent, so a vault of a thousand tasks did a thousand-entry index
 * build for each one of them, on every re-render.
 *
 * Counted from the leaves up: a task's totals are its children's totals plus
 * the children themselves, so each task is visited once however deep the tree.
 */
export function allSubtaskProgress(
  tasks: TaskMetadata[],
): Map<string, { done: number; total: number }> {
  const byParent = indexByParent(tasks);
  const progress = new Map<string, { done: number; total: number }>();

  const visiting = new Set<string>();
  const compute = (id: string): { done: number; total: number } => {
    const cached = progress.get(id);
    if (cached) return cached;

    // A cycle would otherwise recurse forever. Reporting zero for the task
    // that closes the loop keeps the count finite and the list drawable.
    if (visiting.has(id)) return { done: 0, total: 0 };
    visiting.add(id);

    let done = 0;
    let total = 0;
    for (const child of byParent.get(id) ?? []) {
      const below = compute(normalise(child.id));
      total += 1 + below.total;
      done += (child.status === 'done' ? 1 : 0) + below.done;
    }

    visiting.delete(id);
    const result = { done, total };
    progress.set(id, result);
    return result;
  };

  for (const task of tasks) compute(normalise(task.id));
  return progress;
}

/**
 * The tasks that may be chosen as a parent for `task`.
 *
 * Excludes the task itself and everything beneath it, because either would
 * make a cycle — the picker is where a cycle would be created deliberately,
 * so it is where it is prevented.
 */
export function eligibleParents(task: TaskMetadata, tasks: TaskMetadata[]): TaskMetadata[] {
  const byId = new Map<string, TaskMetadata>();
  for (const t of tasks) byId.set(normalise(t.id), t);

  const taskId = normalise(task.id);
  return tasks.filter((candidate) => {
    const id = normalise(candidate.id);
    if (id === taskId) return false;
    return !ancestorsOf(candidate, byId).some((a) => normalise(a.id) === taskId);
  });
}
