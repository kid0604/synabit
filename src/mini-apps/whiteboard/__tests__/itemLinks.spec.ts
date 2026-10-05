import { describe, expect, it } from 'vitest';
import { cleanLink, vaultTarget } from '../itemLinks';

describe('a link on an item', () => {
  it('takes web pages, email and vault links, adding the scheme a web address left out', () => {
    expect(cleanLink('example.com/a')).toBe('https://example.com/a');
    expect(cleanLink('http://x.test')).toBe('http://x.test/');
    expect(cleanLink('mailto:an@acme.test')).toBe('mailto:an@acme.test');
    expect(cleanLink('synabit://task/Tasks%2Fa.md')).toBe('synabit://task/Tasks%2Fa.md');
  });

  it('refuses what one click should never run', () => {
    expect(cleanLink('javascript:alert(1)')).toBeNull();
    expect(cleanLink('file:///etc/passwd')).toBeNull();
    expect(cleanLink('  ')).toBeNull();
  });

  it('reads where a vault link goes', () => {
    expect(vaultTarget('synabit://person/People%2Fan.md')).toEqual({ kind: 'person', id: 'People/an.md' });
    expect(vaultTarget('https://x.test')).toBeNull();
  });
});

describe('a vault link that is not well formed', () => {
  it('is read as written rather than thrown on', () => {
    expect(vaultTarget('synabit://note/50%-off.md')).toEqual({ kind: 'note', id: '50%-off.md' });
    expect(vaultTarget('synabit://note/%E0')).toEqual({ kind: 'note', id: '%E0' });
    expect(vaultTarget('synabit://note/a%20b.md')).toEqual({ kind: 'note', id: 'a b.md' });
  });
});
