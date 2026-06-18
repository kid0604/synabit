import { ref, computed, watch } from 'vue';
import type { Ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { NoteItem } from '../helpers';
import { logger } from '../../../utils/logger';

export function useNoteManager(
  notes: Ref<NoteItem[]>,
  isCaseSensitiveSearch: Ref<boolean>,
  vaultPath: Ref<string>,
) {
  const viewMode = ref<'editor' | 'manager'>('editor');
  const managerFilter = ref('');
  const managerSearchQuery = ref('');
  const managerBackendSearchIds = ref<string[] | null>(null);
  let managerSearchTimeout: ReturnType<typeof setTimeout>;
  const managerCurrentPage = ref(1);
  const managerItemsPerPage = 50;

  const managerFilteredNotes = computed(() => {
    let result = notes.value;
    if (managerSearchQuery.value.trim()) {
       // Backend FTS5 results available
       if (managerBackendSearchIds.value !== null) {
           const idSet = new Set(managerBackendSearchIds.value);
           result = result.filter(n => idSet.has(n.id));
           const orderMap = new Map(managerBackendSearchIds.value.map((id, i) => [id, i]));
           result = result.sort((a, b) => (orderMap.get(a.id) ?? 999) - (orderMap.get(b.id) ?? 999));
       } else {
           // Fallback: local search while backend is loading
           const q = managerSearchQuery.value.trim();
           const isTagSearch = q.startsWith('#');
           const searchTerm = isTagSearch ? q.slice(1) : q;
           const searchStr = isCaseSensitiveSearch.value ? searchTerm : searchTerm.toLowerCase();
           const match = (text: string) => {
              if (!text) return false;
              return isCaseSensitiveSearch.value ? text.includes(searchStr) : text.toLowerCase().includes(searchStr);
           };
           result = result.filter(n => {
              if (isTagSearch) return n.tags.some(t => match(t));
              return match(n.title) || n.tags.some(t => match(t)) || match(n.content);
           });
       }
    }
    if (managerFilter.value === 'notes' || !managerFilter.value || managerFilter.value === 'tags') return result;
    else if (managerFilter.value === 'pinned') return result.filter(n => n.pinned);
    else return result.filter(n => n.tags.includes(managerFilter.value));
  });

  watch([managerSearchQuery, managerFilter], () => {
    managerCurrentPage.value = 1;
  });

  // Debounced backend search for Note Manager
  watch(managerSearchQuery, (q) => {
    clearTimeout(managerSearchTimeout);
    if (!q.trim()) {
        managerBackendSearchIds.value = null;
        return;
    }
    managerSearchTimeout = setTimeout(async () => {
        try {
            const resp = await invoke<{ results: { id: string }[], total_count: number, query_time_ms: number }>('search_notes', {
                vaultPath: vaultPath.value,
                query: q
            });
            if (managerSearchQuery.value === q) {
                managerBackendSearchIds.value = resp.results.map(r => r.id);
            }
        } catch (e) {
            logger.error('Manager backend search error', e);
        }
    }, 200);
  });

  const managerTotalPages = computed(() => Math.ceil(managerFilteredNotes.value.length / managerItemsPerPage));

  const managerPaginatedNotes = computed(() => {
    const start = (managerCurrentPage.value - 1) * managerItemsPerPage;
    return managerFilteredNotes.value.slice(start, start + managerItemsPerPage);
  });

  const openNoteManager = (filterType: string, hideSidebarOnMobile?: () => void) => {
    managerFilter.value = filterType;
    viewMode.value = 'manager';
    if (window.innerWidth < 768 && hideSidebarOnMobile) {
      hideSidebarOnMobile();
    }
  };

  const managerNextPage = () => {
    if (managerCurrentPage.value < managerTotalPages.value) managerCurrentPage.value++;
  };

  const managerPrevPage = () => {
    if (managerCurrentPage.value > 1) managerCurrentPage.value--;
  };

  return {
    viewMode,
    managerFilter,
    managerSearchQuery,
    managerBackendSearchIds,
    managerCurrentPage,
    managerItemsPerPage,
    managerFilteredNotes,
    managerTotalPages,
    managerPaginatedNotes,
    openNoteManager,
    managerNextPage,
    managerPrevPage,
  };
}
