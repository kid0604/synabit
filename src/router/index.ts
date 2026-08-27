import { createRouter, createWebHashHistory, RouteRecordRaw } from 'vue-router';
import { appInPlatformScope } from '../shared/platformScope';

// Mini App Components — lazy loaded for code splitting
const NoteApp = () => import('../mini-apps/note/NoteApp.vue');
const QuickCap = () => import('../mini-apps/quickcap/QuickCapApp.vue');
const Tasks = () => import('../mini-apps/task/TaskApp.vue');
const CalendarApp = () => import('../mini-apps/calendar/CalendarApp.vue');
const Nexus = () => import('../mini-apps/nexus/NexusApp.vue');
const FilesApp = () => import('../mini-apps/files/FilesApp.vue');
const WhiteboardApp = () => import('../mini-apps/whiteboard/WhiteboardApp.vue');
const PeopleApp = () => import('../mini-apps/people/PeopleApp.vue');
const FinanceApp = () => import('../mini-apps/finance/FinanceApp.vue');
const FeedsApp = () => import('../mini-apps/feeds/FeedsApp.vue');
const MessagesApp = () => import('../mini-apps/messages/MessagesApp.vue');

const routes: Array<RouteRecordRaw> = [
  { path: '/', redirect: '/nexus' },
  { path: '/nexus', name: 'nexus', component: Nexus },
  { path: '/messages', name: 'messages', component: MessagesApp },
  { path: '/chat', redirect: '/messages' },
  { path: '/note', name: 'note', component: NoteApp },
  { path: '/quickcap', name: 'quickcap', component: QuickCap },
  { path: '/task', name: 'task', component: Tasks },
  { path: '/calendar', name: 'calendar', component: CalendarApp },
  { path: '/file', name: 'file', component: FilesApp },
  { path: '/whiteboard', name: 'whiteboard', component: WhiteboardApp },
  { path: '/people', name: 'people', component: PeopleApp },
  { path: '/finance', name: 'finance', component: FinanceApp },
  { path: '/feeds', name: 'feeds', component: FeedsApp },
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
