import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ref } from 'vue';
import { useTaskSearch } from '../composables/useTaskSearch';
import { getTodayStr, type TaskMetadata } from '../types';

// The query engine is exercised separately below; most of these cover the
// bucketing and the local filters, which decide whether a task is visible at
// all. The shape is the one `run_node_query` really returns.
const invokeMock = vi.hoisted(() => vi.fn(async () => ({ rows: [] as { id: string }[] })));
vi.mock('@tauri-apps/api/core', () => ({ invoke: invokeMock }));

const today = getTodayStr();
const shift = (days: number): string => {
  const d = new Date(`${today}T00:00:00`);
  d.setDate(d.getDate() + days);
  return d.toLocaleDateString('en-CA');
};
const yesterday = shift(-1);
const tomorrow = shift(1);

let seq = 0;
const task = (over: Partial<TaskMetadata> = {}): TaskMetadata =>
  ({
    id: `Tasks/${seq++}.md`,
    path: '',
    title: 'a task',
    status: 'todo',
    is_transferred: false,
    transferred_to: '',
    track_progress: false,
    priority: '',
    start_date: '',
    due_date: '',
    comment: '',
    source_link: '',
    tags: [],
    preview: '',
    created_at: '',
    updated_at: '',
    completed_at: '',
    project_id: '',
    custom_fields: {},
    ...over,
  }) as TaskMetadata;

const setup = (list: TaskMetadata[]) => useTaskSearch(ref(list));

describe('GTD bucketing', () => {
  it('puts a task due today, or overdue, in Today', () => {
    const s = setup([task({ due_date: today }), task({ due_date: yesterday })]);
    s.activeCategory.value = 'today';
    expect(s.activeCategoryTasks.value).toHaveLength(2);
  });

  it('treats a start date that has arrived as Today', () => {
    const s = setup([task({ start_date: yesterday })]);
    s.activeCategory.value = 'today';
    expect(s.activeCategoryTasks.value).toHaveLength(1);
  });

  it('puts a future task in Upcoming and keeps it out of Today', () => {
    const s = setup([task({ due_date: tomorrow })]);
    s.activeCategory.value = 'today';
    expect(s.activeCategoryTasks.value).toHaveLength(0);
    s.activeCategory.value = 'upcoming';
    expect(s.activeCategoryTasks.value).toHaveLength(1);
  });

  it('puts a task with no dates in Someday', () => {
    const s = setup([task()]);
    s.activeCategory.value = 'someday';
    expect(s.activeCategoryTasks.value).toHaveLength(1);
  });

  /**
   * A delegated task is somebody else's now. It belongs in Transferred and
   * nowhere else, or it would sit in Today asking to be done twice.
   */
  it('routes a transferred task out of the dated buckets', () => {
    const s = setup([task({ due_date: today, is_transferred: true })]);
    s.activeCategory.value = 'today';
    expect(s.activeCategoryTasks.value).toHaveLength(0);
    s.activeCategory.value = 'transferred';
    expect(s.activeCategoryTasks.value).toHaveLength(1);
  });

  /**
   * The reason `completed_at` has to be stamped in the local date: ticking a
   * task off should leave it visible for the rest of the day, not make it
   * disappear from under the cursor.
   */
  it('keeps a task finished today visible in Today', () => {
    const s = setup([task({ status: 'done', completed_at: today, due_date: today })]);
    s.activeCategory.value = 'today';
    expect(s.activeCategoryTasks.value).toHaveLength(1);
  });

  it('drops a task finished yesterday out of Today', () => {
    const s = setup([task({ status: 'done', completed_at: yesterday, due_date: yesterday })]);
    s.activeCategory.value = 'today';
    expect(s.activeCategoryTasks.value).toHaveLength(0);
  });

  it('shows finished tasks under All', () => {
    const s = setup([task({ status: 'done', completed_at: yesterday })]);
    s.activeCategory.value = 'all';
    expect(s.activeCategoryTasks.value).toHaveLength(1);
  });

  it('scopes a project bucket to that project', () => {
    const s = setup([task({ project_id: 'Projects/a.md' }), task({ project_id: 'Projects/b.md' })]);
    s.activeCategory.value = 'project:Projects/a.md';
    expect(s.activeCategoryTasks.value).toHaveLength(1);
  });
});

describe('category counts', () => {
  it('counts open work only, and each task once', () => {
    const s = setup([
      task({ due_date: today }),
      task({ due_date: tomorrow }),
      task(),
      task({ is_transferred: true }),
      task({ status: 'done', completed_at: today }),
    ]);
    expect(s.categoryCounts.value).toEqual({
      all: 4, today: 1, upcoming: 1, someday: 1, transferred: 1,
    });
  });
});

describe('local filter syntax', () => {
  it('filters by priority', () => {
    const s = setup([task({ priority: 'P1' }), task({ priority: 'P3' })]);
    s.activeCategory.value = 'all';
    s.searchQuery.value = 'p:1';
    expect(s.activeCategoryTasks.value).toHaveLength(1);
  });

  it('filters by tag, with # or tag:', () => {
    const s = setup([task({ tags: ['urgent'] }), task({ tags: ['later'] })]);
    s.activeCategory.value = 'all';
    s.searchQuery.value = '#urgent';
    expect(s.activeCategoryTasks.value).toHaveLength(1);
    s.searchQuery.value = 'tag:urgent';
    expect(s.activeCategoryTasks.value).toHaveLength(1);
  });

  it('filters by status', () => {
    const s = setup([task({ status: 'in_progress' }), task({ status: 'todo' })]);
    s.activeCategory.value = 'all';
    s.searchQuery.value = 'status:in_progress';
    expect(s.activeCategoryTasks.value).toHaveLength(1);
  });

  it('handles is: and not:', () => {
    const s = setup([task({ is_transferred: true }), task({ is_transferred: false })]);
    s.activeCategory.value = 'all';
    s.searchQuery.value = 'is:transferred';
    expect(s.activeCategoryTasks.value).toHaveLength(1);
    s.searchQuery.value = 'not:transferred';
    expect(s.activeCategoryTasks.value).toHaveLength(1);
  });

  it('filters by a custom property and its value', () => {
    const s = setup([
      task({ custom_fields: { cost: '100' } }),
      task({ custom_fields: { cost: '250' } }),
      task({ custom_fields: {} }),
    ]);
    s.activeCategory.value = 'all';
    s.searchQuery.value = 'prop:cost';
    expect(s.activeCategoryTasks.value).toHaveLength(2);
    s.searchQuery.value = 'prop:cost=100';
    expect(s.activeCategoryTasks.value).toHaveLength(1);
  });

  /**
   * A filter token is not free text. Leaving `p:1` in the text query sent it
   * to FTS5, which matched nothing, and the filter silently emptied the view.
   */
  it('does not treat a filter token as text to search for', () => {
    const s = setup([task({ priority: 'P1', title: 'ship it' })]);
    s.activeCategory.value = 'all';
    s.searchQuery.value = 'p:1';
    expect(s.activeCategoryTasks.value).toHaveLength(1);
  });
});


/**
 * The Tasks app used to strip every token out and hand the bare words to the
 * text index, so a note could ask what was overdue before September and this
 * screen could not. It now asks the same engine the notes do.
 */
describe('asking the vault query engine', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    invokeMock.mockClear();
    invokeMock.mockResolvedValue({ rows: [] });
  });
  afterEach(() => vi.useRealTimers());

  const ask = async (s: ReturnType<typeof setup>, query: string) => {
    s.searchQuery.value = query;
    await vi.advanceTimersByTimeAsync(300);
  };

  it('sends the query to run_node_query, scoped to tasks', async () => {
    const s = setup([task()]);
    await ask(s, 'milk');
    expect(invokeMock).toHaveBeenCalledWith('run_node_query', { query: 'is:task milk' });
  });

  /** The whole point: these were unreachable from this screen before. */
  it('hands over the filters only the engine can answer', async () => {
    const s = setup([task()]);
    await ask(s, 'due_date:<2026-09-01');
    expect(invokeMock).toHaveBeenCalledWith('run_node_query', { query: 'is:task due_date:<2026-09-01' });
  });

  it('does not ask when there is nothing for the engine to do', async () => {
    const s = setup([task({ priority: 'P1' })]);
    await ask(s, 'p:1');
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it('keeps only what the engine returned', async () => {
    const wanted = task({ title: 'wanted' });
    const other = task({ title: 'other' });
    invokeMock.mockResolvedValue({ rows: [{ id: wanted.id }] });
    const s = setup([wanted, other]);
    s.activeCategory.value = 'all';
    await ask(s, 'anything');
    expect(s.activeCategoryTasks.value.map((t) => t.title)).toEqual(['wanted']);
  });

  /** `sort:` is the engine's; the list must honour the order it answered in. */
  it('keeps the order the engine answered in', async () => {
    const first = task({ title: 'first' });
    const second = task({ title: 'second' });
    invokeMock.mockResolvedValue({ rows: [{ id: second.id }, { id: first.id }] });
    const s = setup([first, second]);
    s.activeCategory.value = 'all';
    await ask(s, 'sort:-due_date x');
    expect(s.activeCategoryTasks.value.map((t) => t.title)).toEqual(['second', 'first']);
  });

  it('still applies the browser-owned filters on top', async () => {
    const p1 = task({ priority: 'P1', title: 'urgent' });
    const p3 = task({ priority: 'P3', title: 'later' });
    invokeMock.mockResolvedValue({ rows: [{ id: p1.id }, { id: p3.id }] });
    const s = setup([p1, p3]);
    s.activeCategory.value = 'all';
    await ask(s, 'thing p:1');
    expect(s.activeCategoryTasks.value.map((t) => t.title)).toEqual(['urgent']);
  });

  /**
   * A failed query falls back to matching the words in the browser, the same
   * way it does for the fraction of a second before the engine answers. Showing
   * everything would be worse — the user typed a search and would get a list
   * that ignores it.
   */
  it('matches the words itself when the engine errors', async () => {
    invokeMock.mockRejectedValue(new Error('db is busy'));
    const s = setup([task({ title: 'buy milk' }), task({ title: 'call the bank' })]);
    s.activeCategory.value = 'all';
    await ask(s, 'milk');
    expect(s.activeCategoryTasks.value.map((t) => t.title)).toEqual(['buy milk']);
  });

  it('does not leave the list narrowed to nothing by a failure alone', async () => {
    invokeMock.mockRejectedValue(new Error('db is busy'));
    const s = setup([task({ title: 'buy milk' })]);
    s.activeCategory.value = 'all';
    // Only browser-owned tokens: nothing was asked of the engine, so its
    // failure cannot be what empties the list.
    await ask(s, 'p:1');
    expect(s.activeCategoryTasks.value).toHaveLength(0);
    await ask(s, '');
    expect(s.activeCategoryTasks.value).toHaveLength(1);
  });
});
