/**
 * Which apps the shell offers, where a route that is not offered goes, and
 * which app Synabit opens on.
 *
 * Two policies narrow the list of apps, and one preference arranges what is
 * left:
 *
 * - **Platform scope** (`platformScope.ts`) — what this build ships at all. A
 *   phone does not have Finance.
 * - **Simple mode** (`simpleMode.ts`) — what the user asked to see. It hides
 *   and never deletes, so it works exactly like platform scope from the
 *   shell's point of view: the app is not in the sidebar, not in More Apps,
 *   and not reachable by route. Turning it off brings everything back as it was.
 * - **`hiddenSidebarApps`** — the user's own sidebar arrangement. An app hidden
 *   there is still *offered*: it sits in More Apps and its route works.
 *
 * These are pure functions of their inputs, with the platform check passed
 * in, so the rules can be tested without a Tauri runtime and read in one place
 * instead of being re-derived in App.vue, the mobile layout, the router and
 * Settings — four copies that would drift the way the app list once did (see
 * `appRegistry.ts`).
 */

import { appInPlatformScope } from './platformScope';
import { SIMPLE_MODE_HOME, appVisibleInSimpleMode } from './simpleMode';

type InScope = (appId: string) => boolean;

/** The screen that always exists when simple mode is off. */
export const FULL_MODE_HOME = 'nexus';

/**
 * Whether the shell offers this app at all: in the sidebar or More Apps, and
 * reachable by route.
 */
export function appOffered(appId: string, simpleMode: boolean, inScope: InScope = appInPlatformScope): boolean {
  if (!inScope(appId)) return false;
  if (simpleMode && !appVisibleInSimpleMode(appId)) return false;
  return true;
}

/**
 * Where a navigation to `routeName` should go instead, or `null` to let it
 * through.
 *
 * Routes that are not apps (a redirect, a future non-app screen) pass: this
 * only closes doors, it does not invent them. The destination is the one
 * screen that always exists in the current mode — Nexus normally, Notes in
 * simple mode, where Nexus is itself hidden. Both are in the phone's scope.
 */
export function redirectTarget(
  routeName: string | undefined,
  simpleMode: boolean,
  isApp: (name: string) => boolean,
  inScope: InScope = appInPlatformScope,
  /**
   * An app the user chose to open although simple mode hides it (see
   * `simpleModePass`). It only lifts simple mode: an app this platform does
   * not ship stays closed whatever was asked.
   */
  passed: string | null = null,
): string | null {
  if (!routeName || !isApp(routeName)) return null;
  if (appOffered(routeName, simpleMode, inScope)) return null;
  if (passed === routeName && appOffered(routeName, false, inScope)) return null;
  return simpleMode ? SIMPLE_MODE_HOME : FULL_MODE_HOME;
}

/**
 * The apps that get a button of their own, in order: offered, and not taken
 * off the sidebar by the user. The phone's bottom bar and its swipe order are
 * this list (the bar then takes the first four).
 */
export function sidebarApps(
  appIds: readonly string[],
  hiddenSidebarApps: readonly string[],
  simpleMode: boolean,
  inScope: InScope = appInPlatformScope,
): string[] {
  return appIds.filter((id) => appOffered(id, simpleMode, inScope) && !hiddenSidebarApps.includes(id));
}

export interface StartAppInput {
  /** The stored `defaultApp`, or its default when nothing was ever stored. */
  defaultApp: string;
  /** Whether the user ever picked a start app in Settings. */
  defaultAppChosen: boolean;
  simpleMode: boolean;
  /**
   * Whether the vault holds no notes and no tasks. `null` while that is not
   * known yet, which counts as "not empty": the rule below may only ever move a
   * new user, never somebody whose vault simply has not been read yet.
   */
  vaultEmpty: boolean | null;
}

/**
 * The app Synabit opens on.
 *
 * 1. **An empty vault opens QuickCap** — but only for somebody who never chose
 *    a start app. A new user landing on Nexus sees an empty graph and nothing
 *    that says "write here"; QuickCap is one keystroke from a first note. The
 *    moment the user picks a start app in Settings, that choice wins for good,
 *    empty vault or not, because respecting a setting only sometimes is worse
 *    than not having it.
 * 2. **The chosen app**, when the shell offers it.
 * 3. **Otherwise the mode's home**: the chosen app is hidden by simple mode or
 *    not shipped on this platform. The stored choice is left alone, so turning
 *    simple mode off opens it again.
 */
export function startApp(input: StartAppInput, inScope: InScope = appInPlatformScope): string {
  const { defaultApp, defaultAppChosen, simpleMode, vaultEmpty } = input;
  if (!defaultAppChosen && vaultEmpty === true && appOffered('quickcap', simpleMode, inScope)) {
    return 'quickcap';
  }
  if (appOffered(defaultApp, simpleMode, inScope)) return defaultApp;
  return simpleMode ? SIMPLE_MODE_HOME : FULL_MODE_HOME;
}

/**
 * The app a link of this type opens in — the `type` that `open-node` carries,
 * which is a handler name more than a node type (see `nodeRoutes.ts`).
 * `undefined` when the shell has no handler for it.
 */
const APP_FOR_OPEN_TYPE: Readonly<Record<string, string>> = {
  note: 'note',
  moment: 'note',
  quickcap: 'quickcap',
  task: 'task',
  project: 'task',
  calendar: 'calendar',
  whiteboard: 'whiteboard',
  messages: 'messages',
  syn_memory: 'messages',
  syn_skill: 'messages',
  safe: 'safe',
  person: 'people',
  finance_month: 'finance',
  feed_source: 'feeds',
  pdf: 'file',
  pdf_highlight: 'file',
  file: 'file',
};

export function appForOpenType(type: string): string | undefined {
  return APP_FOR_OPEN_TYPE[type];
}

/**
 * Whether a link into `appId` is blocked by simple mode alone — the case where
 * saying so, and offering a way through, is worth doing. An app the platform
 * does not ship is not this: there is nothing to open anyway.
 */
export function hiddenBySimpleMode(appId: string, simpleMode: boolean, inScope: InScope = appInPlatformScope): boolean {
  return simpleMode && !appOffered(appId, true, inScope) && appOffered(appId, false, inScope);
}
