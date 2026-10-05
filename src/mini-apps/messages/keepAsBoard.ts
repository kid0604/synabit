/**
 * A diagram out of a conversation, as a board that can be rearranged.
 *
 * # Why a second door
 *
 * `keepAsNote` keeps the drawing. This one keeps the *layout* — and they are
 * not the same thing, because Mermaid's layout is nobody's choice. Two data
 * centres that mirror each other come out lopsided, a core switch lands three
 * rows from the one it talks to, and there is no syntax for "put these two
 * side by side". The person who knows what the picture means cannot fix it.
 *
 * A board stores a position per item, so once the drawing is a board the
 * arranging is ordinary dragging. Mermaid does the first pass; the person does
 * the part Mermaid cannot be told about.
 *
 * The same line applies as everywhere else in this file's neighbourhood: the
 * button is the **app's**, offered because the block is a diagram. It is never
 * something the model asked for. See `keepAsNote` and
 * `docs/syn-the-conversation-2026-09-10.md` §2.
 */
import { invoke } from '@tauri-apps/api/core';
import { boardFromDiagram } from '../../shared/diagramToBoard';
import { newBoardData } from '../whiteboard/boardFile';
import { writeItemChanges } from '../whiteboard/boardWrites';
import { boardText } from '../whiteboard/boardDisk';
import type { WhiteboardData } from '../whiteboard/boardFile';

/** What the vault hands back when a board is created. */
export interface KeptBoard {
  id: string;
  path: string;
  title: string;
  data: WhiteboardData;
}

/**
 * The board a drawn diagram becomes, written to the vault.
 *
 * The **drawn** diagram: the SVG on screen, not the source text. Mermaid has
 * already decided where everything goes by then, and those decisions are the
 * only reason this is a conversion rather than a fresh layout.
 */
export const boardFromSvg = (svg: string, title: string): WhiteboardData => {
  const data = newBoardData(title);
  const drawn = boardFromDiagram(svg);
  data.nodes = drawn.nodes;
  data.edges = drawn.edges;
  return data;
};

/** Write it, and hand back what is needed to open it. */
export const keepAsBoard = async (
  vaultPath: string,
  svg: string,
  title: string,
): Promise<KeptBoard> => {
  const data = boardFromSvg(svg, title);
  const meta = await invoke<{ id: string; path: string }>('create_whiteboard', {
    vaultPath,
    title,
    tags: [] as string[],
    content: boardText(data),
  });
  return { id: meta.id, path: meta.path, title, data };
};

/**
 * Save what dragging changed, and nothing else.
 *
 * The pane beside the conversation moves boxes; everything else a board can
 * have — colours, shapes, freehand, text — belongs to the Whiteboard app. So
 * this reads the board as it is on disk now and moves the boxes there, rather
 * than writing back the copy the pane opened with: that copy is older than
 * anything the Whiteboard app, Syn or a sync has written since, and writing
 * it whole put their changes back the way they were.
 *
 * A box is moved when its stamp is later than the one on disk, so a box the
 * Whiteboard app moved after the pane did keeps the Whiteboard's place.
 */
export const saveBoard = async (
  vaultPath: string,
  path: string,
  data: WhiteboardData,
): Promise<void> => {
  // Positions only, and only where moved here later than anywhere else. A
  // board that is gone, unreadable or from a newer build is not written at
  // all: written from the pane's copy, a board deleted in the Whiteboard app
  // came back, and half a file was replaced by the pane's older whole one.
  const ok = await writeItemChanges(
    vaultPath,
    path,
    data.nodes.map((n) => ({ id: n.id, position: n.position, updated: n.updated ?? 0 })),
  );
  if (!ok) throw new Error(`${path} could not be saved from the pane: it is gone, unreadable or from a newer build`);
};

/**
 * Which board an answer is about, read off what its tools reported doing.
 *
 * # Why not the link in the prose
 *
 * That was the first attempt, and it failed on the first real answer. A
 * `[[link]]` carries a title; a title has to be looked up; and the title in
 * question was "Kiến trúc 2 Data Center (DC 1 - DC 2)", with brackets and
 * dashes that the search did not match. The tools name the file they worked
 * on, which needs no lookup and cannot be ambiguous.
 *
 * # Why a reading is not enough to be shown
 *
 * Because the second attempt showed the wrong boards. An answer that changed
 * one board had, on its way there, read two others while working out which
 * board was which — and "the last two files mentioned" were those two. What
 * somebody wants to see under an answer is what the answer *did*: a board it
 * drew or changed. A board it only read is shown when the answer did nothing
 * else, because then reading it was the point.
 */
export const boardsTouchedBy = (
  log: Array<{ tool_name?: string; result_preview?: string }> | null | undefined,
  limit = 2,
): string[] => {
  const path = (call: { result_preview?: string }): string | null =>
    /Whiteboards\/[^"'\s\\]+\.whiteboard\.json/.exec(call.result_preview ?? '')?.[0] ?? null;

  const written: string[] = [];
  const read: string[] = [];
  for (const call of log ?? []) {
    const where = /^(draw|edit)_board$/.test(call.tool_name ?? '')
      ? written
      : call.tool_name === 'read_board'
        ? read
        : null;
    const found = where && path(call);
    if (!where || !found) continue;
    // Kept once, at its last mention: a board drawn and then changed again is
    // one board, and the order somebody reads it in is the order it was left.
    const already = where.indexOf(found);
    if (already !== -1) where.splice(already, 1);
    where.push(found);
  }

  return (written.length ? written : read).slice(-limit);
};
