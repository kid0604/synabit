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
