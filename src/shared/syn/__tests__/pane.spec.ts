import { describe, it, expect, vi } from 'vitest';

/**
 * The pane module talks to Tauri at import time — it listens for the page and
 * asks once what is already open — so both have to exist before it loads.
 */
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve(null)) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));

const { typedAddress, PANE_BAR } = await import('../pane');

/**
 * What somebody typed into the browsing pane's address bar.
 *
 * A browser you cannot type an address into is a viewer, and the empty cookie
 * jar only makes sense if the person is expected to go and sign into things in
 * there themselves. So the bar has to accept what people actually type, which
 * is a bare host.
 */
describe('the address bar', () => {
  it('takes a bare host the way every address bar does', () => {
    expect(typedAddress('vnexpress.net')).toBe('https://vnexpress.net');
    expect(typedAddress('  jira.company.com/browse/X-1  ')).toBe('https://jira.company.com/browse/X-1');
  });

  it('leaves an address that already says what it is', () => {
    expect(typedAddress('https://a.test/x')).toBe('https://a.test/x');
    expect(typedAddress('http://192.168.1.4:8080')).toBe('http://192.168.1.4:8080');
  });

  it('has nothing to do with an empty box', () => {
    expect(typedAddress('')).toBe('');
    expect(typedAddress('   ')).toBe('');
  });

  /**
   * Not a search box. A sentence gets an `https://` glued to it and is then
   * refused by the backend's guard, which is where that answer belongs — one
   * rule about where this browser may go, applied to Syn and to the person
   * alike.
   */
  it('does not quietly turn a sentence into a search', () => {
    expect(typedAddress('kết quả mu everton')).toBe('https://kết quả mu everton');
  });

  /**
   * Rust reserves this many pixels out of the pane's rectangle for the app to
   * draw the bar in. A test in `pane.rs` reads this file and fails if the two
   * stop agreeing; this one is here so the number is not changed here by
   * accident either.
   */
  it('is the height Rust reserved for it', () => {
    expect(PANE_BAR).toBe(36);
  });
});
