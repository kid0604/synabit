import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../../utils/logger';

/**
 * How much of the window the browsing pane is taking, as a fraction.
 *
 * # Why a fraction and not a pixel count
 *
 * Because the pane is kept by the runtime at a **rate** of the window —
 * `auto_resize` in wry stores `width_rate` and reapplies it on every resize. A
 * CSS width expressed in the same fraction therefore stays correct through
 * every drag with nothing to notify and nothing to drift. A pixel count would
 * need a resize listener on this side, and two numbers that have to agree are
 * two numbers that eventually do not.
 *
 * # Why a module-level ref rather than an event
 *
 * The pane is opened from the Syn mini-app and the room for it is made by the
 * app's root element in `App.vue`. One shared ref is the whole of that
 * conversation. An event would be the same thing with a delivery guarantee to
 * worry about.
 *
 * Zero means closed, which is also what every platform that has no pane
 * reports — so nothing has to ask whether a pane is possible before reading it.
 */
export const paneShare = ref(0);

/**
 * Where the pane opens when nothing else is said.
 *
 * A real page rather than a blank one: a blank pane says nothing about whether
 * text reflows sensibly at that width, which is half of what there is to look
 * at.
 */
export const SOMEWHERE_TO_START = 'https://vnexpress.net/';

/**
 * Open the browsing pane, or send it somewhere new.
 *
 * The share comes back from Rust, which owns the layout arithmetic — including
 * the decision that a window too narrow for both keeps the conversation and
 * gets no pane at all. Zero back means exactly that, and the app draws itself
 * full width as it always did.
 */
export async function openPane(url: string = SOMEWHERE_TO_START): Promise<void> {
  try {
    paneShare.value = await invoke<number>('syn_pane_open', { url });
  } catch (e) {
    logger.error('[Syn] The browsing pane would not open', e);
    paneShare.value = 0;
  }
}

/**
 * Take the pane off the screen for the length of a drag, and put it back.
 *
 * # Why a drag needs this
 *
 * The pane is a separate OS webview, and only one webview has the pointer at a
 * time. Drag its edge and the pane moves to meet the pointer — which puts the
 * pointer **on the pane**, and this webview stops receiving mouse events at
 * all. The drag dies after one frame, and from the outside it looks like a
 * handle that does nothing. Which is exactly what it looked like.
 *
 * `setPointerCapture` does not help. That keeps events flowing across DOM
 * elements; this boundary is below the DOM.
 *
 * So the pane is hidden for the pull. The whole window belongs to this webview
 * again, the drag is tracked from start to finish, and the pane comes back at
 * the width that was chosen.
 */
export async function paneDragging(dragging: boolean): Promise<void> {
  try {
    await invoke('syn_pane_dragging', { dragging });
  } catch (e) {
    logger.error('[Syn] Could not put the pane aside for the drag', e);
  }
}

/**
 * Where the pane was before a drag started, so a failed drag can put it back.
 *
 * The way home is `dragPaneTo`, and it is the only one — but if the call that
 * brings it home is the call that fails, the pane is left parked off the right
 * edge with nothing to fetch it. This is that.
 */
let lastGood = 0;

/**
 * Settle the pane at the width the drag ended on.
 *
 * Once, on release, rather than per frame — with the pane hidden there is
 * nothing to move until then, and the preview line is drawn in CSS. The
 * clamping is Rust's: `pane::layout` holds the floors, and a copy of them here
 * would be a second opinion that drifts the first time one of them changed.
 */
export async function dragPaneTo(share: number): Promise<void> {
  const before = lastGood || paneShare.value;
  try {
    paneShare.value = await invoke<number>('syn_pane_resize', { share });
    lastGood = paneShare.value;
  } catch (e) {
    logger.error('[Syn] The pane would not move', e);
    // Parked off the edge with the call that fetches it having failed. One
    // attempt to put it back where it was; if that fails too, the globe closes
    // and reopens it.
    try {
      paneShare.value = await invoke<number>('syn_pane_resize', { share: before });
    } catch (again) {
      logger.error('[Syn] And could not be put back; close and reopen it', again);
    }
  }
}

/** Put it away and give the app the whole window back. */
export async function closePane(): Promise<void> {
  try {
    await invoke('syn_pane_close');
  } catch (e) {
    logger.error('[Syn] The browsing pane would not close', e);
  } finally {
    // Whatever the backend said, the app stops leaving room. A pane that failed
    // to close is a visible problem; a gap kept for a pane that is gone is an
    // invisible one.
    paneShare.value = 0;
  }
}
