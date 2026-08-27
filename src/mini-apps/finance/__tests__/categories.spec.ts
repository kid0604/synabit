import { describe, it, expect } from 'vitest';
import { categoryName, nameIsTaken, newCategoryId, toCategories } from '../categories';

describe('toCategories', () => {
  /**
   * The shape a vault holds before the storage repair reaches it. Read as
   * objects it would render as a column of blanks, so it is read as what it is.
   */
  it('reads the old list of bare strings', () => {
    expect(toCategories(['Salary', 'Bonus'])).toEqual([
      { id: 'Salary', name: 'Salary' },
      { id: 'Bonus', name: 'Bonus' },
    ]);
  });

  it('reads the new list unchanged', () => {
    expect(toCategories([{ id: 'Salary', name: 'Lương' }])).toEqual([
      { id: 'Salary', name: 'Lương' },
    ]);
  });

  it('reads a mixed list, which is what a half-finished repair leaves', () => {
    expect(toCategories(['Salary', { id: 'Bonus', name: 'Thưởng' }])).toEqual([
      { id: 'Salary', name: 'Salary' },
      { id: 'Bonus', name: 'Thưởng' },
    ]);
  });

  it('falls back to the id when a name went missing', () => {
    expect(toCategories([{ id: 'Salary' }])).toEqual([{ id: 'Salary', name: 'Salary' }]);
  });

  it('drops entries with nothing usable in them', () => {
    expect(toCategories(['', null, {}, { name: 'no id' }, 42])).toEqual([]);
  });

  it('answers with an empty list for anything that is not one', () => {
    expect(toCategories(undefined)).toEqual([]);
    expect(toCategories('Salary')).toEqual([]);
  });
});

describe('categoryName', () => {
  const categories = [{ id: 'Food & Dining', name: 'Ăn uống' }];

  /** The whole point: a year of history keeps up with the new name. */
  it('gives the current name for an id a transaction still holds', () => {
    expect(categoryName(categories, 'Food & Dining')).toBe('Ăn uống');
  });

  /**
   * A category that was deleted leaves transactions behind holding its id —
   * which, for anything that predates ids, is the name it was filed under.
   * Showing that beats showing nothing.
   */
  it('falls back to the id, which reads as the original name', () => {
    expect(categoryName(categories, 'Entertainment')).toBe('Entertainment');
  });
});

describe('nameIsTaken', () => {
  const categories = [
    { id: 'Salary', name: 'Salary' },
    { id: 'cat-1', name: 'Bonus' },
  ];

  it('ignores case and surrounding space', () => {
    expect(nameIsTaken(categories, '  salary ')).toBe(true);
  });

  it('does not count a category against itself while it is being renamed', () => {
    expect(nameIsTaken(categories, 'Salary', 'Salary')).toBe(false);
  });

  it('says no for a name nobody has', () => {
    expect(nameIsTaken(categories, 'Freelance')).toBe(false);
  });
});

describe('newCategoryId', () => {
  /**
   * Not derived from the name, because the name is the one thing about a
   * category that is meant to change.
   */
  it('mints something that is not the name', () => {
    const id = newCategoryId();
    expect(id).toMatch(/^cat-\d+-\d+$/);
  });
});
