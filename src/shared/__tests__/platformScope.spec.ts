import { describe, it, expect, afterEach } from 'vitest';
import {
  MOBILE_APPS,
  MOBILE_TASK_VIEWS,
  isMobileOS,
  appInPlatformScope,
  taskViewInPlatformScope,
} from '../platformScope';
import { BUILT_IN_APPS } from '../appRegistry';

/**
 * What ships on a phone is a product decision, and the kind that drifts
 * silently: somebody adds a mini-app, nobody thinks about the phone, and it
 * appears there untested. These pin the decision so changing it has to be
 * deliberate.
 */

/**
 * Every mini-app the desktop has.
 *
 * Read from the registry rather than copied out of it. This used to be a
 * hand-written list "mirroring ALL_APPS in App.vue", which is a mirror that
 * nothing keeps clean: a twelfth app would have been added to the product and
 * not to this list, and the phone assertions below would have gone on passing
 * while saying nothing about it.
 */
const DESKTOP_APPS = BUILT_IN_APPS.map((a) => a.id);

describe('platform scope', () => {
  afterEach(() => {
    isMobileOS.value = false;
  });

  it('ships everything on the desktop', () => {
    isMobileOS.value = false;
    for (const app of DESKTOP_APPS) {
      expect(appInPlatformScope(app)).toBe(true);
    }
    for (const view of ['list', 'board', 'table', 'matrix']) {
      expect(taskViewInPlatformScope(view)).toBe(true);
    }
  });

  it('ships only the companion set on a phone', () => {
    isMobileOS.value = true;
    const shipped = DESKTOP_APPS.filter(appInPlatformScope);
    expect(shipped.sort()).toEqual([...MOBILE_APPS].sort());
  });

  /**
   * Named individually rather than compared against the constant, so widening
   * the list cannot make this test agree with itself.
   */
  it('keeps the mouse-designed apps off the phone', () => {
    isMobileOS.value = true;
    for (const app of ['whiteboard', 'finance', 'file', 'people', 'messages', 'calendar', 'feeds']) {
      expect(appInPlatformScope(app), `${app} should not ship on mobile`).toBe(false);
    }
  });

  it('keeps capture, reading, search and tasks on the phone', () => {
    isMobileOS.value = true;
    for (const app of ['quickcap', 'note', 'nexus', 'task']) {
      expect(appInPlatformScope(app), `${app} should ship on mobile`).toBe(true);
    }
  });

  /**
   * Board and Matrix move cards with HTML5 drag-and-drop, which touch never
   * fires. If either is allowed back on mobile it must be because the dragging
   * was rewritten, not because this list was edited.
   */
  it('offers only the list view on a phone', () => {
    isMobileOS.value = true;
    expect(taskViewInPlatformScope('list')).toBe(true);
    for (const view of ['board', 'matrix', 'table']) {
      expect(taskViewInPlatformScope(view), `${view} should not ship on mobile`).toBe(false);
    }
    expect([...MOBILE_TASK_VIEWS]).toEqual(['list']);
  });

  /** Four, so all of them fit along the bottom bar with no More menu. */
  it('keeps the phone to four apps', () => {
    expect(MOBILE_APPS.length).toBe(4);
  });

  it('treats an unknown app as out of scope on a phone', () => {
    isMobileOS.value = true;
    expect(appInPlatformScope('something-new')).toBe(false);
  });
});
