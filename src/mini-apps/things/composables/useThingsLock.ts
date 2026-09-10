import { ref } from 'vue';
import { useAppLockStore } from '../../../stores/useAppLockStore';

type AppLock = Pick<
  ReturnType<typeof useAppLockStore>,
  'isEnabled' | 'isNoteProtected' | 'isNoteAccessible' | 'unlockNote' | 'touchNoteSession' | 'toggleProtectedNote'
>;

/**
 * The PIN a protected note asks for in Notes, asked for here as well.
 *
 * Things shows every kind in the vault, notes included, and it asked for
 * nothing: a note that took a PIN to open in Notes opened here on one click,
 * body, fields, history and all. Protection belongs to the note, not to the
 * app it happens to be opened in.
 *
 * The same rule Notes applies, and the same split between what shows and what
 * does not: the title and the fields in a row are the note's label and stay
 * visible, as they do in the Notes sidebar; anything drawn from the body waits
 * for the PIN.
 */
export function useThingsLock(appLock: AppLock = useAppLockStore()) {
  /** Whether opening this node right now takes the PIN. */
  const isLocked = (id: string) =>
    appLock.isEnabled && appLock.isNoteProtected(id) && !appLock.isNoteAccessible(id);

  /**
   * Whether a node's body must stay out of previews, locked or not.
   *
   * Nexus hides a protected note's snippet even once it has been unlocked, and
   * a snippet shown under somebody else's node reads as that node's to show.
   */
  const hidesBody = (id: string) => appLock.isEnabled && appLock.isNoteProtected(id);

  /** What is waiting on the PIN: whose it is, and what to do once it is given. */
  const pending = ref<{ id: string; then: () => unknown } | null>(null);

  /**
   * Do `action` now, or once the PIN for `id` has been given.
   *
   * Nothing happens to the screen while it waits. Whatever was open stays
   * open, and cancelling leaves everything exactly as it was — which is how
   * Notes behaves, and why the node is never fetched before it is unlocked.
   */
  const whenUnlocked = async (id: string, action: () => unknown) => {
    if (!isLocked(id)) {
      await action();
      return;
    }
    pending.value = { id, then: action };
  };

  const unlocked = async () => {
    const waiting = pending.value;
    pending.value = null;
    if (!waiting) return;
    appLock.unlockNote(waiting.id);
    await waiting.then();
  };

  const cancel = () => {
    pending.value = null;
  };

  /** Keep an open protected note unlocked while it is being worked on. */
  const touch = (id: string | null) => {
    if (id) appLock.touchNoteSession(id);
  };

  /**
   * Protect a copy the way its original was protected.
   *
   * A duplicate carries the whole body, so an unprotected copy of a protected
   * note is the note with the lock taken off.
   */
  const protectCopy = async (originalId: string, copyId: string) => {
    if (!appLock.isNoteProtected(originalId) || appLock.isNoteProtected(copyId)) return;
    await appLock.toggleProtectedNote(copyId);
  };

  return { isLocked, hidesBody, pending, whenUnlocked, unlocked, cancel, touch, protectCopy };
}
