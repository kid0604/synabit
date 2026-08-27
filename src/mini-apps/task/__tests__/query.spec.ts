import { describe, it, expect } from 'vitest';
import { translateTaskQuery } from '../query';

const backend = (raw: string) => translateTaskQuery(raw).backend;

describe('what reaches the query engine', () => {
  it('sends nothing for an empty search', () => {
    expect(backend('')).toBeNull();
    expect(backend('   ')).toBeNull();
  });

  it('always says which nodes are being asked about', () => {
    expect(backend('milk')).toBe('is:task milk');
  });

  /**
   * The trap. `is:` means *node type* to the engine, so `is:transferred` would
   * ask for nodes of a type nothing has and match nothing at all — a filter
   * that silently empties the list.
   */
  it('never lets a task flag through as a type', () => {
    for (const raw of ['is:transferred', 'not:transferred', 'is:tracked', 'not:tracked']) {
      expect(backend(`milk ${raw}`), raw).toBe('is:task milk');
    }
  });

  it('translates the state shorthands into the engine’s own syntax', () => {
    expect(backend('is:completed')).toBe('is:task status:done');
    expect(backend('is:todo')).toBe('is:task status:todo');
    expect(backend('is:in_progress')).toBe('is:task status:in_progress');
  });

  it('strips the shorthands the browser still owns', () => {
    expect(backend('milk p:1')).toBe('is:task milk');
    expect(backend('milk @Mai')).toBe('is:task milk');
    expect(backend('milk prop:cost=100')).toBe('is:task milk');
    expect(backend('milk prop:cost')).toBe('is:task milk');
  });

  /** These are the reason for the change; they must survive untouched. */
  it('passes through everything only the engine can answer', () => {
    expect(backend('date:today')).toBe('is:task date:today');
    expect(backend('due_date:<2026-09-01')).toBe('is:task due_date:<2026-09-01');
    expect(backend('priority:>2')).toBe('is:task priority:>2');
    expect(backend('sort:-due_date')).toBe('is:task sort:-due_date');
    expect(backend('limit:20')).toBe('is:task limit:20');
    expect(backend('in:title plan')).toBe('is:task in:title plan');
  });

  it('passes tags through, which the engine understands', () => {
    expect(backend('#work')).toBe('is:task #work');
  });

  it('keeps an exclusion, which is the engine’s job', () => {
    expect(backend('plan -draft')).toBe('is:task plan -draft');
  });

  it('handles a query mixing both dialects', () => {
    expect(backend('report is:transferred p:1 due_date:<2026-09-01 #work'))
      .toBe('is:task report due_date:<2026-09-01 #work');
  });

  it('leaves no double spaces behind when it strips', () => {
    expect(backend('a p:1 b')).not.toMatch(/ {2}/);
  });
});

/**
 * A round trip to be told "everything matches" is a round trip wasted, and on
 * a keystroke-by-keystroke search it is one per keystroke.
 */
describe('when the engine has nothing to do', () => {
  it('does not ask about a search of purely browser-owned tokens', () => {
    for (const raw of ['is:transferred', 'p:1', '@Mai', 'prop:cost=100', 'is:tracked not:transferred']) {
      expect(backend(raw), raw).toBeNull();
    }
  });

  it('does ask as soon as there is a word to search for', () => {
    expect(backend('is:transferred milk')).toBe('is:task milk');
  });

  it('does ask for a tag, a date or a comparison', () => {
    expect(backend('p:1 #work')).not.toBeNull();
    expect(backend('p:1 date:today')).not.toBeNull();
    expect(backend('p:1 due_date:<2026-09-01')).not.toBeNull();
  });
});

/**
 * The JavaScript filters read the raw string. Handing them a rewritten one
 * would mean a second translation to keep in step with the first.
 */
describe('what the browser still sees', () => {
  it('gets exactly what the user typed', () => {
    const raw = 'report is:transferred p:1 @Mai';
    expect(translateTaskQuery(raw).overlay).toBe(raw);
  });

  it('is trimmed but otherwise untouched', () => {
    expect(translateTaskQuery('  milk  ').overlay).toBe('milk');
  });
});
