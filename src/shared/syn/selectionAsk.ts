import { ref, type InjectionKey, type Ref } from 'vue';

/**
 * Select some text, then ask Syn about it — the parts that are not a component.
 *
 * Cmd+J already asks about whatever is selected; see `focus.ts`. What it does
 * not do is say so. A shortcut is invisible until somebody tells you it exists,
 * and on a phone or a tablet there is no keyboard to press it on. So the
 * reading apps put a small button beside a selection, and the button opens the
 * same bar with the same focus the key would have.
 */

/**
 * The window event a mini-app raises to ask about some text.
 *
 * A window event rather than a prop or an emit, for the reason
 * `syn-ask-in-thread` is one: mini-apps are mounted generically by the router,
 * and the bar belongs to `App.vue`, above all of them. There is nothing to
 * thread a prop through.
 */
export const SYN_ASK_ABOUT = 'syn-ask-about-selection';

export interface AskAboutDetail {
  /** The text the person selected, exactly as it was read. */
  selection: string;
}

/**
 * Ask Syn about this text. Nothing happens unless `App.vue` is listening and
 * the bar is allowed — it applies the same locks to this as to the key.
 */
export function askSynAbout(selection: string): void {
  window.dispatchEvent(
    new CustomEvent<AskAboutDetail>(SYN_ASK_ABOUT, { detail: { selection } }),
  );
}

/**
 * Whether anything shaped like Syn may be offered right now.
 *
 * Provided by `App.vue`, which already decides this for Cmd+J: a vault is open,
 * Syn is switched on for it, and neither the app nor the mini-app on screen is
 * behind a PIN. Injected rather than recomputed, so that the button and the key
 * cannot disagree — a button that appeared when the key would have been
 * refused would open nothing, and be the worse of the two for looking like it
 * should work.
 *
 * Anything mounted outside `App.vue` — a test, a stray window — injects
 * nothing, and `synAskFallback` says no. Not offering Syn is the safe mistake.
 */
export interface SynAskContext {
  allowed: Readonly<Ref<boolean>>;
  /**
   * The key that does the same thing, as this platform spells it — `⌘J` or
   * `Ctrl+J` — or `null` where there is no keyboard to press it on. Shown in
   * the button's tooltip, which is how a button teaches the shortcut and
   * stops being needed.
   */
  shortcut: Readonly<Ref<string | null>>;
}

export const SYN_ASK: InjectionKey<SynAskContext> = Symbol('syn-ask');

export function synAskFallback(): SynAskContext {
  return { allowed: ref(false), shortcut: ref<string | null>(null) };
}

/** A rectangle on screen, in viewport pixels. `DOMRect` fits. */
export interface ScreenRect {
  top: number;
  bottom: number;
  left: number;
  right: number;
}

/**
 * Where the button goes, or `null` when the selection is not on screen.
 *
 * # Why this is arithmetic and not anchor positioning
 *
 * CSS anchor positioning would say this in two declarations, and it is not
 * Baseline at all — and a macOS user's WebView is whatever their macOS came
 * with, so "not yet" means "not for years" there. There is also nothing to anchor
 * to: a selection is a range, not an element. The Popover API would handle the
 * stacking, and it is Baseline *Newly* available (January 2025), which the
 * policy allows only where it degrades on its own; a popover that does not open
 * does not degrade to anything. So: a fixed-position button, placed from
 * `getBoundingClientRect()`, which every engine has had for as long as there
 * have been engines.
 *
 * # Above or below
 *
 * Above by default, the way every selection toolbar sits, so it does not cover
 * the lines that follow. On a touch screen, below: iOS and Android both put
 * their own Copy / Select All callout directly above a selection, and a button
 * in the same place would be drawn underneath it or on top of it. Either one is
 * flipped when there is no room, and the button is kept on screen sideways so
 * that a selection at the edge of a narrow window still gets one it can reach.
 */
export function placeNearSelection(
  rect: ScreenRect,
  viewport: { width: number; height: number },
  size: { width: number; height: number },
  prefer: 'above' | 'below' = 'above',
  gap = 8,
): { top: number; left: number } | null {
  // Scrolled out of sight. A button pointing at nothing is worse than none.
  if (rect.bottom < 0 || rect.top > viewport.height) return null;

  const above = rect.top - size.height - gap;
  const below = rect.bottom + gap;
  const fitsAbove = above >= gap;
  const fitsBelow = below + size.height <= viewport.height - gap;

  let top: number;
  if (prefer === 'above') top = fitsAbove || !fitsBelow ? above : below;
  else top = fitsBelow || !fitsAbove ? below : above;
  // Neither fits — a selection taller than the screen. Pin it inside the
  // window rather than off either edge.
  top = Math.min(Math.max(top, gap), Math.max(gap, viewport.height - size.height - gap));

  const centre = (rect.left + rect.right) / 2;
  const left = Math.min(
    Math.max(centre - size.width / 2, gap),
    Math.max(gap, viewport.width - size.width - gap),
  );

  return { top: Math.round(top), left: Math.round(left) };
}
