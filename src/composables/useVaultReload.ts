import { getCurrentInstance, onActivated, onDeactivated, onUnmounted } from 'vue';
import { logger } from '../utils/logger';

/**
 * Re-reading a screen's data after the vault changed, without doing it for
 * screens nobody is looking at.
 *
 * # Why this exists
 *
 * Every mini-app sits in the shell's `<keep-alive>`, so every app visited this
 * session stays mounted — and stays subscribed to the vault. Each one carried
 * its own copy of the same few lines: on `vault:file-modified`, wait 300 ms,
 * re-read everything. A sync that touched a hundred files had every app
 * visited since launch re-reading its whole data set, behind the one screen
 * actually showing.
 *
 * # What it does
 *
 * `schedule(fn)` is the old `debouncedLoad`: the latest call wins, after a
 * pause. While the component is deactivated (hidden by the keep-alive) nothing
 * runs — the screen is marked stale instead, and the next `onActivated` runs
 * the full `reload` once. Whatever was scheduled while hidden is covered by
 * that, which is why a hidden screen keeps no queue of its own.
 *
 * A component that is not inside a keep-alive never deactivates, and behaves
 * exactly as the hand-written debounce did.
 */
export interface VaultReload {
  /** Run `fn` (by default the full reload) after `ms` of quiet. Hidden: mark stale. */
  schedule: (fn?: () => unknown, ms?: number) => void;
  /** Run `fn` now if the screen is showing; otherwise mark it stale. */
  whenShown: (fn?: () => unknown) => void;
  /** Whether the screen is showing — false between deactivation and activation. */
  isActive: () => boolean;
}

export function useVaultReload(reload: () => unknown, defaultDelay = 300): VaultReload {
  let active = true;
  let stale = false;
  let timer: ReturnType<typeof setTimeout> | null = null;

  const run = (fn: () => unknown) => {
    try {
      const result = fn();
      if (result && typeof (result as Promise<unknown>).catch === 'function') {
        (result as Promise<unknown>).catch((e) => logger.error('[VaultReload] Reload failed', e));
      }
    } catch (e) {
      logger.error('[VaultReload] Reload failed', e);
    }
  };

  const cancel = () => {
    if (timer) clearTimeout(timer);
    timer = null;
  };

  const whenShown = (fn: () => unknown = reload) => {
    if (!active) {
      stale = true;
      return;
    }
    run(fn);
  };

  const schedule = (fn: () => unknown = reload, ms = defaultDelay) => {
    cancel();
    if (!active) {
      stale = true;
      return;
    }
    timer = setTimeout(() => {
      timer = null;
      // Hidden while waiting: the same as arriving hidden.
      whenShown(fn);
    }, ms);
  };

  if (getCurrentInstance()) {
    onDeactivated(() => {
      active = false;
      // A reload due in a moment is a reload of a screen that just went away.
      if (timer) {
        cancel();
        stale = true;
      }
    });
    onActivated(() => {
      active = true;
      if (stale) {
        stale = false;
        run(reload);
      }
    });
    onUnmounted(cancel);
  }

  return { schedule, whenShown, isActive: () => active };
}
