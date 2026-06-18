import { ref } from 'vue';

export function useNoteLock(
  appLockStore: any,
  handleNoteSelect: (id: string) => void,
) {
  const showNoteLockScreen = ref(false);
  const pendingNoteId = ref<string | null>(null);
  const pendingNoteAction = ref<'view' | 'unprotect'>('view');
  const noteLockTitle = ref('Enter PIN to view this note');

  const handleNoteLockUnlocked = () => {
    showNoteLockScreen.value = false;
    if (pendingNoteId.value) {
      const id = pendingNoteId.value;
      pendingNoteId.value = null;
      if (pendingNoteAction.value === 'view') {
        appLockStore.unlockNote(id);
        handleNoteSelect(id);
      } else if (pendingNoteAction.value === 'unprotect') {
        appLockStore.toggleProtectedNote(id);
      }
    }
  };

  const toggleNoteLock = (noteId: string, closeContextMenu: () => void) => {
    closeContextMenu();
    if (appLockStore.isNoteProtected(noteId)) {
      // Removing protection → require PIN
      pendingNoteId.value = noteId;
      pendingNoteAction.value = 'unprotect';
      noteLockTitle.value = 'Enter PIN to unlock this note';
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
