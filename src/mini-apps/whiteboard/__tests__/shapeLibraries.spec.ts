import { describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import { cleanGlyph, glyphSvg } from '../glyph';
import { findIcons, loadIcons } from '../iconCatalog';
import { itemFrom, libraryPath, parseLibrary, serialise } from '../shapeLibraries';

describe('glyphs', () => {
  it('keeps drawing elements and geometry, and nothing else', () => {
    const g = cleanGlyph({
      set: 'x', name: 'evil', viewBox: [0, 0, 24, 24],
      parts: [
        ['path', { d: 'M1 1L23 23', onclick: 'alert(1)', style: 'fill:url(javascript:x)' }],
        ['script', { src: 'x.js' }],
        ['circle', { cx: 12, cy: 12, r: 'javascript:alert(1)' }],
        ['rect', { x: 1, y: 1, width: 4, height: 4, href: 'http://x' }],
        'nonsense',
      ],
    })!;
    expect(g.parts).toEqual([
      ['path', { d: 'M1 1L23 23' }],
      ['circle', { cx: 12, cy: 12 }],
      ['rect', { x: 1, y: 1, width: 4, height: 4 }],
    ]);
    expect(cleanGlyph({ parts: [['script', {}]] })).toBeNull();
    expect(cleanGlyph({ viewBox: [0, 0, 0, 24], parts: [['path', { d: 'M0 0' }]] })).toBeNull();
  });

  it('is drawn as a file other tools can open, with its colour escaped', () => {
    const svg = glyphSvg(cleanGlyph({ set: 'lucide', name: 'x', parts: [['path', { d: 'M1 1L2 2' }]] })!, '#f00"/><script>', 48, 48);
    expect(svg).toContain('<path d="M1 1L2 2"/>');
    expect(svg).not.toContain('<script>');
    expect(svg).toContain('width="48"');
  });
});

describe('the icon catalog', () => {
  it('reads every Lucide icon as a drawing, and finds them by name', async () => {
    const icons = await loadIcons();
    expect(icons.length).toBeGreaterThan(1500);
    const server = icons.find((i) => i.name === 'server')!;
    expect(server.glyph.parts.length).toBeGreaterThan(0);
    expect(server.glyph.viewBox).toEqual([0, 0, 24, 24]);
    // Names that start with the word come first.
    expect(findIcons(icons, 'data')[0].name).toMatch(/^data/);
    expect(findIcons(icons, 'hard drive').every((i) => i.words.includes('hard') && i.words.includes('drive'))).toBe(true);
    expect(findIcons(icons, '')).toBe(icons);
  });
});

describe('shape libraries', () => {
  it('reads an Excalidraw library, both versions, each piece starting at 0,0', async () => {
    const v2 = JSON.stringify({
      type: 'excalidrawlib', version: 2,
      libraryItems: [
        { id: 'a', name: 'Server', elements: [{ id: 'r', type: 'rectangle', x: 300, y: 200, width: 100, height: 60 }] },
        { id: 'b', name: 'Bad', elements: [] },
      ],
    });
    const out = (await parseLibrary(v2, 'cloud.excalidrawlib'))!;
    expect(out.name).toBe('cloud');
    expect(out.items).toHaveLength(1);
    expect(out.items[0].title).toBe('Server');
    expect(out.items[0].nodes[0].position).toEqual({ x: 0, y: 0 });

    const v1 = JSON.stringify({ type: 'excalidrawlib', version: 1, library: [[{ id: 't', type: 'text', x: 5, y: 5, text: 'Hello' }]] });
    expect((await parseLibrary(v1, 'old.excalidrawlib'))!.items[0].title).toBe('Hello');
  });

  it('reads a draw.io library, packed or plain, and keeps picture pieces as pictures', async () => {
    const model = '<mxGraphModel><root><mxCell id="0"/><mxCell id="1" parent="0"/><mxCell id="2" value="Box" style="rounded=1;" vertex="1" parent="1"><mxGeometry x="40" y="40" width="120" height="60" as="geometry"/></mxCell></root></mxGraphModel>';
    const stream = new Response(new TextEncoder().encode(encodeURIComponent(model))).body!.pipeThrough(new CompressionStream('deflate-raw'));
    const packed = btoa(String.fromCharCode(...new Uint8Array(await new Response(stream).arrayBuffer())));
    // As an XML writer saves it: the JSON escaped, markup and all.
    const esc = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
    const text = `<mxlibrary>${esc(JSON.stringify([
      { xml: packed, w: 120, h: 60, title: 'Packed box' },
      { xml: model, w: 120, h: 60, title: 'Plain box' },
      { data: 'data:image/png;base64,iVBORw0KGgo=', w: 32, h: 32, title: 'Logo' },
      { data: 'javascript:alert(1)', title: 'No' },
    ]))}</mxlibrary>`;
    const out = (await parseLibrary(text, 'kit.xml'))!;
    expect(out.items.map((i) => i.title)).toEqual(['Packed box', 'Plain box', 'Logo']);
    expect(out.items[0].nodes[0].data.label).toBe('Box');
    expect(out.items[0].nodes[0].position).toEqual({ x: 0, y: 0 });
    expect(out.items[2].image).toEqual({ dataUri: 'data:image/png;base64,iVBORw0KGgo=', width: 32, height: 32 });
  });

  it('reads its own files back, and refuses what is not a library', async () => {
    const item = itemFrom('Pair', [
      { id: 'a', type: 'shape', position: { x: 100, y: 100 }, data: { label: 'A' } },
      { id: 'b', type: 'shape', position: { x: 300, y: 150 }, data: { label: 'B' } },
    ] as any, [
      { id: 'e', source: 'a', target: 'b', data: { waypoints: [{ x: 200, y: 120 }] } },
      { id: 'out', source: 'a', target: 'elsewhere' },
    ] as any);
    expect(item.nodes[1].position).toEqual({ x: 200, y: 50 });
    expect(item.edges.map((e) => e.id)).toEqual(['e']);
    expect(item.edges[0].data!.waypoints[0]).toEqual({ x: 100, y: 20 });

    const back = (await parseLibrary(serialise({ path: '', name: 'Mine', items: [item] }), 'x.boardlib.json'))!;
    expect(back.name).toBe('Mine');
    expect(back.items[0].nodes).toHaveLength(2);

    expect(await parseLibrary('not json', 'a.json')).toBeNull();
    expect(await parseLibrary('{"type":"excalidraw","elements":[]}', 'a.excalidraw')).toBeNull();
  });

  it('names a library file after the library, without clashing', () => {
    expect(libraryPath('AWS: icons/2024', [])).toBe('Whiteboards/Libraries/AWS icons 2024.boardlib.json');
    expect(libraryPath('Kit', ['Whiteboards/Libraries/Kit.boardlib.json'])).toBe('Whiteboards/Libraries/Kit 2.boardlib.json');
    expect(libraryPath('///', [])).toBe('Whiteboards/Libraries/Library.boardlib.json');
  });
});

describe('a picture in a data: URL', () => {
  it('is decoded without fetching it', async () => {
    const { dataUrlBlob } = await import('../imageAssets');
    const png = dataUrlBlob('data:image/png;base64,iVBORw0KGgo=');
    expect(png.type).toBe('image/png');
    expect(new Uint8Array(await png.arrayBuffer()).slice(0, 4)).toEqual(new Uint8Array([0x89, 0x50, 0x4e, 0x47]));
    const svg = dataUrlBlob('data:image/svg+xml,%3Csvg%2F%3E');
    expect(svg.type).toBe('image/svg+xml');
    expect(await svg.text()).toBe('<svg/>');
  });
});

describe('finding icons', () => {
  it('takes Vietnamese, with or without marks', async () => {
    const icons = await loadIcons();
    expect(findIcons(icons, 'máy chủ')[0].name).toMatch(/^server/);
    expect(findIcons(icons, 'may chu')[0].name).toMatch(/^server/);
    expect(findIcons(icons, 'cơ sở dữ liệu')[0].name).toMatch(/^database/);
    // An English start of a name is not taken for a Vietnamese word.
    expect(findIcons(icons, 'co')[0].name).toMatch(/co/);
  });
});
