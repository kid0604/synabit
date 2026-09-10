import { describe, it, expect, vi } from 'vitest';

/**
 * The pane module talks to Tauri at import time — it listens for the page and
 * asks once what is already open — so both have to exist before it loads.
 */
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(() => Promise.resolve(null)) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));

vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn(() => Promise.resolve()) }));

const { typedAddress, PANE_BAR, leavesTheApp } = await import('../pane');

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

  /**
   * Syn answered with a link to an article. Clicking it navigated the app's own
   * webview to that article — the whole window became a news site, with no
   * sidebar, no conversation and no way back, because the way back is the app
   * and the app was gone. Quitting was the only exit.
   */
  it('knows a link that would take the app off its own pages', () => {
    expect(leavesTheApp('https://genk.vn/poco-f9-ultra.chn')).toBe(true);
    expect(leavesTheApp('http://vnexpress.net/')).toBe(true);
    expect(leavesTheApp('  https://genk.vn/  ')).toBe(true);
  });

  /**
   * A single-page app navigates inside itself all day. Only an absolute address
   * belonging to somebody else is a departure.
   */
  it('leaves the app navigating inside itself alone', () => {
    expect(leavesTheApp('#')).toBe(false);
    expect(leavesTheApp('#/messages')).toBe(false);
    expect(leavesTheApp('/notes/a.md')).toBe(false);
    expect(leavesTheApp('synabit://note/Notes%2Fa.md')).toBe(false);
    expect(leavesTheApp('')).toBe(false);
    expect(leavesTheApp(window.location.origin + '/index.html')).toBe(false);
  });

  /**
   * The listener has to be at the document, not in each component that renders
   * markdown. There are four of those already, and the next one would arrive
   * without this thought attached — Notes had the same hole, because its editor
   * handles `synabit://` links and falls through on everything else.
   */
  it('is wired up once, at the document', async () => {
    const app = (await import('../../../App.vue?raw')).default;

    expect(app).toContain("document.addEventListener('click', followExternalLink)");
    expect(app).toContain("document.removeEventListener('click', followExternalLink)");

    const handler = app.split('const followExternalLink = (e: MouseEvent) => {')[1]
      ?.split('\n};')[0] ?? '';
    expect(handler, 'App.vue should still have followExternalLink').toBeTruthy();
    // Composes with the components that stop their own clicks — `ArticleReader`
    // and the wiki-link handler both do — rather than racing them.
    expect(handler).toContain('e.defaultPrevented');
    expect(handler).toContain('leavesTheApp(href)');
    expect(handler).toContain('e.preventDefault()');
  });

  /**
   * A phone has no `add_child` at all, and a window too narrow to hold a
   * conversation *and* a browser gets no pane by design. Both end in the
   * person's own browser — and a **refused** address must not, which is why
   * neither decision is made on this side. Only Rust can tell those apart.
   */
  it('asks Rust for the page and does not second-guess the answer', async () => {
    const pane = await import('../pane');
    const source = (await import('../pane?raw')).default;

    const body = source.split('export async function openBeside')[1]?.split('\n}')[0] ?? '';
    expect(body, 'openBeside should still be there').toBeTruthy();
    expect(body).toContain("invoke<number>('syn_open_page'");
    expect(
      body,
      'a fallback here could not tell "no room" from "refused", and would open the second',
    ).not.toContain('openUrl');

    expect(typeof pane.openBeside).toBe('function');
  });
});

/**
 * How much of the app's width is furniture, and who is allowed to know.
 *
 * The pane's floor was measuring the app's whole webview and calling it the
 * conversation. Inside that webview sit the icon rail and whichever mini-app
 * sidebar is showing — sixty-four pixels plus a thread list somebody can pull
 * to 560 — so a floor of 320 left the conversation with less than nothing, and
 * the pane could be dragged straight across it until it reached the sidebar.
 *
 * The fix is a fact travelling to the policy, not a copy of the policy: this
 * side reports what it is showing, and `pane::layout` still decides where the
 * edge stops.
 */
describe('how much room the app needs', () => {
  it('reports the rail plus the sidebar, and nothing when there is no sidebar', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    const { sidebarRoom, RAIL } = await import('../pane');
    const said = () => (invoke as unknown as { mock: { calls: unknown[][] } }).mock.calls
      .filter(([name]) => name === 'syn_pane_room')
      .map(([, args]) => (args as { chrome: number }).chrome);

    sidebarRoom.value = 320;
    await new Promise(r => setTimeout(r, 0));
    expect(said().at(-1)).toBe(RAIL + 320);

    // Nothing beside the conversation is nothing, not a bare rail: an app that
    // has not said gets the floor exactly as it was.
    sidebarRoom.value = 0;
    await new Promise(r => setTimeout(r, 0));
    expect(said().at(-1)).toBe(0);
  });

  /**
   * A sidebar drag fires on every pointer move. The floor only moves when the
   * number does, and each call re-lays-out two webviews.
   */
  it('says nothing when nothing changed', async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    const { sidebarRoom } = await import('../pane');
    const count = () => (invoke as unknown as { mock: { calls: unknown[][] } }).mock.calls
      .filter(([name]) => name === 'syn_pane_room').length;

    sidebarRoom.value = 400;
    await new Promise(r => setTimeout(r, 0));
    const before = count();

    sidebarRoom.value = 400;
    await new Promise(r => setTimeout(r, 0));
    expect(count()).toBe(before);
  });

  /**
   * `RAIL` is Tailwind's `w-16` in `App.vue`, written here as a number because
   * this is the one place that has to add it up. Two numbers that have to agree
   * are two numbers that drift.
   */
  it('is the width the rail is actually drawn at', async () => {
    const { RAIL } = await import('../pane');
    const app = (await import('../../../App.vue?raw')).default;

    expect(RAIL).toBe(64);
    expect(app, 'the rail is still w-16, which is 4rem').toContain("'w-16 flex-shrink-0");
  });
});
