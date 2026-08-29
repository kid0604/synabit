import { createRouter, createWebHashHistory, RouteRecordRaw } from 'vue-router';
import { appInPlatformScope } from '../shared/platformScope';
import { BUILT_IN_APPS } from '../shared/appRegistry';

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
 * Keep the platform's scope closed.
 *
 * Hiding an app from the navigation is not the same as it being absent. Routes
 * stay reachable by deep link, by a restored session, and by `defaultApp` when
 * a user who set Finance as their landing screen on the desktop opens the app
 * on their phone. Any of those would drop them into a screen built for a mouse
 * with no way back that makes sense.
 *
 * Redirecting rather than refusing: the destination does not exist here, so the
 * honest answer is the one screen that always does.
 */
router.beforeEach((to) => {
  const name = to.name as string | undefined;
  if (name && !appInPlatformScope(name)) {
    return { name: 'nexus' };
  }
  return true;
});

export default router;
