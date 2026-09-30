import { describe, it, expect } from 'vitest';
import { detailLabel } from '../detailLabels';

const t = (key: string) => `«${key}»`;

describe('detailLabel', () => {
  it('translates a stored preset label for display', () => {
    expect(detailLabel('Email', t)).toBe('«people.detail_email»');
    expect(detailLabel('How We Met', t)).toBe('«people.how_we_met»');
  });

  it('leaves what somebody typed, and brand names, as they are', () => {
    expect(detailLabel('Nickname', t)).toBe('Nickname');
    expect(detailLabel('LinkedIn', t)).toBe('LinkedIn');
  });
});
