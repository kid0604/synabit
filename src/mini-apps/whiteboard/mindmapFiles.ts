/**
 * Mind maps from other apps' files, read as the tree `buildMindmap` draws.
 *
 * Three formats, because between them they are how a mind map leaves almost
 * any other tool: OPML (MindNode, OmniOutliner, Logseq, Workflowy, and most
 * outliners), FreeMind's `.mm` (FreeMind, Freeplane, and many export to it),
 * and XMind's own `.xmind`. Only the tree comes over — words, links, order —
 * because how another app draws a map is not how this board does.
 */
import type { OutlineItem } from './mindmap';

/** The most ideas taken from one file: past this a board stops being one to work on. */
const MAX_IDEAS = 2000;

/** A tree of at most `MAX_IDEAS`, its labels one line each; null when nothing is left. */
function capped(tree: OutlineItem | null): OutlineItem | null {
  if (!tree) return null;
  let left = MAX_IDEAS;
  const trim = (item: OutlineItem): OutlineItem | null => {
    if (left <= 0) return null;
    left--;
    const label = item.label.replace(/\s+/g, ' ').trim();
    const children = item.children.map(trim).filter((k): k is OutlineItem => !!k);
    return { label, children, ...(item.link ? { link: item.link } : {}) };
  };
  const out = trim(tree);
  return out && (out.label || out.children.length) ? out : null;
}

/** Only links a board follows: a web page or something in the vault (see `cleanLink`). */
function linkOf(raw: string | null | undefined): string | undefined {
  const text = raw?.trim();
  return text && /^(https?:|mailto:|synabit:\/\/)/i.test(text) ? text : undefined;
}

/** Several top-level ideas under one root, named by the file; one stands as the root itself. */
function rooted(tops: OutlineItem[], title: string): OutlineItem | null {
  if (!tops.length) return null;
  if (tops.length === 1) return tops[0];
  return { label: title, children: tops };
}

function xml(text: string): Document | null {
  const doc = new DOMParser().parseFromString(text, 'application/xml');
  return doc.querySelector('parsererror') ? null : doc;
}

/** The child elements of `el` with this tag name, in order — not every one below it. */
function own(el: Element, tag: string): Element[] {
  return Array.from(el.children).filter((c) => c.localName === tag);
}

/** An OPML outline: `<outline text>` within `<outline text>`, its title the root of several. */
export function fromOpml(text: string, fileTitle = ''): OutlineItem | null {
  if (!/<opml[\s>]/i.test(text)) return null;
  const doc = xml(text);
  const body = doc?.querySelector('body');
  if (!body) return null;
  const read = (el: Element): OutlineItem => {
    const link = linkOf(el.getAttribute('url') ?? el.getAttribute('htmlUrl'));
    return {
      label: el.getAttribute('text') ?? el.getAttribute('title') ?? '',
      children: own(el, 'outline').map(read),
      ...(link ? { link } : {}),
    };
  };
  const title = doc?.querySelector('head > title')?.textContent?.trim() || fileTitle;
  return capped(rooted(own(body, 'outline').map(read), title));
}

/** A FreeMind or Freeplane map: `<node TEXT>` within `<node>`, under `<map>`. */
export function fromFreemind(text: string, fileTitle = ''): OutlineItem | null {
  if (!/<map[\s>]/i.test(text)) return null;
  const map = xml(text)?.querySelector('map');
  if (!map) return null;
  const read = (el: Element): OutlineItem => {
    // Words formatted in the app are kept as HTML in a `richcontent` of type NODE.
    const rich = own(el, 'richcontent').find((r) => (r.getAttribute('TYPE') ?? '').toUpperCase() === 'NODE');
    const label = el.getAttribute('TEXT') ?? rich?.textContent ?? '';
    const link = linkOf(el.getAttribute('LINK'));
    return { label, children: own(el, 'node').map(read), ...(link ? { link } : {}) };
  };
  return capped(rooted(own(map, 'node').map(read), fileTitle));
}

/**
 * An XMind file's first sheet. Since XMind Zen (2018) the map is
 * `content.json`; files from XMind 8 and before carry `content.xml`. Both are
 * read, the newer first.
 */
export async function fromXmind(bytes: Uint8Array): Promise<OutlineItem | null> {
  const files = await unzip(bytes, ['content.json', 'content.xml']);
  const json = files.get('content.json');
  if (json) {
    try {
      const sheets = JSON.parse(json);
      const root = Array.isArray(sheets) ? sheets[0]?.rootTopic : null;
      if (root) {
        const read = (topic: any): OutlineItem => {
          const link = linkOf(typeof topic?.href === 'string' ? topic.href : undefined);
          const kids: any[] = Array.isArray(topic?.children?.attached) ? topic.children.attached : [];
          return { label: String(topic?.title ?? ''), children: kids.map(read), ...(link ? { link } : {}) };
        };
        return capped(read(root));
      }
    } catch {
      // Not the JSON it should be: try the older form, if it is there.
    }
  }
  const old = files.get('content.xml');
  const topic = old ? xml(old)?.querySelector('sheet > topic') : null;
  if (!topic) return null;
  const read = (el: Element): OutlineItem => {
    const title = own(el, 'title')[0]?.textContent ?? '';
    const link = linkOf(el.getAttributeNS('http://www.w3.org/1999/xlink', 'href') ?? el.getAttribute('xlink:href'));
    const kids = own(el, 'children').flatMap((c) => own(c, 'topics'))
      .filter((t) => (t.getAttribute('type') ?? 'attached') === 'attached')
      .flatMap((t) => own(t, 'topic'));
    return { label: title, children: kids.map(read), ...(link ? { link } : {}) };
  };
  return capped(read(topic));
}

/** Which of these files a mind map arrives as, by its name. */
export function isMindmapFile(name: string): boolean {
  return /\.(opml|mm|xmind)$/i.test(name);
}

/** A mind-map file read as a tree: by its name first, then by what is in it. */
export async function readMindmapFile(file: File): Promise<OutlineItem | null> {
  const title = file.name.replace(/\.[^.]+$/, '');
  if (/\.xmind$/i.test(file.name)) return fromXmind(new Uint8Array(await file.arrayBuffer()));
  const text = await file.text();
  if (/\.mm$/i.test(file.name)) return fromFreemind(text, title);
  return fromOpml(text, title) ?? fromFreemind(text, title);
}

// ── Zip ─────────────────────────────────────────────────────

/**
 * The named files of a zip archive, as text. Only what an `.xmind` needs:
 * files stored or deflated, found through the central directory; anything
 * else in the archive is not read.
 */
export async function unzip(bytes: Uint8Array, wanted: string[]): Promise<Map<string, string>> {
  const out = new Map<string, string>();
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  // The end-of-directory record is the last thing in the file, before a
  // comment of at most 65535 bytes.
  let end = -1;
  for (let i = bytes.length - 22; i >= Math.max(0, bytes.length - 22 - 65535); i--) {
    if (view.getUint32(i, true) === 0x06054b50) { end = i; break; }
  }
  if (end < 0) return out;
  const count = view.getUint16(end + 10, true);
  let at = view.getUint32(end + 16, true);
  const names = new TextDecoder();
  for (let n = 0; n < count && at + 46 <= bytes.length; n++) {
    if (view.getUint32(at, true) !== 0x02014b50) break;
    const method = view.getUint16(at + 10, true);
    const size = view.getUint32(at + 20, true);
    const nameLength = view.getUint16(at + 28, true);
    const extraLength = view.getUint16(at + 30, true);
    const commentLength = view.getUint16(at + 32, true);
    const local = view.getUint32(at + 42, true);
    const name = names.decode(bytes.subarray(at + 46, at + 46 + nameLength));
    at += 46 + nameLength + extraLength + commentLength;
    if (!wanted.includes(name) || local + 30 > bytes.length || view.getUint32(local, true) !== 0x04034b50) continue;
    const start = local + 30 + view.getUint16(local + 26, true) + view.getUint16(local + 28, true);
    const packed = bytes.subarray(start, start + size);
    try {
      if (method === 0) out.set(name, new TextDecoder().decode(packed));
      else if (method === 8) {
        const stream = new Response(packed).body!.pipeThrough(new DecompressionStream('deflate-raw'));
        out.set(name, await new Response(stream).text());
      }
    } catch {
      // A damaged entry is one file less, not a failed import of the rest.
    }
  }
  return out;
}
