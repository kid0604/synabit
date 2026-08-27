import { describe, it, expect } from 'vitest';
import {
  relationshipsOf,
  normalizeRelationships,
  relationshipLabel,
  matchesRelationship,
  titleCase,
} from '../composables/relationships';

const person = (relationship_type: unknown) => ({ properties: { relationship_type } });

describe('reading relationships', () => {
  it('reads a list as a list', () => {
    expect(relationshipsOf(person(['Friend', 'Colleague']))).toEqual(['Friend', 'Colleague']);
  });

  it('still reads a vault written before this was a list', () => {
    // Every screen has to keep working on the day this ships, not after
    // somebody remembers to migrate.
    expect(relationshipsOf(person('Friend, Colleague'))).toEqual(['Friend', 'Colleague']);
    expect(relationshipsOf(person('  Friend ,  Colleague  '))).toEqual(['Friend', 'Colleague']);
  });

  it('keeps a comma that belongs to the name itself', () => {
    // The reason for the change: as one string, this was two relationships
    // and there was no way to tell.
    expect(relationshipsOf(person(['Bạn, đồng nghiệp cũ']))).toEqual(['Bạn, đồng nghiệp cũ']);
  });

  it('treats an absent, empty or wrong-shaped value as none', () => {
    expect(relationshipsOf(person(undefined))).toEqual([]);
    expect(relationshipsOf(person(''))).toEqual([]);
    expect(relationshipsOf(person([]))).toEqual([]);
    expect(relationshipsOf(person(', , '))).toEqual([]);
    expect(relationshipsOf(person(42))).toEqual([]);
    expect(relationshipsOf({})).toEqual([]);
    expect(relationshipsOf(null)).toEqual([]);
  });

  it('drops anything in the list that is not text', () => {
    expect(normalizeRelationships(['Friend', null, 7, '  ', 'Colleague'])).toEqual([
      'Friend',
      'Colleague',
    ]);
  });
});

describe('showing relationships', () => {
  it('joins them for a place with room for one line', () => {
    expect(relationshipLabel(person(['Friend', 'Colleague']))).toBe('Friend, Colleague');
    expect(relationshipLabel(person([]))).toBe('');
  });
});

describe('searching relationships', () => {
  it('matches any one of them, in either shape', () => {
    expect(matchesRelationship(person(['Friend', 'Colleague']), 'colle')).toBe(true);
    expect(matchesRelationship(person('Friend, Colleague'), 'colle')).toBe(true);
    expect(matchesRelationship(person(['Friend']), 'mentor')).toBe(false);
  });

  it('does not match everybody on an empty search', () => {
    expect(matchesRelationship(person(['Friend']), '')).toBe(false);
    expect(matchesRelationship(person(['Friend']), '   ')).toBe(false);
  });
});

describe('titleCase', () => {
  it('capitalises each word and leaves the rest alone', () => {
    expect(titleCase('close friend')).toBe('Close Friend');
    expect(titleCase('  đồng   nghiệp ')).toBe('Đồng Nghiệp');
    expect(titleCase('')).toBe('');
  });
});
