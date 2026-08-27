import { describe, it, expect, vi } from 'vitest';
import { ref, computed, nextTick } from 'vue';
import { useTaskSelection } from '../composables/useTaskSelection';
import type { TaskMetadata } from '../types';

const task = (id: string, over: Partial<TaskMetadata> = {}): TaskMetadata =>
  ({
    id, path: id, title: id, status: 'todo', priority: '', project_id: '',
    recurrence: 'none', custom_fields: {}, ...over,
  }) as TaskMetadata;

const harness = (list: TaskMetadata[]) => {
  const tasks = ref(list);
  const visible = ref(list);
  const visibleComputed = computed(() => visible.value);
  const writeNode = vi.fn(async () => {});
  const toast = vi.fn();
  const t = (key: string) => key;
  const api = useTaskSelection(visibleComputed, tasks, { writeNode }, toast, t);
  return { api, tasks, visible, writeNode, toast };
};

describe('picking tasks out', () => {
  it('starts with nothing selected and no checkboxes', () => {
    const h = harness([task('a'), task('b')]);
    expect(h.api.isSelecting.value).toBe(false);
  });

  it('toggles one task on and off again', () => {
    const h = harness([task('a'), task('b')]);
    h.api.selectOne('a');
    expect(h.api.isSelected('a')).toBe(true);
    h.api.selectOne('a');
    expect(h.api.isSelected('a')).toBe(false);
  });

  it('selects a run from the last click to this one', () => {
    const h = harness([task('a'), task('b'), task('c'), task('d')]);
    h.api.selectOne('a');
    h.api.selectRange('c');
    expect([...h.api.selectedIds.value].sort()).toEqual(['a', 'b', 'c']);
  });

  it('selects a run backwards too', () => {
    const h = harness([task('a'), task('b'), task('c')]);
    h.api.selectOne('c');
    h.api.selectRange('a');
    expect([...h.api.selectedIds.value].sort()).toEqual(['a', 'b', 'c']);
  });

  it('treats a range with no starting point as a plain click', () => {
    const h = harness([task('a'), task('b')]);
    h.api.selectRange('b');
    expect([...h.api.selectedIds.value]).toEqual(['b']);
  });

  it('selects and clears everything on screen', () => {
    const h = harness([task('a'), task('b')]);
    h.api.toggleAllVisible();
    expect(h.api.allVisibleSelected.value).toBe(true);
    h.api.toggleAllVisible();
    expect(h.api.selectedIds.value.size).toBe(0);
  });

  /**
   * A task filtered out of the view while selected would otherwise stay in the
   * set invisibly, and the next bulk action would reach a task the user can no
   * longer see.
   */
  it('drops a task that leaves the view', async () => {
    const h = harness([task('a'), task('b')]);
    h.api.selectOne('a');
    h.api.selectOne('b');
    h.visible.value = [task('b')];
    await nextTick();
    expect([...h.api.selectedIds.value]).toEqual(['b']);
  });
});

describe('acting on the selection', () => {
  it('marks the selected tasks done', async () => {
    const h = harness([task('a'), task('b'), task('c')]);
    h.api.selectOne('a');
    h.api.selectOne('b');
    await h.api.completeSelected();
    expect(h.writeNode).toHaveBeenCalledTimes(2);
    expect(h.tasks.value[0].status).toBe('done');
    expect(h.tasks.value[2].status).toBe('todo');
  });

  it('stamps a completion date', async () => {
    const h = harness([task('a')]);
    h.api.selectOne('a');
    await h.api.completeSelected();
    expect(h.tasks.value[0].completed_at).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });

  /**
   * Completing a repeating task is a decision about one occurrence. Doing that
   * to twenty at once is not what "mark these done" means to anybody.
   */
  it('leaves a repeating task alone', async () => {
    const h = harness([task('a', { recurrence: 'daily' }), task('b')]);
    h.api.toggleAllVisible();
    await h.api.completeSelected();
    expect(h.tasks.value[0].status).toBe('todo');
    expect(h.tasks.value[1].status).toBe('done');
  });

  it('sets a priority on all of them', async () => {
    const h = harness([task('a'), task('b')]);
    h.api.toggleAllVisible();
    await h.api.setPriorityOnSelection('P1');
    expect(h.tasks.value.map((t) => t.priority)).toEqual(['P1', 'P1']);
  });

  it('moves them to a project', async () => {
    const h = harness([task('a'), task('b')]);
    h.api.toggleAllVisible();
    await h.api.setProjectOnSelection('Projects/x.md');
    expect(h.tasks.value.map((t) => t.project_id)).toEqual(['Projects/x.md', 'Projects/x.md']);
  });

  it('does not write a task that already has the value', async () => {
    const h = harness([task('a', { priority: 'P1' }), task('b')]);
    h.api.toggleAllVisible();
    await h.api.setPriorityOnSelection('P1');
    expect(h.writeNode).toHaveBeenCalledTimes(1);
  });

  it('clears the selection once the change lands', async () => {
    const h = harness([task('a')]);
    h.api.selectOne('a');
    await h.api.setPriorityOnSelection('P2');
    expect(h.api.selectedIds.value.size).toBe(0);
  });

  /** Carrying on past a failure would leave nobody knowing how far it got. */
  it('stops at the first failure and says how far it got', async () => {
    const h = harness([task('a'), task('b'), task('c')]);
    h.writeNode.mockResolvedValueOnce(undefined).mockRejectedValueOnce(new Error('disk'));
    h.api.toggleAllVisible();
    await h.api.setPriorityOnSelection('P1');
    expect(h.writeNode).toHaveBeenCalledTimes(2);
    expect(h.toast).toHaveBeenCalledWith('task.bulk_failed');
  });

  it('keeps the selection after a failure, so it can be retried', async () => {
    const h = harness([task('a'), task('b')]);
    h.writeNode.mockRejectedValueOnce(new Error('disk'));
    h.api.toggleAllVisible();
    await h.api.setPriorityOnSelection('P1');
    expect(h.api.selectedIds.value.size).toBe(2);
  });

  it('does nothing at all with an empty selection', async () => {
    const h = harness([task('a')]);
    await h.api.completeSelected();
    expect(h.writeNode).not.toHaveBeenCalled();
  });
});
