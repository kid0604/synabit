import { ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
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
 * Rust says how much room to leave; nothing here works it out.
 *
 * A return value only reaches whoever called, and the globe is not the only
 * thing that opens the pane — **Syn** opens it to look something up, and then
 * nothing on this side has called anything. The app went on drawing itself full
 * width while the pane painted over the conversation, which is what a browser
 * appearing on top of your work looks like.
 *
 * Listened for once, at module load, because the pane belongs to the window and
 * not to any component that might be unmounted when the news arrives.
 */
listen<number>('syn-pane-share', event => {
  paneShare.value = event.payload;
}).catch(e => logger.error('[Syn] Could not listen for the pane', e));

/**
 * How wide the icon rail down the left of the window is, in CSS pixels.
 *
 * `w-16` in `App.vue`, which is Tailwind's 4rem. Repeated here because this is
 * the one place that has to add it up, and `the_rail_is_the_width_this_says_it_is`
 * fails if the class changes.
 */
export const RAIL = 64;

/**
 * How wide the current mini-app's own sidebar is, in CSS pixels.
 *
 * # Why anything outside the app cares
 *
 * Because the browsing pane's floor was measuring the app's whole webview and
 * calling it the conversation. Inside that webview sit this rail and a thread
 * list somebody can pull between 240 and 560 — so a floor of 320 left the
 * conversation with less than nothing, and the pane could be dragged clean over
 * it and kept going until it hit the sidebar.
 *
 * How wide that furniture is right now is a fact only this side has. The policy
 * — how much conversation must survive — stays in `pane::layout`. This carries
 * the fact there and carries no opinion with it.
 *
 * Zero means "nothing beside the conversation", which is what an app that has
 * not said gets, and is the floor as it was before any of this.
 */
export const sidebarRoom = ref(0);

let roomTold = -1;

/**
 * Tell Rust, when it has changed, and only then.
 *
 * A drag of the sidebar edge fires on every pointer move; the floor only moves
 * when the number does, and each call re-lays-out two webviews.
 */
watch(sidebarRoom, async room => {
  const chrome = room > 0 ? RAIL + room : 0;
  if (chrome === roomTold) return;
  roomTold = chrome;
  try {
    await invoke('syn_pane_room', { chrome });
  } catch (e) {
    logger.error('[Syn] Could not say how much room the app needs', e);
  }
}, { immediate: true });

/**
 * How tall the strip above the pane is, in CSS pixels.
 *
 * The app draws the address bar there and Rust reserves it out of the pane's
 * rectangle — `pane::BAR`, which this must equal. Two numbers that have to
 * agree are two numbers that drift, so a test in `pane.rs` reads this file and
 * fails if they stop matching.
 */
export const PANE_BAR = 36;

/** The page in the pane, or nothing when there is no pane. */
export interface PanePage {
  url: string;
  title: string;
}

/**
 * What the pane is showing.
 *
 * Rust is the only thing that can know: the pane is an operating-system webview
 * beside the app, not an element in it, so nothing here can read its address.
 */
export const panePage = ref<PanePage | null>(null);

listen<PanePage | null>('syn-pane-page', event => {
  panePage.value = event.payload;
}).catch(e => logger.error('[Syn] Could not listen for the page', e));

/**
 * Ask once, at load, what is already open.
 *
 * The pane outlives a reload of the front end — it belongs to the window, and
 * a hot reload replaces only the app's own webview. Without this the bar comes
 * back blank beside a pane that is still showing a page.
 */
invoke<PanePage | null>('syn_pane_page')
  .then(page => { panePage.value = page; })
  .catch(() => { /* No pane, or a platform without one. Blank is correct. */ });

/** Back, and forward again — the pane's own history, one step at a time. */
export async function panePageBack(): Promise<void> {
  try {
    await invoke('syn_pane_back');
  } catch (e) {
    logger.error('[Syn] The pane would not go back', e);
  }
}

export async function panePageForward(): Promise<void> {
  try {
    await invoke('syn_pane_forward');
  } catch (e) {
    logger.error('[Syn] The pane would not go forward', e);
  }
}

/**
 * What somebody typed in the address bar, as an address.
 *
 * A bare host becomes `https://`, because that is what everybody types and
 * refusing it would make the bar worse than every other address bar. Anything
 * with a space in it is left alone and will be refused by the backend's guard,
 * which is the right place for that answer — this is not a search box.
 */
export function typedAddress(raw: string): string {
  const text = raw.trim();
  if (!text) return '';
  return /^https?:\/\//i.test(text) ? text : `https://${text}`;
}

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
    await invoke<number>('syn_pane_open', { url });
  } catch (e) {
    logger.error('[Syn] The browsing pane would not open', e);
    paneShare.value = 0;
  }
}

/**
 * Move the pane's edge, live, while it is being dragged.
 *
 * # Why the pane moves with the pointer rather than after it
 *
 * There was a version that pushed the pane off the right edge for the length of
 * the pull, drew a line where the edge would land, and put it back on release.
 * It worked and it looked terrible: the column went white while the pane was
 * away, and flashed as it came back, every single time.
 *
 * That trick existed for a real reason — the pane moves to meet the pointer,
 * which puts the pointer *on the pane*, and this webview then gets no mouse
 * events at all. But live dragging did not fail because of the mechanism. It
 * failed because the floors were so tight that the pane never moved, so its
 * edge stood still while the pointer walked onto it. With floors that leave
 * room, the edge keeps up and the pointer stays on this side of it.
 *
 * Coalesced to one call a frame: a pointer produces far more events than a
 * webview can usefully be moved, and the rest buy nothing but IPC.
 */

/**
 * Settle the pane at the width the drag ended on.
 *
 * Once, on release, rather than per frame — with the pane hidden there is
 * nothing to move until then, and the preview line is drawn in CSS. The
 * clamping is Rust's: `pane::layout` holds the floors, and a copy of them here
 * would be a second opinion that drifts the first time one of them changed.
 */
let dragPending = false;
let dragWanted = 0;

export function dragPaneTo(share: number): void {
  dragWanted = share;
  if (dragPending) return;
  dragPending = true;

  requestAnimationFrame(async () => {
    dragPending = false;
    try {
      await invoke<number>('syn_pane_resize', { share: dragWanted });
    } catch (e) {
      logger.error('[Syn] The pane would not move', e);
    }
  });
}

/**
 * Whether following this link would take the app off its own pages.
 *
 * # Why this question has to be asked at all
 *
 * Because nothing was asking it. A link in one of Syn's answers renders as an
 * ordinary `<a href="https://…">`, and clicking one in a Tauri webview
 * navigates **that webview** — which is the app. The whole window becomes a
 * news site: no sidebar, no conversation, no back button, because the app's
 * chrome is the app and the app is gone. There is no way out except quitting.
 *
 * It is not only Syn's answers. A note's editor lets `synabit://` links through
 * and falls through on everything else, so a plain link typed into a note does
 * the same thing.
 *
 * Relative links and `#` anchors are the app navigating inside itself, which is
 * what a single-page app does all day. Only an absolute http address that
 * belongs to somebody else is a departure.
 */
export function leavesTheApp(href: string): boolean {
  if (!/^https?:\/\//i.test(href.trim())) return false;
  try {
    return new URL(href).origin !== window.location.origin;
  } catch {
    return false;
  }
}

/**
 * Open a page for the person — beside the conversation, or in their browser.
 *
 * The one door for following a link, wherever the link was: a source chip under
 * an answer, a link inside the answer, a link in a note. They look the same to
 * whoever clicks them, so they behave the same.
 *
 * The pane is the right home when there is one: a real browser with its own
 * session, visible, with a way back and a way out — which is exactly what was
 * missing when the app navigated itself away.
 *
 * **The fallback is not decided here.** A phone has no pane at all and a narrow
 * window has no room for one, and both of those end in the person's own
 * browser — but a *refused* address must not, and only Rust knows which of the
 * two happened. `syn_open_page` holds that rule; see its doc comment for why
 * splitting it would have quietly turned a refusal into an opening.
 *
 * What comes back is the share of the window the pane took, which is zero when
 * the page went to the browser instead.
 */
export async function openBeside(url: string): Promise<number> {
  try {
    return await invoke<number>('syn_open_page', { url });
  } catch (e) {
    logger.error('[Syn] Could not open that page', e);
    return 0;
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
