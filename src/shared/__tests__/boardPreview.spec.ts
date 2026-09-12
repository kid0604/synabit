import { describe, it, expect } from 'vitest';
import { boardPreview } from '../boardPreview';
import type { WhiteboardData, WBNode } from '../../mini-apps/whiteboard/boardFile';

const shape = (id: string, x: number, y: number, w: number, h: number, label: string): WBNode => ({
  id,
  type: 'shape',
  position: { x, y },
  data: { shapeType: 'rectangle', label, width: w, height: h },
});

const board = (nodes: WBNode[], edges: WhiteboardData['edges'] = []): WhiteboardData => ({
  schemaVersion: 1,
  title: 'Kiến trúc 2 DC',
  tags: [],
  created_at: '2026-09-12T00:00:00Z',
  viewport: { x: 0, y: 0, zoom: 1 },
  nodes,
  edges,
});

/**
 * "I drew you a board" followed by a link is three actions before anybody sees
 * it: click, wait for the Whiteboard app to mount, come back. The picture is
 * rectangles and lines — it belongs in the answer.
 */
describe('a board, small, under the answer', () => {
  const frame = shape('f1', 0, 0, 400, 300, 'DC1');
  const one = shape('a', 40, 60, 160, 56, 'SW Core');
  const two = shape('b', 40, 160, 160, 56, 'F5 TPZ');
  const drawn = boardPreview(
    board([frame, one, two], [
      { id: 'e1', source: 'a', target: 'b', type: 'default', data: { label: 'trunk' } },
    ]),
  );

  it('draws every box, and the line between them', () => {
    expect(drawn.match(/<rect/g), 'three boxes').toHaveLength(3);
    expect(drawn).toContain('<line');
    expect(drawn).toContain('SW Core');
    expect(drawn).toContain('F5 TPZ');
  });

  /** A frame drawn over what it holds hides it. */
  it('puts the frame behind what it holds', () => {
    expect(drawn.indexOf('DC1')).toBeLessThan(drawn.indexOf('SW Core'));
  });

  it('fits the picture to the room a bubble has', () => {
    const viewBox = /viewBox="([-\d. ]+)"/.exec(drawn)?.[1]?.split(' ').map(Number) ?? [];
    expect(viewBox, 'the whole board is inside the view').toHaveLength(4);
    expect(viewBox[2]).toBeGreaterThanOrEqual(400);
    expect(drawn).toMatch(/max-width:\d+px/);
  });

  /**
   * A label the size of a full stop is a smudge pretending to be a word. On a
   * five-thousand-pixel board every name would be one, so below a readable
   * size they are drawn as the line of text they would have been.
   */
  it('draws a bar where a word would be too small to read', () => {
    const far: WBNode[] = [];
    for (let i = 0; i < 12; i++) far.push(shape(`n${i}`, i * 900, i * 300, 200, 56, `Item ${i}`));
    const wide = boardPreview(board(far));

    expect(wide).not.toContain('Item 3');
    expect(wide.match(/<rect/g)?.length, 'a box and a bar each').toBe(24);
  });

  it('says nothing about a board with nothing on it', () => {
    expect(boardPreview(board([]))).toBe('');
  });

  /** A board is somebody's writing, and it goes into markup. */
  it('escapes what is written on a box', () => {
    const risky = boardPreview(board([shape('x', 0, 0, 200, 56, '<script>alert(1)</script>')]));
    expect(risky).not.toContain('<script>');
    expect(risky).toContain('&lt;script&gt;');
  });

  /** Freehand belongs in a preview too: a board without the pen marks on it is
   *  not the board its owner remembers. */
  it('draws what was drawn by hand', () => {
    const scribble: WBNode = {
      id: 's1',
      type: 'stroke',
      position: { x: 10, y: 10 },
      data: { color: '#7c3aed', points: [[0, 0, 0.5], [10, 20, 0.5], [30, 25, 0.5]] },
    };
    expect(boardPreview(board([scribble]))).toContain('<polyline');
  });
});
