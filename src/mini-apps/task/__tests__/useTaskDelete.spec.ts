import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ref } from 'vue';
import { useTaskDelete, UNDO_WINDOW_MS } from '../composables/useTaskDelete';
import type { TaskMetadata } from '../types';

const task = (id: string, parent = ''): TaskMetadata =>
  ({ id, path: id, title: id, parent_id: parent, status: 'todo', custom_fields: {} }) as TaskMetadata;

const harness = (list: TaskMetadata[]) => {
  const tasks = ref(list);
  // Typed with their arguments so the assertions below can read them back.
  const trashNode = vi.fn(async (_p: { relPath: string }) => '.trash/x.md');
  const writeNode = vi.fn(async (_p: Record<string, unknown>) => {});
  const onFailed = vi.fn();
  const api = useTaskDelete({ tasks, ns: { trashNode, writeNode }, onFailed });
  return { api, tasks, trashNode, writeNode, onFailed };
};

const ids = (tasks: TaskMetadata[]) => tasks.map((t) => t.id);

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

/**
 * The whole design in one line: nothing reaches disk until the window closes,
 * because sync turns a moved file into a tombstone the moment it moves.
 */
describe('the undo window', () => {
  it('takes the task off the list at once', async () => {
    const h = harness([task('a'), task('b')]);
    await h.api.deleteTaskTree(h.tasks.value[0], 'keep');
    expect(ids(h.tasks.value)).toEqual(['b']);
  });

  it('writes nothing while the window is open', async () => {
    const h = harness([task('a')]);
    await h.api.deleteTaskTree(h.tasks.value[0], 'keep');
    await vi.advanceTimersByTimeAsync(UNDO_WINDOW_MS - 500);
    expect(h.trashNode).not.toHaveBeenCalled();
  });

  it('moves the file once the window closes', async () => {
    const h = harness([task('a')]);
    await h.api.deleteTaskTree(h.tasks.value[0], 'keep');
    await vi.advanceTimersByTimeAsync(UNDO_WINDOW_MS + 100);
    expect(h.trashNode).toHaveBeenCalledWith({ relPath: 'a' });
  });

  it('touches nothing at all when undone', async () => {
    const h = harness([task('a')]);
    await h.api.deleteTaskTree(h.tasks.value[0], 'keep');
    h.api.undo();
    await vi.advanceTimersByTimeAsync(UNDO_WINDOW_MS * 2);
    expect(h.trashNode).not.toHaveBeenCalled();
    expect(h.writeNode).not.toHaveBeenCalled();
  });

  /** A task that jumps to the top on being restored reads as a different task. */
  it('puts the task back where it was', async () => {
    const h = harness([task('a'), task('b'), task('c')]);
    await h.api.deleteTaskTree(h.tasks.value[1], 'keep');
    expect(ids(h.tasks.value)).toEqual(['a', 'c']);
    h.api.undo();
    expect(ids(h.tasks.value)).toEqual(['a', 'b', 'c']);
  });

  /** Otherwise the watcher's next reload puts it straight back under the toast. */
  it('reports a pending task as hidden, and stops once it is gone', async () => {
    const h = harness([task('a')]);
    await h.api.deleteTaskTree(h.tasks.value[0], 'keep');
    expect(h.api.isHidden('a')).toBe(true);
    await vi.advanceTimersByTimeAsync(UNDO_WINDOW_MS + 100);
    expect(h.api.isHidden('a')).toBe(false);
  });

  it('stops hiding a task that was undone', async () => {
    const h = harness([task('a')]);
    await h.api.deleteTaskTree(h.tasks.value[0], 'keep');
    h.api.undo();
    expect(h.api.isHidden('a')).toBe(false);
  });

  /** The toast must never offer to bring back something other than what it names. */
  it('commits the first delete when a second one starts', async () => {
    const h = harness([task('a'), task('b')]);
    await h.api.deleteTaskTree(h.tasks.value[0], 'keep');
    await h.api.deleteTaskTree(h.tasks.value.find((t) => t.id === 'b')!, 'keep');
    expect(h.trashNode).toHaveBeenCalledWith({ relPath: 'a' });
    expect(h.api.pending.value?.removed.map((r) => r.task.id)).toEqual(['b']);
  });
});

describe('keeping subtasks', () => {
  it('hands the children to the deleted task’s own parent', async () => {
    const all = [task('gran'), task('parent', 'gran'), task('child', 'parent')];
    const h = harness(all);
    await h.api.deleteTaskTree(all[1], 'keep');
    expect(all[2].parent_id).toBe('gran');
  });

  it('shows the new parent immediately, before anything is written', async () => {
    const all = [task('gran'), task('parent', 'gran'), task('child', 'parent')];
    const h = harness(all);
    await h.api.deleteTaskTree(all[1], 'keep');
    expect(all[2].parent_id).toBe('gran');
    expect(h.writeNode).not.toHaveBeenCalled();
  });

  it('puts the old parent back when undone', async () => {
    const all = [task('gran'), task('parent', 'gran'), task('child', 'parent')];
    const h = harness(all);
    await h.api.deleteTaskTree(all[1], 'keep');
    h.api.undo();
    expect(all[2].parent_id).toBe('parent');
  });

  it('writes the re-parent only when the window closes', async () => {
    const all = [task('parent'), task('child', 'parent')];
    const h = harness(all);
    await h.api.deleteTaskTree(all[0], 'keep');
    await vi.advanceTimersByTimeAsync(UNDO_WINDOW_MS + 100);
    expect(h.writeNode).toHaveBeenCalledTimes(1);
    expect(h.writeNode.mock.calls[0][0]).toMatchObject({ relPath: 'child' });
  });

  it('leaves the children on the list', async () => {
    const all = [task('parent'), task('child', 'parent')];
    const h = harness(all);
    await h.api.deleteTaskTree(all[0], 'keep');
    expect(ids(h.tasks.value)).toEqual(['child']);
  });
});

describe('deleting the whole subtree', () => {
  it('takes every descendant off the list', async () => {
    const all = [task('a'), task('b', 'a'), task('c', 'b'), task('other')];
    const h = harness(all);
    await h.api.deleteTaskTree(all[0], 'all');
    expect(ids(h.tasks.value)).toEqual(['other']);
  });

  /** Leaves before branches: a run that stops leaves a tree, not orphans. */
  it('trashes the deepest task first and the parent last', async () => {
    const all = [task('a'), task('b', 'a'), task('c', 'b')];
    const h = harness(all);
    await h.api.deleteTaskTree(all[0], 'all');
    await vi.advanceTimersByTimeAsync(UNDO_WINDOW_MS + 100);
    expect(h.trashNode.mock.calls.map((c: any) => c[0].relPath)).toEqual(['c', 'b', 'a']);
  });

  it('restores the whole subtree in its original order', async () => {
    const all = [task('a'), task('b', 'a'), task('c', 'b'), task('other')];
    const h = harness(all);
    await h.api.deleteTaskTree(all[0], 'all');
    h.api.undo();
    expect(ids(h.tasks.value)).toEqual(['a', 'b', 'c', 'other']);
  });
});

describe('deleting a selection', () => {
  it('deletes each selected task', async () => {
    const all = [task('a'), task('b'), task('c')];
    const h = harness(all);
    await h.api.deleteMany([all[0], all[2]], '2 tasks');
    expect(ids(h.tasks.value)).toEqual(['b']);
  });

  /** Otherwise the surviving child points at a parent that no longer exists. */
  it('takes the subtasks of a selected task with it', async () => {
    const all = [task('a'), task('kid', 'a'), task('b')];
    const h = harness(all);
    await h.api.deleteMany([all[0]], '1 task');
    expect(ids(h.tasks.value)).toEqual(['b']);
  });

  it('does not delete the same task twice when a parent and child are both selected', async () => {
    const all = [task('a'), task('kid', 'a')];
    const h = harness(all);
    await h.api.deleteMany([all[0], all[1]], '2 tasks');
    await vi.advanceTimersByTimeAsync(UNDO_WINDOW_MS + 100);
    expect(h.trashNode.mock.calls.map((c: any) => c[0].relPath)).toEqual(['kid', 'a']);
  });

  it('restores every one of them', async () => {
    const all = [task('a'), task('b'), task('c')];
    const h = harness(all);
    await h.api.deleteMany([all[0], all[2]], '2 tasks');
    h.api.undo();
    expect(ids(h.tasks.value)).toEqual(['a', 'b', 'c']);
  });
});

describe('when the move fails', () => {
  /** They left the list at request time, so silence would read as success. */
  it('puts the tasks back and says so', async () => {
    const h = harness([task('a'), task('b')]);
    h.trashNode.mockRejectedValueOnce(new Error('disk is full'));
    await h.api.deleteTaskTree(h.tasks.value[0], 'keep');
    await vi.advanceTimersByTimeAsync(UNDO_WINDOW_MS + 100);
    expect(ids(h.tasks.value)).toEqual(['a', 'b']);
    expect(h.onFailed).toHaveBeenCalledWith(1);
  });
});
