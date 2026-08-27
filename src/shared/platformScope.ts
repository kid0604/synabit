/**
 * What ships on a phone.
 *
 * Synabit on the desktop is eleven mini-apps. The Android release is a
 * companion to that, not a copy of it: capture, read, search, and tick things
 * off. The rest — an infinite canvas, dense finance tables, a PDF annotator —
 * are built for a mouse and a wide screen, and shipping them on a phone would
 * mean carrying their touch handling, their translations and their
 * accessibility through every release for screens nobody would choose to use.
 *
 * Two things follow, and both matter:
 *
 * 1. This is **platform policy, not user preference**. `hiddenSidebarApps` in
 *    the app store is the user's own choice about their sidebar; reusing it
 *    here would let an app be switched back on and would fight with what they
 *    picked. The two are combined, never conflated.
 *
 * 2. The gate is the **operating system**, never the window size. A desktop
 *    window dragged narrow switches to the mobile *layout* — that is
 *    `useMobileLayout` — and it must not make Finance disappear. Anything in
 *    this file keys off the real OS.
 */

import { ref } from 'vue';
import { type as osType } from '@tauri-apps/plugin-os';
import { logger } from '../utils/logger';

/**
 * The mini-apps a phone gets, in the order they appear.
 *
 * Four, so all of them fit along the bottom bar and nothing is pushed into a
 * "More" menu that hides half the product.
 *
 * Calendar and Feeds are the candidates for the release after this one — both
 * read well on a phone. They are absent because adding an app is not flipping a
 * flag here: it is another full pass of touch targets, accessibility and
 * low-end device testing.
 */
export const MOBILE_APPS = ['nexus', 'quickcap', 'note', 'task'] as const;

/**
 * The task views a phone gets.
 *
 * Board and Matrix move cards with the HTML5 drag-and-drop API, which does not
 * fire from touch events — on a phone they render fine and simply cannot be
 * used. Table is a wide grid. List is the one that works, and it is the one a
 * phone is for anyway: see what is due, tick it off.
 */
export const MOBILE_TASK_VIEWS = ['list'] as const;

/**
 * Whether this is a phone or tablet, as opposed to a small desktop window.
 *
 * `type()` from the OS plugin is synchronous in Tauri 2 — the value is resolved
 * when the plugin loads — so this needs no waiting and no loading state, and a
 * router guard can consult it directly.
 *
 * A `ref` rather than a plain boolean so templates re-render if it is ever
 * established later than first paint.
 */
export const isMobileOS = ref(detectMobileOS());

function detectMobileOS(): boolean {
  try {
    return ['android', 'ios'].includes(osType().toLowerCase());
  } catch (e) {
    // Running outside Tauri, or the plugin is unavailable. Desktop is the safe
    // assumption: it shows everything, and a missing app is a worse failure
    // than an app that is awkward on a small screen.
    logger.warn('Could not determine the platform; assuming desktop', e);
    return false;
  }
}

/** Is this mini-app part of the release for the current platform? */
export function appInPlatformScope(appId: string): boolean {
  if (!isMobileOS.value) return true;
  return (MOBILE_APPS as readonly string[]).includes(appId);
}

/** Is this task view available on the current platform? */
export function taskViewInPlatformScope(view: string): boolean {
  if (!isMobileOS.value) return true;
  return (MOBILE_TASK_VIEWS as readonly string[]).includes(view);
}
