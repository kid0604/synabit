import { describe, it, expect } from 'vitest';
import { FORM_GOVERNED_KEYS, taskProperties } from '../types';

/**
 * What the edit form is allowed to show, and what a save is allowed to touch.
 *
 * A task file carries whatever the user put in it. The app has always kept
 * those keys across a save — `taskProperties` spreads them in first — but no
 * screen ever displayed one, so a field somebody typed by hand was preserved
 * and invisible at the same time.
 *
 * Showing them means deciding which keys are the form's business, and the cost
 * of getting that boundary wrong is not symmetric: showing one key too few is
 * a field the user cannot edit, and showing one too many is a raw text box
 * over a value the app computes.
 */
describe('the boundary between the form and the file', () => {
  it('governs every field the form has a real control for', () => {
    for (const typed of [
      'status',
      'priority',
      'due_date',
      'start_date',
      'due_time',
      'reminders',
      'recurrence',
      'tags',
      'project_id',
      'parent_id',
      'completed_at',
      // No box in the form, but the Matrix view writes it on every drag.
      // "Has a control" is about the app, not about this screen.
      'eisenhower_quadrant',
    ]) {
      expect(
        FORM_GOVERNED_KEYS.has(typed),
        `${typed} has a control already; a second raw box over it loses data`,
      ).toBe(true);
    }
  });

  /**
   * `node_id` is the file's identity to the sync engine. A hand-edited one
   * splits the note into two documents on the next sync, and a nulled one is
   * worse — the file gets a fresh identity and every other device keeps the
   * old.
   */
  it('never offers the keys the app owns', () => {
    for (const structural of ['node_id', 'type', 'title', 'created_at', 'updated_at', 'order']) {
      expect(
        FORM_GOVERNED_KEYS.has(structural),
        `${structural} belongs to the app and must not be editable as text`,
      ).toBe(true);
    }
  });

  /** The point of the exercise: a field nobody wrote code for. */
  it('offers a field the user invented', () => {
    for (const invented of ['energy', 'aliases', 'mood', 'estimate_hours']) {
      expect(FORM_GOVERNED_KEYS.has(invented)).toBe(false);
    }
  });

  /**
   * Not everything the Tasks app fails to read belongs to the user.
   *
   * `checklist` is on every task in an older vault and nothing in the app
   * touches it — which made it look like exactly the case this section is
   * for, until it turned out to be declared on the app's own `TaskMetadata`
   * in `types/ipc.ts`. It is a feature that was started and left. Offering it
   * as an editable field invites someone to fill in something nothing reads.
   */
  it('does not offer the app its own unfinished fields back', () => {
    expect(FORM_GOVERNED_KEYS.has('checklist')).toBe(true);
  });

  /**
   * The write path still carries everything, governed or not.
   *
   * This is what made the fields survive while nothing showed them, and it has
   * to keep working now that some of them are shown: the ones the form does
   * not display are not sent, and patch semantics leave them alone.
   */
  it('keeps a hand-written field across a save', () => {
    const written = taskProperties({
      custom_fields: { energy: 'low', checklist: '' },
      status: 'todo',
      tags: ['work'],
    });

    expect(written.energy).toBe('low');
    expect(written.checklist).toBe('');
    expect(written.status).toBe('todo');
  });

  /**
   * A typed field wins over the same key in `custom_fields`.
   *
   * Both halves reach `taskProperties` and the file is one object; if the
   * order were the other way round, editing a due date in the picker would be
   * overwritten by whatever the file said when the form opened.
   */
  it('lets the form win over the file for a field it governs', () => {
    const written = taskProperties({
      custom_fields: { status: 'done', energy: 'low' },
      status: 'todo',
    });

    expect(written.status).toBe('todo');
    expect(written.energy).toBe('low');
  });
});
