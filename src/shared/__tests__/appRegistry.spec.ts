import { describe, it, expect } from 'vitest';
import { BUILT_IN_APPS, appById, appName } from '../appRegistry';
import router from '../../router';
// The sidebar's markup as text. `?raw` rather than `node:fs` because this
// project carries no `@types/node`, and adding it to read one file would put
// Node's globals in scope for every browser module in `src/`.
import appVueSource from '../../App.vue?raw';

/**
 * The registry exists to stop three lists disagreeing. A test is what keeps it
 * doing that.
 *
 * The failure it guards against is not hypothetical: Messages carried
 * `MessageCircle` in the sidebar and `MessageSquare` in Settings, because the
 * two lists were written months apart. That kind of drift is invisible in
 * review — both files look right on their own.
 */
describe('the app registry', () => {
  it('gives every app a route under its own id', () => {
    const routed = new Set(
      router.getRoutes()
        .map((r) => r.name)
        .filter((n): n is string => typeof n === 'string'),
    );

    for (const app of BUILT_IN_APPS) {
      expect(routed.has(app.id), `${app.id} has no route`).toBe(true);
      const route = router.getRoutes().find((r) => r.name === app.id);
      expect(route?.path, `${app.id} is not served at /${app.id}`).toBe(`/${app.id}`);
    }
  });

  /**
   * The other direction, and the one that actually broke things: a named route
   * for an app nobody ships is reachable by deep link and by a restored
   * session, and lands the user on a screen with no way back to it.
   *
   * Redirects are exempt because they have no name — `/chat` and `/syn` are
   * old addresses kept alive, not apps.
   */
  it('has no named route that is not an app', () => {
    const named = router.getRoutes()
      .map((r) => r.name)
      .filter((n): n is string => typeof n === 'string');

    for (const name of named) {
      expect(appById(name), `route "${name}" is not in the registry`).toBeDefined();
    }
  });

  it('keeps ids unique', () => {
    const ids = BUILT_IN_APPS.map((a) => a.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  /**
   * Ids are written to disk — `hiddenSidebarApps`, `protectedApps` and
   * `defaultApp` all store them — so renaming one silently discards a setting
   * the user made. Spelled out rather than derived, so a rename has to come
   * here and be argued for.
   */
  it('keeps the ids that settings on disk refer to', () => {
    expect(BUILT_IN_APPS.map((a) => a.id)).toEqual([
      'nexus', 'messages', 'quickcap', 'note', 'task',
      'calendar', 'file', 'whiteboard', 'people', 'finance', 'feeds', 'things',
    ]);
  });

  it('gives every app a name and an icon', () => {
    for (const app of BUILT_IN_APPS) {
      expect(app.name.length, `${app.id} has no name`).toBeGreaterThan(0);
      expect(app.icon, `${app.id} has no icon`).toBeTruthy();
    }
  });

  /**
   * `appName` is called with `route.name`, which is not always an app — the PIN
   * prompt asks it for whatever screen is being unlocked. Returning the raw id
   * keeps that sentence readable instead of leaving a hole in it.
   */
  it('falls back to the id for a name it does not ship', () => {
    expect(appName('task')).toBe('Tasks');
    expect(appName('something-new')).toBe('something-new');
    expect(appById('something-new')).toBeUndefined();
  });

  /**
   * The sidebar is the one consumer that did not become derived.
   *
   * Its buttons stay hand-written because each carries a different badge —
   * unread messages, waiting caps, unread articles — so a generated button
   * would need a slot for every one of them. The cost of keeping them by hand
   * is that a twelfth app can be added to the registry, get a route, get a
   * Settings checkbox, and still have nowhere to click.
   *
   * Reading the source is blunt, but it is the only check that reaches markup
   * without mounting `App.vue` and every Tauri plugin it pulls in.
   */
  it('gives every app a sidebar button', () => {
    for (const app of BUILT_IN_APPS) {
      expect(
        appVueSource.includes(`isAppVisible('${app.id}')`),
        `${app.id} is in the registry but has no sidebar button in App.vue`,
      ).toBe(true);
    }
  });
});
