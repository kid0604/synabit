import { describe, it, expect, vi } from 'vitest';
import { useTaskFilters } from '../composables/useTaskFilters';

const node = (id: string, title: string, props: Record<string, unknown> = {}) =>
  ({ id, title, properties: { query: 'p:1', view_mode: 'list', sort: 'due', group: 'none', ...props } });

const harness = (nodes: unknown[] = []) => {
  const getNodeSummaries = vi.fn(async () => nodes);
  const writeNode = vi.fn(async (_p: Record<string, unknown>) => {});
  const trashNode = vi.fn(async (_p: { relPath: string }) => '.trash/x');
  const toast = vi.fn();
  const api = useTaskFilters({ getNodeSummaries, writeNode, trashNode }, toast, (k: string) => k);
  return { api, getNodeSummaries, writeNode, trashNode, toast };
};

describe('reading saved searches', () => {
  it('reads a filter out of its node', async () => {
    const h = harness([node('Filters/a.md', 'Overdue at work', { query: 'due_date:<2026-09-01 #work' })]);
    await h.api.load();
    expect(h.api.filters.value).toEqual([{
      id: 'Filters/a.md', name: 'Overdue at work',
      query: 'due_date:<2026-09-01 #work', viewMode: 'list', sort: 'due', group: 'none',
    }]);
  });

  it('lists them by name', async () => {
    const h = harness([node('Filters/b.md', 'Zebra'), node('Filters/a.md', 'Apple')]);
    await h.api.load();
    expect(h.api.filters.value.map((f) => f.name)).toEqual(['Apple', 'Zebra']);
  });

  /**
   * A filter is a file, so its fields can say anything — hand-edited, merged
   * between two devices, written by a version that had modes this one does not.
   * A view mode the app lacks would leave the screen blank; an unknown sort
   * would throw inside the comparator.
   */
  it('falls back rather than trusting what the file says', async () => {
    const h = harness([node('Filters/a.md', 'Odd', {
      view_mode: 'gantt', sort: 'by-vibes', group: 'phase-of-moon',
    })]);
    await h.api.load();
    expect(h.api.filters.value[0]).toMatchObject({ viewMode: 'list', sort: 'updated', group: 'none' });
  });

  it('copes with a filter that has no query at all', async () => {
    const h = harness([node('Filters/a.md', 'Empty', { query: undefined })]);
    await h.api.load();
    expect(h.api.filters.value[0].query).toBe('');
  });

  it('shows an empty list rather than failing when the query errors', async () => {
    const h = harness();
    h.getNodeSummaries.mockRejectedValueOnce(new Error('db is busy'));
    await h.api.load();
    expect(h.api.filters.value).toEqual([]);
  });
});

describe('saving a search', () => {
  const draft = {
    name: 'Overdue', query: 'due_date:<2026-09-01',
    viewMode: 'board' as const, sort: 'due' as const, group: 'project' as const,
  };

  it('writes it into the vault as a filter node', async () => {
    const h = harness();
    await h.api.save(draft);
    expect(h.writeNode).toHaveBeenCalledWith(expect.objectContaining({
      nodeType: 'filter',
      title: 'Overdue',
      properties: { query: 'due_date:<2026-09-01', view_mode: 'board', sort: 'due', group: 'project' },
      eventType: 'created',
    }));
  });

  it('puts it under Filters/ with an id of its own', async () => {
    const h = harness();
    const saved = await h.api.save(draft);
    expect(saved!.id).toMatch(/^Filters\/[0-9a-f-]{36}\.md$/);
  });

  /** The arrangement travels with the query, or it needs redoing every time. */
  it('keeps the view and grouping alongside the query', async () => {
    const h = harness();
    const saved = await h.api.save(draft);
    expect(saved).toMatchObject({ viewMode: 'board', sort: 'due', group: 'project' });
  });

  it('appears in the list, still sorted by name', async () => {
    const h = harness([node('Filters/z.md', 'Zebra')]);
    await h.api.load();
    await h.api.save({ ...draft, name: 'Apple' });
    expect(h.api.filters.value.map((f) => f.name)).toEqual(['Apple', 'Zebra']);
  });

  it('says so and keeps the list unchanged when the write fails', async () => {
    const h = harness();
    h.writeNode.mockRejectedValueOnce(new Error('disk is full'));
    const saved = await h.api.save(draft);
    expect(saved).toBeNull();
    expect(h.api.filters.value).toEqual([]);
    expect(h.toast).toHaveBeenCalledWith('task.filter_save_failed');
  });
});

describe('renaming', () => {
  it('writes the new name', async () => {
    const h = harness([node('Filters/a.md', 'Old')]);
    await h.api.load();
    await h.api.rename(h.api.filters.value[0], 'New');
    expect(h.writeNode).toHaveBeenCalledWith(expect.objectContaining({ title: 'New' }));
  });

  it('ignores an empty or unchanged name', async () => {
    const h = harness([node('Filters/a.md', 'Same')]);
    await h.api.load();
    await h.api.rename(h.api.filters.value[0], '   ');
    await h.api.rename(h.api.filters.value[0], 'Same');
    expect(h.writeNode).not.toHaveBeenCalled();
  });

  it('puts the old name back when the write fails', async () => {
    const h = harness([node('Filters/a.md', 'Old')]);
    await h.api.load();
    h.writeNode.mockRejectedValueOnce(new Error('disk'));
    await h.api.rename(h.api.filters.value[0], 'New');
    expect(h.api.filters.value[0].name).toBe('Old');
  });
});

describe('deleting', () => {
  /** A filter holds no work, but it is still a file the user made. */
  it('sends it to the trash rather than unlinking it', async () => {
    const h = harness([node('Filters/a.md', 'Gone')]);
    await h.api.load();
    await h.api.remove(h.api.filters.value[0]);
    expect(h.trashNode).toHaveBeenCalledWith({ relPath: 'Filters/a.md' });
    expect(h.api.filters.value).toEqual([]);
  });

  it('keeps it listed when the delete fails', async () => {
    const h = harness([node('Filters/a.md', 'Stays')]);
    await h.api.load();
    h.trashNode.mockRejectedValueOnce(new Error('locked'));
    await h.api.remove(h.api.filters.value[0]);
    expect(h.api.filters.value).toHaveLength(1);
    expect(h.toast).toHaveBeenCalledWith('task.filter_delete_failed');
  });
});
