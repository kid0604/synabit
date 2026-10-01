import { describe, it, expect } from 'vitest';
import { feedErrorKey } from '../feedErrors';

describe('feed errors, in words', () => {
  it('names the problem instead of printing the backend message', () => {
    expect(feedErrorKey('Not a usable URL: relative URL without a base', 'x').key).toBe('feeds.error_url_invalid');
    expect(feedErrorKey('Refusing to fetch localhost — that address is on this machine or its private network', 'x').key).toBe('feeds.error_url_private');
    expect(feedErrorKey('HTTP 404 Not Found from https://genk.vn/rss', 'x').key).toBe('feeds.error_not_found');
    expect(feedErrorKey('HTTP 403 Forbidden from https://genk.vn', 'x')).toEqual({ key: 'feeds.error_site_refused', params: { status: '403' } });
    expect(feedErrorKey(new Error('error sending request for url'), 'x').key).toBe('feeds.error_unreachable');
  });

  it('falls back to the caller’s sentence for anything unknown', () => {
    expect(feedErrorKey('something odd', 'feeds.add_failed').key).toBe('feeds.add_failed');
  });
});
