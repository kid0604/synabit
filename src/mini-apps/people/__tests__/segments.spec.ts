import { describe, it, expect } from 'vitest';
import {
  segmentFromNode,
  segmentToProperties,
  isEmptySegment,
  personMatches,
  peopleIn,
  emptySegment,
  type Segment,
} from '../composables/segments';

const NOW = new Date(2026, 7, 25); // 25 August 2026

const segment = (overrides: Partial<Segment> = {}): Segment => ({
  id: 'Filters/a.md',
  ...emptySegment(),
  ...overrides,
});

const person = (title: string, properties: Record<string, any> = {}) => ({
  id: `People/${title}.md`,
  title,
  properties,
});

/** Somebody last contacted `days` ago, on a monthly cadence. */
const tracked = (title: string, days: number) =>
  person(title, {
    contact_frequency: 'monthly',
    last_contacted: new Date(NOW.getTime() - days * 86400_000).toISOString().slice(0, 10),
  });

describe('reading and writing a segment', () => {
  it('survives a round trip through a filter node', () => {
    const draft = {
      name: 'Colleagues in Đà Nẵng',
      query: 'đà nẵng',
      relationships: ['Colleague'],
      tags: ['work'],
      statuses: ['overdue' as const],
      birthdayWithinDays: 30,
    };
    const properties = segmentToProperties(draft);
    const back = segmentFromNode({ id: 'Filters/a.md', title: draft.name, properties });

    expect(back).toEqual({ id: 'Filters/a.md', ...draft });
  });

  it('says what it is about, so a filter over notes is not offered here', () => {
    expect(segmentToProperties(emptySegment()).subject).toBe('person');
  });

  it('clears a condition with null rather than an empty list', () => {
    // A write is a patch, and an empty array would sit in the file forever.
    const properties = segmentToProperties(emptySegment());
    expect(properties.relationships).toBeNull();
    expect(properties.tags).toBeNull();
    expect(properties.query).toBeNull();
    expect(properties.birthday_within_days).toBeNull();
  });

  it('reads a node that is missing everything', () => {
    const back = segmentFromNode({ id: 'Filters/a.md', title: '', properties: {} });
    expect(back.name).toBe('Untitled');
    expect(back.relationships).toEqual([]);
    expect(back.birthdayWithinDays).toBeNull();
  });

  it('knows when a segment asks nothing at all', () => {
    expect(isEmptySegment(emptySegment())).toBe(true);
    expect(isEmptySegment({ ...emptySegment(), query: '  ' })).toBe(true);
    expect(isEmptySegment({ ...emptySegment(), tags: ['work'] })).toBe(false);
  });
});

describe('who a segment is about', () => {
  it('matches on any one of the values in a condition', () => {
    const s = segment({ relationships: ['Colleague', 'Client'] });
    expect(personMatches(s, person('An', { relationship_type: ['Colleague'] }), NOW)).toBe(true);
    expect(personMatches(s, person('Bình', { relationship_type: ['Client'] }), NOW)).toBe(true);
    expect(personMatches(s, person('Cường', { relationship_type: ['Friend'] }), NOW)).toBe(false);
  });

  it('narrows across conditions rather than widening', () => {
    // Colleagues who are also overdue — not colleagues plus everybody overdue.
    const s = segment({ relationships: ['Colleague'], statuses: ['overdue'] });
    const colleagueOverdue = { ...tracked('An', 60), properties: { ...tracked('An', 60).properties, relationship_type: ['Colleague'] } };
    const colleagueFresh = { ...tracked('Bình', 1), properties: { ...tracked('Bình', 1).properties, relationship_type: ['Colleague'] } };
    const friendOverdue = { ...tracked('Cường', 60), properties: { ...tracked('Cường', 60).properties, relationship_type: ['Friend'] } };

    expect(personMatches(s, colleagueOverdue, NOW)).toBe(true);
    expect(personMatches(s, colleagueFresh, NOW)).toBe(false);
    expect(personMatches(s, friendOverdue, NOW)).toBe(false);
  });

  it('searches a name, a relationship, a tag and a detail alike', () => {
    const an = person('An Nguyễn', {
      tags: ['vietnam'],
      details: [{ label: 'Location', value: 'Đà Nẵng', type: 'text' }],
      relationship_type: ['Colleague'],
    });
    for (const query of ['nguyễn', 'vietnam', 'đà nẵng', 'colleague', 'Location']) {
      expect(personMatches(segment({ query }), an, NOW), query).toBe(true);
    }
    expect(personMatches(segment({ query: 'hà nội' }), an, NOW)).toBe(false);
  });

  it('reads a relationship stored the old way too', () => {
    const s = segment({ relationships: ['Colleague'] });
    expect(personMatches(s, person('An', { relationship_type: 'Friend, Colleague' }), NOW)).toBe(true);
  });

  it('finds birthdays inside a window and not outside it', () => {
    const s = segment({ birthdayWithinDays: 7 });
    expect(personMatches(s, person('An', { birthday: '1994-08-27' }), NOW)).toBe(true);
    expect(personMatches(s, person('Bình', { birthday: '1994-10-01' }), NOW)).toBe(false);
    // No birthday on file cannot be inside a window.
    expect(personMatches(s, person('Cường', {}), NOW)).toBe(false);
  });

  it('never includes the vault owner', () => {
    // "Me" is the vault's idea of itself, not a contact.
    const s = segment({ query: 'me' });
    expect(personMatches(s, person('Me', { is_owner: true }), NOW)).toBe(false);
  });

  it('filters a list and leaves its order alone', () => {
    const people = [tracked('An', 60), tracked('Bình', 1), tracked('Cường', 90)];
    const got = peopleIn(segment({ statuses: ['overdue'] }), people, NOW);
    expect(got.map(p => p.title)).toEqual(['An', 'Cường']);
  });

  it('matches everybody when it asks nothing', () => {
    const people = [person('An'), person('Bình')];
    expect(peopleIn(segment(), people, NOW)).toHaveLength(2);
  });
});
