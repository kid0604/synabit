import { describe, expect, it } from 'vitest';
import { fromDrawio, fromExcalidraw, looksLikeMermaid } from '../importers';
import { jpegToPdf } from '../pdf';

describe('fromExcalidraw', () => {
  const file = JSON.stringify({
    type: 'excalidraw',
    elements: [
      { id: 'r', type: 'rectangle', x: 10, y: 20, width: 200, height: 100, strokeColor: '#1e1e1e', backgroundColor: '#a5d8ff', strokeWidth: 2, strokeStyle: 'dashed', roundness: { type: 3 } },
      { id: 'rt', type: 'text', containerId: 'r', text: 'API', fontSize: 20 },
      { id: 'e', type: 'ellipse', x: 400, y: 20, width: 120, height: 120, strokeColor: '#e03131', backgroundColor: 'transparent' },
      { id: 'a', type: 'arrow', x: 210, y: 70, points: [[0, 0], [190, 10]], startBinding: { elementId: 'r' }, endBinding: { elementId: 'e' }, endArrowhead: 'triangle', startArrowhead: null },
      { id: 'at', type: 'text', containerId: 'a', text: 'calls' },
      { id: 't', type: 'text', x: 0, y: 300, width: 100, text: 'Loose words', fontSize: 28 },
      { id: 'f', type: 'freedraw', x: 0, y: 400, points: [[0, 0], [10, 5], [20, 0], [30, 8]], strokeWidth: 2 },
      { id: 'gone', type: 'rectangle', x: 0, y: 0, width: 10, height: 10, isDeleted: true },
    ],
  });
  const out = fromExcalidraw(file)!;

  it('brings boxes with their words and look', () => {
    const r = out.nodes.find((n) => n.id === 'r')!;
    expect(r.data).toMatchObject({ shapeType: 'roundedRect', label: 'API', color: '#1e1e1e', fillColor: '#a5d8ff', dashStyle: 'dashed', width: 200 });
    expect(out.nodes.find((n) => n.id === 'e')!.data.fillColor).toBeUndefined();
  });

  it('turns a joined arrow into a line with its end and label', () => {
    expect(out.edges).toEqual([
      expect.objectContaining({ source: 'r', target: 'e', data: expect.objectContaining({ markerEnd: 'arrow', markerStart: 'none', label: 'calls' }) }),
    ]);
  });

  it('keeps loose words and ink, and leaves out what was deleted', () => {
    expect(out.nodes.find((n) => n.id === 't')).toMatchObject({ type: 'text', data: { label: 'Loose words', fontSize: 28 } });
    expect(out.nodes.find((n) => n.id === 'f')?.type).toBe('stroke');
    expect(out.nodes.some((n) => n.id === 'gone')).toBe(false);
  });

  it('says no to something that is not an Excalidraw file', () => {
    expect(fromExcalidraw('not json')).toBeNull();
    expect(fromExcalidraw('{"hello":1}')).toBeNull();
  });
});

const DRAWIO_MODEL = `<mxGraphModel><root>
  <mxCell id="0"/><mxCell id="1" parent="0"/>
  <mxCell id="a" value="&lt;b&gt;Start&lt;/b&gt;" style="ellipse;whiteSpace=wrap;fillColor=#dae8fc;strokeColor=#6c8ebf;" vertex="1" parent="1"><mxGeometry x="40" y="40" width="120" height="60" as="geometry"/></mxCell>
  <mxCell id="lane" value="Team" style="swimlane;" vertex="1" parent="1"><mxGeometry x="300" y="0" width="300" height="200" as="geometry"/></mxCell>
  <mxCell id="b" value="Check" style="rhombus;" vertex="1" parent="lane"><mxGeometry x="20" y="40" width="80" height="80" as="geometry"/></mxCell>
  <mxCell id="ab" value="go" style="edgeStyle=orthogonalEdgeStyle;endArrow=ERmany;startArrow=diamond;startFill=0;" edge="1" parent="1" source="a" target="b"><mxGeometry relative="1" as="geometry"/></mxCell>
</root></mxGraphModel>`;

describe('fromDrawio', () => {
  it('reads boxes, containers, and lines with their ends', async () => {
    const out = (await fromDrawio(`<mxfile><diagram>${DRAWIO_MODEL}</diagram></mxfile>`))!;
    expect(out.nodes.find((n) => n.id === 'a')).toMatchObject({ type: 'shape', data: { shapeType: 'ellipse', label: 'Start', fillColor: '#dae8fc' } });
    expect(out.nodes.find((n) => n.id === 'lane')).toMatchObject({ type: 'frame', data: { label: 'Team' } });
    // Inside the lane, placed relative to it.
    expect(out.nodes.find((n) => n.id === 'b')).toMatchObject({ position: { x: 320, y: 40 }, data: { shapeType: 'diamond' } });
    expect(out.edges[0]).toMatchObject({ source: 'a', target: 'b', type: 'step', data: { markerEnd: 'er-many', markerStart: 'diamond-open', label: 'go' } });
  });

  it('reads the compressed form the desktop app saves', async () => {
    const encoded = encodeURIComponent(DRAWIO_MODEL);
    const stream = new Response(new TextEncoder().encode(encoded)).body!.pipeThrough(new CompressionStream('deflate-raw'));
    const bytes = new Uint8Array(await new Response(stream).arrayBuffer());
    const packed = btoa(String.fromCharCode(...bytes));
    const out = await fromDrawio(`<mxfile><diagram id="x" name="Page-1">${packed}</diagram></mxfile>`);
    expect(out?.nodes.map((n) => n.id).sort()).toEqual(['a', 'b', 'lane']);
  });
});

describe('looksLikeMermaid', () => {
  it('knows Mermaid source by its first word', () => {
    expect(looksLikeMermaid('flowchart LR\n  A --> B')).toBe(true);
    expect(looksLikeMermaid('%% comment\nsequenceDiagram\n A->>B: hi')).toBe(true);
    expect(looksLikeMermaid('graphs are nice')).toBe(false);
  });
});

describe('jpegToPdf', () => {
  it('writes a cross-reference table that points at each object', () => {
    const pdf = new TextDecoder('latin1').decode(jpegToPdf(new Uint8Array([0xff, 0xd8, 0xff, 0xd9]), 2, 2, 1.5, 1.5));
    expect(pdf.startsWith('%PDF-1.4')).toBe(true);
    const xrefAt = Number(/startxref\n(\d+)/.exec(pdf)![1]);
    expect(pdf.slice(xrefAt, xrefAt + 4)).toBe('xref');
    const offsets = [...pdf.slice(xrefAt).matchAll(/^(\d{10}) 00000 n $/gm)].map((m) => Number(m[1]));
    offsets.forEach((offset, i) => expect(pdf.slice(offset, offset + 7)).toBe(`${i + 1} 0 obj`));
  });
});
