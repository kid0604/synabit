import { describe, it, expect, vi, afterEach } from 'vitest';

import { focusWithSelection, captureFocus, MAX_SELECTION_READ } from '../focus';
import { askSynAbout, placeNearSelection, SYN_ASK_ABOUT, synAskFallback } from '../selectionAsk';

/**
 * A selection handed over by a button reaches Syn exactly as the key's does.
 *
 * The button remembers the text when it appears rather than reading the
 * screen when pressed, because on a phone the press can clear the selection
 * first. That makes it a second road to the same place, and the only way two
 * roads stay the same road is a test that walks both.
 */
describe('asking about a selection someone already made', () => {
  const where = { app: 'file', node: 'Docs/pricing.md' };

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('carries the text and where it came from', () => {
    expect(focusWithSelection(where, 'giá per-seat')).toEqual({
      app: 'file',
      node: 'Docs/pricing.md',
      selection: 'giá per-seat',
    });
  });

  it('is what the key would have sent for the same selection', () => {
    vi.spyOn(window, 'getSelection').mockReturnValue({
      toString: () => '  giá per-seat \n',
    } as unknown as Selection);
    expect(focusWithSelection(where, '  giá per-seat \n')).toEqual(captureFocus(where));
  });

  it('treats whitespace as nothing, the way the key does', () => {
    expect(focusWithSelection(where, '   \n\t')).toEqual({ app: 'file', node: 'Docs/pricing.md' });
    expect(focusWithSelection(where, undefined)).toEqual({ app: 'file', node: 'Docs/pricing.md' });
  });

  it('is cut at the same length as the key', () => {
    const long = 'a'.repeat(MAX_SELECTION_READ + 50);
    expect(focusWithSelection(where, long)?.selection).toHaveLength(MAX_SELECTION_READ);
  });

  /** The thread the person chose goes with it, as it does with the key. */
  it('keeps the thread', () => {
    expect(focusWithSelection({ ...where, thread: 'Threads/pricing.md' }, 'x')?.thread).toBe(
      'Threads/pricing.md',
    );
  });
});

describe('the event a mini-app raises', () => {
  it('carries the selection to whoever is listening', () => {
    const heard: string[] = [];
    const listener = (e: Event) => heard.push((e as CustomEvent).detail.selection);
    window.addEventListener(SYN_ASK_ABOUT, listener);
    askSynAbout('đoạn này');
    window.removeEventListener(SYN_ASK_ABOUT, listener);
    expect(heard).toEqual(['đoạn này']);
  });

  /** Mounted outside `App.vue`, nothing Syn-shaped is offered. */
  it('says no when nobody provided an answer', () => {
    const fallback = synAskFallback();
    expect(fallback.allowed.value).toBe(false);
    expect(fallback.shortcut.value).toBeNull();
  });
});

/**
 * Where the button goes. Measured in a 1000×800 window, for a 100×36 button.
 */
describe('placing the button beside a selection', () => {
  const viewport = { width: 1000, height: 800 };
  const size = { width: 100, height: 36 };
  const line = (top: number, left = 400, right = 600) => ({ top, bottom: top + 20, left, right });

  it('sits above the selection, centred on it', () => {
    expect(placeNearSelection(line(300), viewport, size)).toEqual({ top: 256, left: 450 });
  });

  /** A touch screen's own Copy / Select All callout is above. */
  it('sits below on a touch screen', () => {
    expect(placeNearSelection(line(300), viewport, size, 'below')).toEqual({ top: 328, left: 450 });
  });

  it('flips below when there is no room above', () => {
    expect(placeNearSelection(line(10), viewport, size)?.top).toBe(38);
  });

  it('flips above when there is no room below', () => {
    expect(placeNearSelection(line(770), viewport, size, 'below')?.top).toBe(726);
  });

  /** A selection taller than the window: pinned inside it, not off an edge. */
  it('stays on screen when neither side fits', () => {
    const tall = { top: 5, bottom: 795, left: 400, right: 600 };
    const at = placeNearSelection(tall, viewport, size)!;
    expect(at.top).toBeGreaterThanOrEqual(8);
    expect(at.top + size.height).toBeLessThanOrEqual(viewport.height - 8);
  });

  it('is kept inside the window sideways', () => {
    expect(placeNearSelection(line(300, 0, 20), viewport, size)?.left).toBe(8);
    expect(placeNearSelection(line(300, 980, 1000), viewport, size)?.left).toBe(892);
  });

  /** Scrolled away: no button pointing at nothing. */
  it('is not placed for a selection that is off screen', () => {
    expect(placeNearSelection(line(-100), viewport, size)).toBeNull();
    expect(placeNearSelection(line(900), viewport, size)).toBeNull();
  });
});
