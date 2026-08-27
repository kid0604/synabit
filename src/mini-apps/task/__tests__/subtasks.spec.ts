import { describe, it, expect } from 'vitest';
import {
  allSubtaskProgress,
  childrenOf,
  descendantsOf,
  buildTaskTree,
  flattenTaskTree,
  subtaskProgress,
  eligibleParents,
  isInCycle,
} from '../subtasks';
import type { TaskMetadata } from '../types';

const task = (id: string, parent = '', status = 'todo'): TaskMetadata =>
  ({ id, path: id, title: id, parent_id: parent, status }) as TaskMetadata;

const ids = (tasks: { task: TaskMetadata }[]) => tasks.map((n) => n.task.id);

describe('buildTaskTree', () => {
  it('nests a child under its parent', () => {
    const roots = buildTaskTree([task('a'), task('b', 'a')]);
    expect(ids(roots)).toEqual(['a']);
    expect(ids(roots[0].children)).toEqual(['b']);
  });

  it('records how deep each task sits', () => {
    const roots = buildTaskTree([task('a'), task('b', 'a'), task('c', 'b')]);
    const flat = flattenTaskTree(roots);
    expect(flat.map((n) => [n.task.id, n.depth])).toEqual([['a', 0], ['b', 1], ['c', 2]]);
  });

  it('lists a parent immediately before its children', () => {
    const roots = buildTaskTree([task('a'), task('z'), task('b', 'a')]);
    expect(ids(flattenTaskTree(roots))).toEqual(['a', 'b', 'z']);
  });

  /**
   * The filtered-view case. Today shows a subtask that is due today while its
   * parent is not; hiding it because the parent is absent would drop a task
   * the user explicitly asked to see.
   */
  it('shows a child whose parent is not in the list', () => {
    const roots = buildTaskTree([task('b', 'Tasks/missing.md')]);
    expect(ids(roots)).toEqual(['b']);
    expect(roots[0].depth).toBe(0);
  });

  it('reads a Windows parent path as the same task', () => {
    const roots = buildTaskTree([task('Tasks/a.md'), task('Tasks/b.md', 'Tasks\\a.md')]);
    expect(ids(roots)).toEqual(['Tasks/a.md']);
    expect(ids(roots[0].children)).toEqual(['Tasks/b.md']);
  });

  /** Hand-edited files can say anything; nothing here may hang or vanish. */
  it('does not lose a task that is its own parent', () => {
    const roots = buildTaskTree([task('a', 'a')]);
    expect(ids(roots)).toEqual(['a']);
  });

  it('does not hang or lose tasks in a two-task cycle', () => {
    const roots = buildTaskTree([task('a', 'b'), task('b', 'a')]);
    expect(ids(flattenTaskTree(roots)).sort()).toEqual(['a', 'b']);
  });

  it('does not hang or lose tasks in a longer cycle', () => {
    const roots = buildTaskTree([task('a', 'c'), task('b', 'a'), task('c', 'b')]);
    expect(ids(flattenTaskTree(roots)).sort()).toEqual(['a', 'b', 'c']);
  });

  it('keeps every task given to it', () => {
    const input = [task('a'), task('b', 'a'), task('c', 'missing'), task('d', 'd'), task('e')];
    expect(flattenTaskTree(buildTaskTree(input))).toHaveLength(input.length);
  });

  it('handles an empty list', () => {
    expect(buildTaskTree([])).toEqual([]);
  });
});

describe('subtaskProgress', () => {
  it('counts nothing for a task with no children', () => {
    expect(subtaskProgress(task('a'), [task('a')])).toEqual({ done: 0, total: 0 });
  });

  it('counts the immediate children', () => {
    const all = [task('a'), task('b', 'a', 'done'), task('c', 'a')];
    expect(subtaskProgress(all[0], all)).toEqual({ done: 1, total: 2 });
  });

  /** 1/2 while a grandchild is outstanding is a number that misleads. */
  it('counts every descendant, not just the first level', () => {
    const all = [task('a'), task('b', 'a', 'done'), task('c', 'b'), task('d', 'b', 'done')];
    expect(subtaskProgress(all[0], all)).toEqual({ done: 2, total: 3 });
  });

  it('does not count the task itself', () => {
    const all = [task('a', '', 'done'), task('b', 'a')];
    expect(subtaskProgress(all[0], all)).toEqual({ done: 0, total: 1 });
  });

  it('terminates on a cycle', () => {
    const all = [task('a', 'c'), task('b', 'a'), task('c', 'b')];
    expect(subtaskProgress(all[0], all).total).toBeLessThanOrEqual(all.length);
  });
});

describe('eligibleParents', () => {
  it('offers the other tasks', () => {
    const all = [task('a'), task('b'), task('c')];
    expect(eligibleParents(all[0], all).map((t) => t.id)).toEqual(['b', 'c']);
  });

  it('never offers the task itself', () => {
    const all = [task('a')];
    expect(eligibleParents(all[0], all)).toEqual([]);
  });

  /** The picker is where a cycle would be made on purpose, so it is where it is stopped. */
  it('never offers a child of the task', () => {
    const all = [task('a'), task('b', 'a')];
    expect(eligibleParents(all[0], all)).toEqual([]);
  });

  it('never offers a deeper descendant', () => {
    const all = [task('a'), task('b', 'a'), task('c', 'b')];
    expect(eligibleParents(all[0], all)).toEqual([]);
  });

  it('still offers an unrelated branch', () => {
    const all = [task('a'), task('b', 'a'), task('x'), task('y', 'x')];
    expect(eligibleParents(all[0], all).map((t) => t.id)).toEqual(['x', 'y']);
  });
});

describe('isInCycle', () => {
  const byId = (tasks: TaskMetadata[]) => new Map(tasks.map((t) => [t.id, t]));

  it('is false for an ordinary chain', () => {
    const all = [task('a'), task('b', 'a')];
    expect(isInCycle(all[1], byId(all))).toBe(false);
  });

  it('is true when the chain comes back round', () => {
    const all = [task('a', 'b'), task('b', 'a')];
    expect(isInCycle(all[0], byId(all))).toBe(true);
  });

  it('is false for a cycle the task is not part of', () => {
    const all = [task('a'), task('b', 'c'), task('c', 'b')];
    expect(isInCycle(all[0], byId(all))).toBe(false);
  });
});

describe('childrenOf', () => {
  it('returns only the immediate children', () => {
    const all = [task('a'), task('b', 'a'), task('c', 'b'), task('d', 'a')];
    expect(childrenOf(all[0], all).map((t) => t.id)).toEqual(['b', 'd']);
  });

  it('returns nothing for a leaf', () => {
    const all = [task('a'), task('b', 'a')];
    expect(childrenOf(all[1], all)).toEqual([]);
  });

  it('matches a Windows parent path', () => {
    const all = [task('Tasks/a.md'), task('Tasks/b.md', 'Tasks\\a.md')];
    expect(childrenOf(all[0], all).map((t) => t.id)).toEqual(['Tasks/b.md']);
  });
});

describe('descendantsOf', () => {
  it('returns the whole subtree', () => {
    const all = [task('a'), task('b', 'a'), task('c', 'b'), task('d', 'a')];
    expect(descendantsOf(all[0], all).map((t) => t.id).sort()).toEqual(['b', 'c', 'd']);
  });

  /**
   * The order the delete relies on. Removing leaves before branches means a
   * run that stops half way leaves a tree with its top attached, not a scatter
   * of orphans pointing at a parent that is already gone.
   */
  it('returns the deepest tasks first', () => {
    const all = [task('a'), task('b', 'a'), task('c', 'b'), task('d', 'c')];
    expect(descendantsOf(all[0], all).map((t) => t.id)).toEqual(['d', 'c', 'b']);
  });

  it('does not include the task itself', () => {
    const all = [task('a'), task('b', 'a')];
    expect(descendantsOf(all[0], all).map((t) => t.id)).not.toContain('a');
  });

  it('returns nothing for a task with no children', () => {
    expect(descendantsOf(task('a'), [task('a'), task('x')])).toEqual([]);
  });

  it('terminates on a cycle and lists each task once', () => {
    const all = [task('a', 'c'), task('b', 'a'), task('c', 'b')];
    const found = descendantsOf(all[0], all).map((t) => t.id);
    expect(new Set(found).size).toBe(found.length);
    expect(found).not.toContain('a');
  });

  it('does not follow a sibling branch', () => {
    const all = [task('a'), task('b', 'a'), task('x'), task('y', 'x')];
    expect(descendantsOf(all[0], all).map((t) => t.id)).toEqual(['b']);
  });
});

/**
 * The list renderer needs one figure per row, and calling `subtaskProgress`
 * from the template rebuilt the parent index once per parent — quadratic in
 * the size of the vault. These pin that the batched version agrees with the
 * single one, so the fast path cannot drift from the correct one.
 */
describe('allSubtaskProgress', () => {
  const agrees = (all: TaskMetadata[]) => {
    const batch = allSubtaskProgress(all);
    for (const t of all) {
      expect(batch.get(t.id), t.id).toEqual(subtaskProgress(t, all));
    }
  };

  it('agrees with the single-task version on a flat list', () => {
    agrees([task('a'), task('b'), task('c')]);
  });

  it('agrees on one level of children', () => {
    agrees([task('a'), task('b', 'a', 'done'), task('c', 'a')]);
  });

  it('agrees on a deep tree', () => {
    agrees([task('a'), task('b', 'a', 'done'), task('c', 'b'), task('d', 'c', 'done')]);
  });

  it('agrees on several separate trees', () => {
    agrees([task('a'), task('b', 'a'), task('x'), task('y', 'x', 'done'), task('lone')]);
  });

  it('agrees when a parent is missing from the list', () => {
    agrees([task('orphan', 'gone'), task('a'), task('b', 'a')]);
  });

  it('has an entry for every task, including the leaves', () => {
    const all = [task('a'), task('b', 'a')];
    const batch = allSubtaskProgress(all);
    expect(batch.get('a')).toEqual({ done: 0, total: 1 });
    expect(batch.get('b')).toEqual({ done: 0, total: 0 });
  });

  it('terminates on a cycle', () => {
    const batch = allSubtaskProgress([task('a', 'c'), task('b', 'a'), task('c', 'b')]);
    expect(batch.size).toBe(3);
  });

  it('handles an empty list', () => {
    expect(allSubtaskProgress([]).size).toBe(0);
  });
});
