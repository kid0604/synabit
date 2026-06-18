import { ref, computed, watch } from 'vue';
import type { Ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { NoteItem } from '../helpers';
import { logger } from '../../../utils/logger';

export function useNoteSearch(
  notes: Ref<NoteItem[]>,
  recentNoteIds: Ref<string[]>,
  selectedTags: Ref<Set<string>>,
  vaultPath: Ref<string>,
) {
  const searchQuery = ref('');
  const isCaseSensitiveSearch = ref(false);
  const backendSearchIds = ref<string[] | null>(null);
  let searchTimeout: ReturnType<typeof setTimeout>;

  const filteredNotes = computed(() => {
    let result = notes.value;
    let isSearch = false;
    // Use backend FTS5 search results when available
    if (searchQuery.value.trim() && backendSearchIds.value !== null) {
        isSearch = true;
        const idSet = new Set(backendSearchIds.value);
        result = result.filter(n => idSet.has(n.id));
        // Preserve the order from backend (BM25 ranked)
        const orderMap = new Map(backendSearchIds.value.map((id, i) => [id, i]));
        result = result.sort((a, b) => (orderMap.get(a.id) ?? 999) - (orderMap.get(b.id) ?? 999));
    } else if (searchQuery.value.trim()) {
        isSearch = true;
        // Fallback: local search while backend is loading
        const q = searchQuery.value.trim();
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
    if (selectedTags.value.size > 0) {
        result = result.filter(n => n.tags.some(t => selectedTags.value.has(t)));
    }
    
    if (isSearch) {
        return result.sort((a,b) => {
            if (a.pinned && !b.pinned) return -1;
            if (!a.pinned && b.pinned) return 1;
            return 0;
        });
    }

    return result.sort((a,b) => {
        if (a.pinned && !b.pinned) return -1;
        if (!a.pinned && b.pinned) return 1;
        const aIndex = recentNoteIds.value.indexOf(a.id);
        const bIndex = recentNoteIds.value.indexOf(b.id);
        const aScore = aIndex === -1 ? 999999 : aIndex;
        const bScore = bIndex === -1 ? 999999 : bIndex;
        if (aScore !== bScore) return aScore - bScore;
        return b.date.localeCompare(a.date);
    });
  });

  const allPinnedNotes = computed(() => filteredNotes.value.filter(n => n.pinned));
  const topPinnedNotes = computed(() => allPinnedNotes.value.slice(0, 5));
  const recentNotes = computed(() => filteredNotes.value.filter(n => !n.pinned).slice(0, 10));

  // Debounced backend search
  watch(searchQuery, (q) => {
    clearTimeout(searchTimeout);
    if (!q.trim()) {
        backendSearchIds.value = null;
        return;
    }
    searchTimeout = setTimeout(async () => {
        try {
            const resp = await invoke<{ results: { id: string }[], total_count: number, query_time_ms: number }>('search_notes', {
                vaultPath: vaultPath.value,
                query: q
            });
            // Only apply if query hasn't changed
            if (searchQuery.value === q) {
                backendSearchIds.value = resp.results.map(r => r.id);
            }
        } catch (e) {
            logger.error('Backend search error', e);
        }
    }, 200);
  });

  return {
    searchQuery,
    isCaseSensitiveSearch,
    backendSearchIds,
    filteredNotes,
    allPinnedNotes,
    topPinnedNotes,
    recentNotes,
  };
}
