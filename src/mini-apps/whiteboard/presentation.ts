import type { WBNode } from './boardFile';
import type { Box } from './inkLayer';

export interface Slide {
  /** The frame shown, or null for the whole board. */
  id: string | null;
  title: string;
  box: Box;
}

/**
 * A board's frames in the order they are presented: as they are read, row by
 * row from the top and left to right along each row.
 *
 * A frame belongs to a row when its top is above the middle of the row's
 * first frame — a row of frames drawn by hand is never quite level, and
 * sorting by top alone put a slightly lower frame on the left after its
 * neighbour on the right.
 */
export function slidesOf(nodes: WBNode[]): Slide[] {
  const frames = nodes
    .filter((n) => n.type === 'frame')
    .map((n) => ({
      id: n.id,
      title: String(n.data?.label ?? ''),
      box: { x: n.position.x, y: n.position.y, width: Number(n.data?.width) || 480, height: Number(n.data?.height) || 320 },
    }))
    .sort((a, b) => a.box.y - b.box.y);

  const rows: (typeof frames)[] = [];
  for (const frame of frames) {
    const row = rows[rows.length - 1];
    if (row && frame.box.y < row[0].box.y + row[0].box.height / 2) row.push(frame);
    else rows.push([frame]);
  }
  return rows.flatMap((row) => row.sort((a, b) => a.box.x - b.box.x));
}
