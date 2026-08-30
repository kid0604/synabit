import { ref } from 'vue';
import { useNodeService } from '../../../composables/useNodeService';
import { logger } from '../../../utils/logger';

export interface LinkedNode {
  id: string;
  node_type: string;
  title: string;
  preview?: string;
  updated_at?: string;
}

/**
 * What else in the vault points at the node on screen.
 *
 * `node_edges` records every link between nodes and has done since before
 * tasks had subtasks, so this works for a `book` and an `animal` without
 * anything being taught about either. Edges are recorded between stable
 * identities rather than paths, so a node keeps its backlinks through a
 * rename or a move.
 *
 * Loaded when a node is opened, never with the list. One query for the one
 * node someone is looking at; two hundred queries to draw a panel nobody has
 * opened is the version of this that does not ship.
 */
export function useThingsLinks() {
  const ns = useNodeService();
  const backlinks = ref<LinkedNode[]>([]);
  const loading = ref(false);

  /**
   * A request that has been overtaken is dropped rather than applied.
   *
   * Arrowing down a list issues one query per node and they do not come back
   * in the order they left. Without this the panel shows the previous node's
   * links under the current node's title, which reads as a wrong answer rather
   * than a late one.
   */
  let token = 0;

  const load = async (id: string, title: string) => {
    const mine = ++token;
    loading.value = true;
    try {
      const found = await ns.getLinkedNodes(title, id);
      if (mine !== token) return;
      backlinks.value = (found ?? []) as LinkedNode[];
    } catch (e) {
      if (mine !== token) return;
      logger.error('[Things] Could not read backlinks', e);
      backlinks.value = [];
    } finally {
      if (mine === token) loading.value = false;
    }
  };

  const clear = () => {
    token++;
    backlinks.value = [];
  };

  return { backlinks, loading, load, clear };
}
