/**
 * How the list arranges itself.
 *
 * Until now there was one arrangement and nobody chose it: the database hands
 * back `ORDER BY updated_at DESC`, so editing a task sent it to the top, and
 * once subtasks arrived it took its whole subtree with it. That is a fine
 * default for "what did I touch last" and a poor one for everything else.
 *
 * Sorting happens on the flat list, before the tree is built. `buildTaskTree`
 * keeps the order it is given, both for roots and for each parent's children,
 * so one sort arranges every level at once — a parent's subtasks come out in
 * the same order as the parents themselves.
 */

import { type TaskMetadata, taskDueAt, getTodayStr } from './types';

/**
 * One collator, built once and reused.
 *
 * `a.localeCompare(b, undefined, { sensitivity: 'base' })` builds a fresh
 * `Intl.Collator` on every call, and a sort calls its comparator O(n log n)
 * times — twice each, once for the mode and once for the tie-break. Measured
 * over five thousand tasks that was 13ms of collator construction against
 * 0.4ms of actual comparing: thirty times the cost, and by a wide margin the
 * most expensive thing the list did.
 *
 * `undefined` for the locale keeps the behaviour identical to what it replaces
 * — the runtime's own locale, which is what an omitted argument already meant.
 * Vietnamese and English both sort correctly; only the construction is gone.
 */
const collator = new Intl.Collator(undefined, { sensitivity: 'base' });
const byTitle = (a: string, b: string) => collator.compare(a || '', b || '');

export const SORT_MODES = ['updated', 'manual', 'due', 'priority', 'title', 'created'] as const;
export type SortMode = (typeof SORT_MODES)[number];

export const GROUP_MODES = ['none', 'project', 'priority', 'due', 'status'] as const;
export type GroupMode = (typeof GROUP_MODES)[number];

export const isSortMode = (v: string): v is SortMode =>
  (SORT_MODES as readonly string[]).includes(v);
export const isGroupMode = (v: string): v is GroupMode =>
  (GROUP_MODES as readonly string[]).includes(v);

/** Newest first, and a task with no timestamp sorts last rather than first. */
const timeDesc = (a: string, b: string): number => {
  const ta = a ? new Date(a.replace(' ', 'T')).getTime() : NaN;
  const tb = b ? new Date(b.replace(' ', 'T')).getTime() : NaN;
  if (Number.isNaN(ta) && Number.isNaN(tb)) return 0;
  if (Number.isNaN(ta)) return 1;
  if (Number.isNaN(tb)) return -1;
  return tb - ta;
};

/**
 * P1 first, then P2, P3, P4, then no priority at all.
 *
 * Unset sorts last rather than as a fifth level. "No priority" is the absence
 * of a decision, not a decision that it is the least important thing here.
 */
const priorityRank = (p: string): number => {
  const match = /^P([1-4])$/.exec(p || '');
  return match ? Number(match[1]) : 99;
};

const compareFor: Record<SortMode, (a: TaskMetadata, b: TaskMetadata) => number> = {
  updated: (a, b) => timeDesc(a.updated_at, b.updated_at),
  created: (a, b) => timeDesc(a.created_at, b.created_at),
  title: (a, b) => byTitle(a.title, b.title),
  priority: (a, b) => priorityRank(a.priority) - priorityRank(b.priority),
  // Soonest first; a task with no deadline is not urgent and goes last.
  due: (a, b) => {
    const da = taskDueAt(a)?.getTime();
    const db = taskDueAt(b)?.getTime();
    if (da === undefined && db === undefined) return 0;
    if (da === undefined) return 1;
    if (db === undefined) return -1;
    return da - db;
  },
  // The order cards were dragged into on the board, so the two views agree.
  // Cards never dragged keep their old newest-first arrangement.
  manual: (a, b) => {
    const ka = a.custom_fields?.['order'];
    const kb = b.custom_fields?.['order'];
    const sa = typeof ka === 'string' ? ka : null;
    const sb = typeof kb === 'string' ? kb : null;
    if (sa !== null && sb !== null) return sa < sb ? -1 : sa > sb ? 1 : 0;
    if (sa !== null) return -1;
    if (sb !== null) return 1;
    return timeDesc(a.created_at, b.created_at);
  },
};

/**
 * The list in the chosen order.
 *
 * Ties break on title, then on id. Without a total order the arrangement can
 * differ between two renders of the same data — `Array.prototype.sort` is
 * stable, but the list it is given is rebuilt from the database on every
 * watcher tick and does not always arrive the same way round.
 */
export function sortTasks(tasks: TaskMetadata[], mode: SortMode): TaskMetadata[] {
  const compare = compareFor[mode] ?? compareFor.updated;
  return [...tasks].sort((a, b) =>
    compare(a, b)
    || byTitle(a.title, b.title)
    || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0));
}

export interface TaskGroup {
  /** Stable key, for `v-for` and for remembering which groups are collapsed. */
  key: string;
  /** An i18n key, or a literal when the name comes from the data. */
  label: string;
  /** True when `label` is already text rather than something to translate. */
  literal: boolean;
  tasks: TaskMetadata[];
}

const dueBucket = (task: TaskMetadata): string => {
  if (!task.due_date) return 'none';
  const today = getTodayStr();
  if (task.due_date < today) return 'overdue';
  if (task.due_date === today) return 'today';
  // Seven days rather than "this calendar week": on a Sunday the latter is a
  // bucket with nothing in it.
  const week = new Date();
  week.setDate(week.getDate() + 7);
  return task.due_date <= week.toLocaleDateString('en-CA') ? 'week' : 'later';
};

const DUE_BUCKET_ORDER = ['overdue', 'today', 'week', 'later', 'none'];
const STATUS_ORDER = ['in_progress', 'todo', 'backlog', 'done'];

/**
 * The list cut into sections.
 *
 * Grouping flattens the tree: a section headed "P1" cannot sensibly nest a P3
 * subtask under its parent, and a subtask whose parent is in another section
 * would have to be drawn twice or not at all. Nesting is what you get when
 * nothing is grouped, which is the default.
 */
export function groupTasks(
  tasks: TaskMetadata[],
  mode: GroupMode,
  projectTitle: (id: string) => string,
): TaskGroup[] {
  if (mode === 'none') {
    return [{ key: 'all', label: '', literal: true, tasks }];
  }

  const buckets = new Map<string, TaskMetadata[]>();
  const keyOf = (task: TaskMetadata): string => {
    switch (mode) {
      case 'project': return task.project_id || '';
      case 'priority': return task.priority || '';
      case 'status': return task.status || 'todo';
      case 'due': return dueBucket(task);
      default: return '';
    }
  };

  for (const task of tasks) {
    const key = keyOf(task);
    const bucket = buckets.get(key);
    if (bucket) bucket.push(task);
    else buckets.set(key, [task]);
  }

  const order = (key: string): number => {
    if (mode === 'due') return DUE_BUCKET_ORDER.indexOf(key);
    if (mode === 'status') return STATUS_ORDER.indexOf(key);
    if (mode === 'priority') return priorityRank(key);
    // Projects sort by name, with the tasks belonging to none of them last.
    return key ? 0 : 1;
  };

  const label = (key: string): { label: string; literal: boolean } => {
    if (mode === 'project') {
      return key
        ? { label: projectTitle(key), literal: true }
        : { label: 'task.group_no_project', literal: false };
    }
    if (mode === 'priority') {
      return key ? { label: key, literal: true } : { label: 'task.group_no_priority', literal: false };
    }
    if (mode === 'status') return { label: `task.status_${key}`, literal: false };
    return { label: `task.due_bucket_${key}`, literal: false };
  };

  return [...buckets.entries()]
    .map(([key, groupTasks]) => ({ key: key || '__none__', ...label(key), tasks: groupTasks }))
    .sort((a, b) => {
      const ka = a.key === '__none__' ? '' : a.key;
      const kb = b.key === '__none__' ? '' : b.key;
      return order(ka) - order(kb) || byTitle(a.label, b.label);
    });
}
