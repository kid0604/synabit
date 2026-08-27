import { ref, type Ref } from 'vue';
import { isSortMode, isGroupMode, type SortMode, type GroupMode } from '../sorting';
import { logger } from '../../../utils/logger';

/**
 * Searches worth keeping.
 *
 * A saved filter is a node in the vault — `Filters/<uuid>.md` — rather than an
 * entry in the app's settings. Three things follow from that, and all three are
 * the reason for it: it syncs between devices like everything else, it is a
 * file the user can open and edit by hand, and it survives a reinstall because
 * it was never in the app to begin with.
 *
 * What it stores is a query string, and that string is now worth storing: since
 * the Tasks search moved onto `run_node_query`, a filter can say
 * `due_date:<2026-09-01 #work sort:-priority` rather than only the handful of
 * shorthands the browser used to understand.
 *
 * The arrangement travels with it. "Overdue at work" almost certainly wants a
 * particular view and grouping, and a filter that restored the query but not
 * the layout would need adjusting by hand every time it was opened.
 */
export interface SavedFilter {
  /** The node's path, which is its id. */
  id: string;
  name: string;
  query: string;
  viewMode: 'list' | 'board' | 'table' | 'matrix';
  sort: SortMode;
  group: GroupMode;
}

const VIEW_MODES = ['list', 'board', 'table', 'matrix'] as const;
const isViewMode = (v: unknown): v is SavedFilter['viewMode'] =>
  typeof v === 'string' && (VIEW_MODES as readonly string[]).includes(v);

/**
 * Read a filter out of a node, refusing to trust any of it.
 *
 * A filter is a file, so its fields can say anything — hand-edited, merged
 * between two devices, written by an older version. A view mode the app does
 * not have would leave the screen blank; a sort it does not know would throw
 * in the comparator. Each one falls back rather than propagates.
 */
function toFilter(node: any): SavedFilter {
  const props = node.properties ?? {};
  return {
    id: node.id,
    name: node.title || 'Untitled',
    query: typeof props.query === 'string' ? props.query : '',
    viewMode: isViewMode(props.view_mode) ? props.view_mode : 'list',
    sort: isSortMode(String(props.sort ?? '')) ? (props.sort as SortMode) : 'updated',
    group: isGroupMode(String(props.group ?? '')) ? (props.group as GroupMode) : 'none',
  };
}

export function useTaskFilters(ns: any, showToast: (msg: string) => void, t: (k: string, n?: any) => string) {
  const filters = ref<SavedFilter[]>([]);

  const load = async () => {
    try {
      const nodes = await ns.getNodeSummaries('filter');
      filters.value = (nodes as any[]).map(toFilter)
        .sort((a, b) => a.name.localeCompare(b.name));
    } catch (e) {
      logger.error('Failed to load saved filters', e);
      filters.value = [];
    }
  };

  const write = async (filter: SavedFilter, eventType?: 'created') => {
    await ns.writeNode({
      relPath: filter.id,
      nodeType: 'filter',
      title: filter.name,
      properties: {
        query: filter.query,
        view_mode: filter.viewMode,
        sort: filter.sort,
        group: filter.group,
      },
      // A filter has no body. Saying nothing about it leaves whatever a user
      // wrote in the file — a note to themselves about what the filter is for.
      eventType,
    });
  };

  /** Keep the current search, with the arrangement it is being viewed in. */
  const save = async (draft: Omit<SavedFilter, 'id'>) => {
    const filter: SavedFilter = { ...draft, id: `Filters/${crypto.randomUUID()}.md` };
    try {
      await write(filter, 'created');
      filters.value = [...filters.value, filter].sort((a, b) => a.name.localeCompare(b.name));
      showToast(t('task.filter_saved', { name: filter.name }));
      return filter;
    } catch (e) {
      logger.error('Failed to save a filter', e);
      showToast(t('task.filter_save_failed'));
      return null;
    }
  };

  const rename = async (filter: SavedFilter, name: string) => {
    const trimmed = name.trim();
    if (!trimmed || trimmed === filter.name) return;
    const previous = filter.name;
    filter.name = trimmed;
    try {
      await write(filter);
      filters.value = [...filters.value].sort((a, b) => a.name.localeCompare(b.name));
    } catch (e) {
      logger.error('Failed to rename a filter', e);
      filter.name = previous;
      showToast(t('task.filter_save_failed'));
    }
  };

  /**
   * To the trash, like every other node.
   *
   * A filter holds no work, so losing one costs only the typing — but it is
   * still a file the user made, and the app has one answer for those.
   */
  const remove = async (filter: SavedFilter) => {
    try {
      await ns.trashNode({ relPath: filter.id });
      filters.value = filters.value.filter(f => f.id !== filter.id);
    } catch (e) {
      logger.error('Failed to delete a filter', e);
      showToast(t('task.filter_delete_failed'));
    }
  };

  const byId = (id: string): SavedFilter | undefined =>
    filters.value.find(f => f.id === id);

  return { filters: filters as Ref<SavedFilter[]>, load, save, rename, remove, byId };
}
