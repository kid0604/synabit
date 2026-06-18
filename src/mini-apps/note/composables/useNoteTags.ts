import { ref, computed } from 'vue';
import type { Ref, ComputedRef } from 'vue';
import type { NoteItem } from '../helpers';
import { buildNotePayload } from '../helpers';

interface TagNode {
  name: string;
  basename: string;
  count: number;
  expanded: boolean;
  children: TagNode[];
}

export function useNoteTags(
  notes: Ref<NoteItem[]>,
  currentNoteId: Ref<string | null>,
  currentContent: ComputedRef<string>,
  ns: any,
  scanVault: () => Promise<void>,
) {
  const newTagInput = ref('');
  const tagTree = ref<TagNode[]>([]);
  const selectedTags = ref<Set<string>>(new Set());

  const allTags = computed(() => {
    const counts = new Map<string, number>();
    notes.value.forEach(note => { note.tags.forEach(tag => { counts.set(tag, (counts.get(tag) || 0) + 1); }); });
    return Array.from(counts.entries()).map(([name, count]) => ({ name, count })).sort((a,b) => b.count - a.count);
  });

  const topTags = computed(() => allTags.value.slice(0, 10));

  const addTag = async (e: KeyboardEvent) => {
    if (e.key === 'Enter' && newTagInput.value.trim()) {
        const note = notes.value.find(n => n.id === currentNoteId.value);
        if (note && !note.tags.includes(newTagInput.value.trim())) {
            note.tags.push(newTagInput.value.trim());
            newTagInput.value = '';
            await ns.writeNode(buildNotePayload(note, currentContent.value));
            scanVault();
        }
    }
  };

  const removeTag = async (tagToRemove: string) => {
    const note = notes.value.find(n => n.id === currentNoteId.value);
    if (note) {
        note.tags = note.tags.filter(t => t !== tagToRemove);
        await ns.writeNode(buildNotePayload(note, currentContent.value));
        scanVault();
    }
  };

  const buildTagTree = (allNotes: NoteItem[]) => {
    const map = new Map<string, { count: number, children: Set<string> }>();
    allNotes.forEach(n => {
      n.tags.forEach(tagPath => {
         const parts = tagPath.split('/');
         const parent = parts[0];
         if (!map.has(parent)) map.set(parent, { count: 0, children: new Set() });
         map.get(parent)!.count++;
         if (parts.length > 1) {
            const childName = `${parent}/${parts[1]}`;
            map.get(parent)!.children.add(childName);
            if (!map.has(childName)) map.set(childName, { count: 0, children: new Set() });
            map.get(childName)!.count++;
         }
      })
    });
    const tree: TagNode[] = [];
    map.forEach((data, name) => {
      if (!name.includes('/')) {
        const children: TagNode[] = Array.from(data.children).map(childName => ({
          name: childName, basename: childName.split('/')[1], count: map.get(childName)?.count || 0, expanded: false, children: []
        }));
        tree.push({ name, basename: name, count: data.count, expanded: true, children });
      }
    });
    tagTree.value = tree.sort((a,b) => a.name.localeCompare(b.name));
  };

  const toggleTagSelection = (tagName: string) => {
    const newSet = new Set(selectedTags.value);
    if (newSet.has(tagName)) newSet.delete(tagName);
    else newSet.add(tagName);
    selectedTags.value = newSet;
  };

  return {
    newTagInput,
    tagTree,
    selectedTags,
    allTags,
    topTags,
    addTag,
    removeTag,
    buildTagTree,
    toggleTagSelection,
  };
}
