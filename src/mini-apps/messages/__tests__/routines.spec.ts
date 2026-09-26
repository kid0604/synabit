import { describe, it, expect } from 'vitest';
import { daysInWords, toggleDay } from '../routines';

const t = (key: string) => key.replace('syn.', '');

describe('a routine said in words', () => {
  it('says every day and weekdays as such, and lists the rest in week order', () => {
    expect(daysInWords([], t)).toBe('routine_every_day');
    expect(daysInWords([1, 2, 3, 4, 5, 6, 7], t)).toBe('routine_every_day');
    expect(daysInWords([5, 4, 3, 2, 1], t)).toBe('weekday_1–weekday_5');
    expect(daysInWords([7, 3], t)).toBe('weekday_3, weekday_7');
  });

  it('adds a day, and takes it away again', () => {
    expect(toggleDay([1, 5], 3)).toEqual([1, 3, 5]);
    expect(toggleDay([1, 3, 5], 3)).toEqual([1, 5]);
  });
});
