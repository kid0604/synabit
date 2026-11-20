/**
 * A mind map made a board of its own, from outside the Whiteboard app: a
 * markmap block in a note opened as one, so that the note and the board share
 * one map rather than each keeping a copy.
 */
import { invoke } from '@tauri-apps/api/core';
import { newBoardData, stampElement } from './boardFile';
import { boardText } from './boardDisk';
import { buildMindmap, MINDMAP_COLORS, type OutlineItem } from './mindmap';

const newId = (prefix: string) => `${prefix}_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;

/** Write the board; hand back where it is, for an embed or a link to it. */
export async function createMindmapBoard(
  vaultPath: string,
  tree: OutlineItem,
  title: string,
): Promise<{ id: string; path: string; title: string }> {
  const data = newBoardData(title);
  const map = buildMindmap(tree, { x: 0, y: 0 }, MINDMAP_COLORS, newId);
  for (const n of map.nodes) stampElement(n);
  for (const e of map.edges) stampElement(e);
  data.nodes = map.nodes;
  data.edges = map.edges;
  const meta = await invoke<{ id: string; path: string }>('create_whiteboard', {
    vaultPath,
    title,
    tags: [] as string[],
    content: boardText(data),
  });
  return { id: meta.id, path: meta.path, title };
}
