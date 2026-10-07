import { describe, it, expect } from 'vitest';
import {
  parseNumberInput, parseDateInput, rawFromInput, displayText, draftOf, fits, compareCells, noteTarget,
  type Column,
} from '../model';

const number: Column = { name: 'n', type: 'number' };

describe('reading a number somebody typed', () => {
  it('reads Vietnamese the Vietnamese way', () => {
    expect(parseNumberInput('45.000', 'vi')).toBe(45000);
    expect(parseNumberInput('1.234.567', 'vi')).toBe(1234567);
    expect(parseNumberInput('45.000,5', 'vi')).toBe(45000.5);
    expect(parseNumberInput('1,5', 'vi')).toBe(1.5);
    expect(parseNumberInput('1.5', 'vi')).toBe(1.5);
    expect(parseNumberInput('45.000 ₫', 'vi')).toBe(45000);
    expect(parseNumberInput('-2.500', 'vi')).toBe(-2500);
  });

  it('reads English the English way', () => {
    expect(parseNumberInput('45,000', 'en')).toBe(45000);
    expect(parseNumberInput('45,000.5', 'en')).toBe(45000.5);
    expect(parseNumberInput('45.000', 'en')).toBe(45);
    expect(parseNumberInput('$1,200', 'en')).toBe(1200);
  });

  it('reads a percentage as a fraction', () => {
    expect(parseNumberInput('7,5%', 'vi')).toBe(0.075);
    expect(parseNumberInput('12%', 'en')).toBe(0.12);
  });

  it('reads nothing that is not a number', () => {
    expect(parseNumberInput('abc', 'vi')).toBeNull();
    expect(parseNumberInput('1.2.3', 'en')).toBeNull();
    expect(parseNumberInput('', 'vi')).toBeNull();
  });

  it('round-trips through the editor in either language', () => {
    for (const locale of ['vi', 'en']) {
      for (const raw of ['1.234', '45000.5', '0.075', '-3', '1234567']) {
        expect(rawFromInput(number, draftOf(number, raw, locale), locale)).toBe(raw);
      }
    }
  });
});

describe('writing what was typed', () => {
  it('keeps what does not fit, as typed', () => {
    expect(rawFromInput(number, 'khoảng 50k', 'vi')).toBe('khoảng 50k');
    expect(fits(number, 'khoảng 50k')).toBe(false);
  });

  it('writes dates as ISO, day first in Vietnamese', () => {
    const date: Column = { name: 'd', type: 'date' };
    expect(rawFromInput(date, '1/10/2026', 'vi')).toBe('2026-10-01');
    expect(rawFromInput(date, '1/10/2026', 'en')).toBe('2026-01-10');
    expect(rawFromInput(date, '2026-10-01 14:30', 'vi')).toBe('2026-10-01');
    expect(rawFromInput({ ...date, time: true }, '2026-10-01T14:30', 'vi')).toBe('2026-10-01 14:30');
    expect(parseDateInput('31/2/2026', 'vi')).toBeNull();
  });

  it('writes checkboxes, lists and notes in their file forms', () => {
    expect(rawFromInput({ name: 'c', type: 'checkbox' }, 'có', 'vi')).toBe('[x]');
    expect(rawFromInput({ name: 'm', type: 'multi' }, 'a,b , c', 'vi')).toBe('a, b, c');
    expect(rawFromInput({ name: 'n', type: 'note' }, 'Sách/Sapiens', 'vi')).toBe('[[Sách/Sapiens]]');
    expect(noteTarget('[[Sách/Sapiens|Sapiens]]')).toBe('Sách/Sapiens');
  });
});

describe('showing a cell', () => {
  it('formats a number by its column', () => {
    expect(displayText({ ...number, format: 'vnd' }, '45000', 'vi')).toMatch(/45\.000\s?₫/);
    expect(displayText({ ...number, format: 'percent' }, '0.075', 'en')).toBe('7.5%');
    expect(displayText(number, 'abc', 'vi')).toBe('abc');
  });
});

describe('sorting cells', () => {
  const collator = new Intl.Collator('vi', { numeric: true });
  it('sorts numbers as numbers, selects by their options, misfits last', () => {
    expect(['10', '9', 'x', '100'].sort((a, b) => compareCells(number, a, b, collator))).toEqual(['9', '10', '100', 'x']);
    const select: Column = { name: 's', type: 'select', options: ['Thấp', 'Vừa', 'Cao'] };
    expect(['Cao', 'Thấp', 'Vừa'].sort((a, b) => compareCells(select, a, b, collator))).toEqual(['Thấp', 'Vừa', 'Cao']);
  });
});
