import { describe, expect, it } from 'vitest';
import { buildMindmap, childrenOf, fromMarkdown, fromOutline, MINDMAP_COLORS, toOpml, toOutline, type OutlineItem } from '../mindmap';
import { fromFreemind, fromOpml, fromXmind, unzip } from '../mindmapFiles';

/** A tree with only its words, to compare against. */
const words = (t: OutlineItem | null): any => (t ? [t.label, ...t.children.map(words)] : null);

let n = 0;
const newId = (prefix: string) => `${prefix}_${++n}`;

describe('a markmap block read as a tree', () => {
  it('takes headings as levels and lists under them, and leaves the front matter out', () => {
    const md = [
      '---',
      'markmap:',
      '  colorFreezeLevel: 2',
      '---',
      '# Trip',
      '## Before',
      '- Book **flights**',
      '  - [x] Passport',
      '## During',
      'A paragraph is the idea\'s note, not a branch.',
      '- See [[Old town|the old town]]',
      '```js',
      '- not a list in code',
      '```',
    ].join('\n');
    expect(words(fromMarkdown(md))).toEqual(['Trip', ['Before', ['Book flights', ['Passport']]], ['During', ['See the old town']]]);
  });

  it('shares a root between several top ideas, and reads a one-item list', () => {
    expect(words(fromMarkdown('# A\n# B', 'Map'))).toEqual(['Map', ['A'], ['B']]);
    expect(words(fromMarkdown('- Only'))).toEqual(['Only']);
    expect(fromMarkdown('just words\n\nno map')).toBeNull();
  });

  it('keeps where a linked idea points', () => {
    const tree = fromMarkdown('- [Plan](synabit://note/Notes%2FPlan.md)\n  - Step');
    expect(tree?.label).toBe('Plan');
    expect(tree?.link).toBe('synabit://note/Notes%2FPlan.md');
  });
});

describe('a map built from a tree', () => {
  const tree: OutlineItem = {
    label: 'Root',
    children: ['A', 'B', 'C', 'D', 'E'].map((label) => ({ label, children: [{ label: `${label}1`, children: [] }] })),
  };

  it('shares the main branches between both sides and colours each', () => {
    const map = buildMindmap(tree, { x: 0, y: 0 }, MINDMAP_COLORS, newId);
    const byLabel = new Map(map.nodes.map((m) => [m.data.label, m]));
    expect(['A', 'B', 'C'].map((l) => byLabel.get(l)!.data.direction)).toEqual(['right', 'right', 'right']);
    expect(['D', 'E'].map((l) => byLabel.get(l)!.data.direction)).toEqual(['left', 'left']);
    // Left-hand branches sit left of the root, and grow further left.
    const root = byLabel.get('Root')!;
    expect(byLabel.get('D')!.position.x).toBeLessThan(root.position.x);
    expect(byLabel.get('D1')!.position.x).toBeLessThan(byLabel.get('D')!.position.x);
    expect(byLabel.get('A1')!.data.color).toBe(byLabel.get('A')!.data.color);
    expect(new Set(['A', 'B', 'C', 'D', 'E'].map((l) => byLabel.get(l)!.data.color)).size).toBe(5);
    expect(root.data.color).toBe(MINDMAP_COLORS[0]);
    // A left branch joins its parent on the parent's left.
    const toD = map.edges.find((e) => e.target === byLabel.get('D')!.id)!;
    expect([toD.sourceHandle, toD.targetHandle]).toEqual(['left-source', 'right-target']);
  });

  it('keeps a small map on one side', () => {
    const map = buildMindmap({ label: 'R', children: [{ label: 'a', children: [] }, { label: 'b', children: [] }] }, { x: 0, y: 0 }, MINDMAP_COLORS, newId);
    expect(map.nodes.filter((m) => m.data.level === 1).every((m) => m.data.direction === 'right')).toBe(true);
  });

  it('round-trips through an outline, links and all', () => {
    const linked: OutlineItem = { label: 'Root', children: [{ label: 'Plan', link: 'synabit://note/Notes%2FPlan.md', children: [{ label: 'Step', children: [] }] }] };
    const map = buildMindmap(linked, { x: 0, y: 0 }, MINDMAP_COLORS, newId);
    const text = toOutline(map.rootId, map.nodes, map.edges);
    expect(text).toBe('- Root\n  - [Plan](synabit://note/Notes%2FPlan.md)\n    - Step');
    const back = fromOutline(text);
    expect(words(back)).toEqual(['Root', ['Plan', ['Step']]]);
    expect(back?.children[0].link).toBe('synabit://note/Notes%2FPlan.md');
  });
});

describe('OPML', () => {
  it('writes the tree out and reads it back', () => {
    const tree: OutlineItem = { label: 'R & D', children: [{ label: 'Say "hi"', link: 'https://example.com/?a=1&b=2', children: [{ label: '<b>', children: [] }] }] };
    const map = buildMindmap(tree, { x: 0, y: 0 }, MINDMAP_COLORS, newId);
    const opml = toOpml(map.rootId, map.nodes, map.edges);
    expect(opml).toContain('<outline text="R &amp; D">');
    const back = fromOpml(opml);
    expect(words(back)).toEqual(['R & D', ['Say "hi"', ['<b>']]]);
    expect(back?.children[0].link).toBe('https://example.com/?a=1&b=2');
  });

  it('puts several outlines under the file’s title, and takes no link it would not follow', () => {
    const opml = '<opml version="2.0"><head><title>Ideas</title></head><body><outline text="One" url="javascript:alert(1)"/><outline text="Two"/></body></opml>';
    const tree = fromOpml(opml);
    expect(words(tree)).toEqual(['Ideas', ['One'], ['Two']]);
    expect(tree?.children[0].link).toBeUndefined();
    expect(fromOpml('<html/>')).toBeNull();
  });
});

describe('FreeMind', () => {
  it('reads nodes, rich text and links', () => {
    const mm = `<map version="1.0.1"><node TEXT="Root"><node TEXT="A" LINK="https://a.example"><node TEXT="A1"/></node>
      <node><richcontent TYPE="NODE"><html><body><p>Rich</p></body></html></richcontent></node></node></map>`;
    const tree = fromFreemind(mm);
    expect(words(tree)).toEqual(['Root', ['A', ['A1']], ['Rich']]);
    expect(tree?.children[0].link).toBe('https://a.example');
  });
});

/** A zip of these files, deflated, as XMind writes one. */
async function zip(files: Record<string, string>): Promise<Uint8Array> {
  const enc = new TextEncoder();
  const parts: Uint8Array[] = [];
  const central: Uint8Array[] = [];
  let offset = 0;
  for (const [name, text] of Object.entries(files)) {
    const nameBytes = enc.encode(name);
    const raw = enc.encode(text);
    const stream = new Response(raw).body!.pipeThrough(new CompressionStream('deflate-raw'));
    const packed = new Uint8Array(await new Response(stream).arrayBuffer());
    const local = new DataView(new ArrayBuffer(30));
    local.setUint32(0, 0x04034b50, true);
    local.setUint16(8, 8, true);
    local.setUint32(18, packed.length, true);
    local.setUint32(22, raw.length, true);
    local.setUint16(26, nameBytes.length, true);
    parts.push(new Uint8Array(local.buffer), nameBytes, packed);
    const entry = new DataView(new ArrayBuffer(46));
    entry.setUint32(0, 0x02014b50, true);
    entry.setUint16(10, 8, true);
    entry.setUint32(20, packed.length, true);
    entry.setUint32(24, raw.length, true);
    entry.setUint16(28, nameBytes.length, true);
    entry.setUint32(42, offset, true);
    central.push(new Uint8Array(entry.buffer), nameBytes);
    offset += 30 + nameBytes.length + packed.length;
  }
  const dirSize = central.reduce((s, p) => s + p.length, 0);
  const end = new DataView(new ArrayBuffer(22));
  end.setUint32(0, 0x06054b50, true);
  end.setUint16(10, Object.keys(files).length, true);
  end.setUint32(12, dirSize, true);
  end.setUint32(16, offset, true);
  const all = [...parts, ...central, new Uint8Array(end.buffer)];
  const out = new Uint8Array(all.reduce((s, p) => s + p.length, 0));
  let at = 0;
  for (const p of all) { out.set(p, at); at += p.length; }
  return out;
}

describe('XMind', () => {
  it('reads the first sheet of a newer file', async () => {
    const content = JSON.stringify([{ rootTopic: { title: 'Root', children: { attached: [{ title: 'A', href: 'https://a.example', children: { attached: [{ title: 'A1' }] } }, { title: 'B' }] } } }]);
    const tree = await fromXmind(await zip({ 'metadata.json': '{}', 'content.json': content }));
    expect(words(tree)).toEqual(['Root', ['A', ['A1']], ['B']]);
    expect(tree?.children[0].link).toBe('https://a.example');
  });

  it('reads an XMind 8 file, leaving out floating topics', async () => {
    const xml = `<?xml version="1.0"?><xmap-content xmlns="urn:xmind:xmap:xmlns:content:2.0" xmlns:xlink="http://www.w3.org/1999/xlink"><sheet><topic><title>Root</title>
      <children><topics type="attached"><topic xlink:href="https://b.example"><title>B</title></topic></topics><topics type="detached"><topic><title>Loose</title></topic></topics></children>
      </topic></sheet></xmap-content>`;
    const tree = await fromXmind(await zip({ 'content.xml': xml }));
    expect(words(tree)).toEqual(['Root', ['B']]);
    expect(tree?.children[0].link).toBe('https://b.example');
  });

  it('is not fooled by something that is not a zip', async () => {
    expect(await fromXmind(new TextEncoder().encode('not a zip at all'))).toBeNull();
    expect((await unzip(new Uint8Array(10), ['content.json'])).size).toBe(0);
  });
});

describe('the tree a map is drawn from', () => {
  it('has the lines buildMindmap draws as its branches', () => {
    const map = buildMindmap({ label: 'R', children: [{ label: 'a', children: [{ label: 'b', children: [] }] }] }, { x: 0, y: 0 }, MINDMAP_COLORS, newId);
    const kids = childrenOf(map.nodes, map.edges);
    expect(kids.get(map.rootId)?.length).toBe(1);
  });
});
