import { describe, it, expect } from 'vitest';
import { splitMentionQuery } from '../mentionQuery';

describe('splitMentionQuery', () => {
  it('treats a query with no bar as a plain search', () => {
    expect(splitMentionQuery('công ty')).toEqual({ search: 'công ty', alias: '' });
  });

  it('reads the text after the bar as what the link should say', () => {
    // The case the feature exists for: a note whose registered name is not
    // what anyone wants to read mid-sentence.
    expect(splitMentionQuery('công ty cổ phần abc|công ty cũ')).toEqual({
      search: 'công ty cổ phần abc',
      alias: 'công ty cũ',
    });
  });

  it('trims around the bar, because that is where people put spaces', () => {
    expect(splitMentionQuery('  abc  |  công ty cũ  ')).toEqual({
      search: 'abc',
      alias: 'công ty cũ',
    });
  });

  it('splits on the first bar so an alias may contain one', () => {
    expect(splitMentionQuery('abc|a|b')).toEqual({ search: 'abc', alias: 'a|b' });
  });

  it('reports no alias while the bar has just been typed', () => {
    // Mid-keystroke state: the menu should still search, and still offer the
    // title, rather than blanking out until something follows the bar.
    expect(splitMentionQuery('abc|')).toEqual({ search: 'abc', alias: '' });
  });

  it('keeps an empty search searchable rather than undefined', () => {
    expect(splitMentionQuery('')).toEqual({ search: '', alias: '' });
  });
});
