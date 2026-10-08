import { createRouter, createWebHashHistory, RouteRecordRaw } from 'vue-router';
import { BUILT_IN_APPS, appById } from '../shared/appRegistry';
import { redirectTarget } from '../shared/appAccess';
import { useAppStore } from '../stores/useAppStore';
import { simpleModePass } from '../shared/simpleMode';
import { noteNavigation } from '../composables/useBackGuard';

/**
 * One route per mini-app, generated from the registry.
 *
 * An app's id is its route name and its path, so the registry is enough to
 * build these: a route named `task` at `/task`. Written out by hand, this list
 * was a third copy of "what apps exist" and could fall out of step with the two
 * in the UI — an app could be offered in Settings and be unreachable, or be
 * routable and invisible.
 *
 * Components stay lazily loaded; the registry holds the same `() => import()`
 * this file used to declare, so each app is still its own chunk.
 */
const appRoutes: Array<RouteRecordRaw> = BUILT_IN_APPS.map((app) => ({
  path: `/${app.id}`,
  name: app.id,
  component: app.view,
}));

const routes: Array<RouteRecordRaw> = [
  { path: '/', redirect: '/nexus' },
  ...appRoutes,
  // Two names that outlived their screens. Kept as redirects because they are
  // in users' restored sessions and in deep links already sent.
  { path: '/chat', redirect: '/messages' },
  { path: '/syn', redirect: '/messages' },
];

const router = createRouter({
  // Using hash history because Tauri apps run from index.html on file:// or custom protocol
  // and history mode might face issues with deep linking / page reloads
  history: createWebHashHistory(),
  routes,
});

/**
 * Keep the platform's scope closed, and simple mode's.
 *
 * Hiding an app from the navigation is not the same as it being absent. Routes
 * stay reachable by deep link, by a restored session, and by `defaultApp` when
 * a user who set Finance as their landing screen on the desktop opens the app
 * on their phone. Any of those would drop them into a screen built for a mouse
 * with no way back that makes sense.
 *
 * Simple mode closes the same door for the same reason: a link into Whiteboard
 * would open an app whose sidebar button is gone, with no way to find it again.
 * Its home is Notes rather than Nexus, because Nexus is one of the apps it
 * hides. The rule itself lives in `shared/appAccess.ts`.
 *
 * Redirecting rather than refusing: the destination does not exist here, so the
 * honest answer is the one screen that always does.
 *
 * The store is read inside the guard, not at module load: Pinia is installed
 * before the router in `main.ts`, but this file is imported before either.
 */
router.beforeEach((to) => {
  // "Open it anyway" lasts until the user goes somewhere else.
  if (simpleModePass.value && to.name !== simpleModePass.value) simpleModePass.value = null;
  const target = redirectTarget(
    to.name as string | undefined,
    useAppStore().simpleMode,
    (name) => appById(name) !== undefined,
    undefined,
    simpleModePass.value,
  );
  return target ? { name: target } : true;
});

/**
 * Tell the back guard when a navigation is under way, so a dialog that closes
 * and opens another app on one click does not spend a `history.back()` that
 * would undo the navigation. See `noteNavigation` in useBackGuard.
 *
 * Registered after the guard above so it only hears navigations that guard
 * let through; `afterEach` runs for a cancelled or redirected one too, and
 * `onError` for one that threw, so the flag cannot stick.
 */
router.beforeEach(() => {
  noteNavigation(true);
});
router.afterEach(() => noteNavigation(false));
router.onError(() => noteNavigation(false));

export default router;
