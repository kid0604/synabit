import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ref, computed } from 'vue';
import { setActivePinia, createPinia } from 'pinia';
import type { TaskMetadata } from '../types';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { useTaskCrud } from '../composables/useTaskCrud';
import { useAppStore } from '../../../stores/useAppStore';
import { pendingDeleteQuestion, answerDeleteQuestion } from '../../../composables/useConfirmDelete';

const task = (id: string, parent = ''): TaskMetadata =>
  ({ id, path: id, title: `Task ${id}`, parent_id: parent, status: 'todo', custom_fields: {} }) as TaskMetadata;

const harness = (list: TaskMetadata[]) => {
  const tasks = ref(list);
  const ns = { trashNode: vi.fn(async () => '.trash/x.md'), writeNode: vi.fn(async () => {}) };
  const bus = { emit: vi.fn(), on: vi.fn(), off: vi.fn() };
  const crud = useTaskCrud(
    tasks, ref([]), ref('/vault'), ns, bus,
    ref('all'), computed(() => null), ref(0),
  );
  return { crud, tasks, ns };
};

const flush = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  setActivePinia(createPinia());
  pendingDeleteQuestion.value = null;
});
afterEach(() => {
  pendingDeleteQuestion.value = null;
});

/** One press, one undo toast — unless "Ask before deleting" is on. */
describe('deleting a task', () => {
  it('asks nothing when the setting is off', async () => {
    const h = harness([task('a'), task('b')]);
    await h.crud.deleteTask(h.tasks.value[0]);
    expect(pendingDeleteQuestion.value).toBeNull();
    expect(h.tasks.value.map((t) => t.id)).toEqual(['b']);
    expect(h.crud.pendingDelete.value).not.toBeNull();
  });

  it('asks by name when the setting is on, and does nothing on no', async () => {
    useAppStore().confirmBeforeDelete = true;
    const h = harness([task('a'), task('b')]);
    const done = h.crud.deleteTask(h.tasks.value[0]);
    await flush();
    expect(pendingDeleteQuestion.value?.name).toBe('Task a');
    answerDeleteQuestion(false);
    await done;
    expect(h.tasks.value.map((t) => t.id)).toEqual(['a', 'b']);
    expect(h.crud.pendingDelete.value).toBeNull();
  });

  it('deletes on yes', async () => {
    useAppStore().confirmBeforeDelete = true;
    const h = harness([task('a'), task('b')]);
    const done = h.crud.deleteTask(h.tasks.value[0]);
    await flush();
    answerDeleteQuestion(true);
    await done;
    expect(h.tasks.value.map((t) => t.id)).toEqual(['b']);
  });

  /** The subtask question is the answer; a second yes/no after it is noise. */
  it('does not ask again after the subtask question', async () => {
    useAppStore().confirmBeforeDelete = true;
    const h = harness([task('a'), task('c', 'a')]);
    const done = h.crud.deleteTask(h.tasks.value[0]);
    await flush();
    expect(h.crud.pendingSubtreeDelete.value?.count).toBe(1);
    h.crud.answerSubtreeDelete('all');
    await done;
    expect(pendingDeleteQuestion.value).toBeNull();
    expect(h.tasks.value).toEqual([]);
  });
});
