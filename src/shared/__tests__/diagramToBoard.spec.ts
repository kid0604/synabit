import { describe, it, expect } from 'vitest';
import { boardFromDiagram } from '../diagramToBoard';
import flowchart from './fixtures/flowchart.svg?raw';

/**
 * Mermaid decides where everything goes and takes no instruction about it.
 *
 * For a system drawing that is the whole complaint: two data centres that
 * ought to mirror each other come out lopsided — in this very fixture DC1 sits
 * 129 pixels below DC2 for no reason anybody asked for — and the person who
 * knows what the picture means has no way to move a box.
 *
 * The board does store a position per item. So the conversion is not a
 * redrawing: it is handing over the layout Mermaid already computed, in a
 * form that can be dragged.
 */
describe('a drawn diagram, as a board', () => {
  const board = boardFromDiagram(flowchart);
  const byLabel = (label: string) =>
    board.nodes.find(n => (n.data as { label?: string }).label === label);
  const size = (label: string) => {
    const d = byLabel(label)?.data as { width: number; height: number };
    return d;
  };

  it('brings every box and every group across', () => {
    expect(board.nodes).toHaveLength(7);
    for (const label of ['TCTV', 'F5 TPZ', 'Kong API Gateway', 'DC1 — Nội bộ', 'DC2 — Dự phòng']) {
      expect(byLabel(label), `${label} is missing`).toBeDefined();
    }
  });

  /** The point of the whole conversion: where things are is kept. */
  it('keeps what Mermaid worked out about where things go', () => {
    const kong = byLabel('Kong API Gateway')!;
    const dc = byLabel('DC1 — Nội bộ')!;
    const within = (inner: typeof kong, outer: typeof dc) => {
      const o = outer.data as { width: number; height: number };
      return (
        inner.position.x >= outer.position.x &&
        inner.position.y >= outer.position.y &&
        inner.position.x <= outer.position.x + o.width &&
        inner.position.y <= outer.position.y + o.height
      );
    };
    expect(within(kong, dc), 'a box that was inside its subgraph has left it').toBe(true);

    // Mermaid centres a node on its transform; a board places its corner.
    expect(kong.position).toEqual({ x: 410.9921875 - 185.328125 / 2 + 60, y: 188 - 54 / 2 + 60 });
    expect(size('Kong API Gateway')).toMatchObject({ width: 185.328125, height: 54 });
  });

  /** A frame drawn over its contents hides them. */
  it('puts the groups behind the boxes they hold', () => {
    const firstBox = board.nodes.findIndex(n => (n.data as { color: string }).color === '#7c3aed');
    const lastGroup = board.nodes.map(n => (n.data as { color: string }).color)
      .lastIndexOf('#94a3b8');
    expect(lastGroup).toBeLessThan(firstBox);
  });

  it('keeps the lines, and what was written on them', () => {
    expect(board.edges).toHaveLength(5);

    const id = (label: string) => byLabel(label)?.id;
    const tctv = id('TCTV');
    const fromTctv = board.edges.filter(e => e.source === tctv);
    expect(fromTctv, 'TCTV reaches both data centres').toHaveLength(2);

    const labelled = board.edges.find(e => (e.data as { label?: string })?.label === 'MPLS');
    expect(labelled?.source).toBe(tctv);
  });

  /**
   * A line leaves the side that faces where it is going.
   *
   * Mermaid's own waypoints are not carried over: once a box is dragged they
   * describe a route to where it used to be, which is worse than no route.
   */
  it('leaves each box on the side that faces the other one', () => {
    const kong = board.nodes.find(
      n => (n.data as { label: string }).label === 'Kong API Gateway' && n.position.x > 300,
    )!;
    const down = board.edges.find(e => e.source === kong.id)!;
    expect([down.sourceHandle, down.targetHandle]).toEqual(['bottom', 'top']);
  });

  it('has nothing to say about a picture that is not one', () => {
    expect(boardFromDiagram('not an svg at all')).toEqual({ nodes: [], edges: [] });
  });
});

/**
 * `SW_Core` is a perfectly ordinary name for a switch, and Mermaid names the
 * edge between two of them `L_SW_Core_A_SW_Core_B_0`. Split on the first
 * underscore and the diagram loses the connection it was drawn for.
 */
describe('a name with an underscore in it', () => {
  const svg =
    '<svg id="s" xmlns="http://www.w3.org/2000/svg">' +
    '<g class="node" id="s-flowchart-SW_Core_A-0" transform="translate(100, 100)">' +
    '<rect x="-50" y="-25" width="100" height="50"></rect>' +
    '<g class="label"><foreignObject><div><p>SW Core A</p></div></foreignObject></g></g>' +
    '<g class="node" id="s-flowchart-SW_Core_B-1" transform="translate(400, 100)">' +
    '<rect x="-50" y="-25" width="100" height="50"></rect>' +
    '<g class="label"><foreignObject><div><p>SW Core B</p></div></foreignObject></g></g>' +
    '<path data-id="L_SW_Core_A_SW_Core_B_0"></path></svg>';

  it('is still one name', () => {
    const board = boardFromDiagram(svg);
    expect(board.edges).toHaveLength(1);
    const [a, b] = board.nodes;
    expect(board.edges[0]).toMatchObject({ source: a.id, target: b.id });
    expect(
      [board.edges[0].sourceHandle, board.edges[0].targetHandle],
      'and side by side, the line goes across',
    ).toEqual(['right', 'left']);
  });
});

/**
 * A subgraph is a `<g>` with a transform, and what is inside it is placed
 * relative to that.
 *
 * Read as absolute, a real drawing of two data centres came across as three
 * overlapping heaps: forty-four boxes, every one of them within a few pixels
 * of the corner of whichever subgraph held it.
 */
describe('a box inside a subgraph', () => {
  const svg =
    '<svg id="s" xmlns="http://www.w3.org/2000/svg">' +
    '<g class="cluster" id="s-DC"><rect x="1000" y="500" width="400" height="300"></rect>' +
    '<g class="cluster-label"><foreignObject><div><p>DC</p></div></foreignObject></g></g>' +
    '<g transform="translate(1000, 500)">' +
    '<g class="node" id="s-flowchart-A-0" transform="translate(150, 100)">' +
    '<rect x="-50" y="-25" width="100" height="50"></rect>' +
    '<g class="label"><foreignObject><div><p>A</p></div></foreignObject></g></g></g></svg>';

  it('is placed where the subgraph put it, not where its own transform says', () => {
    const board = boardFromDiagram(svg);
    const box = board.nodes.find(n => (n.data as { label: string }).label === 'A')!;
    // 1000 + 150 - 50 + margin, 500 + 100 - 25 + margin.
    expect(box.position).toEqual({ x: 1160, y: 635 });
  });

  /** And a line drawn to the subgraph itself lands on its frame. */
  it('lets a line end on the subgraph', () => {
    const withEdge = svg.replace('</svg>', '<path data-id="L_A_DC_0"></path></svg>');
    const board = boardFromDiagram(withEdge);
    const frame = board.nodes.find(n => (n.data as { label: string }).label === 'DC')!;
    expect(board.edges).toHaveLength(1);
    expect(board.edges[0].target).toBe(frame.id);
  });
});

/**
 * Mermaid draws a database as arcs, a junction as a polygon and a round node
 * as a circle. Reading only rectangles dropped fourteen of one real drawing's
 * forty-four boxes — and on a network diagram the databases are not the parts
 * anybody is willing to lose.
 */
describe('the shapes that are not rectangles', () => {
  const node = (id: string, inner: string) =>
    `<g class="node" id="s-flowchart-${id}-0" transform="translate(200, 200)">${inner}` +
    `<g class="label"><foreignObject><div><p>${id}</p></div></foreignObject></g></g>`;
  const svg =
    '<svg id="s" xmlns="http://www.w3.org/2000/svg">' +
    node('DB', '<path d="M0,11 a53,11 0,0,0 107,0" transform="translate(-53.5, -36)"></path>') +
    node('SW', '<polygon points="9,0 144,0 153,-19 144,-39 9,-39 0,-19" transform="translate(-76,19)"></polygon>') +
    node('NET', '<circle r="50" cx="0" cy="0"></circle>') +
    '</svg>';

  it('come across as themselves', () => {
    const board = boardFromDiagram(svg);
    const shape = (label: string) =>
      board.nodes.find(n => (n.data as { label: string }).label === label)?.data as {
        shapeType: string; width: number; height: number;
      };

    expect(shape('DB')).toMatchObject({ shapeType: 'cylinder', width: 107, height: 72 });
    expect(shape('SW')).toMatchObject({ shapeType: 'hexagon', width: 153, height: 39 });
    expect(shape('NET')).toMatchObject({ shapeType: 'ellipse', width: 100, height: 100 });
  });
});

/**
 * What gets written to the vault is a board file like any other.
 *
 * Not a new format, not a Syn-shaped one: the Whiteboard app opens it from the
 * pane's own button, and a file it could not read would make that button a
 * dead end.
 */
describe('the file a kept diagram becomes', () => {
  it('is a board the Whiteboard app can open', async () => {
    const { boardFromSvg } = await import('../../mini-apps/messages/keepAsBoard');
    const data = boardFromSvg(flowchart, 'Luồng kết nối TCTV');

    expect(data.title).toBe('Luồng kết nối TCTV');
    expect(data.schemaVersion).toBe(1);
    expect(data.nodes).toHaveLength(7);
    expect(data.edges).toHaveLength(5);
    expect(data.viewport).toEqual({ x: 0, y: 0, zoom: 1 });
  });
});
