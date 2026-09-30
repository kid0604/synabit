import { ref, computed, watch } from 'vue';
import type { Ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { NoteItem } from '../helpers';
import { logger } from '../../../utils/logger';
import { looseIncludes } from '../../../utils/diacritics';
import { isTemplatePath } from '../templates/noteTemplates';

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
        const match = (text: string) => {
           if (!text) return false;
           // Case-sensitive search is a deliberate ask for exactness, so it
           // stays literal. Everything else folds tone marks the way the index
           // does, or this interim pass finds less than the results replacing
           // it and the list blinks empty on the way.
           return isCaseSensitiveSearch.value
             ? text.includes(searchTerm)
             : looseIncludes(text, searchTerm);
        };
        result = result.filter(n => {
            if (isTagSearch) return n.tags.some(t => match(t));
            // The list holds each note's opening rather than its body, so this
            // interim pass matches on that. It is only what the user sees for
            // the 200ms before the ranked full-text results below replace it.
            return match(n.title) || n.tags.some(t => match(t)) || match(n.summary);
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
  /**
   * The sidebar's "Recent" list, which is also where search results land.
   *
   * Templates (notes in `Templates/`) are left out of it while nobody is
   * searching: they are scaffolding, not something written lately, and
   * editing one used to push it to the top of the list next to real notes.
   * A search still finds them — that is how somebody gets back to one to
   * change it — and the template picker always lists them. A template the
   * user pinned on purpose stays pinned; that was an explicit ask.
   */
  const recentNotes = computed(() => {
    const searching = searchQuery.value.trim() !== '';
    return filteredNotes.value
      .filter(n => !n.pinned && (searching || !isTemplatePath(n.id)))
      .slice(0, 10);
  });

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
