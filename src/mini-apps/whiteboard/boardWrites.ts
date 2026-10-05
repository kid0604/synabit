import { emit } from '@tauri-apps/api/event';
import { readBoardFile } from './boardFile';
import { boardText, rewriteBoard } from './boardDisk';

/** What one item had changed by an editor: only the fields it touched. */
export interface ItemChange {
  id: string;
  position?: { x: number; y: number };
  /** Only the keys of `data` that were changed. */
  data?: Record<string, unknown>;
  updated: number;
}

/**
 * Write some changes to a board's items back to its file, and nothing else.
 *
 * For editors of a board that are not the Whiteboard app — the board shown
 * inside a note. They hold a copy that was current when they opened, and
 * writing any of that copy back would undo whatever changed the file since:
 * a box renamed by Syn, recoloured in the app, while it is dragged here. So
 * the file is read again, and each change is laid over the item as it is now
 * — the fields this editor touched, nothing more — and only where this change
 * is later than the item's own last change.
 *
 * The write says which version it was made from (see `boardDisk.ts`), so a
 * write from elsewhere in between is read in rather than written over.
 *
 * Returns false, writing nothing, when the file is gone, unreadable, or was
 * saved by a newer build.
 */
export async function writeItemChanges(vaultPath: string, path: string, changes: ItemChange[]): Promise<boolean> {
  if (!changes.length) return true;
  const byId = new Map(changes.map((c) => [c.id, c]));
  let wrote: string | null;
  try {
    wrote = await rewriteBoard(vaultPath, path, (raw) => {
      const read = readBoardFile(raw);
      if (!read.ok) return null;
      const board = read.data;
      board.nodes = board.nodes.map((n) => {
        const ours = byId.get(n.id);
        if (!ours || ours.updated <= (n.updated ?? 0)) return n;
        return {
          ...n,
          ...(ours.position ? { position: { ...ours.position } } : {}),
          data: ours.data ? { ...n.data, ...ours.data } : n.data,
          updated: ours.updated,
        };
      });
      board.metadata = { ...(board.metadata ?? {}), updated_at: new Date().toISOString() };
      return boardText(board);
    });
  } catch {
    return false;
  }
  if (wrote === null) return false;
  await emit('whiteboard-updated', { path, id: path });
  return true;
}

/**
 * Change which projects a board is linked to, from outside the Whiteboard app
 * (a project's resource list).
 *
 * Through the same door as every other writer: read, change, write only if no
 * one wrote in between. The change is stamped (`metadata.stamps`), so a sync
 * settles it by when it was made — an unlink made here is not undone by the
 * other device's copy, which still has the link, merely because that copy was
 * saved later.
 *
 * Returns false, writing nothing, when the board is gone, unreadable, from a
 * newer build, or already as asked.
 */
export async function changeBoardLinks(
  vaultPath: string,
  path: string,
  change: (links: string[]) => string[],
): Promise<boolean> {
  let wrote: string | null;
  try {
    wrote = await rewriteBoard(vaultPath, path, (raw) => {
      const read = readBoardFile(raw);
      if (!read.ok) return null;
      const board = read.data;
      const before: string[] = Array.isArray(board.metadata?.linked_projects) ? board.metadata!.linked_projects : [];
      const after = change([...before]);
      if (after.length === before.length && after.every((l, i) => l === before[i])) return null;
      const now = Date.now();
      board.metadata = {
        ...(board.metadata ?? {}),
        linked_projects: after,
        updated_at: new Date(now).toISOString(),
        stamps: { ...(board.metadata?.stamps ?? {}), 'metadata.linked_projects': now },
      };
      return boardText(board);
    });
  } catch {
    return false;
  }
  return wrote !== null;
}
