import { ref } from 'vue';
import type { Ref } from 'vue';
import type { NoteItem } from '../helpers';
import { buildNotePayload } from '../helpers';
import { logger } from '../../../utils/logger';

export function useNoteSave(
  notes: Ref<NoteItem[]>,
  currentNoteId: Ref<string | null>,
  tabContents: Ref<Record<string, string>>,
  renamedTabs: Map<string, string>,
  ns: any,
  bus: any,
) {
  const saveTimeouts = new Map<string, ReturnType<typeof setTimeout>>();
  const editorRefs = ref<Record<string, any>>({});
  let suppressWatcherUntil = 0;

  const getSuppressWatcherUntil = () => suppressWatcherUntil;
  const setSuppressWatcherUntil = (val: number) => { suppressWatcherUntil = val; };

  const saveNoteForTab = (rawTabId: string) => {
    let tabId = rawTabId;
    while (renamedTabs.has(tabId)) {
        tabId = renamedTabs.get(tabId)!;
    }
    const note = notes.value.find(n => n.id === tabId);
    if (!note) { logger.warn('[NoteApp] saveNoteForTab: note not found for', tabId); return; }
    const existing = saveTimeouts.get(tabId);
    if (existing) clearTimeout(existing);
    saveTimeouts.set(tabId, setTimeout(async () => {
        saveTimeouts.delete(tabId);
        suppressWatcherUntil = Date.now() + 3000;
        const content = tabContents.value[tabId] || '';
        const fullRaw = content;
        try {
            await ns.writeNode(buildNotePayload(note, fullRaw));
            note.summary = content.substring(0, 150).trim();
            bus.emit('note:updated-external', { id: note.id, content });
            // Notify transclusion nodes that this note's blocks may have changed
            window.dispatchEvent(new CustomEvent('synabit-block-refresh', {
              detail: { nodeId: note.id }
            }));
        } catch(e) { logger.error("Failed to save note:", String(e)); }
    }, 600));
  };

  const onEditorUpdate = (val: string, rawTabId: string) => {
    let tabId = rawTabId;
    while (renamedTabs.has(tabId)) {
        tabId = renamedTabs.get(tabId)!;
    }
    tabContents.value[tabId] = val;
    if (currentNoteId.value === tabId) {
        bus.emit('note:updated-external', { id: tabId, content: val });
    }
    saveNoteForTab(tabId);
  };

  return {
    saveTimeouts,
    editorRefs,
    saveNoteForTab,
    onEditorUpdate,
    getSuppressWatcherUntil,
    setSuppressWatcherUntil,
  };
}
