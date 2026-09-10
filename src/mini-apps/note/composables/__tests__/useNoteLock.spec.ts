import { describe, it, expect, vi } from 'vitest';
import { useNoteLock } from '../useNoteLock';

const store = () => ({ unlockNote: vi.fn(), toggleProtectedNote: vi.fn(), isNoteProtected: vi.fn() });

describe('useNoteLock', () => {
  // The history of a protected note is gated behind the same PIN as the note.
  // Entering it opens the history that was asked for, not the note.
  it('opens the history that was waiting on the PIN', () => {
    const appLock = store();
    const select = vi.fn();
    const openHistory = vi.fn();
    const lock = useNoteLock(appLock, select, openHistory);

    lock.pendingNoteId.value = 'Notes/secret.md';
    lock.pendingNoteAction.value = 'history';
    lock.showNoteLockScreen.value = true;
    lock.handleNoteLockUnlocked();

    expect(appLock.unlockNote).toHaveBeenCalledWith('Notes/secret.md');
    expect(openHistory).toHaveBeenCalledWith('Notes/secret.md');
    expect(select).not.toHaveBeenCalled();
    expect(lock.showNoteLockScreen.value).toBe(false);
  });

  it('still opens the note for a plain view', () => {
    const appLock = store();
    const select = vi.fn();
    const openHistory = vi.fn();
    const lock = useNoteLock(appLock, select, openHistory);

    lock.pendingNoteId.value = 'Notes/secret.md';
    lock.pendingNoteAction.value = 'view';
    lock.handleNoteLockUnlocked();

    expect(select).toHaveBeenCalledWith('Notes/secret.md');
    expect(openHistory).not.toHaveBeenCalled();
  });
});
