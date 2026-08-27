import { describe, it, expect, vi, afterEach } from 'vitest';
import { sortTasks, groupTasks, isSortMode, isGroupMode } from '../sorting';
import { buildTaskTree, flattenTaskTree } from '../subtasks';
import type { TaskMetadata } from '../types';

const task = (id: string, over: Partial<TaskMetadata> = {}): TaskMetadata =>
  ({
    id, path: id, title: id, status: 'todo', priority: '', project_id: '',
    due_date: '', due_time: '', parent_id: '', custom_fields: {},
    created_at: '2026-01-01 00:00:00', updated_at: '2026-01-01 00:00:00', ...over,
  }) as TaskMetadata;

const ids = (list: TaskMetadata[]) => list.map((t) => t.id);

describe('sortTasks', () => {
  it('sorts by due date, soonest first', () => {
    const all = [task('c', { due_date: '2026-09-01' }), task('a', { due_date: '2026-08-01' })];
    expect(ids(sortTasks(all, 'due'))).toEqual(['a', 'c']);
  });

  /** A task with no deadline is not urgent; it must not lead the list. */
  it('puts tasks with no due date last', () => {
    const all = [task('none'), task('dated', { due_date: '2026-08-01' })];
    expect(ids(sortTasks(all, 'due'))).toEqual(['dated', 'none']);
  });

  it('uses the time of day when two tasks share a date', () => {
    const all = [
      task('afternoon', { due_date: '2026-08-01', due_time: '15:00' }),
      task('morning', { due_date: '2026-08-01', due_time: '09:00' }),
    ];
    expect(ids(sortTasks(all, 'due'))).toEqual(['morning', 'afternoon']);
  });

  it('sorts by priority, P1 first', () => {
    const all = [task('p3', { priority: 'P3' }), task('p1', { priority: 'P1' })];
    expect(ids(sortTasks(all, 'priority'))).toEqual(['p1', 'p3']);
  });

  /** Unset is the absence of a decision, not a fifth level of importance. */
  it('puts tasks with no priority last', () => {
    const all = [task('none'), task('p4', { priority: 'P4' })];
    expect(ids(sortTasks(all, 'priority'))).toEqual(['p4', 'none']);
  });

  it('sorts by title, ignoring case and accents', () => {
    const all = [task('b', { title: 'banana' }), task('a', { title: 'Apple' })];
    expect(ids(sortTasks(all, 'title'))).toEqual(['a', 'b']);
  });

  it('sorts by last touched, newest first', () => {
    const all = [
      task('old', { updated_at: '2026-01-01 00:00:00' }),
      task('new', { updated_at: '2026-06-01 00:00:00' }),
    ];
    expect(ids(sortTasks(all, 'updated'))).toEqual(['new', 'old']);
  });

  it('follows the board order in manual mode', () => {
    const all = [
      task('second', { custom_fields: { order: 'W' } }),
      task('first', { custom_fields: { order: 'D' } }),
    ];
    expect(ids(sortTasks(all, 'manual'))).toEqual(['first', 'second']);
  });

  it('puts never-dragged tasks after the arranged ones', () => {
    const all = [task('loose'), task('placed', { custom_fields: { order: 'V' } })];
    expect(ids(sortTasks(all, 'manual'))).toEqual(['placed', 'loose']);
  });

  /**
   * The list is rebuilt from the database on every watcher tick and does not
   * always arrive the same way round. Without a tie-break the same data can
   * render in two different orders, which reads as the list twitching.
   */
  it('is stable when everything ties', () => {
    const all = [task('b'), task('a'), task('c')];
    const once = ids(sortTasks(all, 'due'));
    const again = ids(sortTasks([...all].reverse(), 'due'));
    expect(once).toEqual(again);
  });

  it('leaves the input alone', () => {
    const all = [task('b'), task('a')];
    sortTasks(all, 'title');
    expect(ids(all)).toEqual(['b', 'a']);
  });

  it('falls back to last touched for a mode it does not know', () => {
    const all = [task('old', { updated_at: '2026-01-01 00:00:00' }), task('new', { updated_at: '2026-06-01 00:00:00' })];
    expect(ids(sortTasks(all, 'nonsense' as never))).toEqual(['new', 'old']);
  });
});

/**
 * One sort has to arrange every level, because `buildTaskTree` keeps the order
 * it is given for a parent's children as well as for the roots.
 */
describe('sorting reaches inside the tree', () => {
  it('sorts a parent’s subtasks among themselves', () => {
    const all = [
      task('parent'),
      task('later', { parent_id: 'parent', due_date: '2026-09-01' }),
      task('sooner', { parent_id: 'parent', due_date: '2026-08-01' }),
    ];
    const rows = flattenTaskTree(buildTaskTree(sortTasks(all, 'due')));
    // A child can never precede its parent in a flattened tree, whatever the
    // sort says; what the sort decides is the order of the siblings.
    expect(ids(rows.map((r) => r.task))).toEqual(['parent', 'sooner', 'later']);
  });

  it('sorts the roots among themselves as well', () => {
    const all = [
      task('second', { due_date: '2026-09-01' }),
      task('first', { due_date: '2026-08-01' }),
      task('kid', { parent_id: 'second' }),
    ];
    const rows = flattenTaskTree(buildTaskTree(sortTasks(all, 'due')));
    expect(ids(rows.map((r) => r.task))).toEqual(['first', 'second', 'kid']);
  });

  it('keeps children under their parent whatever the sort', () => {
    const all = [
      task('b-parent', { title: 'b' }),
      task('a-child', { title: 'a', parent_id: 'b-parent' }),
    ];
    const rows = flattenTaskTree(buildTaskTree(sortTasks(all, 'title')));
    expect(rows.map((r) => [r.task.id, r.depth])).toEqual([['b-parent', 0], ['a-child', 1]]);
  });
});

describe('groupTasks', () => {
  const title = (id: string) => ({ 'Projects/a.md': 'Alpha' }[id] ?? id);

  it('makes one group of everything when grouping is off', () => {
    const groups = groupTasks([task('a'), task('b')], 'none', title);
    expect(groups).toHaveLength(1);
    expect(ids(groups[0].tasks)).toEqual(['a', 'b']);
  });

  it('groups by project and names them', () => {
    const groups = groupTasks(
      [task('a', { project_id: 'Projects/a.md' }), task('loose')], 'project', title,
    );
    expect(groups[0].label).toBe('Alpha');
    expect(groups[0].literal).toBe(true);
  });

  it('puts tasks with no project last', () => {
    const groups = groupTasks(
      [task('loose'), task('a', { project_id: 'Projects/a.md' })], 'project', title,
    );
    expect(ids(groups[groups.length - 1].tasks)).toEqual(['loose']);
  });

  it('groups by priority in importance order', () => {
    const groups = groupTasks(
      [task('p3', { priority: 'P3' }), task('p1', { priority: 'P1' }), task('none')],
      'priority', title,
    );
    expect(groups.map((g) => g.key)).toEqual(['P1', 'P3', '__none__']);
  });

  it('groups by status in the order work moves through', () => {
    const groups = groupTasks(
      [task('d', { status: 'done' }), task('t', { status: 'todo' }), task('p', { status: 'in_progress' })],
      'status', title,
    );
    expect(groups.map((g) => g.key)).toEqual(['in_progress', 'todo', 'done']);
  });

  it('gives every group a key that can be used to tell them apart', () => {
    const groups = groupTasks([task('a'), task('b', { priority: 'P1' })], 'priority', title);
    expect(new Set(groups.map((g) => g.key)).size).toBe(groups.length);
  });

  it('loses nothing', () => {
    const all = [task('a', { priority: 'P1' }), task('b'), task('c', { priority: 'P2' })];
    const groups = groupTasks(all, 'priority', title);
    expect(groups.flatMap((g) => g.tasks)).toHaveLength(all.length);
  });
});

describe('grouping by due date', () => {
  afterEach(() => vi.useRealTimers());

  const at = (iso: string) => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(iso));
  };

  it('separates overdue, today, this week, later and undated, in that order', () => {
    at('2026-08-23T09:00:00');
    const groups = groupTasks([
      task('later', { due_date: '2026-12-01' }),
      task('none'),
      task('today', { due_date: '2026-08-23' }),
      task('overdue', { due_date: '2026-08-01' }),
      task('week', { due_date: '2026-08-27' }),
    ], 'due', (x) => x);
    expect(groups.map((g) => g.key)).toEqual(['overdue', 'today', 'week', 'later', 'none']);
  });

  /** Seven days out, not "this calendar week" — on a Sunday that is empty. */
  it('counts a week as the next seven days', () => {
    at('2026-08-23T09:00:00'); // a Sunday
    const groups = groupTasks([task('sixDays', { due_date: '2026-08-29' })], 'due', (x) => x);
    expect(groups[0].key).toBe('week');
  });
});

describe('mode guards', () => {
  it('recognises the modes on offer', () => {
    expect(isSortMode('due')).toBe(true);
    expect(isGroupMode('project')).toBe(true);
  });

  it('rejects anything else, so a stale setting cannot break the list', () => {
    expect(isSortMode('by-vibes')).toBe(false);
    expect(isGroupMode('')).toBe(false);
  });
});

/**
 * A guard on the cost, written as a fact about the code rather than a
 * stopwatch. `String.prototype.localeCompare` with options builds an
 * `Intl.Collator` every time it is called, and a comparator is called
 * O(n log n) times — measured at thirty times the cost of the sort itself over
 * five thousand tasks, and by a distance the most expensive thing the list did.
 *
 * Asserting on elapsed milliseconds would flake on a loaded machine. Asserting
 * that the slow call is never made says the same thing and cannot.
 */
describe('sorting does not rebuild a collator per comparison', () => {
  const manyTasks = Array.from({ length: 200 }, (_, i) =>
    task(`t${i}`, { title: `Task ${(i * 7) % 200}` }));

  const withoutLocaleCompare = (fn: () => void) => {
    const original = String.prototype.localeCompare;
    let calls = 0;
    String.prototype.localeCompare = function (...args: Parameters<typeof original>) {
      calls += 1;
      return original.apply(this, args);
    };
    try { fn(); } finally { String.prototype.localeCompare = original; }
    return calls;
  };

  it('never calls localeCompare while sorting by title', () => {
    expect(withoutLocaleCompare(() => sortTasks(manyTasks, 'title'))).toBe(0);
  });

  it('never calls it for the tie-break either', () => {
    expect(withoutLocaleCompare(() => sortTasks(manyTasks, 'due'))).toBe(0);
  });

  it('never calls it while grouping', () => {
    expect(withoutLocaleCompare(() => groupTasks(manyTasks, 'priority', (x) => x))).toBe(0);
  });

  /** The speed is worth nothing if the order changed. */
  it('still orders accented and mixed-case titles the way a reader expects', () => {
    const all = [
      task('c', { title: 'Ổn định' }),
      task('a', { title: 'ăn sáng' }),
      task('b', { title: 'Bánh mì' }),
    ];
    expect(ids(sortTasks(all, 'title'))).toEqual(['a', 'b', 'c']);
  });

  it('still ignores case', () => {
    const all = [task('b', { title: 'apple' }), task('a', { title: 'Apple' })];
    // Equal under base sensitivity, so the id decides — and does so stably.
    expect(ids(sortTasks(all, 'title'))).toEqual(['a', 'b']);
  });
});
