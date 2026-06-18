import { ref } from 'vue';
import type { Ref } from 'vue';
import type { NoteItem } from '../helpers';
import { buildNotePayload } from '../helpers';

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
  editorRefs: Ref<any[]>,
) {
  const renameModal = ref<{ show: boolean; noteId: string; value: string }>({ show: false, noteId: '', value: '' });

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
            renamedTabs.set(oldId, newPath);
            
            if (savedContent !== undefined) {
                tabContents.value[newPath] = tabContents.value[oldId] || savedContent;
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
                localStorage.setItem('synabit_recent_notes', JSON.stringify(recentNoteIds.value));
            }
        }

        if (currentNoteId.value === oldId) {
            currentNoteId.value = newPath;
        }
        
        const contentBody = tabContents.value[newPath] || savedContent || note.content;
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
        if (editorRefs.value && editorRefs.value.length > 0) {
            editorRefs.value.forEach(ref => {
                if (ref && typeof ref.focus === 'function') ref.focus();
            });
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
            if (tabContents.value[oldId] !== undefined) {
                tabContents.value[newPath] = tabContents.value[oldId];
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
                localStorage.setItem('synabit_recent_notes', JSON.stringify(recentNoteIds.value));
            }
        }

        currentNoteId.value = newPath;
        const contentBody = tabContents.value[newPath] || savedContent || note.content;
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
