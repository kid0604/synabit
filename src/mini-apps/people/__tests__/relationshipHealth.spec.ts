import { describe, it, expect } from 'vitest';
import {
  contactStatus,
  contactPercent,
  contactDotClass,
  daysSinceContact,
  daysUntilDue,
  cadenceDays,
  relationshipAge,
} from '../composables/useRelationshipHealth';

/** Now, fixed, so a test never depends on the day it runs. */
const NOW = new Date('2026-08-25T12:00:00Z').getTime();

/** A person last contacted `days` ago, on `cadence`. */
const person = (days: number | null, cadence: string | null) => ({
  properties: {
    ...(days === null ? {} : { last_contacted: new Date(NOW - days * 86400_000).toISOString() }),
    ...(cadence === null ? {} : { contact_frequency: cadence }),
  },
});

describe('cadence', () => {
  it('reads the days each cadence allows', () => {
    expect(cadenceDays(person(0, 'weekly'))).toBe(7);
    expect(cadenceDays(person(0, 'biweekly'))).toBe(14);
    expect(cadenceDays(person(0, 'monthly'))).toBe(30);
    expect(cadenceDays(person(0, 'quarterly'))).toBe(90);
    expect(cadenceDays(person(0, 'yearly'))).toBe(365);
  });

  it('treats a cadence it does not know as untracked, not as 60 days', () => {
    // Two of the three copies of this table used to fall back to 60 days,
    // which invented a deadline nobody asked for.
    expect(cadenceDays(person(0, 'fortnightly'))).toBeNull();
    expect(contactStatus(person(90, 'fortnightly'), NOW)).toBe('unknown');
  });

  it('counts whole days since the last contact', () => {
    expect(daysSinceContact(person(0, 'weekly'), NOW)).toBe(0);
    expect(daysSinceContact(person(9, 'weekly'), NOW)).toBe(9);
    expect(daysSinceContact(person(null, 'weekly'), NOW)).toBeNull();
    expect(daysSinceContact({ properties: { last_contacted: 'never' } }, NOW)).toBeNull();
  });

  it('reports days until due, negative once late', () => {
    expect(daysUntilDue(person(2, 'weekly'), NOW)).toBe(5);
    expect(daysUntilDue(person(9, 'weekly'), NOW)).toBe(-2);
    expect(daysUntilDue(person(2, null), NOW)).toBeNull();
  });
});

describe('status', () => {
  it('moves through the bands as the cadence runs out', () => {
    // Boundaries as fractions of a 30-day cadence: .5, .85, 1.2
    expect(contactStatus(person(0, 'monthly'), NOW)).toBe('thriving');
    expect(contactStatus(person(15, 'monthly'), NOW)).toBe('thriving');
    expect(contactStatus(person(16, 'monthly'), NOW)).toBe('on_track');
    expect(contactStatus(person(25, 'monthly'), NOW)).toBe('on_track');
    expect(contactStatus(person(26, 'monthly'), NOW)).toBe('due_soon');
    expect(contactStatus(person(36, 'monthly'), NOW)).toBe('due_soon');
    expect(contactStatus(person(37, 'monthly'), NOW)).toBe('overdue');
  });

  it('is unknown until both a cadence and a last contact exist', () => {
    expect(contactStatus(person(null, 'weekly'), NOW)).toBe('unknown');
    expect(contactStatus(person(40, null), NOW)).toBe('unknown');
    expect(contactStatus({ properties: {} }, NOW)).toBe('unknown');
  });

  it('agrees with the reminders widget about who is overdue', () => {
    // The widget used to call anything past 1.0 overdue while the person's
    // own card only said so past 1.2 — the same contact read two ways.
    const late = person(33, 'monthly'); // ratio 1.1
    expect(contactStatus(late, NOW)).toBe('due_soon');
    expect(contactStatus(person(40, 'monthly'), NOW)).toBe('overdue');
  });
});

describe('sorting and display', () => {
  it('scores an untracked person as needing nothing', () => {
    // Sorted ascending, "needs attention" puts the most overdue first; a
    // person with no cadence is not late for anything, so they sort last.
    expect(contactPercent(person(null, null), NOW)).toBe(100);
    expect(contactPercent(person(0, 'monthly'), NOW)).toBe(100);
    expect(contactPercent(person(15, 'monthly'), NOW)).toBe(50);
    expect(contactPercent(person(60, 'monthly'), NOW)).toBe(0);
  });

  it('orders a mixed list by who needs attention first', () => {
    const list = [
      { id: 'untracked', ...person(null, null) },
      { id: 'fresh', ...person(1, 'monthly') },
      { id: 'late', ...person(45, 'monthly') },
      { id: 'due', ...person(28, 'monthly') },
    ];
    const order = [...list]
      .sort((a, b) => contactPercent(a, NOW) - contactPercent(b, NOW))
      .map(p => p.id);
    expect(order).toEqual(['late', 'due', 'fresh', 'untracked']);
  });

  it('shows no dot at all for an untracked person', () => {
    // A grey dot on every row would claim a status where there is none.
    expect(contactDotClass(person(null, null), NOW)).toBe('');
    expect(contactDotClass(person(1, 'monthly'), NOW)).toBe('bg-green-500');
    expect(contactDotClass(person(40, 'monthly'), NOW)).toBe('bg-red-500');
  });

  it('describes how long the relationship has been on file', () => {
    const at = (iso: string) => relationshipAge({ created_at: iso }, NOW);
    expect(at('2026-08-20T12:00:00Z')).toBe('5d');
    expect(at('2026-05-25T12:00:00Z')).toBe('3mo');
    expect(at('2024-05-25T12:00:00Z')).toBe('2y 3mo');
    expect(relationshipAge({}, NOW)).toBe('');
  });
});
