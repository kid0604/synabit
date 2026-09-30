import { ref, computed, onBeforeUnmount, onDeactivated, getCurrentInstance } from 'vue';
import { i18n } from '../i18n';
import { showAppNotice } from './useAppNotice';

/** How long an undoable action waits before it is done for real. Matches Notes and Tasks. */
export const UNDO_WINDOW_MS = 7000;

/**
 * An action held back long enough to take it back — the shape `useTaskDelete`
 * uses, without the task-specific parts.
 *
 * The caller changes the screen straight away (hides the row, say) and hands
 * over two functions: `commit`, which does the real work once the window
 * closes, and `revert`, which puts the screen back if the user presses Undo.
 * Nothing is written until the window closes, so an undo is never a race
 * against sync noticing the change.
 *
 * Only one action waits at a time. The waiting action is done for real, early,
 * whenever the undo could no longer be seen or reached:
 *
 * * **another action starts** — the new one takes the toast;
 * * **the screen goes away** — unmounted, or deactivated: the apps live in a
 *   `<keep-alive>`, so switching app never unmounts them, and a toast that
 *   vanished with its app must not leave a delete waiting unseen;
 * * **the window is hidden** — on Android the OS kills a backgrounded app
 *   without warning, and a delete the user was told had happened would
 *   silently never happen. `visibilitychange` is the last moment JavaScript is
 *   reliably given.
 *
 * A failure after the window puts the screen back and says so: `onError` if
 * the caller wants its own words, else the shared "Couldn't delete" notice.
 *
 * Pair with `UndoToast`:
 *
 *   <UndoToast :show="undo.show.value" :restart-key="undo.key.value"
 *     :message="undo.message.value" :undo-label="$t('common.undo')"
 *     :seconds="undo.seconds" @undo="undo.undo"
 *     @pause="undo.pause" @resume="undo.resume" />
 */
export function useUndoableAction(options: {
  /** Say so when the real work failed; the screen has already been put back. */
  onError?: (e: unknown) => void;
  windowMs?: number;
} = {}) {
  const { onError, windowMs = UNDO_WINDOW_MS } = options;
  interface Pending {
    message: string;
    key: string;
    commit: () => void | Promise<void>;
    revert: () => void;
  }

  const pending = ref<Pending | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;
  /** When the running timer fires, so a pause can keep what was left. */
  let deadline = 0;
  let remaining = windowMs;
  let serial = 0;

  /**
   * Take the waiting action off the toast, synchronously.
   *
   * Everything that ends a wait goes through here first, before any `await`.
   * An `await` between reading `pending` and clearing it is what let a third
   * delete, started while the first was still being written, be overwritten by
   * the second and never happen at all.
   */
  function take(): Pending | null {
    clearTimeout(timer);
    timer = undefined;
    const p = pending.value;
    pending.value = null;
    return p;
  }

  async function perform(p: Pending) {
    try {
      await p.commit();
    } catch (e) {
      // The action failed after the screen already showed it done: put the
      // screen back so it does not lie, and say why.
      p.revert();
      if (onError) onError(e);
      else showAppNotice(i18n.global.t('common.delete_failed'), 'error');
    }
  }

  function arm(ms: number) {
    clearTimeout(timer);
    deadline = Date.now() + ms;
    timer = setTimeout(() => void commit(), ms);
  }

  /** Do the waiting action now, e.g. before a reload that would show it again. */
  async function commit() {
    const p = take();
    if (p) await perform(p);
  }

  /** Start an undoable action. `message` is the whole sentence the toast shows. */
  function run(message: string, commitFn: Pending['commit'], revert: Pending['revert']): Promise<void> {
    const previous = take();
    pending.value = { message, key: `${++serial}`, commit: commitFn, revert };
    remaining = windowMs;
    arm(windowMs);
    return previous ? perform(previous) : Promise.resolve();
  }

  function undo() {
    take()?.revert();
  }

  /**
   * Hold the countdown while the pointer or focus is on the toast. Seven
   * seconds is enough for most people and not for all of them; WCAG 2.2.1
   * asks that a time limit can be paused.
   */
  function pause() {
    if (!pending.value || timer === undefined) return;
    remaining = Math.max(0, deadline - Date.now());
    clearTimeout(timer);
    timer = undefined;
  }

  function resume() {
    if (pending.value && timer === undefined) arm(remaining);
  }

  const onHidden = () => {
    if (document.visibilityState === 'hidden') void commit();
  };

  if (getCurrentInstance()) {
    document.addEventListener('visibilitychange', onHidden);
    onDeactivated(() => void commit());
    onBeforeUnmount(() => {
      document.removeEventListener('visibilitychange', onHidden);
      void commit();
    });
  }

  return {
    run,
    undo,
    commit,
    pause,
    resume,
    show: computed(() => pending.value !== null),
    message: computed(() => pending.value?.message ?? ''),
    key: computed(() => pending.value?.key ?? ''),
    seconds: windowMs / 1000,
  };
}
