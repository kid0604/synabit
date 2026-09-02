import { ref } from 'vue';

/**
 * Dragging the edge of a sidebar to make it wider or narrower.
 *
 * Lifted out of Notes rather than copied into Things. It was already the only
 * implementation of this in the app, and two of them would have been two sets
 * of minimums, two clamps, and two answers to what happens when the pointer
 * leaves the window.
 *
 * The widths are per-mount, so each app keeps its own — Things opening at 260
 * does not shrink Notes to match. They are not remembered across launches,
 * which is what Notes has always done and is a real gap; nothing here persists.
 */

export interface SidebarSizing {
  /** Where it starts, before anybody drags. */
  initial: number;
  /** Narrow enough to be worth having, wide enough to still read. */
  min?: number;
  max?: number;
}

export interface SidebarResizeOptions {
  left?: SidebarSizing;
  right?: SidebarSizing;
}

const DEFAULT_LEFT: Required<SidebarSizing> = { initial: 300, min: 220, max: 600 };
const DEFAULT_RIGHT: Required<SidebarSizing> = { initial: 288, min: 200, max: 600 };

/** Below this the sidebars overlay the screen instead of sitting beside it. */
const LAP = 768;

export function useSidebarResize(options: SidebarResizeOptions = {}) {
  const left = { ...DEFAULT_LEFT, ...options.left };
  const right = { ...DEFAULT_RIGHT, ...options.right };

  const leftWidth = ref(left.initial);
  const showLeft = ref(window.innerWidth >= LAP);
  const rightWidth = ref(right.initial);
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
      leftWidth.value = Math.max(left.min, Math.min(e.clientX - edge.value, left.max));
    } else if (isDraggingRight.value) {
      rightWidth.value = Math.max(right.min, Math.min(window.innerWidth - e.clientX, right.max));
    }
  };

  const onMouseUp = () => {
    isDraggingLeft.value = false;
    isDraggingRight.value = false;
  };

  return {
    leftWidth,
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
