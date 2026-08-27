import { describe, it, expect, beforeEach, vi } from 'vitest';
import { forgetViewport, recallViewport, rememberViewport } from '../viewportMemory';

/**
 * The test environment has no storage of its own, so bring one.
 *
 * `viewportMemory` reads `window.localStorage` and gives up quietly when
 * there is none, which is the right behaviour in a webview that has it
 * disabled — and would make every assertion below pass by accident.
 */
const memory = new Map<string, string>();
const fakeStorage: Storage = {
  get length() {
    return memory.size;
  },
  clear: () => memory.clear(),
  getItem: (key: string) => memory.get(key) ?? null,
  key: (index: number) => [...memory.keys()][index] ?? null,
  removeItem: (key: string) => void memory.delete(key),
  setItem: (key: string, value: string) => void memory.set(key, String(value)),
};
vi.stubGlobal('localStorage', fakeStorage);

describe('where a board was left', () => {
  beforeEach(() => {
    memory.clear();
  });

  it('is actually being stored, so the rest of these mean something', () => {
    rememberViewport('a', { x: 1, y: 2, zoom: 3 });
    expect(memory.size).toBe(1);
  });

  it('comes back the way it was put in', () => {
    rememberViewport('Whiteboards/a.whiteboard.json', { x: -120, y: 40.5, zoom: 0.75 });
    expect(recallViewport('Whiteboards/a.whiteboard.json')).toEqual({
      x: -120,
      y: 40.5,
      zoom: 0.75,
    });
  });

  it('is kept per board', () => {
    rememberViewport('a', { x: 1, y: 1, zoom: 1 });
    rememberViewport('b', { x: 2, y: 2, zoom: 2 });
    expect(recallViewport('a')?.x).toBe(1);
    expect(recallViewport('b')?.x).toBe(2);
  });

  it('says nothing about a board that has not been opened here', () => {
    expect(recallViewport('never-seen')).toBeNull();
  });

  it('refuses a stored value that would leave nothing on screen', () => {
    // A zoom of zero is a canvas the user cannot see anything on, and there is
    // no gesture that gets them back out of it.
    fakeStorage.setItem('whiteboard:viewport:a', JSON.stringify({ x: 0, y: 0, zoom: 0 }));
    expect(recallViewport('a')).toBeNull();

    fakeStorage.setItem('whiteboard:viewport:b', JSON.stringify({ x: 0, y: 0, zoom: -1 }));
    expect(recallViewport('b')).toBeNull();
  });

  it('refuses a stored value that is not a viewport', () => {
    fakeStorage.setItem('whiteboard:viewport:a', 'not json');
    expect(recallViewport('a')).toBeNull();

    fakeStorage.setItem('whiteboard:viewport:b', JSON.stringify({ x: 'left', y: 0, zoom: 1 }));
    expect(recallViewport('b')).toBeNull();

    fakeStorage.setItem('whiteboard:viewport:c', JSON.stringify({ x: 0, y: 0 }));
    expect(recallViewport('c')).toBeNull();
  });

  it('forgets a board that is gone', () => {
    rememberViewport('a', { x: 1, y: 1, zoom: 1 });
    forgetViewport('a');
    expect(recallViewport('a')).toBeNull();
  });

  it('does nothing at all without a board id', () => {
    rememberViewport('', { x: 1, y: 1, zoom: 1 });
    expect(memory.size).toBe(0);
    expect(recallViewport('')).toBeNull();
  });
});
