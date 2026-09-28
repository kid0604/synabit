import { describe, it, expect } from 'vitest';
import { railOverflow, railSlots } from '../railFit';

const apps = ['nexus', 'messages', 'quickcap', 'note', 'task', 'calendar', 'file', 'whiteboard', 'people', 'finance', 'feeds', 'things', 'safe'];

describe('the sidebar rail', () => {
  it('counts 40px buttons with 12px between them', () => {
    expect(railSlots(40)).toBe(1);
    expect(railSlots(91)).toBe(1);
    expect(railSlots(92)).toBe(2);
    expect(railSlots(0)).toBe(0);
    expect(railSlots(-10)).toBe(0);
  });

  it('moves nothing when everything fits', () => {
    expect(railOverflow(apps, 13, false)).toEqual([]);
    expect(railOverflow(apps, 20, true)).toEqual([]);
  });

  /** The More button needs a slot of its own, so one more app moves than overflowed. */
  it('moves the apps at the end, leaving a slot for More', () => {
    expect(railOverflow(apps, 12, false)).toEqual(['things', 'safe']);
    expect(railOverflow(apps, 10, false)).toEqual(['finance', 'feeds', 'things', 'safe']);
  });

  it('counts the More button that is already there for hidden apps', () => {
    expect(railOverflow(apps, 13, true)).toEqual(['safe']);
    expect(railOverflow(apps.slice(0, 12), 13, true)).toEqual([]);
  });

  it('always leaves one app on the rail', () => {
    expect(railOverflow(apps, 0, false)).toEqual(apps.slice(1));
    expect(railOverflow(apps, 1, true)).toEqual(apps.slice(1));
  });
});
