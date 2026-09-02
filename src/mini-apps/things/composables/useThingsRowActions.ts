import { ref, onScopeDispose } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { useNodeService } from '../../../composables/useNodeService';
import { folderForType } from '../../../shared/nodeRoutes';
import { logger } from '../../../utils/logger';

/**
 * What a row's menu actually does.
 *
 * Every one of these works on a node of any kind, which is the test for
 * belonging here at all: pinning and locking are real actions and they are
 * note-shaped, so they stayed in the Notes menu.
 */
/**
 * How long the offer to put a node back stays up.
 *
 * The node is already in `.trash/` by the time the offer appears, so this is
 * not a delay before the delete — it is the window in which undoing costs one
 * click rather than a trip to the trash panel. The toast's countdown bar reads
 * the same number, so what the bar shows and what the offer does agree.
 */
export const UNDO_WINDOW_SECONDS = 8;

export function useThingsRowActions(vaultPath: () => string) {
  const ns = useNodeService();

  /** The node waiting to be really gone, and where it went. */
  const trashed = ref<{ title: string; trashPath: string; key: string } | null>(null);

  /**
   * Move a node into `.trash/` and offer it back for a while.
   *
   * Trash rather than unlink: a node is usually the only copy of something,
   * and the gesture that loses it is a mis-aimed click on a small icon in a
   * menu. The move is a rename within one filesystem, so the node is never in
   * neither place.
   */
  /** Ends the offer, whether it was taken, replaced, or simply ran out. */
  let timer: ReturnType<typeof setTimeout> | undefined;

  const dismissUndo = () => {
    clearTimeout(timer);
    trashed.value = null;
  };

  const remove = async (id: string, title: string) => {
    try {
      const trashPath = await ns.trashNode({ relPath: id });
      trashed.value = { title, trashPath, key: id };
      // The offer has to end by itself. Without this the toast stayed on
      // screen long after its countdown bar had emptied, which said the undo
      // was still there when the bar said it was gone.
      clearTimeout(timer);
      timer = setTimeout(dismissUndo, UNDO_WINDOW_SECONDS * 1000);
    } catch (e) {
      logger.error('[Things] Could not delete', e);
    }
  };

  const undoRemove = async () => {
    const waiting = trashed.value;
    if (!waiting) return;
    dismissUndo();
    try {
      await invoke('restore_from_trash', {
        vaultPath: vaultPath(),
        trashPath: waiting.trashPath,
      });
    } catch (e) {
      logger.error('[Things] Could not restore', e);
    }
  };

  // A timer outliving the screen would write to a ref nothing is reading.
  onScopeDispose(() => clearTimeout(timer));

  /**
   * Pin a node, or let it go.
   *
   * A patch naming one key, which is the whole reason this is safe on a kind
   * nobody wrote code for: `pinned` is set and every other key on the file
   * goes unmentioned, so a `book` keeps its author and a task keeps its
   * status. Writing the node back whole would be how a pin loses a field.
   *
   * The type comes from the node rather than a constant, for the reason
   * `nodeRoutes` gives: a writer that decides a node's type for itself is how
   * a task opened elsewhere was saved as a note.
   */
  const setPinned = async (id: string, pinned: boolean) => {
    try {
      const node = await ns.getNode(id);
      if (!node) return;
      await ns.writeNode({
        relPath: id,
        nodeType: node.node_type,
        title: node.title ?? '',
        properties: { pinned },
      });
    } catch (e) {
      logger.error('[Things] Could not pin', e);
    }
  };

  /**
   * Copy the node's own id, which is its path in the vault.
   *
   * The thing worth having on the clipboard: it is what a query returns, what
   * a link points at, and what to open in an editor outside the app.
   */
  const copyPath = async (id: string) => {
    try {
      await navigator.clipboard.writeText(id);
    } catch (e) {
      logger.warn('[Things] Could not copy the path', e);
    }
  };

  /**
   * Make a second one, carrying everything the first had.
   *
   * The whole node, properties included — a duplicate that dropped the fields
   * would be a new blank node with a familiar name, which is not what anybody
   * means by duplicate. `node_id` is left out deliberately: the copy is a
   * different thing and must not claim the original's identity, or sync sees
   * one document in two places.
   */
  const duplicate = async (id: string): Promise<string | null> => {
    try {
      const node = await ns.getNode(id);
      if (!node) return null;

      const { node_id: _identity, ...properties } = (node.properties ?? {}) as Record<string, unknown>;
      const relPath = `${folderForType(node.node_type)}/${crypto.randomUUID()}.md`;

      await ns.writeNode({
        relPath,
        // From the node, never a constant.
        nodeType: node.node_type,
        title: node.title ? `${node.title} (copy)` : '',
        properties,
        content: node.content ?? '',
        eventType: 'created',
      });
      return relPath;
    } catch (e) {
      logger.error('[Things] Could not duplicate', e);
      return null;
    }
  };

  return { trashed, remove, undoRemove, dismissUndo, copyPath, duplicate, setPinned };
}
