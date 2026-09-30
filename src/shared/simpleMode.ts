/**
 * Simple mode: Synabit for someone who wants a notebook, a to-do list and a
 * calendar, and nothing that needs explaining.
 *
 * It hides; it never deletes or converts. Everything a user made in a hidden
 * app stays in the folder, and turning the mode off brings the app back as it
 * was. The same holds inside apps that stay: power tools (query blocks, graph
 * views, syntax hints) are left out of menus, and a note that already holds
 * one still shows it.
 */

import { ref } from 'vue';

/** The apps simple mode keeps in the sidebar, in this order. */
export const SIMPLE_MODE_APPS = ['quickcap', 'note', 'task', 'calendar', 'people', 'finance'] as const;

/** Where simple mode opens when the chosen start app is one it hides. */
export const SIMPLE_MODE_HOME = 'note';

export function appVisibleInSimpleMode(appId: string): boolean {
  return (SIMPLE_MODE_APPS as readonly string[]).includes(appId);
}

/**
 * One hidden app, let through once because the user asked to open it anyway.
 *
 * A link in a note can point at a whiteboard or a file, and simple mode hides
 * the apps those live in. Refusing silently was the old behaviour — the click
 * did nothing — so the shell now says why and offers to open it regardless.
 * This is that "regardless": the router lets this one app through until the
 * user navigates somewhere else, and then it is closed again. Nothing is
 * stored; simple mode itself is unchanged.
 */
export const simpleModePass = ref<string | null>(null);

/**
 * Which words the first-run mode question uses on this platform.
 *
 * The desktop copy names calendar, people and money, which a phone does not
 * ship — promising them there is promising something that is not in the box.
 * The suffix picks the phone's wording when the calendar is out of scope,
 * which is the one test that tells the two releases apart.
 */
export function modeChoiceCopy(inScope: (appId: string) => boolean): '' | '_phone' {
  return inScope('calendar') ? '' : '_phone';
}
