import { describe, it, expect } from 'vitest';
import { appOffered, redirectTarget, sidebarApps, startApp, FULL_MODE_HOME, appForOpenType, hiddenBySimpleMode } from '../appAccess';
import { SIMPLE_MODE_APPS, SIMPLE_MODE_HOME, modeChoiceCopy } from '../simpleMode';
import { MOBILE_APPS } from '../platformScope';
import { BUILT_IN_APPS, appById } from '../appRegistry';

/**
 * The shell's three questions — offered, where to instead, where to start —
 * answered for desktop and phone with the platform check passed in, so these
 * do not depend on the OS the tests run on.
 */
const desktop = () => true;
const phone = (id: string) => (MOBILE_APPS as readonly string[]).includes(id);
const isApp = (name: string) => appById(name) !== undefined;
const ALL = BUILT_IN_APPS.map((a) => a.id);

describe('appOffered', () => {
  it('offers every app on the desktop when simple mode is off', () => {
    expect(ALL.every((id) => appOffered(id, false, desktop))).toBe(true);
  });

  it('offers exactly the simple-mode apps when it is on', () => {
    expect(ALL.filter((id) => appOffered(id, true, desktop))).toEqual([...SIMPLE_MODE_APPS]);
  });

  it('never offers what the platform does not ship, in either mode', () => {
    expect(appOffered('finance', false, phone)).toBe(false);
    expect(appOffered('finance', true, phone)).toBe(false);
  });
});

describe('redirectTarget (the router guard)', () => {
  it('lets offered apps through', () => {
    expect(redirectTarget('whiteboard', false, isApp, desktop)).toBeNull();
    expect(redirectTarget('note', true, isApp, desktop)).toBeNull();
  });

  it('sends apps simple mode hides to its home', () => {
    for (const id of ['nexus', 'messages', 'whiteboard', 'feeds', 'things', 'safe', 'file']) {
      expect(redirectTarget(id, true, isApp, desktop), id).toBe(SIMPLE_MODE_HOME);
    }
  });

  it('keeps the platform redirect to Nexus when simple mode is off', () => {
    expect(redirectTarget('finance', false, isApp, phone)).toBe(FULL_MODE_HOME);
  });

  it('sends out-of-scope apps to Notes, not Nexus, in simple mode on a phone', () => {
    expect(redirectTarget('finance', true, isApp, phone)).toBe(SIMPLE_MODE_HOME);
    expect(redirectTarget('nexus', true, isApp, phone)).toBe(SIMPLE_MODE_HOME);
  });

  it('leaves routes that are not apps alone', () => {
    expect(redirectTarget(undefined, true, isApp, desktop)).toBeNull();
    expect(redirectTarget('quick-entry', true, isApp, desktop)).toBeNull();
  });

  it('always redirects somewhere that is itself let through', () => {
    for (const inScope of [desktop, phone]) {
      for (const simple of [false, true]) {
        for (const id of ALL) {
          const target = redirectTarget(id, simple, isApp, inScope);
          if (target) expect(redirectTarget(target, simple, isApp, inScope)).toBeNull();
        }
      }
    }
  });
});

describe('sidebarApps', () => {
  it('drops apps the user hid and keeps registry order', () => {
    expect(sidebarApps(ALL, ['task', 'feeds'], false, desktop)).toEqual(ALL.filter((id) => id !== 'task' && id !== 'feeds'));
  });

  it('shows only simple-mode apps, still respecting the user’s own hiding', () => {
    expect(sidebarApps(ALL, ['people'], true, desktop)).toEqual(['quickcap', 'note', 'task', 'calendar', 'finance']);
  });

  it('combines simple mode with the phone’s scope', () => {
    expect(sidebarApps(ALL, [], true, phone)).toEqual(['quickcap', 'note', 'task']);
  });
});

describe('startApp', () => {
  const base = { defaultApp: 'nexus', defaultAppChosen: false, simpleMode: false, vaultEmpty: null };

  it('opens the default app while emptiness is unknown', () => {
    expect(startApp(base, desktop)).toBe('nexus');
  });

  it('opens QuickCap for a new user with an empty vault', () => {
    expect(startApp({ ...base, vaultEmpty: true }, desktop)).toBe('quickcap');
    expect(startApp({ ...base, vaultEmpty: true, simpleMode: true }, desktop)).toBe('quickcap');
  });

  it('does not move somebody whose vault has something in it', () => {
    expect(startApp({ ...base, vaultEmpty: false }, desktop)).toBe('nexus');
  });

  it('always respects a start app chosen in Settings, even on an empty vault', () => {
    expect(startApp({ ...base, defaultApp: 'nexus', defaultAppChosen: true, vaultEmpty: true }, desktop)).toBe('nexus');
    expect(startApp({ ...base, defaultApp: 'task', defaultAppChosen: true, vaultEmpty: true }, desktop)).toBe('task');
  });

  it('falls back to simple mode’s home when the chosen app is hidden', () => {
    expect(startApp({ ...base, defaultAppChosen: true, simpleMode: true }, desktop)).toBe(SIMPLE_MODE_HOME);
    expect(startApp({ ...base, defaultApp: 'file', defaultAppChosen: true, simpleMode: true }, desktop)).toBe(SIMPLE_MODE_HOME);
    expect(startApp({ ...base, defaultApp: 'calendar', defaultAppChosen: true, simpleMode: true }, desktop)).toBe('calendar');
  });

  it('falls back to Nexus when the platform does not ship the chosen app', () => {
    expect(startApp({ ...base, defaultApp: 'calendar', defaultAppChosen: true }, phone)).toBe('nexus');
  });
});

describe('"open it anyway" in simple mode', () => {
  it('lets the one passed app through, and only that one', () => {
    expect(redirectTarget('whiteboard', true, isApp, desktop, 'whiteboard')).toBeNull();
    expect(redirectTarget('file', true, isApp, desktop, 'whiteboard')).toBe(SIMPLE_MODE_HOME);
  });

  it('never opens an app the platform does not ship', () => {
    expect(redirectTarget('whiteboard', true, isApp, phone, 'whiteboard')).toBe(SIMPLE_MODE_HOME);
  });

  it('knows which links simple mode alone is blocking', () => {
    expect(hiddenBySimpleMode('whiteboard', true, desktop)).toBe(true);
    expect(hiddenBySimpleMode('note', true, desktop)).toBe(false);
    expect(hiddenBySimpleMode('whiteboard', false, desktop)).toBe(false);
    // Not shipped on a phone: nothing to open anyway, so nothing to offer.
    expect(hiddenBySimpleMode('whiteboard', true, phone)).toBe(false);
  });

  it('maps every link type to a real app', () => {
    for (const type of ['note', 'moment', 'whiteboard', 'safe', 'syn_memory', 'pdf_highlight', 'person', 'finance_month', 'feed_source', 'project']) {
      expect(isApp(appForOpenType(type)!), type).toBe(true);
    }
    expect(appForOpenType('nonsense')).toBeUndefined();
  });
});

describe('modeChoiceCopy', () => {
  it('uses the phone wording where the calendar is not shipped', () => {
    expect(modeChoiceCopy(desktop)).toBe('');
    expect(modeChoiceCopy(phone)).toBe('_phone');
  });
});
