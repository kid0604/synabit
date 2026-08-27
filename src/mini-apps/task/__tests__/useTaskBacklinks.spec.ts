import { describe, it, expect, vi } from 'vitest';
import { ref, nextTick } from 'vue';
import { useTaskBacklinks } from '../composables/useTaskBacklinks';
import type { TaskMetadata } from '../types';

const task = (id: string, title = id): TaskMetadata =>
  ({ id, path: id, title } as TaskMetadata);

const node = (id: string, node_type: string, over: Record<string, unknown> = {}) =>
  ({ id, node_type, title: id, content: '', updated_at: '2026-01-01', ...over });

const harness = (linked: unknown[] = []) => {
  const editingTask = ref<TaskMetadata | null>(null);
  const getLinkedNodes = vi.fn(async (_t: string, _id: string) => linked);
  const api = useTaskBacklinks(editingTask, { getLinkedNodes });
  return { api, editingTask, getLinkedNodes };
};

const settle = async () => { await nextTick(); await Promise.resolve(); await Promise.resolve(); };

describe('loading a task’s backlinks', () => {
  it('asks for nothing until a task is opened', () => {
    const h = harness();
    expect(h.getLinkedNodes).not.toHaveBeenCalled();
  });

  it('asks by the task’s id, so a rename does not lose them', async () => {
    const h = harness();
    h.editingTask.value = task('Tasks/a.md', 'Ship it');
    await settle();
    expect(h.getLinkedNodes).toHaveBeenCalledWith('Ship it', 'Tasks/a.md');
  });

  it('reports what points at the task', async () => {
    const h = harness([node('Notes/plan.md', 'note', { title: 'Sprint plan', content: 'we agreed to ship' })]);
    h.editingTask.value = task('Tasks/a.md');
    await settle();
    expect(h.api.backlinks.value).toEqual([{
      id: 'Notes/plan.md', node_type: 'note', title: 'Sprint plan',
      preview: 'we agreed to ship', updated_at: '2026-01-01',
    }]);
  });

  it('keeps every kind of node, not only notes', async () => {
    const h = harness([
      node('Notes/a.md', 'note'), node('Events/b.md', 'event'),
      node('People/c.md', 'person'), node('Tasks/d.md', 'task'),
    ]);
    h.editingTask.value = task('Tasks/a.md');
    await settle();
    expect(h.api.backlinks.value.map((b) => b.node_type).sort())
      .toEqual(['event', 'note', 'person', 'task']);
  });

  /** A note pasted into its own task's body is noise, not a reference. */
  it('does not list the task as pointing at itself', async () => {
    const h = harness([node('Tasks/a.md', 'task'), node('Notes/x.md', 'note')]);
    h.editingTask.value = task('Tasks/a.md');
    await settle();
    expect(h.api.backlinks.value.map((b) => b.id)).toEqual(['Notes/x.md']);
  });

  it('collapses whitespace in the preview and cuts it short', async () => {
    const long = 'a'.repeat(300);
    const h = harness([node('Notes/a.md', 'note', { content: `  first\n\n   second ${long}` })]);
    h.editingTask.value = task('Tasks/a.md');
    await settle();
    const preview = h.api.backlinks.value[0].preview;
    expect(preview.startsWith('first second')).toBe(true);
    expect(preview.length).toBeLessThanOrEqual(120);
  });

  it('falls back to the filename when a node has no title', async () => {
    const h = harness([node('Notes/untitled.md', 'note', { title: '' })]);
    h.editingTask.value = task('Tasks/a.md');
    await settle();
    expect(h.api.backlinks.value[0].title).toBe('untitled.md');
  });

  it('clears the panel when the modal is closed', async () => {
    const h = harness([node('Notes/a.md', 'note')]);
    h.editingTask.value = task('Tasks/a.md');
    await settle();
    h.editingTask.value = null;
    await settle();
    expect(h.api.backlinks.value).toEqual([]);
  });

  it('shows an empty panel rather than failing when the query errors', async () => {
    const h = harness();
    h.getLinkedNodes.mockRejectedValueOnce(new Error('db is busy'));
    h.editingTask.value = task('Tasks/a.md');
    await settle();
    expect(h.api.backlinks.value).toEqual([]);
    expect(h.api.loading.value).toBe(false);
  });
});

/**
 * Opening two tasks quickly issues two queries, and the slower one can land
 * second. Without the guard the panel shows the first task's backlinks under
 * the second task's title — which reads as the app attributing someone else's
 * notes to this task.
 */
describe('a request that has been overtaken', () => {
  it('does not overwrite the newer task’s backlinks', async () => {
    const editingTask = ref<TaskMetadata | null>(null);
    let resolveFirst: (v: unknown[]) => void = () => {};
    const getLinkedNodes = vi.fn()
      .mockImplementationOnce(() => new Promise((r) => { resolveFirst = r as never; }))
      .mockImplementationOnce(async () => [node('Notes/second.md', 'note')]);

    const api = useTaskBacklinks(editingTask, { getLinkedNodes });

    editingTask.value = task('Tasks/first.md');
    await nextTick();
    editingTask.value = task('Tasks/second.md');
    await settle();

    // The first query finally answers, long after its task was closed.
    resolveFirst([node('Notes/first.md', 'note')]);
    await settle();

    expect(api.backlinks.value.map((b) => b.id)).toEqual(['Notes/second.md']);
  });
});
