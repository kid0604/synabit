import { ref, computed, watch, type Ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { type TaskMetadata, getTodayStr } from '../types';
import { translateTaskQuery } from '../query';

/**
 * `vaultPath` used to be a parameter. `run_node_query` reads the index rather
 * than the folder, so there is nothing left to point it at.
 */
export function useTaskSearch(tasks: Ref<TaskMetadata[]>) {
  const searchQuery = ref('');
  const activeCategory = ref<string>('today');
  const backendSearchIds = ref<string[] | null>(null);
  let taskSearchTimeout: ReturnType<typeof setTimeout>;

  /**
   * Ask the vault's query engine, and keep what it answers.
   *
   * `run_node_query` is the same engine the query blocks inside notes use, so
   * a search here understands everything one of those does: dates, comparisons
   * like `due_date:<2026-09-01`, sorting, limits. The Tasks app used to send
   * the bare words to the text index and drop every token on the floor, which
   * is why a note could ask what was overdue and this screen could not.
   *
   * `translateTaskQuery` decides the split. What it hands back as `backend` is
   * the engine's business; the filters further down still read the raw string
   * for the shorthands the engine has no word for.
   */
  watch(searchQuery, (q) => {
    clearTimeout(taskSearchTimeout);
    const { backend } = translateTaskQuery(q);
    if (!backend) {
      backendSearchIds.value = null;
      return;
    }
    taskSearchTimeout = setTimeout(async () => {
      try {
        const resp = await invoke<{ rows: { id: string }[] }>('run_node_query', { query: backend });
        // Dropped if the box moved on while this was in flight, so a slow
        // answer cannot narrow the list to a query nobody is looking at.
        if (translateTaskQuery(searchQuery.value).backend === backend) {
          backendSearchIds.value = resp.rows.map(r => r.id);
        }
      } catch (e) {
        console.error('Task query error', e);
        backendSearchIds.value = null;
      }
    }, 200);
  });

  const searchedTasks = computed(() => {
    let result = tasks.value;
    
    if (searchQuery.value.trim()) {
      const query = searchQuery.value.toLowerCase();
      const { backend } = translateTaskQuery(searchQuery.value);

      // Layer 1: whatever the query engine narrowed it to, in its order.
      if (backend && backendSearchIds.value !== null) {
        const idSet = new Set(backendSearchIds.value);
        result = result.filter(t => idSet.has(t.id));
        const orderMap = new Map(backendSearchIds.value.map((id, i) => [id, i]));
        result = result.sort((a, b) => (orderMap.get(a.id) ?? 999) - (orderMap.get(b.id) ?? 999));
      } else if (backend && backendSearchIds.value === null) {
        // The engine has not answered yet. A rough local match beats an empty
        // list for the fraction of a second before it does.
        const words = searchQuery.value.toLowerCase()
          .replace(/\b[a-z_]+:[^\s]+/gi, ' ')
          .replace(/(?:^|\s)[#@-][^\s]+/g, ' ')
          .trim();
        if (words) {
          result = result.filter(t =>
            t.title.toLowerCase().includes(words) ||
            t.preview.toLowerCase().includes(words) ||
            t.tags.some(tag => tag.toLowerCase().includes(words))
          );
        }
      }
      
      // Layer 2: Local domain-specific post-filters
      const isQuery = (prop: string) => query.includes(`is:${prop}`);
      const notQuery = (prop: string) => query.includes(`not:${prop}`);
      const pQueryMatch = query.match(/(?:p|priority):([1-4])/);
      const statusQueryMatch = query.match(/status:([a-z_]+)/);
      const tagQueryMatch = query.match(/(?:#|tag:)([^\s]+)/);
      const assignQueryMatch = query.match(/@([^\s]+)/);
      const customPropMatches = [...query.matchAll(/prop:([^:=\s]+)(?:=([^\s]+))?/g)];
      
      const hasDomainFilters = isQuery('transferred') || isQuery('tracked') || isQuery('completed') || isQuery('todo') || isQuery('in_progress') ||
          notQuery('transferred') || notQuery('tracked') ||
          pQueryMatch || statusQueryMatch || tagQueryMatch || assignQueryMatch || customPropMatches.length > 0;
      
      if (hasDomainFilters) {
        result = result.filter(t => {
          if (isQuery('transferred') && !t.is_transferred) return false;
          if (notQuery('transferred') && t.is_transferred) return false;
          if (isQuery('tracked') && !t.track_progress) return false;
          if (notQuery('tracked') && t.track_progress) return false;
          
          if (isQuery('completed') && t.status !== 'done') return false;
          if (isQuery('todo') && t.status !== 'todo') return false;
          if (isQuery('in_progress') && t.status !== 'in_progress') return false;
          
          if (pQueryMatch && t.priority !== `P${pQueryMatch[1]}`) return false;
          if (statusQueryMatch && t.status !== statusQueryMatch[1]) return false;
          
          if (tagQueryMatch) {
             const searchTag = tagQueryMatch[1];
             if (!t.tags.some(tag => tag.toLowerCase() === searchTag || tag.toLowerCase().includes(searchTag))) return false;
          }
          
          if (assignQueryMatch) {
             const searchName = assignQueryMatch[1];
             if (!t.transferred_to?.toLowerCase().includes(searchName)) return false;
          }
          
          for (const match of customPropMatches) {
              const key = match[1];
              const expectedValue = match[2];
              if (!t.custom_fields || t.custom_fields[key] === undefined) return false;
              if (expectedValue && String(t.custom_fields[key]).toLowerCase() !== expectedValue) return false;
          }
          return true;
        });
      }
    }
    return result;
  });

  const categoryCounts = computed(() => {
    const todayStrLocal = getTodayStr();
    
    let all = 0, today = 0, upcoming = 0, someday = 0, transferred = 0;
    
    searchedTasks.value.forEach(t => {
      if (t.status === 'done') return;
      all++;
      if (t.is_transferred) {
        transferred++;
        return;
      }
      
      let isToday = false;
      if (t.due_date && t.due_date <= todayStrLocal) isToday = true;
      else if (t.start_date && t.start_date <= todayStrLocal) isToday = true;
      
      if (isToday) {
        today++;
        return;
      }
      
      let isUpcoming = false;
      if (t.start_date && t.start_date > todayStrLocal) isUpcoming = true;
      else if (t.due_date && t.due_date > todayStrLocal) isUpcoming = true;
      
      if (isUpcoming) upcoming++;
      else someday++;
    });
    
    return { all, today, upcoming, someday, transferred };
  });

  const activeCategoryTasks = computed(() => {
    const today = getTodayStr();
    
    return searchedTasks.value.filter(t => {
      // A saved search covers the whole vault; its query is what narrows it,
      // and that has already been applied above. Bucketing it further would
      // silently drop results the query asked for.
      if (activeCategory.value === 'all' || activeCategory.value.startsWith('filter:')) return true;

      if (activeCategory.value.startsWith('project:')) {
        const projId = activeCategory.value.substring(8);
        return t.project_id === projId;
      }

      if (activeCategory.value === 'transferred') return t.is_transferred;
      if (t.is_transferred) return false; 
      
      // Hide completed tasks from all views except 'today' (only if completed today) and 'all'
      if (t.status === 'done') {
           if (activeCategory.value === 'today') {
               return t.completed_at && t.completed_at.startsWith(today);
           }
           return false;
      }
      
      let isToday = false;
      if (t.due_date && t.due_date <= today) isToday = true;
      else if (t.start_date && t.start_date <= today) isToday = true;

      if (activeCategory.value === 'today') return isToday;
      
      if (isToday) return false; 
      
      let isUpcoming = false;
      if (t.start_date && t.start_date > today) isUpcoming = true;
      else if (t.due_date && t.due_date > today) isUpcoming = true;
      
      if (activeCategory.value === 'upcoming') return isUpcoming;
      
      if (activeCategory.value === 'someday') return !isUpcoming;

      return false;
    });
  });

  return {
    searchQuery, activeCategory, backendSearchIds,
    searchedTasks, categoryCounts, activeCategoryTasks,
  };
}
