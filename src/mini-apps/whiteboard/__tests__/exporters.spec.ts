import { describe, expect, it } from 'vitest';
import type { WBEdge, WBNode } from '../boardFile';
import { toDrawio, toExcalidraw } from '../exporters';
import { fromDrawio, fromExcalidraw } from '../importers';

const node = (id: string, type: WBNode['type'], x: number, y: number, data: Record<string, any>): WBNode =>
  ({ id, type, position: { x, y }, data, updated: 1000 });
const edge = (id: string, source: string, target: string, type: string, data: Record<string, any>): WBEdge =>
  ({ id, source, target, type, data, updated: 1000 });

const GLYPH = { set: 'lucide', name: 'server', viewBox: [0, 0, 24, 24], parts: [['rect', { x: 2, y: 2, width: 20, height: 8 }], ['path', { d: 'M6 6h.01' }]] };

const board = {
  title: 'Plan <A&B>',
  nodes: [
    node('a', 'shape', 0, 0, { shapeType: 'rectangle', label: 'API', width: 160, height: 80, color: '#e03131', fillColor: '#a5d8ff', dashStyle: 'dashed', fontSize: 18, borderWidth: 3 }),
    node('b', 'shape', 400, 0, { shapeType: 'ellipse', label: 'DB', width: 120, height: 120, color: '#1e1e1e' }),
    node('c', 'shape', 0, 300, { shapeType: 'roundedRect', label: 'Cache', width: 160, height: 80, rotation: 30 }),
    node('d', 'shape', 400, 300, { shapeType: 'diamond', label: 'ok?', width: 100, height: 100 }),
    node('hex', 'shape', 800, 0, { shapeType: 'hexagon', label: 'Hex', width: 120, height: 100 }),
    node('star', 'shape', 800, 300, { shapeType: 'star', label: 'Star', width: 100, height: 100, fillColor: '#ffec99' }),
    node('t', 'text', 0, 600, { label: '**Bold** words', fontSize: 20, width: 300, color: '#2f9e44' }),
    node('f', 'frame', -50, 1000, { label: 'Area', width: 600, height: 400, color: '#7048e8' }),
    node('inner', 'shape', 0, 1100, { shapeType: 'rectangle', label: 'Inside', width: 160, height: 80 }),
    node('note', 'comment', 0, 0, { label: 'remember' }),
  ],
  edges: [
    edge('e1', 'a', 'b', 'default', { markerEnd: 'arrow', markerStart: 'diamond-open', label: 'calls', color: '#1971c2', strokeWidth: 3, dashStyle: 'dashed' }),
    edge('e2', 'c', 'd', 'step', { markerEnd: 'circle', markerStart: 'none', waypoints: [{ x: 250, y: 340 }, { x: 250, y: 350 }] }),
    edge('e3', 'a', 'hex', 'straight', { markerEnd: 'er-many', markerStart: 'er-one' }),
    edge('e4', 'a', 'note', 'default', { markerEnd: 'arrow' }),
  ],
};

const byId = (nodes: WBNode[], id: string) => nodes.find((n) => n.id === id)!;

describe('toDrawio', () => {
  it('writes a file the importer reads back: kinds, places, sizes, words, colours', async () => {
    const back = (await fromDrawio(await toDrawio(board)))!;
    expect(byId(back.nodes, 'a')).toMatchObject({
      type: 'shape', position: { x: 0, y: 0 },
      data: { shapeType: 'rectangle', label: 'API', width: 160, height: 80, color: '#e03131', fillColor: '#a5d8ff', dashStyle: 'dashed', fontSize: 18 },
    });
    expect(byId(back.nodes, 'b').data).toMatchObject({ shapeType: 'ellipse', label: 'DB', width: 120, height: 120 });
    expect(byId(back.nodes, 'b').data.fillColor).toBeUndefined();
    expect(byId(back.nodes, 'c').data).toMatchObject({ shapeType: 'roundedRect', rotation: 30 });
    expect(byId(back.nodes, 'd').data.shapeType).toBe('diamond');
    expect(byId(back.nodes, 'hex').data).toMatchObject({ shapeType: 'hexagon', label: 'Hex' });
    expect(byId(back.nodes, 't')).toMatchObject({ type: 'text', position: { x: 0, y: 600 }, data: { label: 'Bold words', fontSize: 20, width: 300, color: '#2f9e44' } });
  });

  it('puts what a frame holds in it, and it comes back where it was', async () => {
    const text = await toDrawio(board);
    expect(text).toMatch(/id="inner"[^>]*parent="f"/);
    const back = (await fromDrawio(text))!;
    expect(byId(back.nodes, 'f')).toMatchObject({ type: 'frame', position: { x: -50, y: 1000 }, data: { label: 'Area', width: 600, height: 400, color: '#7048e8' } });
    expect(byId(back.nodes, 'inner').position).toEqual({ x: 0, y: 1100 });
  });

  it('writes lines with their ends, bends, label and look', async () => {
    const back = (await fromDrawio(await toDrawio(board)))!;
    const e1 = back.edges.find((e) => e.id === 'e1')!;
    expect(e1).toMatchObject({ source: 'a', target: 'b', type: 'default', data: { markerEnd: 'arrow', markerStart: 'diamond-open', label: 'calls', color: '#1971c2', strokeWidth: 3, dashStyle: 'dashed' } });
    const e2 = back.edges.find((e) => e.id === 'e2')!;
    expect(e2).toMatchObject({ type: 'step', data: { markerEnd: 'circle', markerStart: 'none', waypoints: [{ x: 250, y: 340 }, { x: 250, y: 350 }] } });
    expect(back.edges.find((e) => e.id === 'e3')!.data).toMatchObject({ markerEnd: 'er-many', markerStart: 'er-one' });
  });

  it('leaves comments, and lines to them, out', async () => {
    const text = await toDrawio(board);
    expect(text).not.toContain('remember');
    expect(text).not.toContain('id="e4"');
  });

  it('draws a shape draw.io lacks as a stencil of its outline', async () => {
    const text = await toDrawio(board);
    const doc = new DOMParser().parseFromString(text, 'application/xml');
    const style = doc.querySelector('mxCell[id="star"]')!.getAttribute('style')!;
    const packed = /shape=stencil\(([^)]+)\)/.exec(style)![1];
    const bytes = Uint8Array.from(atob(packed), (c) => c.charCodeAt(0));
    const stream = new Response(bytes).body!.pipeThrough(new DecompressionStream('deflate-raw'));
    const stencil = decodeURIComponent(await new Response(stream).text());
    expect(stencil).toMatch(/^<shape w="96" h="96" aspect="variable"/);
    expect(stencil).toContain('<move x="48" y="1"/>');
    expect(stencil).toContain('<close/>');
    expect(style).toContain('fillColor=#ffec99');
    // The importer has no stencils: it comes back as a box, in its place.
    const back = (await fromDrawio(text))!;
    expect(byId(back.nodes, 'star')).toMatchObject({ position: { x: 800, y: 300 }, data: { label: 'Star', width: 100, height: 100 } });
  });

  it('escapes what would break the XML, and the words come back exactly', async () => {
    const text = await toDrawio({ nodes: [node('x', 'shape', 0, 0, { shapeType: 'rectangle', label: 'a <b> & "c"\nnext' })], edges: [], title: 'T <&>' });
    expect(new DOMParser().parseFromString(text, 'application/xml').querySelector('parsererror')).toBeNull();
    expect(text).toContain('name="T &lt;&amp;&gt;"');
    const back = (await fromDrawio(text))!;
    expect(back.nodes[0].data.label).toBe('a <b> & "c"\nnext');
  });

  it('writes icons and pictures as image cells, in draw.io\'s data-URI form', async () => {
    const text = await toDrawio({
      nodes: [
        node('g', 'shape', 0, 0, { shapeType: 'glyph', glyph: GLYPH, label: 'Server', width: 64, height: 64, color: '#1971c2' }),
        node('img', 'image', 100, 0, { assetPath: 'assets/x.png', width: 200, height: 100 }),
        node('missing', 'image', 400, 0, { assetPath: 'assets/gone.png', width: 200, height: 100 }),
      ],
      edges: [],
    }, { imageData: async (p) => (p === 'assets/x.png' ? 'data:image/png;base64,iVBORw0KGgo=' : null) });
    const doc = new DOMParser().parseFromString(text, 'application/xml');
    const glyph = doc.querySelector('mxCell[id="g"]')!;
    expect(glyph.getAttribute('value')).toBe('Server');
    const svg = /image=data:image\/svg\+xml,([^;]+)/.exec(glyph.getAttribute('style')!)![1];
    expect(atob(svg)).toContain('stroke="#1971c2"');
    expect(doc.querySelector('mxCell[id="img"]')!.getAttribute('style')).toContain('image=data:image/png,iVBORw0KGgo=;');
    expect(text).not.toContain(';base64');
    expect(doc.querySelector('mxCell[id="missing"]')!.getAttribute('value')).toBe('gone.png');
  });

  it('writes ink as a stencil of its outline, filled with its colour', async () => {
    const text = await toDrawio({
      nodes: [node('s', 'stroke', 10, 20, { svgPath: 'M 1,1 Q 5,5 10,1 T 20,1 Z', points: [[1, 1, 0.5], [20, 1, 0.5]], width: 22, height: 8, color: '#e03131', size: 4, opacity: 0.85 })],
      edges: [],
    });
    expect(text).toMatch(/style="shape=stencil\([^)]+\);strokeColor=none;fillColor=#e03131;opacity=85;/);
    expect(text).toContain('x="10" y="20" width="22" height="8"');
  });

  it('is the same file every time', async () => {
    expect(await toDrawio(board)).toBe(await toDrawio(board));
  });
});

describe('toExcalidraw', () => {
  const read = async (b: Parameters<typeof toExcalidraw>[0] = board, opts: Parameters<typeof toExcalidraw>[1] = {}) => JSON.parse(await toExcalidraw(b, opts));

  it('writes a file the importer reads back: kinds, places, sizes, words, colours', async () => {
    const back = fromExcalidraw(await toExcalidraw(board))!;
    expect(byId(back.nodes, 'a')).toMatchObject({
      type: 'shape', position: { x: 0, y: 0 },
      data: { shapeType: 'rectangle', label: 'API', width: 160, height: 80, color: '#e03131', fillColor: '#a5d8ff', dashStyle: 'dashed', fontSize: 18, borderWidth: 3 },
    });
    expect(byId(back.nodes, 'b').data).toMatchObject({ shapeType: 'ellipse', label: 'DB' });
    expect(byId(back.nodes, 'c').data).toMatchObject({ shapeType: 'roundedRect', rotation: 30 });
    expect(byId(back.nodes, 'd').data.shapeType).toBe('diamond');
    expect(byId(back.nodes, 't')).toMatchObject({ type: 'text', position: { x: 0, y: 600 }, data: { label: 'Bold words', fontSize: 20, width: 300, color: '#2f9e44' } });
    expect(byId(back.nodes, 'f')).toMatchObject({ type: 'frame', position: { x: -50, y: 1000 }, data: { label: 'Area', width: 600, height: 400 } });
    expect(back.nodes.some((n) => n.id === 'note')).toBe(false);
  });

  it('writes lines as bound arrows with their ends and label', async () => {
    const file = await read();
    const back = fromExcalidraw(JSON.stringify(file))!;
    expect(back.edges.find((e) => e.id === 'e1')).toMatchObject({
      source: 'a', target: 'b', data: { markerEnd: 'arrow', markerStart: 'diamond-open', label: 'calls', color: '#1971c2', strokeWidth: 3, dashStyle: 'dashed' },
    });
    expect(back.edges.find((e) => e.id === 'e2')!.data).toMatchObject({ markerEnd: 'circle', waypoints: [{ x: 250, y: 340 }, { x: 250, y: 350 }] });
    expect(back.edges.find((e) => e.id === 'e3')!.data).toMatchObject({ markerEnd: 'er-many', markerStart: 'er-one' });
    expect(back.edges.some((e) => e.id === 'e4')).toBe(false);

    const els = new Map<string, any>(file.elements.map((e: any) => [e.id, e]));
    const e1 = els.get('e1');
    // From the right side of `a` to the left side of `b`.
    expect([e1.x, e1.y]).toEqual([160, 40]);
    expect(e1.points.at(-1)).toEqual([240, 20]);
    expect(els.get('a').boundElements).toEqual(expect.arrayContaining([{ type: 'text', id: 'a-label' }, { type: 'arrow', id: 'e1' }, { type: 'arrow', id: 'e3' }]));
    expect(els.get('e1-label')).toMatchObject({ containerId: 'e1', text: 'calls' });
  });

  it('draws other shapes as an outline polygon with a box holding the words', async () => {
    const file = await read();
    const holder = file.elements.find((e: any) => e.id === 'hex');
    expect(holder).toMatchObject({ type: 'rectangle', strokeColor: 'transparent', x: 800, y: 0, width: 120, height: 100 });
    const outline = file.elements.find((e: any) => e.id === 'hex-outline-0');
    expect(outline).toMatchObject({ type: 'line', polygon: true, groupIds: holder.groupIds });
    // Six corners and back to the start; the corners reach the item's edges.
    expect(outline.points).toHaveLength(7);
    expect(outline.width).toBe(120);
    expect(file.elements.find((e: any) => e.id === 'hex-label')).toMatchObject({ containerId: 'hex', text: 'Hex' });
    expect(file.elements.find((e: any) => e.id === 'star-outline-0').backgroundColor).toBe('#ffec99');
    // Lines join to the box.
    expect(file.elements.find((e: any) => e.id === 'e3').endBinding.elementId).toBe('hex');
  });

  it('gives every element the fields Excalidraw needs', async () => {
    const file = await read();
    expect(file).toMatchObject({ type: 'excalidraw', version: 2, source: 'synabit', appState: { viewBackgroundColor: '#ffffff' } });
    const required = ['id', 'type', 'x', 'y', 'width', 'height', 'angle', 'strokeColor', 'backgroundColor', 'fillStyle', 'strokeWidth', 'strokeStyle',
      'roughness', 'opacity', 'groupIds', 'frameId', 'roundness', 'seed', 'version', 'versionNonce', 'isDeleted', 'boundElements', 'updated', 'link', 'locked'];
    for (const el of file.elements) {
      for (const key of required) expect(el, `${el.id}.${key}`).toHaveProperty(key);
      expect(el.roughness).toBe(0);
      if (el.type === 'text') for (const key of ['text', 'originalText', 'fontSize', 'fontFamily', 'textAlign', 'verticalAlign', 'containerId', 'lineHeight']) expect(el).toHaveProperty(key);
    }
    expect(new Set(file.elements.map((e: any) => e.id)).size).toBe(file.elements.length);
    expect(file.elements.find((e: any) => e.id === 'inner').frameId).toBe('f');
    expect(file.elements.find((e: any) => e.id === 'c').angle).toBeCloseTo(Math.PI / 6);
    // Frames last, after what they hold.
    expect(file.elements.at(-1).id).toBe('f');
  });

  it('writes icons and pictures as images kept in files, and a box for a picture it cannot read', async () => {
    const file = await read({
      nodes: [
        node('g', 'shape', 0, 0, { shapeType: 'glyph', glyph: GLYPH, label: 'Server', width: 64, height: 64 }),
        node('img', 'image', 100, 0, { assetPath: 'assets/x.png', width: 200, height: 100, rotation: 90 }),
        node('missing', 'image', 400, 0, { assetPath: 'assets/gone.png', width: 200, height: 100 }),
      ],
      edges: [],
    }, { imageData: async (p: string) => (p === 'assets/x.png' ? 'data:image/png;base64,iVBORw0KGgo=' : null) });
    const g = file.elements.find((e: any) => e.id === 'g');
    expect(g.type).toBe('image');
    expect(file.files[g.fileId]).toMatchObject({ mimeType: 'image/svg+xml', id: g.fileId });
    expect(file.files[g.fileId].dataURL).toMatch(/^data:image\/svg\+xml;base64,/);
    expect(file.elements.find((e: any) => e.id === 'g-label')).toMatchObject({ text: 'Server', groupIds: g.groupIds });
    const img = file.elements.find((e: any) => e.id === 'img');
    expect(img).toMatchObject({ type: 'image', status: 'saved', angle: Math.PI / 2 });
    expect(file.files[img.fileId]).toMatchObject({ mimeType: 'image/png', dataURL: 'data:image/png;base64,iVBORw0KGgo=' });
    expect(file.elements.find((e: any) => e.id === 'missing')).toMatchObject({ type: 'rectangle', strokeStyle: 'dashed' });
    expect(file.elements.find((e: any) => e.id === 'missing-label').text).toBe('gone.png');
  });

  it('writes ink as freehand, and it comes back as ink where it was', async () => {
    const points = [[2, 2, 0.5], [10, 6, 0.6], [20, 2, 0.5], [30, 8, 0.4]];
    const file = await read({ nodes: [node('s', 'stroke', 100, 200, { svgPath: 'M 0,0 Z', points, width: 32, height: 10, color: '#e03131', size: 4, opacity: 0.85 })], edges: [] });
    const s = file.elements[0];
    expect(s).toMatchObject({ type: 'freedraw', x: 102, y: 202, strokeColor: '#e03131', strokeWidth: 2, opacity: 85, simulatePressure: false, pressures: [0.5, 0.6, 0.5, 0.4] });
    expect(s.points[0]).toEqual([0, 0]);
    const back = fromExcalidraw(JSON.stringify(file))!;
    expect(back.nodes[0]).toMatchObject({ type: 'stroke', data: { color: '#e03131', size: 4 } });
    expect(Math.abs(back.nodes[0].position.x - 100)).toBeLessThan(4);
    expect(Math.abs(back.nodes[0].position.y - 200)).toBeLessThan(4);
  });

  it('keeps groups, and leaves out what a folded branch hides', async () => {
    const file = await read({
      nodes: [
        node('r', 'mindmap', 0, 0, { label: 'Root', level: 0, color: '#6366f1', collapsed: true }),
        node('k', 'mindmap', 200, 0, { label: 'Kid', level: 1, color: '#6366f1' }),
        node('g1', 'sticky', 0, 300, { label: 'One', color: 'pink', groupId: 'grp' }),
        node('g2', 'sticky', 250, 300, { label: 'Two', groupId: 'grp' }),
      ],
      edges: [edge('rk', 'r', 'k', 'default', {})],
    });
    expect(file.elements.some((e: any) => e.id === 'k' || e.id === 'rk')).toBe(false);
    expect(file.elements.find((e: any) => e.id === 'r')).toMatchObject({ type: 'rectangle', strokeColor: '#6366f1' });
    expect(file.elements.find((e: any) => e.id === 'g1')).toMatchObject({ groupIds: ['grp'], backgroundColor: '#fbcfe8' });
    expect(file.elements.find((e: any) => e.id === 'g2').groupIds).toEqual(['grp']);
  });

  it('is the same file every time', async () => {
    expect(await toExcalidraw(board)).toBe(await toExcalidraw(board));
  });
});

describe('a round trip through Excalidraw', () => {
  it('brings a shape Excalidraw has no box for back as that shape, once', async () => {
    const nodes = [
      { id: 'c', type: 'shape', position: { x: 10, y: 20 }, data: { shapeType: 'cylinder', label: 'DB', color: '#2f9e44', width: 100, height: 120 } },
      { id: 'r', type: 'shape', position: { x: 300, y: 20 }, data: { shapeType: 'rectangle', label: 'API', width: 160, height: 80 } },
    ] as unknown as WBNode[];
    const edges = [{ id: 'e', source: 'r', target: 'c', data: { markerEnd: 'arrow' } }] as unknown as WBEdge[];
    const back = fromExcalidraw(await toExcalidraw({ nodes, edges }))!;
    const cyl = back.nodes.find((n) => n.id === 'c')!;
    expect(cyl.data.shapeType).toBe('cylinder');
    expect(cyl.data.label).toBe('DB');
    expect(cyl.data.color).toBe('#2f9e44');
    expect(back.nodes.filter((n) => n.type === 'stroke')).toHaveLength(0);
    expect(back.edges.map((e) => [e.source, e.target])).toEqual([['r', 'c']]);
  });
});

describe('every shape', () => {
  it('goes to draw.io as a shape it can edit, not a picture', async () => {
    const { SHAPES } = await import('../shapes');
    const nodes = SHAPES.map((s, i) => ({ id: `n${i}`, type: 'shape', position: { x: (i % 20) * 120, y: Math.floor(i / 20) * 120 }, data: { shapeType: s.id, label: s.id, width: 100, height: 100 } })) as unknown as WBNode[];
    const xml = await toDrawio({ nodes, edges: [] });
    const pictures = [...xml.matchAll(/<mxCell id="(n\d+)"[^>]*style="[^"]*shape=image/g)].map((m) => SHAPES[Number(m[1].slice(1))].id);
    expect(pictures).toEqual([]);
    const back = (await fromDrawio(xml))!;
    expect(back.nodes).toHaveLength(SHAPES.length);
  });
});

describe('pictures and icons, there and back', () => {
  const glyph = { set: 'lucide', name: 'server', viewBox: [0, 0, 24, 24], parts: [['rect', { x: 2, y: 2, width: 20, height: 8 }]] };
  const nodes = [
    { id: 'i', type: 'shape', position: { x: 0, y: 0 }, data: { shapeType: 'glyph', glyph, label: 'API', color: '#2f9e44', width: 64, height: 64 } },
    { id: 'p', type: 'image', position: { x: 200, y: 0 }, data: { assetPath: 'assets/a.png', alt: 'Logo', width: 80, height: 40 } },
  ] as unknown as WBNode[];
  const edges = [{ id: 'e', source: 'i', target: 'p', data: { markerEnd: 'arrow' } }] as unknown as WBEdge[];
  const imageData = async () => 'data:image/png;base64,iVBORw0KGgo=';

  it('come back from Excalidraw as the icon and a picture, joined', async () => {
    const back = fromExcalidraw(await toExcalidraw({ nodes, edges }, { imageData }))!;
    const icon = back.nodes.find((n) => n.id === 'i')!;
    expect(icon.data.shapeType).toBe('glyph');
    expect(icon.data.glyph.name).toBe('server');
    expect(icon.data.label).toBe('API');
    expect(back.nodes.filter((n) => n.type === 'text')).toHaveLength(0);
    const pic = back.nodes.find((n) => n.id === 'p')!;
    expect(pic.type).toBe('image');
    expect(pic.data.pendingPicture).toBe('data:image/png;base64,iVBORw0KGgo=');
    expect(back.edges.map((e) => [e.source, e.target])).toEqual([['i', 'p']]);
  });

  it('come back from draw.io as pictures', async () => {
    const back = (await fromDrawio(await toDrawio({ nodes, edges }, { imageData })))!;
    const pic = back.nodes.find((n) => n.id === 'p')!;
    expect(pic.type).toBe('image');
    expect(pic.data.pendingPicture).toBe('data:image/png;base64,iVBORw0KGgo=');
  });
});

describe('files from elsewhere', () => {
  it('cannot name a shape or an arrow end after what every object has', () => {
    const file = JSON.stringify({ type: 'excalidraw', elements: [
      { id: 'a', type: 'rectangle', x: 0, y: 0, width: 10, height: 10, customData: { synabit: { shapeType: 'constructor' } } },
      { id: 'b', type: 'rectangle', x: 50, y: 0, width: 10, height: 10 },
      { id: 'l', type: 'arrow', x: 10, y: 5, points: [[0, 0], [40, 0]], startBinding: { elementId: 'a' }, endBinding: { elementId: 'b' }, endArrowhead: '__proto__', startArrowhead: 'toString' },
    ] });
    const out = fromExcalidraw(file)!;
    expect(out.nodes[0].data.shapeType).toBe('rectangle');
    expect(out.edges[0].data!.markerEnd).toBe('none');
    expect(out.edges[0].data!.markerStart).toBe('none');
  });

  it('keeps a draw.io paste in place when one cell has no numbers', async () => {
    const xml = '<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/>'
      + '<mxCell id="a" value="A" vertex="1" parent="1"><mxGeometry x="abc" y="10" width="w" height="40" as="geometry"/></mxCell></root></mxGraphModel>';
    const out = (await fromDrawio(xml))!;
    expect(out.nodes[0].position).toEqual({ x: 0, y: 10 });
    expect(out.nodes[0].data.width).toBe(160);
  });
});
