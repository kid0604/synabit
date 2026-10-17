import { type TaskMetadata, URGENCY_THRESHOLD_DAYS } from './types';

/** The Eisenhower quadrants, in reading order: the Matrix's, and a note's. */
export const QUADRANTS = ['do_first', 'schedule', 'delegate', 'eliminate'] as const;
export type Quadrant = (typeof QUADRANTS)[number];

export const isQuadrant = (value: unknown): value is Quadrant =>
  typeof value === 'string' && (QUADRANTS as readonly string[]).includes(value);

/**
 * Where the Matrix puts a task: the quadrant it was dragged to, or else one
 * worked out from its priority, its due date and whether it was handed on.
 *
 * Out of `useBoardLogic` so a note's matrix can say where Tasks has a task
 * without mounting the Tasks app to ask.
 */
export function taskQuadrant(
  task: Pick<TaskMetadata, 'priority' | 'due_date' | 'is_transferred'> & { custom_fields?: Record<string, any> },
  now: Date = new Date(),
): string {
  // Priority 1: Explicit override from drag-drop
  if (task.custom_fields?.eisenhower_quadrant) {
    return task.custom_fields.eisenhower_quadrant;
  }
  // Priority 2: Delegate = transferred tasks
  if (task.is_transferred) return 'delegate';
  // Derive importance & urgency
  const isImportant = task.priority === 'P1' || task.priority === 'P2';
  const today = new Date(now);
  today.setHours(0, 0, 0, 0);
  const dueDate = task.due_date ? new Date(task.due_date) : null;
  if (dueDate) dueDate.setHours(0, 0, 0, 0);
  const daysUntilDue = dueDate ? Math.ceil((dueDate.getTime() - today.getTime()) / 86400000) : null;
  const isUrgent = daysUntilDue !== null && daysUntilDue <= URGENCY_THRESHOLD_DAYS;
  // Priority 3: Do First = Important (P1/P2) + Urgent (due ≤ 3 days)
  if (isImportant && isUrgent) return 'do_first';
  // Priority 4: Schedule = Has specific due date, not urgent yet
  if (dueDate && !isUrgent) return 'schedule';
  // Priority 5: Eliminate = Everything else
  return 'eliminate';
}
