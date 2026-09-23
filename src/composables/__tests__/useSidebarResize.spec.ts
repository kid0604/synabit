import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { nextTick } from 'vue';

import { useSidebarResize } from '../useSidebarResize';

/**
 * Dragging a sidebar wider, and the three ways that goes wrong.
 *
 * Shared by Notes and Things, which is the reason it is worth testing at all:
 * a clamp that only Notes exercised would be a clamp Things discovers by
 * having its sidebar swallow the page.
 */

/** A handle sitting on the right edge of a sidebar that starts at `left`. */
const handleIn = (left: number) =>
  ({
    currentTarget: {
      parentElement: { getBoundingClientRect: () => ({ left }) },
    },
  }) as unknown as MouseEvent;

/** A pointer moved to `clientX` with the button still down. */
const moveTo = (clientX: number) => ({ clientX, buttons: 1 }) as MouseEvent;

describe('resizing a sidebar', () => {
  it('opens at the width the app asked for', () => {
    const notes = useSidebarResize({ left: { initial: 300 }, right: { initial: 288 } });
    const things = useSidebarResize({ left: { initial: 260 }, right: { initial: 300 } });

    expect(notes.leftWidth.value).toBe(300);
    expect(things.leftWidth.value).toBe(260);
    expect(things.rightWidth.value).toBe(300);

    // Separate state per mount: Things opening narrower must not drag Notes
    // along with it.
    things.leftWidth.value = 500;
    expect(notes.leftWidth.value).toBe(300);
  });

  /**
   * The measured edge, which used to be the constant 64 — the width of the
   * app's icon rail, written down a second time somewhere that has no other
   * reason to know it. A sidebar starting anywhere else resizes by the wrong
   * amount, and does it smoothly enough to look intentional.
   */
  it('measures where the sidebar starts instead of assuming', () => {
    const s = useSidebarResize({ left: { initial: 260, min: 220, max: 600 } });

    s.startDragLeft(handleIn(64));
    s.onMouseMove(moveTo(64 + 400));
    expect(s.leftWidth.value).toBe(400);

    // The same gesture in a window with no rail at all.
    const bare = useSidebarResize({ left: { initial: 260, min: 220, max: 600 } });
    bare.startDragLeft(handleIn(0));
    bare.onMouseMove(moveTo(400));
    expect(bare.leftWidth.value, 'the edge is read from the sidebar, not fixed').toBe(400);
  });

  it('refuses to go narrower than readable or wider than useful', () => {
    const s = useSidebarResize({ left: { initial: 260, min: 220, max: 600 } });
    s.startDragLeft(handleIn(64));

    s.onMouseMove(moveTo(64 + 10));
    expect(s.leftWidth.value).toBe(220);

    s.onMouseMove(moveTo(64 + 5000));
    expect(s.leftWidth.value).toBe(600);
  });

  it('grows the right sidebar as the pointer moves left', () => {
    const s = useSidebarResize({ right: { initial: 300, min: 220, max: 600 } });
    s.startDragRight();

    s.onMouseMove(moveTo(window.innerWidth - 450));
    expect(s.rightWidth.value).toBe(450);

    s.onMouseMove(moveTo(window.innerWidth - 20));
    expect(s.rightWidth.value).toBe(220);
  });

  /**
   * Let go outside the window — over a native menu, in another app — and no
   * mouseup ever reaches the page. The drag stayed live, so the next idle pass
   * of the mouse resized the sidebar with no button held: the app appearing to
   * move on its own, which is worse than a sidebar that will not resize.
   */
  it('lets go when the button is already up', () => {
    const s = useSidebarResize({ left: { initial: 260, min: 220, max: 600 } });
    s.startDragLeft(handleIn(64));
    s.onMouseMove(moveTo(64 + 400));
    expect(s.leftWidth.value).toBe(400);

    s.onMouseMove({ clientX: 64 + 500, buttons: 0 } as MouseEvent);
    expect(s.isDraggingLeft.value).toBe(false);
    expect(s.leftWidth.value, 'a buttonless move must not resize').toBe(400);

    // And stays let go.
    s.onMouseMove(moveTo(64 + 550));
    expect(s.leftWidth.value).toBe(400);
  });

  it('stops on mouseup', () => {
    const s = useSidebarResize();
    s.startDragLeft(handleIn(0));
    expect(s.isDraggingLeft.value).toBe(true);
    s.onMouseUp();
    expect(s.isDraggingLeft.value).toBe(false);
  });
});

/** The same fake store `useRemembered.spec.ts` uses, for the same reasons. */
const fakeStorage = () => {
  const map = new Map<string, string>();
  return {
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => void map.set(k, v),
    removeItem: (k: string) => void map.delete(k),
    clear: () => map.clear(),
    key: () => null,
    length: 0,
  } as unknown as Storage;
};

describe('a ceiling worked out from the window', () => {
  /// A fixed maximum is right on a big display and broken on a laptop: Nexus
  /// with `max: 900` on a 1024px window left the graph beside it 224px and
  /// called that a layout. A caller that cares passes a function.
  it('asks the function each time rather than freezing one answer', () => {
    let room = 960;
    const bar = useSidebarResize({ left: { initial: 480, min: 320, max: () => room - 360 } });

    bar.startDragLeft(handleIn(0));
    bar.onMouseMove(moveTo(5000));
    expect(bar.leftWidth.value).toBe(600);

    room = 1600;
    bar.onMouseMove(moveTo(5000));
    expect(bar.leftWidth.value).toBe(1240);
  });

  it('still takes a plain number, as every existing caller passes', () => {
    const bar = useSidebarResize({ left: { initial: 300, min: 220, max: 600 } });
    bar.startDragLeft(handleIn(0));
    bar.onMouseMove(moveTo(5000));
    expect(bar.leftWidth.value).toBe(600);
  });

  /// Shrinking the window must not leave a pane wider than the room it now
  /// has — including a width remembered from a larger screen.
  it('pulls a width back inside the bounds when asked', () => {
    let room = 1600;
    const bar = useSidebarResize({ left: { initial: 480, min: 320, max: () => room - 360 } });
    bar.startDragLeft(handleIn(0));
    bar.onMouseMove(moveTo(1200));
    expect(bar.leftWidth.value).toBe(1200);

    room = 800;
    bar.reclamp();
    expect(bar.leftWidth.value).toBe(440);
  });
});

describe('a width that survives a restart', () => {
  beforeEach(() => vi.stubGlobal('localStorage', fakeStorage()));
  afterEach(() => vi.unstubAllGlobals());

  it('opens where it was left, not at the initial', async () => {
    const first = useSidebarResize({ left: { initial: 480, min: 320, max: 900, remember: 'k' } });
    first.startDragLeft(handleIn(0));
    first.onMouseMove(moveTo(640));
    await nextTick();

    const second = useSidebarResize({ left: { initial: 480, min: 320, max: 900, remember: 'k' } });
    expect(second.leftWidth.value).toBe(640);
  });

  /// Without a key it behaves exactly as it always has, which is what keeps
  /// the four older callers unchanged.
  it('forgets when no key was given', async () => {
    const first = useSidebarResize({ left: { initial: 480, min: 320, max: 900 } });
    first.startDragLeft(handleIn(0));
    first.onMouseMove(moveTo(640));
    await nextTick();

    const second = useSidebarResize({ left: { initial: 480, min: 320, max: 900 } });
    expect(second.leftWidth.value).toBe(480);
  });
});
