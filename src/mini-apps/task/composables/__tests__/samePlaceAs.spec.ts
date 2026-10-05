import { describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
import { samePlaceAs } from '../useProjectManager';

describe('a link to a project', () => {
  it('is the same link whatever title it was written with', () => {
    const same = samePlaceAs('[Launch v2](synabit://project/p1)');
    expect(same('[Launch](synabit://project/p1)')).toBe(true);
    expect(same('[Launch v2](synabit://project/p1)')).toBe(true);
    expect(same('[Launch](synabit://project/p10)')).toBe(false);
    expect(same('[Other](synabit://project/p2)')).toBe(false);
  });
});
