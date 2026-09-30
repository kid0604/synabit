import { describe, it, expect } from 'vitest';
import { rovingIndex } from '../roving';

describe('rovingIndex', () => {
  it('moves with the arrows and wraps at both ends', () => {
    expect(rovingIndex('ArrowRight', 0, 3)).toBe(1);
    expect(rovingIndex('ArrowDown', 2, 3)).toBe(0);
    expect(rovingIndex('ArrowLeft', 0, 3)).toBe(2);
    expect(rovingIndex('ArrowUp', 1, 3)).toBe(0);
  });

  it('jumps with Home and End', () => {
    expect(rovingIndex('Home', 2, 3)).toBe(0);
    expect(rovingIndex('End', 0, 3)).toBe(2);
  });

  it('leaves every other key alone', () => {
    expect(rovingIndex('Tab', 0, 3)).toBeNull();
    expect(rovingIndex(' ', 0, 3)).toBeNull();
    expect(rovingIndex('ArrowRight', 0, 0)).toBeNull();
  });
});
