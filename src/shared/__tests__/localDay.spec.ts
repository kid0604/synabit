import { describe, it, expect } from 'vitest';
import { asShown, localDay } from '../localDay';

describe('localDay', () => {
  it('reads the day of a UTC stamp in the reader’s zone', () => {
    // Noon UTC is the same calendar day in every zone from UTC-11 to UTC+11.
    expect(localDay('2026-09-14T12:00:00.000Z')).toBe('2026-09-14');

    const late = '2026-09-14T23:30:00.000Z';
    const d = new Date(late);
    const expected = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
    expect(localDay(late)).toBe(expected);
  });

  it('keeps a bare day and a local stamp without an offset as they are', () => {
    expect(localDay('2026-09-14')).toBe('2026-09-14');
    expect(localDay('2026-09-14 06:00:00')).toBe('2026-09-14');
    expect(localDay('2026-09-14T06:00')).toBe('2026-09-14');
  });

  it('shows something for a value it cannot read, and nothing for no value', () => {
    expect(localDay('last Tuesday')).toBe('last Tuesd');
    expect(localDay('')).toBe('');
    expect(localDay(null)).toBe('');
    expect(localDay(undefined)).toBe('');
  });
});

describe('A table cell, as a person reads it', () => {
  /// The symptom: a list of notes showed `2026-09-20T16:58:33.969Z` under
  /// each one, inside an app whose whole subject is when things happened.
  it('turns a stored instant into the day it was, where the reader is', () => {
    expect(asShown('2026-09-20T16:58:33.969Z')).toBe(localDay('2026-09-20T16:58:33.969Z'));
    expect(asShown('2026-09-20T16:58:33.969Z')).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });

  /// Guessing at a cell's meaning is how a table starts lying about what is in
  /// it. Only a stamp that carries a time is touched.
  it('leaves everything that is not a stamp exactly as it is', () => {
    for (const cell of ['2026-09-20', 'Gặp Khánh ở quán quen', 'Hà Nội', '13', '9.28', '', '2026']) {
      expect(asShown(cell), cell).toBe(cell);
    }
  });
});
