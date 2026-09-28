/**
 * How many app buttons fit in the desktop sidebar, and which ones move to
 * More Apps when they do not.
 *
 * # Why not just let the rail scroll
 *
 * It is the obvious fix and it breaks two things. `overflow-y: auto` forces
 * `overflow-x` to `auto` as well, and every button's tooltip — and the More
 * Apps menu itself — opens to the *right* of the rail, outside it. A scrolling
 * rail clips all of them. So the rail stays unclipped and short windows move
 * the apps that do not fit into More Apps, which already exists for apps the
 * user chose to hide.
 *
 * What it replaces was a rail with no limit at all: at 768px tall it was
 * 840px of buttons inside a shell that hides overflow, and the first input
 * that took focus scrolled the whole app up with no way back.
 */

/** One button (40px) and the gap after it (12px) — `w-10 h-10` and `gap-3`. */
export const RAIL_SLOT_PX = 52;

/** How many 40px buttons, 12px apart, fit in `heightPx`. */
export function railSlots(heightPx: number): number {
  // n buttons take n·40 + (n−1)·12 = n·52 − 12.
  return Math.max(0, Math.floor((heightPx + 12) / RAIL_SLOT_PX));
}

/**
 * The apps, in rail order, that do not fit in `slots` and go to More Apps.
 *
 * `moreShown` is whether the More button is there anyway, because the user
 * hid an app. When it is not, it only appears once something overflows — and
 * then it takes a slot of its own. At least one app always stays on the rail.
 */
export function railOverflow(ids: readonly string[], slots: number, moreShown: boolean): string[] {
  const room = moreShown ? slots - 1 : slots;
  if (ids.length <= room) return [];
  const keep = Math.max(1, slots - 1);
  return ids.slice(keep);
}
