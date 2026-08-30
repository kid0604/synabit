/**
 * Which mini-apps exist, in the order they are offered.
 *
 * This list used to exist three times: `ALL_APPS` in `App.vue` drove the More
 * menu, `ALL_SETTABLE_APPS` in `SettingsModal.vue` drove the hide-and-lock
 * checkboxes, and `routes` in `router/index.ts` decided what was reachable.
 * They had already drifted — Messages carried `MessageCircle` in the sidebar
 * and the More menu but `MessageSquare` in Settings, so the same app wore two
 * icons depending on which screen you found it on.
 *
 * Three copies of "what apps exist" is one copy too many even when they agree,
 * because the next question after this one is whether the list can *grow*: a
 * saved filter pinned to the sidebar is an entry that no `.vue` file declares.
 * A single array is what makes that answerable later; keeping the icons and
 * names honest is what it buys today.
 *
 * What this file is not: the sidebar itself. Those buttons stay hand-written in
 * `App.vue` because each carries its own badge — unread messages, waiting
 * caps, unread articles — and a generated button would have to grow a slot for
 * every one of them. They read their visibility from here, not their markup.
 */

import type { Component } from 'vue';
import {
  Globe,
  MessageCircle,
  Zap,
  FileText,
  CheckSquare,
  Calendar,
  FolderOpen,
  Palette,
  Users,
  Wallet,
  Rss,
  Boxes,
} from 'lucide-vue-next';

export interface AppEntry {
  /**
   * The app's id, which is also its route name and its route path.
   *
   * The same string is what `hiddenSidebarApps`, `protectedApps` and
   * `defaultApp` store, and what `appInPlatformScope` is asked about. Changing
   * one would strand settings already written to a user's disk.
   */
  id: string;
  /** The English name. Sidebar tooltips and Settings labels use it. */
  name: string;
  /** Lucide icon component, rendered with `<component :is>`. */
  icon: Component;
  /** Lazy loader for the app's root component, so each app is its own chunk. */
  view: () => Promise<unknown>;
}

/**
 * The apps this build ships, in sidebar order.
 *
 * Order is load-bearing on a phone: `MOBILE_APPS` says which four a phone gets,
 * and the bottom bar takes the first four that survive the platform filter in
 * this order.
 */
export const BUILT_IN_APPS: readonly AppEntry[] = [
  { id: 'nexus',      name: 'Nexus',      icon: Globe,         view: () => import('../mini-apps/nexus/NexusApp.vue') },
  { id: 'messages',   name: 'Messages',   icon: MessageCircle, view: () => import('../mini-apps/messages/MessagesApp.vue') },
  { id: 'quickcap',   name: 'QuickCap',   icon: Zap,           view: () => import('../mini-apps/quickcap/QuickCapApp.vue') },
  { id: 'note',       name: 'Notes',      icon: FileText,      view: () => import('../mini-apps/note/NoteApp.vue') },
  { id: 'task',       name: 'Tasks',      icon: CheckSquare,   view: () => import('../mini-apps/task/TaskApp.vue') },
  { id: 'calendar',   name: 'Calendar',   icon: Calendar,      view: () => import('../mini-apps/calendar/CalendarApp.vue') },
  { id: 'file',       name: 'Files',      icon: FolderOpen,    view: () => import('../mini-apps/files/FilesApp.vue') },
  { id: 'whiteboard', name: 'Whiteboard', icon: Palette,       view: () => import('../mini-apps/whiteboard/WhiteboardApp.vue') },
  { id: 'people',     name: 'People',     icon: Users,         view: () => import('../mini-apps/people/PeopleApp.vue') },
  { id: 'finance',    name: 'Finance',    icon: Wallet,        view: () => import('../mini-apps/finance/FinanceApp.vue') },
  { id: 'feeds',      name: 'Feeds',      icon: Rss,           view: () => import('../mini-apps/feeds/FeedsApp.vue') },
  { id: 'things',     name: 'Things',     icon: Boxes,         view: () => import('../mini-apps/things/ThingsApp.vue') },
];

/** The entry for an app id, or `undefined` for a name nothing ships. */
export function appById(appId: string): AppEntry | undefined {
  return BUILT_IN_APPS.find((a) => a.id === appId);
}

/**
 * The display name for an app id, falling back to the id itself.
 *
 * The fallback is deliberate: this is called with `route.name`, which can be a
 * route that is not an app at all, and showing the raw name beats showing
 * nothing in a sentence like "Enter PIN to access …".
 */
export function appName(appId: string): string {
  return appById(appId)?.name ?? appId;
}
