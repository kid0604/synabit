import { ref, computed } from 'vue';
import type { Ref } from 'vue';
import type { NoteItem } from '../helpers';

export function useNoteTabs(
  notes: Ref<NoteItem[]>,
  currentNoteId: Ref<string | null>,
  ns: any,
  appLockStore: any,
) {
  const activeTabs = ref<string[]>([]);
  const tabContents = ref<Record<string, string>>({});
  const focusedTitles = ref<Record<string, string>>({});
  const tabAccessTime = new Map<string, number>();
  const renamedTabs = new Map<string, string>();

  const currentContent = computed({
    get: () => currentNoteId.value ? tabContents.value[currentNoteId.value] || '' : '',
    set: (val) => {
      if (currentNoteId.value) {
        tabContents.value[currentNoteId.value] = val;
        // Refresh note session while actively editing
        appLockStore.touchNoteSession(currentNoteId.value);
      }
    }
  });

  const loadNoteFile = async (id: string) => {
    if (!id) return;
    tabAccessTime.set(id, Date.now());
    
    if (!activeTabs.value.includes(id)) {
        if (activeTabs.value.length >= 10) {
            let oldestId = activeTabs.value[0];
            let oldestTime = tabAccessTime.get(oldestId) || Infinity;
            for (const t of activeTabs.value) {
                const time = tabAccessTime.get(t) || 0;
                if (time < oldestTime) {
                    oldestTime = time;
                    oldestId = t;
                }
            }
            activeTabs.value = activeTabs.value.filter(t => t !== oldestId);
            delete tabContents.value[oldestId];
            tabAccessTime.delete(oldestId);
        }
        activeTabs.value.push(id);
    }
    
    if (tabContents.value[id] === undefined) {
        // The body always comes from a fetch now: the list carries only each
        // note's opening, so there is nothing in it to open a note from.
        try {
            const fetchedNode = await ns.getNode(id);
            if (fetchedNode) {
                tabContents.value[id] = fetchedNode.content;

                // A note reached by link or by deep link may not be in the list
                // yet. Put it there so the sidebar shows what is open.
                if (!notes.value.some(n => n.id === id)) {
                    notes.value.unshift({
                        id: fetchedNode.id,
                        title: fetchedNode.title,
                        date: fetchedNode.updated_at || fetchedNode.created_at,
                        path: fetchedNode.rel_path,
                        tags: Array.isArray(fetchedNode.properties?.tags) ? fetchedNode.properties.tags : [],
                        pinned: !!fetchedNode.properties?.pinned,
                        full_width: !!fetchedNode.properties?.full_width,
                        linked_projects: Array.isArray(fetchedNode.properties?.linked_projects) ? fetchedNode.properties.linked_projects : [],
                        summary: fetchedNode.content.substring(0, 150).trim()
                    });
                }
            }
        } catch (e) {
            console.error("Failed to fetch note body", e);
        }
    }
  };

  return {
    activeTabs,
    tabContents,
    focusedTitles,
    tabAccessTime,
    renamedTabs,
    currentContent,
    loadNoteFile,
  };
}
