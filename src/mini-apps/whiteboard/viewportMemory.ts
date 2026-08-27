/**
 * Where each board was last left, on this device.
 *
 * Opening a board put the camera back at the origin every time, however far
 * out the work actually was — a board is an infinite plane, so that is not a
 * small annoyance.
 *
 * The board file has a `viewport` field and this could have been written
 * there, but should not be. Two reasons:
 *
 *   - a pan is not an edit. Writing the file would stamp it as changed, push
 *     the whole board to every other device, and let a scroll win a
 *     last-write-wins comparison against somebody else's real work.
 *   - a camera is not shared. A phone and a desktop want different views of
 *     the same board, and moving one should not move the other.
 *
 * So it lives here, per device, and the file's `viewport` stays what it has
 * always been: where a board opens on a device that has never seen it.
 */

export interface Viewport {
  x: number;
  y: number;
  zoom: number;
}

const KEY_PREFIX = 'whiteboard:viewport:';

/** Storage is absent in some environments and full in others; neither is fatal. */
function storage(): Storage | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

export function rememberViewport(boardId: string, viewport: Viewport): void {
  if (!boardId) return;
  const store = storage();
  if (!store) return;
  try {
    store.setItem(KEY_PREFIX + boardId, JSON.stringify(viewport));
  } catch {
    // A quota error here costs the user a camera position, and throwing would
    // cost them the pan they just did.
  }
}

/** Where this board was left, or null if it has not been opened here. */
export function recallViewport(boardId: string): Viewport | null {
  if (!boardId) return null;
  const store = storage();
  if (!store) return null;

  const raw = store.getItem(KEY_PREFIX + boardId);
  if (!raw) return null;

  try {
    const parsed = JSON.parse(raw);
    const { x, y, zoom } = parsed ?? {};
    // A zoom of zero or NaN is a canvas nobody can see anything on.
    if (![x, y, zoom].every((v) => typeof v === 'number' && Number.isFinite(v))) return null;
    if (zoom <= 0) return null;
    return { x, y, zoom };
  } catch {
    return null;
  }
}

/** Drop what was remembered about a board that no longer exists. */
export function forgetViewport(boardId: string): void {
  if (!boardId) return;
  storage()?.removeItem(KEY_PREFIX + boardId);
}
