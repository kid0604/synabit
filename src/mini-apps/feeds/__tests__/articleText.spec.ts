import { describe, it, expect } from 'vitest';
import { cardTime, plainPreview } from '../articleText';

const now = Date.parse('2026-09-30T10:00:00Z');

describe('when an article was published, on its card', () => {
  it('says nothing rather than "Invalid Date"', () => {
    expect(cardTime('', 'vi', now)).toBe('');
    expect(cardTime('Đỗ Duy Thọ', 'vi', now)).toBe('');
    expect(cardTime("21' trước", 'en', now)).toBe('');
  });

  it('speaks the app language', () => {
    expect(cardTime('2026-09-30T09:39:00Z', 'vi', now)).toContain('21');
    expect(cardTime('2026-09-30T09:39:00Z', 'vi', now)).toMatch(/phút/);
    expect(cardTime('2026-09-30T07:00:00Z', 'en', now)).toMatch(/3 hr|3 hours|3h/);
  });

  it('gives a date once it is more than a week old', () => {
    expect(cardTime('2026-09-01T10:00:00Z', 'en', now)).toMatch(/Sep/);
    expect(cardTime('2025-09-01T10:00:00Z', 'en', now)).toMatch(/2025/);
  });
});

describe('the preview line', () => {
  it('drops a tag the list cut off half way', () => {
    const cut = '<figure>\n\n<img src="https://i1.vnecdn.net/tiasang/2026/08/17/Long-Mon-Phi-Giap';
    expect(plainPreview(cut)).toBe('');
    expect(plainPreview('<p>Các cải cách pháp luật</p><img src="https://x.test/a.jpg')).toBe('Các cải cách pháp luật');
  });

  it('turns entities back into characters and collapses spaces', () => {
    expect(plainPreview('<p>Tom &amp; Jerry\n\n   ride</p>')).toBe('Tom & Jerry ride');
  });

  it('cuts at the limit with an ellipsis', () => {
    expect(plainPreview('a'.repeat(200), 10)).toBe('aaaaaaaaaa…');
  });
});
