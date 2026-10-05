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

/**
 * Finding the board an answer is about.
 *
 * The first version read the `[[link]]` in the prose and looked the title up —
 * and "Kiến trúc 2 Data Center (DC 1 - DC 2)" did not come back from that
 * search. The second read every board the tools mentioned, and showed the two
 * the answer had merely glanced at while working out which board was which.
 */
describe('which board an answer is about', () => {
  /** The real log of the answer that showed the wrong pictures. */
  const flailing = [
    { tool_name: 'read_board', result_preview: 'File: Whiteboards/kien-truc.whiteboard.json\nBoard "Kiến trúc"' },
    { tool_name: 'get_node', result_preview: '{"error":"Node not found","node_id":"Whiteboards/ghost.whiteboard.json"}' },
    { tool_name: 'read_board', result_preview: 'File: Whiteboards/diagram-from-syn.whiteboard.json\nBoard "Diagram from Syn"' },
    { tool_name: 'read_board', result_preview: 'File: Whiteboards/board-2.whiteboard.json\nBoard "board 2"' },
    { tool_name: 'edit_board', result_preview: '{"board":"Whiteboards/kien-truc.whiteboard.json","did":["removed \\"Network Hub\\""]}' },
    { tool_name: 'read_board', result_preview: 'File: Whiteboards/kien-truc.whiteboard.json\nBoard "Kiến trúc"' },
  ];

  it('shows what the answer changed, not what it read on the way', async () => {
    const { boardsTouchedBy } = await import('../../mini-apps/messages/keepAsBoard');
    expect(boardsTouchedBy(flailing)).toEqual(['Whiteboards/kien-truc.whiteboard.json']);
  });

  it('shows a reading when reading was all the answer did', async () => {
    const { boardsTouchedBy } = await import('../../mini-apps/messages/keepAsBoard');
    const reading = flailing.filter(c => c.tool_name === 'read_board').slice(0, 2);
    expect(boardsTouchedBy(reading)).toEqual([
      'Whiteboards/kien-truc.whiteboard.json',
      'Whiteboards/diagram-from-syn.whiteboard.json',
    ]);
  });

  it('counts a board once, however many times it was written', async () => {
    const { boardsTouchedBy } = await import('../../mini-apps/messages/keepAsBoard');
    const twice = [
      { tool_name: 'draw_board', result_preview: '{"board":"Whiteboards/a.whiteboard.json"}' },
      { tool_name: 'edit_board', result_preview: '{"board":"Whiteboards/a.whiteboard.json"}' },
    ];
    expect(boardsTouchedBy(twice)).toEqual(['Whiteboards/a.whiteboard.json']);
  });

  it('has nothing to show for an answer that touched none', async () => {
    const { boardsTouchedBy } = await import('../../mini-apps/messages/keepAsBoard');
    expect(boardsTouchedBy([{ tool_name: 'browse', result_preview: 'a page' }])).toEqual([]);
    expect(boardsTouchedBy(null)).toEqual([]);
  });
});

describe('boardPreview draws every kind of item', () => {
  it('draws sticky notes, words on their own, and frames', () => {
    const svg = boardPreview(board([
      { id: 'f', type: 'frame', position: { x: 0, y: 0 }, data: { label: 'Sprint', width: 600, height: 400 } },
      { id: 's', type: 'sticky', position: { x: 40, y: 40 }, data: { label: 'Ask legal', color: 'pink', width: 200, height: 200 } },
      { id: 't', type: 'text', position: { x: 300, y: 60 }, data: { label: 'Loose words', width: 200 } },
    ]));
    expect(svg).toContain('Sprint');
    expect(svg).toContain('Ask legal');
    expect(svg).toContain('Loose words');
    // The pink paper, not the box colour.
    expect(svg).toContain('#fbcfe8');
  });

  it('has a picture for a board of nothing but writing', () => {
    expect(boardPreview(board([
      { id: 'm', type: 'mindmap', position: { x: 0, y: 0 }, data: { label: 'Root' } },
    ]))).toContain('Root');
  });
});

describe('a preview of handwriting', () => {
  it('keeps only the points it can show, and both ends', () => {
    const points = Array.from({ length: 2000 }, (_, i) => [i * 0.05, 0, 0.5]);
    const stroke = { id: 's', type: 'stroke', position: { x: 0, y: 0 }, data: { points, width: 100, height: 2 } } as WBNode;
    const big = { id: 'b', type: 'shape', position: { x: 0, y: 0 }, data: { width: 4000, height: 3000 } } as WBNode;
    const svg = boardPreview(board([big, stroke]));
    const kept = (svg.match(/<polyline points="([^"]+)"/)?.[1] ?? '').split(' ');
    expect(kept.length).toBeLessThan(200);
    expect(kept[0]).toBe('0.0,0.0');
    expect(kept[kept.length - 1]).toBe('100.0,0.0');
  });
});

describe('a preview of a board from somewhere else', () => {
  it('writes positions and colours as values, never as markup', () => {
    const evil = '0"/><img src=x onerror=alert(1)>';
    const svg = boardPreview(board([
      { id: 'a', type: 'shape', position: { x: evil as any, y: 0 }, data: { label: 'A' } } as WBNode,
      { id: 's', type: 'stroke', position: { x: 0, y: 0 }, data: { color: '"/><script>x</script>', points: [[0, 0], [10, 10]] } } as WBNode,
    ]));
    expect(svg).not.toContain('<img');
    expect(svg).not.toContain('<script');
  });
});
