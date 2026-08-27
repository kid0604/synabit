import { describe, it, expect } from 'vitest';
import { personMatchesQuery, searchPeople } from '../composables/search';

const person = (title: string, properties: Record<string, any> = {}) => ({
  id: `People/${title}.md`,
  title,
  properties,
});

const an = person('An Nguyễn', {
  nickname: 'Ann',
  relationship_type: ['Colleague'],
  tags: ['work', 'vietnam'],
  details: [
    { label: 'Work Email', value: 'an@acme.example', type: 'email' },
    { label: 'Location', value: 'Đà Nẵng', type: 'text' },
  ],
  company: 'Acme Corp',
});

describe('finding somebody by typing', () => {
  it('matches a name, a nickname, a relationship, a tag, a label and a value', () => {
    for (const query of [
      'nguyễn',
      'ann',
      'colleague',
      'vietnam',
      'Work Email',
      'an@acme',
      'đà nẵng',
      'acme corp',
    ]) {
      expect(personMatchesQuery(an, query), query).toBe(true);
    }
  });

  it('does not match what is not there', () => {
    expect(personMatchesQuery(an, 'hà nội')).toBe(false);
    expect(personMatchesQuery(an, 'zzz')).toBe(false);
  });

  it('ignores case and surrounding space', () => {
    expect(personMatchesQuery(an, '  NGUYỄN  ')).toBe(true);
  });

  it('matches everybody on an empty search', () => {
    // An empty box is not a filter; it is the absence of one.
    expect(personMatchesQuery(an, '')).toBe(true);
    expect(personMatchesQuery(an, '   ')).toBe(true);
  });

  it('reads a relationship stored the old way', () => {
    const old = person('Bình', { relationship_type: 'Friend, Colleague' });
    expect(personMatchesQuery(old, 'colleague')).toBe(true);
  });

  it('survives a person with nothing on them', () => {
    expect(personMatchesQuery(person('Cường'), 'cường')).toBe(true);
    expect(personMatchesQuery({ title: 'D', properties: null }, 'd')).toBe(true);
    expect(personMatchesQuery({}, 'anything')).toBe(false);
  });

  it('keeps the order it was given', () => {
    const people = [person('Cường'), person('An'), person('Bình')];
    expect(searchPeople(people, '').map(p => p.title)).toEqual(['Cường', 'An', 'Bình']);
  });
});

describe('at the size a real address book reaches', () => {
  /** Five thousand contacts, each with the fields a real import produces. */
  const many = Array.from({ length: 5000 }, (_, i) =>
    person(`Person${i} Nguyễn`, {
      nickname: `P${i}`,
      relationship_type: ['Colleague'],
      tags: ['work', 'imported'],
      details: [
        { label: 'Work Email', value: `person${i}@example.com`, type: 'email' },
        { label: 'Mobile Phone', value: `+84 90 ${String(i).padStart(4, '0')} 000`, type: 'phone' },
        { label: 'Home Address', value: `${i} Phố Huế, Hà Nội`, type: 'text' },
      ],
      company: `Company ${i}`,
    })
  );

  it('answers a keystroke fast enough not to be felt', () => {
    // The bar is one frame — 16ms — because this runs on every letter typed.
    // The first call over a person also builds their haystack, so the cold
    // pass is the expensive one and it is the one measured.
    const started = performance.now();
    const found = searchPeople(many, 'person4999');
    const cold = performance.now() - started;

    expect(found).toHaveLength(1);

    const warmStarted = performance.now();
    searchPeople(many, 'person4998');
    const warm = performance.now() - warmStarted;

    // Generous against a loaded machine, but it fails loudly if this ever
    // becomes ten times slower.
    console.log(`cold pass: ${cold.toFixed(1)}ms over 5000 contacts`);
    expect(cold, `cold pass took ${cold.toFixed(1)}ms`).toBeLessThan(250);
    console.log(`warm pass: ${warm.toFixed(1)}ms`);
    expect(warm, `warm pass took ${warm.toFixed(1)}ms`).toBeLessThan(50);
  });

  it('does not rebuild what it already worked out', () => {
    // Typing "n", "ng", "ngu", "nguy" is four passes over the same people.
    searchPeople(many, 'n');
    const started = performance.now();
    for (const query of ['ng', 'ngu', 'nguy', 'nguyễ', 'nguyễn']) searchPeople(many, query);
    const elapsed = performance.now() - started;

    console.log(`five keystrokes: ${elapsed.toFixed(1)}ms`);
    expect(elapsed, `five keystrokes took ${elapsed.toFixed(1)}ms`).toBeLessThan(100);
  });
});
