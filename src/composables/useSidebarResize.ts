import { ref, type Ref } from 'vue';
import { useRememberedNumber } from './useRemembered';

/**
 * Dragging the edge of a sidebar to make it wider or narrower.
 *
 * Lifted out of Notes rather than copied into Things. It was already the only
 * implementation of this in the app, and two of them would have been two sets
 * of minimums, two clamps, and two answers to what happens when the pointer
 * leaves the window.
 *
 * The widths are per-mount, so each app keeps its own — Things opening at 260
 * does not shrink Notes to match. A caller that passes `remember` also keeps
 * its width across launches, per device; one that does not is unchanged.
 *
 * The drag handlers read a `MouseEvent`, and a `PointerEvent` **is** one — so
 * a caller may wire `pointermove`/`pointerup` instead and get touch and pen
 * for free. Nexus does; the older callers are still on mouse events.
 */

export interface SidebarSizing {
  /** Where it starts, before anybody drags. */
  initial: number;
  /** Narrow enough to be worth having, wide enough to still read. */
  min?: number;
  /**
   * The widest it may get — a number, or one worked out when asked.
   *
   * A function is for a ceiling that depends on the window: a fixed 900 is
   * right on a big screen and broken on a laptop, where it leaves the pane
   * beside it 200px and calls that a layout. Measured beats assumed, so a
   * caller that cares passes a function reading its own container.
   */
  max?: number | (() => number);
  /**
   * A storage key, if this width should survive a restart.
   *
   * Left out means what it has always meant: the width lasts as long as the
   * mount. A size is a per-device display choice — see `useRemembered` for
   * why that never goes near the vault.
   */
  remember?: string;
}

export interface SidebarResizeOptions {
  left?: SidebarSizing;
  right?: SidebarSizing;
}

/** The bounds every sidebar has; `remember` is the caller's to add. */
type Bounds = Required<Omit<SidebarSizing, 'remember'>> & { remember?: string };

/** The ceiling right now, whichever way the caller expressed it. */
const ceiling = (max: number | (() => number)): number =>
  typeof max === 'function' ? max() : max;

const DEFAULT_LEFT: Bounds = { initial: 300, min: 220, max: 600 };
const DEFAULT_RIGHT: Bounds = { initial: 288, min: 200, max: 600 };

/** Below this the sidebars overlay the screen instead of sitting beside it. */
const LAP = 768;

export function useSidebarResize(options: SidebarResizeOptions = {}) {
  const left = { ...DEFAULT_LEFT, ...options.left };
  const right = { ...DEFAULT_RIGHT, ...options.right };

  /** Remembered when the caller named a key, otherwise just a ref. */
  const width = (sizing: Bounds): Ref<number> =>
    sizing.remember
      ? useRememberedNumber(sizing.remember, {
          min: sizing.min,
          max: ceiling(sizing.max),
          fallback: sizing.initial,
        })
      : ref(sizing.initial);

  const leftWidth = width(left);
  const showLeft = ref(window.innerWidth >= LAP);
  const rightWidth = width(right);
  const showRight = ref(window.innerWidth >= LAP);

  const isDraggingLeft = ref(false);
  const isDraggingRight = ref(false);

  /**
   * Where the left sidebar begins, measured rather than assumed.
   *
   * This used to be `e.clientX - 64`, the width of the app's icon rail written
   * out a second time in a file that has no other reason to know about it.
   * Correct today and quietly wrong the moment that rail changes — and wrong in
   * the way that is hard to name, since the sidebar would still resize, just
   * always by the wrong amount.
   */
  const edge = ref(0);

  const startDragLeft = (event: MouseEvent) => {
    const box = (event.currentTarget as HTMLElement | null)
      ?.parentElement?.getBoundingClientRect();
    edge.value = box?.left ?? 0;
    isDraggingLeft.value = true;
  };

  const startDragRight = () => {
    isDraggingRight.value = true;
  };

  const onMouseMove = (e: MouseEvent) => {
    // Released somewhere the window never heard about it — outside the app,
    // over a native menu, in another window. Without this the drag stays live
    // and the next idle pass of the mouse resizes the sidebar with no button
    // held down, which reads as the app having gone haywire.
    if (e.buttons === 0) {
      isDraggingLeft.value = false;
      isDraggingRight.value = false;
      return;
    }

    if (isDraggingLeft.value) {
      leftWidth.value = Math.max(left.min, Math.min(e.clientX - edge.value, ceiling(left.max)));
    } else if (isDraggingRight.value) {
      rightWidth.value = Math.max(right.min, Math.min(window.innerWidth - e.clientX, ceiling(right.max)));
    }
  };

  const onMouseUp = () => {
    isDraggingLeft.value = false;
    isDraggingRight.value = false;
  };

  /** Bring the widths back inside their bounds, after the window changed. */
  const reclamp = () => {
    leftWidth.value = Math.max(left.min, Math.min(leftWidth.value, ceiling(left.max)));
    rightWidth.value = Math.max(right.min, Math.min(rightWidth.value, ceiling(right.max)));
  };

  return {
    leftWidth,
    reclamp,
    showLeft,
    rightWidth,
    showRight,
    isDraggingLeft,
    isDraggingRight,
    startDragLeft,
    startDragRight,
    onMouseMove,
    onMouseUp,
  };
}
