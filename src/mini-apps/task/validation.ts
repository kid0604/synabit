/**
 * Checking that a task's frontmatter still says something a task can mean.
 *
 * Every field here has a small set of legal values, and the app has always
 * trusted whatever the file held. That was fine while files were only written
 * by this app. It stopped being fine when they started being merged.
 *
 * The whole file — frontmatter and body together — lives in one text CRDT, so
 * merging is character-level. Two devices editing two different fields merge
 * cleanly. Two devices editing *the same* field interleave: `done` against
 * `in_progress` produces `in_pronegress`, and `2026-09-15` against
 * `2026-12-31` produces `2026-129-315`. Both are valid YAML. Nothing failed,
 * nothing was logged, and the task quietly moved to the wrong column with a
 * deadline eleven years out.
 *
 * The proper fix is a field-level CRDT, which is a change to the sync protocol.
 * This is the floor under it: a value that cannot be what it claims to be is
 * not acted on as though it could, and the user is told which field to look at
 * rather than left to notice a task has gone missing.
 *
 * Nothing here writes. Repairing the file would mean two devices independently
 * making "the same" correction, which merges into two corrections — the bug
 * again, wearing a hat. Reading defensively is deterministic everywhere and
 * creates no operations at all.
 */

export const TASK_STATUSES = ['backlog', 'todo', 'in_progress', 'done'] as const;
export const TASK_PRIORITIES = ['P1', 'P2', 'P3', 'P4'] as const;
export const TASK_RECURRENCES = ['none', 'daily', 'weekly', 'monthly', 'yearly'] as const;

/**
 * A real calendar date written `YYYY-MM-DD`.
 *
 * The month and day are checked against the calendar, not just against a
 * regular expression: `2026-02-30` matches the shape and is not a date, and
 * `new Date(2026, 1, 30)` answers "the 2nd of March" rather than refusing.
 * Silently sliding to a different day is exactly the failure being guarded
 * against here.
 */
export function isValidDateString(value: unknown): value is string {
  if (typeof value !== 'string') return false;
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value.trim());
  if (!match) return false;
  const [y, m, d] = [Number(match[1]), Number(match[2]), Number(match[3])];
  if (m < 1 || m > 12 || d < 1) return false;
  return d <= new Date(y, m, 0).getDate();
}

/** A time of day, `HH:mm` or `HH:mm:ss`, within the hours a day actually has. */
export function isValidTimeString(value: unknown): value is string {
  if (typeof value !== 'string') return false;
  const match = /^(\d{1,2}):(\d{2})(?::(\d{2}))?$/.exec(value.trim());
  if (!match) return false;
  const [h, min, sec] = [Number(match[1]), Number(match[2]), Number(match[3] ?? 0)];
  return h <= 23 && min <= 59 && sec <= 59;
}

/** A duration the reminder loop understands: `45m`, `2h`, `1d`. */
export function isValidDuration(value: unknown): value is string {
  return typeof value === 'string' && /^\d+[mhd]$/.test(value.trim().toLowerCase());
}

/** One field whose stored value is not something it could legitimately be. */
export interface FieldIssue {
  field: string;
  /** What the file says, so the user can recognise their own two edits in it. */
  value: string;
}

const oneOf = (list: readonly string[]) => (v: unknown) =>
  typeof v === 'string' && list.includes(v);

/**
 * Which fields to check, and what "fine" means for each.
 *
 * Empty always passes: an unset field is not a damaged one, and every one of
 * these is optional.
 */
const CHECKS: { field: string; ok: (v: unknown) => boolean }[] = [
  { field: 'status', ok: oneOf(TASK_STATUSES) },
  { field: 'priority', ok: oneOf(TASK_PRIORITIES) },
  { field: 'recurrence', ok: oneOf(TASK_RECURRENCES) },
  { field: 'due_date', ok: isValidDateString },
  { field: 'start_date', ok: isValidDateString },
  { field: 'recurrence_end_at', ok: isValidDateString },
  { field: 'due_time', ok: isValidTimeString },
  // `completed_at` is written as a date but older files carry a timestamp.
  { field: 'completed_at', ok: (v) => isValidDateString(v) || (typeof v === 'string' && isValidDateString(v.split(/[ T]/)[0])) },
];

/**
 * The fields of a task that do not hold a value they could hold.
 *
 * Reads the raw properties rather than a mapped task, because mapping is what
 * substitutes the safe defaults — by then the evidence is gone.
 */
export function taskFieldIssues(properties: Record<string, unknown> | undefined | null): FieldIssue[] {
  if (!properties) return [];
  const issues: FieldIssue[] = [];

  for (const { field, ok } of CHECKS) {
    const raw = properties[field];
    if (raw === undefined || raw === null || raw === '') continue;
    if (!ok(raw)) issues.push({ field, value: String(raw) });
  }

  const reminders = properties['reminders'];
  if (Array.isArray(reminders)) {
    const bad = reminders.filter((r) => !isValidDuration(r));
    if (bad.length) issues.push({ field: 'reminders', value: bad.join(', ') });
  }

  return issues;
}

/** The value to behave as though the field held, given what it actually holds. */
export function safeStatus(value: unknown): string {
  return oneOf(TASK_STATUSES)(value) ? (value as string) : 'todo';
}

export function safePriority(value: unknown): string {
  return oneOf(TASK_PRIORITIES)(value) ? (value as string) : '';
}

export function safeRecurrence(value: unknown): string {
  return oneOf(TASK_RECURRENCES)(value) ? (value as string) : 'none';
}

/** A date the app can act on, or empty — never a date it had to invent. */
export function safeDate(value: unknown): string {
  return isValidDateString(value) ? (value as string).trim() : '';
}

export function safeTime(value: unknown): string {
  return isValidTimeString(value) ? (value as string).trim() : '';
}
