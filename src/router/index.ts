import { createRouter, createWebHashHistory, RouteRecordRaw } from 'vue-router';

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
const ChatApp = () => import('../mini-apps/chat/ChatApp.vue');

const routes: Array<RouteRecordRaw> = [
  { path: '/', redirect: '/nexus' },
  { path: '/nexus', name: 'nexus', component: Nexus },
  { path: '/chat', name: 'chat', component: ChatApp },
  { path: '/note', name: 'note', component: NoteApp },
  { path: '/quickcap', name: 'quickcap', component: QuickCap },
  { path: '/task', name: 'task', component: Tasks },
  { path: '/calendar', name: 'calendar', component: CalendarApp },
  { path: '/file', name: 'file', component: FilesApp },
  { path: '/whiteboard', name: 'whiteboard', component: WhiteboardApp },
  { path: '/people', name: 'people', component: PeopleApp },
  { path: '/finance', name: 'finance', component: FinanceApp },
];

const router = createRouter({
  // Using hash history because Tauri apps run from index.html on file:// or custom protocol
  // and history mode might face issues with deep linking / page reloads
  history: createWebHashHistory(),
  routes,
});

export default router;
