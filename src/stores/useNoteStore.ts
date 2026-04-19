import { defineStore } from 'pinia';
import { ref } from 'vue';
import type { NoteMetadata } from '../types/ipc';

export const useNoteStore = defineStore('note', () => {
  const notes = ref<NoteMetadata[]>([]);
  const currentNoteId = ref<string | null>(null);

  // Tab management equivalent from App.vue
  const activeTabs = ref<string[]>([]);
  const tabContents = ref<Record<string, string>>({});
  const tabAccessTime = new Map<string, number>();

  return {
    notes,
    currentNoteId,
    activeTabs,
    tabContents,
    tabAccessTime
  };
});
