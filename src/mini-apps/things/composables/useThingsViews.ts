import { ref, computed } from 'vue';
import { useNodeService } from '../../../composables/useNodeService';
import { logger } from '../../../utils/logger';

/**
 * A way of looking at the vault, kept.
 *
 * A node in `Views/`, like everything else — so it syncs between devices, goes
 * into git, can be opened and corrected in any editor, and shows up in the
 * graph. None of that had to be built; it comes from being a file.
 *
 * `type: view` rather than reusing `filter`. Tasks already saves `filter`
 * nodes and lists every one of them in its own sidebar, so a view saved here
 * would appear in the Tasks filter menu offering a query about books. The
 * prime directive of this app is that the other eleven do not change, and
 * sharing a type would have changed one.
 */
export interface SavedView {
  /** The node's path, which is its id. */
  id: string;
  name: string;
  /** The filter half, as typed. Arrangement is stored separately. */
  query: string;
  layout: 'list' | 'table';
  sort: string;
  sortDescending: boolean;
  group: string;
  columns: string[];
  /**
   * How prominent this view is.
   *
   * `things` lives in this app's rail; `sidebar` gets an entry beside Notes
   * and Tasks. One field, and it is the whole ladder — climbing a rung is
   * editing a value, not learning a new concept.
   */
  home: 'things' | 'sidebar';
  icon?: string;
}

const LAYOUTS = ['list', 'table'] as const;
const HOMES = ['things', 'sidebar'] as const;

/**
 * Read a view out of a node, trusting none of it.
 *
 * A view is a file, so its fields can say anything — hand-edited, merged
 * between two devices, written by an older version. A layout this app does not
 * have would leave the pane blank; a `home` it does not know would lose the
 * view entirely. Each one falls back rather than propagates.
 *
 * Lifted wholesale from `useTaskFilters.toFilter`, which learned it first.
 */
function toView(node: any): SavedView {
  const p = node.properties ?? {};
  const oneOf = <T extends string>(value: unknown, allowed: readonly T[], fallback: T): T =>
    typeof value === 'string' && (allowed as readonly string[]).includes(value)
      ? (value as T)
      : fallback;

  return {
    id: node.id,
    name: node.title || 'Untitled',
    query: typeof p.query === 'string' ? p.query : '',
    layout: oneOf(p.layout, LAYOUTS, 'list'),
    sort: typeof p.sort === 'string' ? p.sort : 'updated_at',
    sortDescending: p.sort_descending !== false,
    group: typeof p.group === 'string' ? p.group : '',
    columns: Array.isArray(p.columns) ? p.columns.filter((c: unknown) => typeof c === 'string') : [],
    home: oneOf(p.home, HOMES, 'things'),
    icon: typeof p.icon === 'string' ? p.icon : undefined,
  };
}

export function useThingsViews() {
  const ns = useNodeService();
  const views = ref<SavedView[]>([]);

  const load = async () => {
    try {
      const nodes = await ns.getNodeSummaries('view');
      views.value = (nodes as any[]).map(toView).sort((a, b) => a.name.localeCompare(b.name));
    } catch (e) {
      logger.error('[Things] Could not read saved views', e);
      views.value = [];
    }
  };

  const write = async (view: SavedView, eventType?: 'created') => {
    await ns.writeNode({
      relPath: view.id,
      nodeType: 'view',
      title: view.name,
      properties: {
        query: view.query,
        layout: view.layout,
        sort: view.sort,
        sort_descending: view.sortDescending,
        group: view.group,
        columns: view.columns,
        home: view.home,
      },
      // No body. Whatever someone wrote in the file — a note to themselves
      // about what this view is for — is left alone.
      eventType,
    });
  };

  const save = async (draft: Omit<SavedView, 'id'>): Promise<SavedView | null> => {
    const view: SavedView = { ...draft, id: `Views/${crypto.randomUUID()}.md` };
    try {
      await write(view, 'created');
      views.value = [...views.value, view].sort((a, b) => a.name.localeCompare(b.name));
      return view;
    } catch (e) {
      logger.error('[Things] Could not save the view', e);
      return null;
    }
  };

  /** Move a view between the rail and the sidebar. The whole ladder. */
  const setHome = async (view: SavedView, home: SavedView['home']) => {
    const moved = { ...view, home };
    try {
      await write(moved);
      const at = views.value.findIndex(v => v.id === view.id);
      if (at >= 0) views.value[at] = moved;
    } catch (e) {
      logger.error('[Things] Could not move the view', e);
    }
  };

  const remove = async (view: SavedView) => {
    try {
      // Trash rather than unlink. A view is somebody's arrangement of their
      // own vault, and the gesture that loses it is a mis-aimed click.
      await ns.trashNode({ relPath: view.id });
      views.value = views.value.filter(v => v.id !== view.id);
    } catch (e) {
      logger.error('[Things] Could not delete the view', e);
    }
  };

  const pinned = computed(() => views.value.filter(v => v.home === 'sidebar'));

  return { views, pinned, load, save, setHome, remove };
}
