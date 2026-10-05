import { describe, expect, it } from 'vitest';
import { isDarkPaper, labelOn } from '../ink';

describe('the colour of words on a shape', () => {
  it('is dark on a light fill and light on a dark one', () => {
    expect(labelOn('#f59e0b')).toBe('#18181b');
    expect(labelOn('#06b6d4')).toBe('#18181b');
    expect(labelOn('#7c3aed')).toBe('#fafafa');
    expect(labelOn('#1f2937')).toBe('#fafafa');
  });

  it('is the paper on a shape filled with ink, and the ink with no fill or a faint one', () => {
    expect(labelOn('#000000')).toContain('--wb-paper');
    expect(labelOn(undefined)).toContain('--wb-ink');
    expect(labelOn('#7c3aed14')).toContain('--wb-ink');
  });

  it('tells dark paper from light', () => {
    expect(isDarkPaper('#1f2937')).toBe(true);
    expect(isDarkPaper('#fefce8')).toBe(false);
    expect(isDarkPaper('not a colour')).toBe(false);
  });
});
