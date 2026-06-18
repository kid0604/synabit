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
        let note = notes.value.find(n => n.id === id);
        if (!note) {
            try {
                const fetchedNode = await ns.getNode(id);
                if (fetchedNode) {
                    note = {
                        id: fetchedNode.id,
                        title: fetchedNode.title,
                        content: fetchedNode.content,
                        date: fetchedNode.updated_at || fetchedNode.created_at,
                        path: fetchedNode.id,
                        tags: Array.isArray(fetchedNode.properties?.tags) ? fetchedNode.properties.tags : [],
                        pinned: !!fetchedNode.properties?.pinned,
                        full_width: !!fetchedNode.properties?.full_width,
                        linked_projects: Array.isArray(fetchedNode.properties?.linked_projects) ? fetchedNode.properties.linked_projects : [],
                        summary: fetchedNode.content.substring(0, 150).trim()
                    };
                    notes.value.unshift(note);
                }
            } catch (e) {
                console.error("Failed to fetch missing note", e);
            }
        }
        
        if (note) {
            tabContents.value[id] = note.content;
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
