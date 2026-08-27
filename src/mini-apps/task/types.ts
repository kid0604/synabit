import { i18n } from '../../i18n';
import { isValidDateString, isValidTimeString, type FieldIssue } from './validation';

// ── Task Types & Constants ──────────────────────────────────────────

export interface TaskMetadata {
  id: string;
  title: string;
  status: string;
  is_transferred: boolean;
  transferred_to: string;
  track_progress: boolean;
  priority: string;
  start_date: string;
  due_date: string;
  /**
   * The time of day the task is due, `HH:mm`, or empty for a whole-day task.
   *
   * Deliberately separate from `due_date` rather than one combined `due_at`,
   * which is the shape Events use. An event genuinely spans an instant to an
   * instant; a task has one deadline, and most tasks have no time at all. Two
   * keys keep `due_date` the single answer to "what day", which every bucket,
   * filter and sort in this app already asks — a combined key would have made
   * all of them parse a datetime to get back the string they had.
   */
  due_time: string;
  /**
   * How long before the deadline to notify, as durations: `['1d', '30m']`.
   *
   * Read by the reminder loop in `chat_engine.rs`, which has fired these for
   * tasks all along — nothing in the Tasks UI ever wrote them, so every task
   * fell to its default of one notification at the deadline.
   */
  reminders: string[];
  /**
   * How the task repeats: `none`, `daily`, `weekly`, `monthly`, `yearly`.
   *
   * Same vocabulary as an Event's, deliberately, but a different mechanism.
   * A recurring event is one file *displayed* as many occurrences. A task
   * carries per-occurrence state — a status, a completion date — so it cannot
   * be expanded that way; see `advanceRecurrence`.
   */
  recurrence: string;
  /** Last date the series may fall on, `YYYY-MM-DD`, or empty for forever. */
  recurrence_end_at: string;
  /**
   * The task this one sits under, as its vault-relative path.
   *
   * A parent that no longer exists is not an error: the subtask is shown at
   * the top level. Deleting a parent would otherwise have to rewrite every
   * child, which is a lot of writes and a lot of sync to record something the
   * reader can work out for itself.
   */
  parent_id: string;
  comment: string;
  source_link: string;
  tags: string[];
  /**
   * The opening of the body, for searching against — not the body.
   *
   * The list is built from `getNodeSummaries`, which does not send bodies: for
   * a vault of ordinary tasks they are the bulk of the payload and none of
   * what any of the four views draws. The edit modal fetches the full node
   * when the user opens one.
   */
  preview: string;
  path: string;
  created_at: string;
  updated_at: string;
  completed_at: string;
  project_id: string;
  custom_fields: Record<string, any>;
  /**
   * Fields whose stored value is not one this field can hold.
   *
   * Almost always the mark of a character-level merge of two edits to the same
   * frontmatter line — see `validation.ts`. Carried on the task so the row can
   * say so; the task itself behaves as though the field were unset.
   */
  issues?: FieldIssue[];
  isNew?: boolean;
}

export const BOARD_COLUMNS = [
  { id: 'backlog', name: 'BACKLOG', class: 'border-t-2 border-gray-400 dark:border-gray-500' },
  { id: 'todo', name: 'TO DO', class: 'border-t-2 border-gray-300 dark:border-gray-600' },
  { id: 'in_progress', name: 'IN PROGRESS', class: 'border-t-2 border-blue-400 dark:border-blue-500' },
  { id: 'done', name: 'DONE', class: 'border-t-2 border-green-400 dark:border-green-500' },
] as const;

export const URGENCY_THRESHOLD_DAYS = 3;

// ── Helper Functions ────────────────────────────────────────────────

export const getTodayStr = (): string => {
  const now = new Date();
  const offset = now.getTimezoneOffset() * 60000;
  const localNow = new Date(now.getTime() - offset);
  return localNow.toISOString().split('T')[0];
};

export const getPriorityClass = (priority: string): string => {
  switch (priority) {
    case 'P1': return 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400';
    case 'P2': return 'bg-orange-100 text-orange-700 dark:bg-orange-900/30 dark:text-orange-400';
    case 'P3': return 'bg-yellow-100 text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-400';
    case 'P4': return 'bg-slate-100 text-slate-700 dark:bg-slate-800/50 dark:text-slate-400';
    default: return '';
  }
};

/**
 * When the task is actually due, or `null` if it has no due date.
 *
 * A task with a time is due at that minute; one without is due at the end of
 * its day, so a whole-day task is not overdue at one minute past midnight.
 */
export const taskDueAt = (task: Pick<TaskMetadata, 'due_date' | 'due_time'>): Date | null => {
  // Validated, not merely split. `'2026-129-315'` — which is what merging two
  // devices' edits to the same `due_date` line produces — split into three
  // truthy numbers and `new Date` rolled them over into July 2037: a task with
  // no deadline for eleven years and nothing anywhere to say why.
  if (!isValidDateString(task.due_date)) return null;
  const [y, m, d] = task.due_date.trim().split('-').map(Number);

  // Built from the parts rather than parsed from a string: `new Date('2026-08-30')`
  // is read as UTC midnight, which in UTC+7 is 07:00 on the 30th and in UTC-5
  // is the evening of the 29th. The multi-argument form is always local.
  const time = (task.due_time || '').trim();
  if (isValidTimeString(time)) {
    const [h, min] = time.split(':').map(Number);
    return new Date(y, m - 1, d, h, min, 0, 0);
  }
  return new Date(y, m - 1, d, 23, 59, 59, 999);
};

export const isOverdue = (task: TaskMetadata): boolean => {
  if (task.status === 'done') return false;
  const due = taskDueAt(task);
  if (!due) return false;
  return due.getTime() < Date.now();
};

/** The reminder durations the picker offers, plus whatever the user typed. */
export const REMINDER_PRESETS = ['5m', '15m', '30m', '1h', '1d'] as const;

/** `45m`, `2h`, `1d` — the shape `chat_engine.rs::parse_duration` understands. */
export const isValidReminder = (value: string): boolean => /^\d+[mhd]$/.test(value.trim().toLowerCase());

export const formatNumber = (val: string | number | null | undefined): string | null => {
  if (!val) return null;
  const num = String(val).replace(/[^0-9.]/g, '');
  if (!num) return null;
  const parts = num.split('.');
  parts[0] = parts[0].replace(/\B(?=(\d{3})+(?!\d))/g, ',');
  return parts.join('.');
};

export const getTransferredName = (rawStr: string | null | undefined): string => {
  if (!rawStr) return i18n.global.t('task.unknown_person');
  const match = rawStr.match(/^\[(.*?)\]\(synabit:\/\/person\/.*?\)$/);
  return match ? `@${match[1]}` : rawStr;
};

export const isLinkedPerson = (rawStr: string | null | undefined): boolean => {
  return /^\[(.*?)\]\(synabit:\/\/person\/.*?\)$/.test(rawStr || '');
};

// ── Writing a task back ─────────────────────────────────────────────

/**
 * The frontmatter keys a task owns.
 *
 * `title` and `type` are not here: the backend writes those from its own
 * arguments and skips them if a caller sends them anyway, so naming them
 * would only be a second way to say the same thing.
 */
const TASK_PROPERTY_KEYS = [
  'status',
  'due_time',
  'reminders',
  'recurrence',
  'recurrence_end_at',
  'parent_id',
  'is_transferred',
  'transferred_to',
  'track_progress',
  'priority',
  'start_date',
  'due_date',
  'comment',
  'source_link',
  'tags',
  'project_id',
  'completed_at',
] as const;

/** As much of a task as any of the write paths happen to hold. */
export type TaskPropertySource = Partial<
  Pick<TaskMetadata, (typeof TASK_PROPERTY_KEYS)[number]>
> & { custom_fields?: Record<string, any> };

/**
 * The frontmatter a task write must carry.
 *
 * Five call sites used to spell this out by hand — the edit modal, quick add,
 * the board drop, the matrix drop, and the calendar's checkbox — and they had
 * drifted apart, each listing a slightly different set of keys.
 *
 * `custom_fields` goes in first because it holds every property the file had,
 * the app's own keys included; the typed fields then overwrite it. That order
 * is what keeps a key the app has no field for — an `aliases` somebody typed
 * into the file by hand — alive across a save.
 *
 * A field the caller does not have stays at whatever `custom_fields` says.
 * The Calendar's task type carries no `priority`, and writing a blank one for
 * it would erase the priority the user set in Tasks.
 */
export function taskProperties(
  task: TaskPropertySource,
  overrides: Record<string, unknown> = {},
): Record<string, unknown> {
  const properties: Record<string, unknown> = { ...(task.custom_fields ?? {}) };

  for (const key of TASK_PROPERTY_KEYS) {
    const value = task[key];
    if (value !== undefined) properties[key] = value;
  }

  return { ...properties, ...overrides };
}
