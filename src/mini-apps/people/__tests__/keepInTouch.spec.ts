import { describe, it, expect, vi } from 'vitest';
import { today, snoozedFrom, useKeepInTouch } from '../composables/useKeepInTouch';
import { contactStatus, daysUntilDue } from '../composables/useRelationshipHealth';

const NOW = new Date(2026, 7, 25, 14, 30); // 25 August 2026, local

const person = (cadence: string | null, lastContacted = '2026-01-01') => ({
  id: 'People/an.md',
  title: 'An Nguyễn',
  properties: {
    ...(cadence === null ? {} : { contact_frequency: cadence }),
    last_contacted: lastContacted,
  },
});

describe('today', () => {
  it('uses the local date, not UTC', () => {
    // Late in the evening in Hanoi, `toISOString()` already says tomorrow;
    // early in the morning it still says yesterday. Either way the cadence
    // would count from the wrong day.
    expect(today(new Date(2026, 7, 25, 23, 59))).toBe('2026-08-25');
    expect(today(new Date(2026, 7, 25, 0, 1))).toBe('2026-08-25');
  });

  it('pads a single-digit month and day', () => {
    expect(today(new Date(2026, 0, 5))).toBe('2026-01-05');
  });
});

describe('snoozing', () => {
  it('moves the clock rather than silencing the cadence', () => {
    // Put a weekly nudge off for a week: it is due again in seven days, which
    // means counting from today.
    const snoozed = { properties: { ...person('weekly').properties, last_contacted: snoozedFrom(person('weekly'), 7, NOW) } };
    expect(daysUntilDue(snoozed, NOW.getTime())).toBe(7);
  });

  it('puts a monthly nudge off by a week, not by a month', () => {
    const subject = person('monthly');
    const snoozed = { properties: { ...subject.properties, last_contacted: snoozedFrom(subject, 7, NOW) } };
    expect(daysUntilDue(snoozed, NOW.getTime())).toBe(7);
    // And it is no longer overdue, which is the whole point of the button.
    expect(contactStatus(snoozed, NOW.getTime())).not.toBe('overdue');
  });

  it('falls back to today for somebody with no cadence', () => {
    // There is nothing to push off, so the honest answer is "seen today".
    expect(snoozedFrom(person(null), 7, NOW)).toBe('2026-08-25');
  });

  it('crosses a month boundary without inventing a date', () => {
    const early = new Date(2026, 8, 3); // 3 September
    expect(snoozedFrom(person('monthly'), 7, early)).toBe('2026-08-11');
  });
});

describe('answering a nudge', () => {
  const fakeNs = () => {
    const writes: any[] = [];
    return { writes, ns: { writeNode: vi.fn(async (p: any) => { writes.push(p); }) } };
  };

  it('records contact as a patch, touching nothing else', async () => {
    const { ns, writes } = fakeNs();
    const ok = await useKeepInTouch(ns).markContacted(person('weekly'));

    expect(ok).toBe(true);
    expect(writes).toHaveLength(1);
    expect(writes[0].relPath).toBe('People/an.md');
    // Only the one key: a write is a patch, and re-sending the rest would
    // overwrite whatever changed since this list was loaded.
    expect(Object.keys(writes[0].properties)).toEqual(['last_contacted']);
    expect(writes[0].properties.last_contacted).toBe(today());
    expect(writes[0].content).toBeUndefined();
  });

  it('reports a failure instead of pretending it worked', async () => {
    const ns = { writeNode: vi.fn(async () => { throw new Error('read-only vault'); }) };
    expect(await useKeepInTouch(ns).markContacted(person('weekly'))).toBe(false);
    expect(await useKeepInTouch(ns).snooze(person('weekly'))).toBe(false);
  });

  it('leaves the person object it was handed alone', async () => {
    const { ns } = fakeNs();
    const subject = person('weekly');
    const before = JSON.stringify(subject);
    await useKeepInTouch(ns).markContacted(subject);
    expect(JSON.stringify(subject)).toBe(before);
  });
});
