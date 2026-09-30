import { describe, it, expect } from 'vitest';
import { when } from '../when';

const NOW = Date.parse('2026-09-30T12:00:00Z');

describe('when', () => {
  it('says "now" for the last minute', () => {
    expect(when('2026-09-30T11:59:40Z', 'en', NOW)).toBe('now');
  });

  it('counts minutes, then hours', () => {
    expect(when('2026-09-30T11:55:00Z', 'en', NOW)).toBe('5 minutes ago');
    expect(when('2026-09-30T09:00:00Z', 'en', NOW)).toBe('3 hours ago');
  });

  it('speaks the reader’s language', () => {
    expect(when('2026-09-30T11:55:00Z', 'vi', NOW)).toContain('5 phút');
  });

  it('gives a date once it is older than a day', () => {
    const text = when('2026-09-20T08:30:00Z', 'en', NOW);
    expect(text).toMatch(/Sep 20, 2026/);
    expect(text).not.toContain('T08');
  });

  it('leaves something that is not a date alone', () => {
    expect(when('yesterday-ish', 'en', NOW)).toBe('yesterday-ish');
  });
});
