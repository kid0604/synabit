import { ref } from 'vue';
import { i18n } from '../../../i18n';
import { showAppNotice } from '../../../composables/useAppNotice';
import { pinErrorKey } from '../../../stores/useAppLockStore';

export function useNoteLock(
  appLockStore: any,
  handleNoteSelect: (id: string) => void,
  openHistory?: (id: string) => void,
) {
  const showNoteLockScreen = ref(false);
  const pendingNoteId = ref<string | null>(null);
  const pendingNoteAction = ref<'view' | 'unprotect' | 'history'>('view');
  // An i18n key, translated where the lock screen renders it.
  const noteLockTitle = ref('note.pin_to_view');

  // The lock screen hands on the PIN it checked; unprotecting passes it to the
  // backend, which checks it again (`update_app_lock_config`).
  const handleNoteLockUnlocked = (pin?: string) => {
    showNoteLockScreen.value = false;
    if (pendingNoteId.value) {
      const id = pendingNoteId.value;
      pendingNoteId.value = null;
      if (pendingNoteAction.value === 'view') {
        appLockStore.unlockNote(id);
        handleNoteSelect(id);
      } else if (pendingNoteAction.value === 'history') {
        appLockStore.unlockNote(id);
        openHistory?.(id);
      } else if (pendingNoteAction.value === 'unprotect') {
        Promise.resolve(appLockStore.toggleProtectedNote(id, pin)).catch((e: unknown) => {
          showAppNotice(i18n.global.t(pinErrorKey(e) ?? 'settings.security.lock_change_failed'), 'error');
        });
      }
    }
  };

  const toggleNoteLock = (noteId: string, closeContextMenu: () => void) => {
    closeContextMenu();
    if (appLockStore.isNoteProtected(noteId)) {
      // Removing protection → require PIN
      pendingNoteId.value = noteId;
      pendingNoteAction.value = 'unprotect';
      noteLockTitle.value = 'note.pin_to_unlock';
      showNoteLockScreen.value = true;
    } else {
      // Adding protection → free
      appLockStore.toggleProtectedNote(noteId);
    }
  };

  return {
    showNoteLockScreen,
    pendingNoteId,
    pendingNoteAction,
    noteLockTitle,
    handleNoteLockUnlocked,
    toggleNoteLock,
  };
}
