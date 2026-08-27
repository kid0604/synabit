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

  /**
   * Where a tab's saves should go now, following any renames it has been
   * through.
   *
   * The visited set is a brake, not decoration. This walks a map that other
   * code writes into, and a chain that loops — `A → B` and `B → A`, which is
   * exactly what renaming a note and then renaming it back produces — used to
   * be walked forever with the interface frozen behind it. `useNoteRename`
   * now avoids creating such a chain; this makes the walk safe regardless of
   * who else ever writes to that map.
   */
  const resolveTabId = (rawTabId: string): string => {
    let tabId = rawTabId;
    const seen = new Set<string>([tabId]);
    while (renamedTabs.has(tabId)) {
        const next = renamedTabs.get(tabId)!;
        if (seen.has(next)) {
            logger.warn('[NoteApp] rename redirects form a loop; stopping at', tabId);
            break;
        }
        seen.add(next);
        tabId = next;
    }
    return tabId;
  };

  const saveNoteForTab = (rawTabId: string) => {
    const tabId = resolveTabId(rawTabId);
    const note = notes.value.find(n => n.id === tabId);
    if (!note) { logger.warn('[NoteApp] saveNoteForTab: note not found for', tabId); return; }
    const existing = saveTimeouts.get(tabId);
    if (existing) clearTimeout(existing);
    saveTimeouts.set(tabId, setTimeout(async () => {
        saveTimeouts.delete(tabId);
        suppressWatcherUntil = Date.now() + 3000;

        // The editor turns the document into markdown on a short delay of its
        // own, so ask it to finish first. A no-op on the usual path — this
        // save was scheduled *by* that serialisation — but a save triggered
        // from anywhere else, a rename most of all, would otherwise write the
        // note as it stood a fifth of a second ago.
        (editorRefs.value?.[tabId] as { flushSerialize?: () => void } | undefined)?.flushSerialize?.();

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
    const tabId = resolveTabId(rawTabId);
    tabContents.value[tabId] = val;
    if (currentNoteId.value === tabId) {
        bus.emit('note:updated-external', { id: tabId, content: val });
    }
    saveNoteForTab(tabId);
  };

  return {
    saveTimeouts,
    editorRefs,
    resolveTabId,
    saveNoteForTab,
    onEditorUpdate,
    getSuppressWatcherUntil,
    setSuppressWatcherUntil,
  };
}
