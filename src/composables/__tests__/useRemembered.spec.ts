import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { nextTick } from 'vue';

import { useRemembered, recallChoice, rememberChoice } from '../useRemembered';

/**
 * A display choice that survives a restart, and every way that can go wrong.
 *
 * Storage outlives the version that wrote it and anybody can open the devtools
 * and edit it, so what comes back is input from outside — the same reason
 * `asFieldKind` validates a stored field kind rather than casting it. A graph
 * told to draw itself as `sparkles` should draw dots, not nothing.
 */

const MARKS = ['dots', 'icons'] as const;

const fakeStorage = () => {
  const map = new Map<string, string>();
  return {
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => void map.set(k, v),
    removeItem: (k: string) => void map.delete(k),
    clear: () => map.clear(),
    key: () => null,
    length: 0,
  } as unknown as Storage;
};

beforeEach(() => vi.stubGlobal('localStorage', fakeStorage()));
afterEach(() => vi.unstubAllGlobals());

describe('remembering a choice', () => {
  it('starts at the fallback when nothing was stored', () => {
    expect(useRemembered('graph', MARKS, 'dots').value).toBe('dots');
  });

  it('comes back where it was left', async () => {
    const first = useRemembered('graph', MARKS, 'dots');
    first.value = 'icons';
    await nextTick();

    // A fresh mount, as a restart would be.
    expect(useRemembered('graph', MARKS, 'dots').value).toBe('icons');
  });

  it('ignores a value this version does not understand', () => {
    rememberChoice('graph', 'sparkles');
    expect(recallChoice('graph', MARKS, 'dots')).toBe('dots');
  });

  it('keeps two settings apart', async () => {
    const graph = useRemembered('graph', MARKS, 'dots');
    const list = useRemembered('list', MARKS, 'dots');
    graph.value = 'icons';
    await nextTick();

    expect(list.value).toBe('dots');
    expect(recallChoice('list', MARKS, 'dots')).toBe('dots');
  });
});

describe('when the device will not store anything', () => {
  /** A private window, a blocked origin, a full disk. None of them is fatal. */
  it('falls back rather than throwing when reading fails', () => {
    vi.stubGlobal('localStorage', {
      getItem: () => {
        throw new Error('blocked');
      },
      setItem: () => {},
    } as unknown as Storage);

    expect(() => recallChoice('graph', MARKS, 'dots')).not.toThrow();
    expect(recallChoice('graph', MARKS, 'dots')).toBe('dots');
  });

  it('carries on when writing fails', async () => {
    vi.stubGlobal('localStorage', {
      getItem: () => null,
      setItem: () => {
        throw new Error('quota');
      },
    } as unknown as Storage);

    const choice = useRemembered('graph', MARKS, 'dots');
    choice.value = 'icons';
    await nextTick();

    // The setting is lost at the next launch, and the app is still usable now.
    expect(choice.value).toBe('icons');
  });

  it('survives having no storage at all', () => {
    vi.stubGlobal('localStorage', undefined);
    expect(recallChoice('graph', MARKS, 'dots')).toBe('dots');
    expect(() => rememberChoice('graph', 'icons')).not.toThrow();
  });
});
