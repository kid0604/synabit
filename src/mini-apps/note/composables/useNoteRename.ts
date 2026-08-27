import { ref } from 'vue';
import type { Ref } from 'vue';
import type { NoteItem } from '../helpers';
import { buildNotePayload, rememberRecentNotes } from '../helpers';

export function useNoteRename(
  notes: Ref<NoteItem[]>,
  currentNoteId: Ref<string | null>,
  ns: any,
  tabContents: Ref<Record<string, string>>,
  activeTabs: Ref<string[]>,
  tabAccessTime: Map<string, number>,
  renamedTabs: Map<string, string>,
  focusedTitles: Ref<Record<string, string>>,
  recentNoteIds: Ref<string[]>,
  saveTimeouts: Map<string, ReturnType<typeof setTimeout>>,
  saveNoteForTab: (tabId: string) => void,
  scanVault: () => Promise<void>,
  editorRefs: Ref<Record<string, any>>,
) {
  const renameModal = ref<{ show: boolean; noteId: string; value: string }>({ show: false, noteId: '', value: '' });

  /**
   * Move everything keyed by a note's old path onto its new one.
   *
   * A note is keyed by its path throughout the front end, so renaming it
   * changes its identity — and every map, list and open tab holding the old
   * one has to be told. This was written out twice, once per rename entry
   * point, and both copies forgot the same line: `note.id` itself.
   *
   * That line is not bookkeeping. `buildNotePayload` writes to `note.id`, so
   * with it left behind the save that follows a rename landed at the path the
   * rename had just moved the file off — recreating it there, and leaving the
   * vault holding the note under both names.
   */
  const migrateNoteIdentity = (
    note: NoteItem,
    oldId: string,
    newPath: string,
    fallbackContent?: string,
  ) => {
    note.id = newPath;
    note.path = newPath;

    // A save queued under the old path before it moved: `useNoteSave` follows
    // this to the new one rather than writing to a file that is no longer there.
    //
    // The delete first is not tidying. Rename a note and then rename it back,
    // and the map holds `A → B` from the first and `B → A` from the second;
    // the loop that follows those redirects then walks between them forever
    // and the app stops responding. `newPath` names a file that exists again,
    // so it cannot redirect anywhere, and saying so breaks the cycle where it
    // would otherwise be created.
    renamedTabs.delete(newPath);
    renamedTabs.set(oldId, newPath);

    const carried = tabContents.value[oldId] ?? fallbackContent;
    if (carried !== undefined) {
      tabContents.value[newPath] = carried;
      delete tabContents.value[oldId];
    }
    if (activeTabs.value.includes(oldId)) {
      activeTabs.value = activeTabs.value.map(id => id === oldId ? newPath : id);
    }
    if (tabAccessTime.has(oldId)) {
      tabAccessTime.set(newPath, tabAccessTime.get(oldId)!);
      tabAccessTime.delete(oldId);
    }
    if (recentNoteIds.value.includes(oldId)) {
      recentNoteIds.value = recentNoteIds.value.map(id => id === oldId ? newPath : id);
      rememberRecentNotes(recentNoteIds.value);
    }
  };

  const handleRenamePrompt = (id: string, closeContextMenu?: () => void) => {
    const note = notes.value.find(n => n.id === id);
    if (!note) return;
    renameModal.value = { show: true, noteId: id, value: note.title };
    closeContextMenu?.();
  };

  const confirmRename = async () => {
    const { noteId, value: newName } = renameModal.value;
    renameModal.value.show = false;
    const note = notes.value.find(n => n.id === noteId);
    if (!note || !newName || newName === note.title) return;
    try {
        const oldId = note.id;
        // Cancel any pending auto-save for the old path to prevent it from recreating the file after rename
        if (saveTimeouts.has(oldId)) {
            clearTimeout(saveTimeouts.get(oldId)!);
            saveTimeouts.delete(oldId);
        }
        const savedContent = tabContents.value[oldId];
        const newPath = await ns.renameNode({ oldRelPath: oldId, newName });
        
        // Secondary cancellation: if the user typed during the await rename_node_file, a new timeout for the old path might have been created.
        let needsSave = false;
        if (saveTimeouts.has(oldId)) {
            clearTimeout(saveTimeouts.get(oldId)!);
            saveTimeouts.delete(oldId);
            needsSave = true;
        }

        note.title = newName;
        if (oldId !== newPath) {
            migrateNoteIdentity(note, oldId, newPath, savedContent);
        }

        if (currentNoteId.value === oldId) {
            currentNoteId.value = newPath;
        }
        
        // Renaming rewrites the file, so it needs the body. The list holds
        // only each note's opening, so fall back to fetching this one.
        const contentBody = tabContents.value[newPath] || savedContent
            || (await ns.getNode(newPath))?.content || '';
        await ns.writeNode(buildNotePayload(note, contentBody));
        
        if (needsSave) {
            saveNoteForTab(newPath);
        }
        if (oldId !== newPath) {
            delete focusedTitles.value[oldId];
        }
        delete focusedTitles.value[newPath];
        scanVault();
    } catch(err) { alert(err); }
  };

  const renameTopTitle = async (e: Event) => {
    const isEnter = e.type === 'keydown' && (e as KeyboardEvent).key === 'Enter';
    const newTitle = (e.target as HTMLInputElement).value.trim();
    const note = notes.value.find(n => n.id === currentNoteId.value);
    
    const focusEditor = () => {
        if (editorRefs.value && currentNoteId.value) {
            const activeEditor = editorRefs.value[currentNoteId.value];
            if (activeEditor && typeof activeEditor.focus === 'function') {
                activeEditor.focus();
            }
        }
    };

    if (!note || note.title === newTitle || !newTitle) {
        if (isEnter) focusEditor();
        if (note) delete focusedTitles.value[note.id];
        return;
    }
    
    try {
        const oldId = note.id;
        // Cancel any pending auto-save for the old path to prevent it from recreating the file after rename
        if (saveTimeouts.has(oldId)) {
            clearTimeout(saveTimeouts.get(oldId)!);
            saveTimeouts.delete(oldId);
        }
        const savedContent = tabContents.value[oldId] || '';
        const newPath = await ns.renameNode({ oldRelPath: oldId, newName: newTitle });
        
        // Secondary cancellation: if the user typed during the await rename_node_file, a new timeout for the old path might have been created.
        let needsSave = false;
        if (saveTimeouts.has(oldId)) {
            clearTimeout(saveTimeouts.get(oldId)!);
            saveTimeouts.delete(oldId);
            needsSave = true;
        }

        note.title = newTitle;
        if (oldId !== newPath) {
            migrateNoteIdentity(note, oldId, newPath, savedContent);
        }

        currentNoteId.value = newPath;
        // Renaming rewrites the file, so it needs the body. The list holds
        // only each note's opening, so fall back to fetching this one.
        const contentBody = tabContents.value[newPath] || savedContent
            || (await ns.getNode(newPath))?.content || '';
        await ns.writeNode(buildNotePayload(note, contentBody));
        scanVault();
        
        if (needsSave) {
            saveNoteForTab(newPath);
        }
        
        if (isEnter) {
            setTimeout(focusEditor, 50);
        }
    } catch(err) { alert(err); }
  };

  return { renameModal, handleRenamePrompt, confirmRename, renameTopTitle };
}
