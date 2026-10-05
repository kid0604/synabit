import { invoke } from '@tauri-apps/api/core';
import type { WBEdge, WBNode } from './boardFile';
import { fromDrawio, fromExcalidraw } from './importers';

/**
 * Shape libraries: sets of ready-made pieces — a company's diagram parts, a
 * cloud provider's icons, a kit of UI controls — that a board takes copies of.
 *
 * People already have these as Excalidraw libraries (`.excalidrawlib`, from
 * libraries.excalidraw.com and elsewhere) and draw.io libraries (an
 * `<mxlibrary>` file). Both come in as this app's own kind, one file per
 * library under `Whiteboards/Libraries/`, so they sync with the vault and
 * outlive the tool they came from. A piece is board items and lines, put down
 * through the same path as a paste; a picture-only piece keeps its picture.
 */

export interface LibraryItem {
  id: string;
  title: string;
  nodes: WBNode[];
  edges: WBEdge[];
  /**
   * When it was added. Two devices that each add pieces to the same library
   * before syncing keep both: the sync merges a library piece by piece, and
   * this is what it dates them by.
   */
  added_at?: string;
  /** A piece that is only a picture: its bytes, until it is put on a board. */
  image?: { dataUri: string; width: number; height: number };
}

export interface ShapeLibrary {
  path: string;
  name: string;
  items: LibraryItem[];
}

export const LIBRARY_DIR = 'Whiteboards/Libraries';
const FORMAT = 'synabit-board-library';

/** Items moved so the piece's top-left corner is at 0,0. */
function atOrigin(nodes: WBNode[], edges: WBEdge[]): { nodes: WBNode[]; edges: WBEdge[] } {
  if (!nodes.length) return { nodes, edges };
  const x = Math.min(...nodes.map((n) => n.position.x));
  const y = Math.min(...nodes.map((n) => n.position.y));
  const shift = (p: { x: number; y: number }) => ({ x: Math.round(p.x - x), y: Math.round(p.y - y) });
  return {
    nodes: nodes.map((n) => ({ ...n, position: shift(n.position) })),
    edges: edges.map((e) => (e.data?.waypoints ? { ...e, data: { ...e.data, waypoints: e.data.waypoints.map(shift) } } : e)),
  };
}

let counter = 0;
const newId = () => `li_${Date.now().toString(36)}_${(counter++).toString(36)}_${Math.random().toString(36).slice(2, 6)}`;
const stamp = () => ({ added_at: new Date().toISOString() });

// ─── Excalidraw ─────────────────────────────────────────────

function fromExcalidrawLib(doc: any): LibraryItem[] {
  // Version 2 names its pieces; version 1 is a bare list of element lists.
  const pieces: { name?: string; elements: any[] }[] = Array.isArray(doc?.libraryItems)
    ? doc.libraryItems.filter((i: any) => Array.isArray(i?.elements)).map((i: any) => ({ name: i.name, elements: i.elements }))
    : Array.isArray(doc?.library)
      ? doc.library.filter(Array.isArray).map((elements: any[]) => ({ elements }))
      : [];
  const items: LibraryItem[] = [];
  for (const [i, piece] of pieces.entries()) {
    const drawn = fromExcalidraw(JSON.stringify({ type: 'excalidraw', elements: piece.elements }));
    if (!drawn) continue;
    const firstWords = piece.elements.find((e: any) => e?.type === 'text' && e.text)?.text;
    items.push({ id: newId(), ...stamp(), title: String(piece.name || firstWords || `#${i + 1}`).slice(0, 80), ...atOrigin(drawn.nodes, drawn.edges) });
  }
  return items;
}

// ─── draw.io ────────────────────────────────────────────────

/** A draw.io library piece's diagram, packed or not: base64 of raw-deflated, URI-encoded XML. */
async function unpack(xml: string): Promise<string | null> {
  const text = xml.trim();
  if (text.startsWith('<')) return text;
  try {
    const bytes = Uint8Array.from(atob(text), (c) => c.charCodeAt(0));
    const stream = new Response(bytes).body!.pipeThrough(new DecompressionStream('deflate-raw'));
    return decodeURIComponent(await new Response(stream).text());
  } catch {
    return null;
  }
}

async function fromMxLibrary(text: string): Promise<LibraryItem[]> {
  // Read as XML, so what an XML writer escaped (`&lt;mxGraphModel&gt;`,
  // "R&amp;D") comes back as written.
  const doc = new DOMParser().parseFromString(text, 'application/xml');
  const inner = doc.querySelector('mxlibrary')?.textContent;
  let list: any[];
  try {
    list = JSON.parse(inner ?? '');
  } catch {
    return [];
  }
  if (!Array.isArray(list)) return [];
  const items: LibraryItem[] = [];
  for (const [i, piece] of list.entries()) {
    const title = String(piece?.title || `#${i + 1}`).slice(0, 80);
    if (typeof piece?.xml === 'string') {
      const xml = await unpack(piece.xml);
      const drawn = xml ? await fromDrawio(xml) : null;
      if (drawn) items.push({ id: newId(), ...stamp(), title, ...atOrigin(drawn.nodes, drawn.edges) });
    } else if (typeof piece?.data === 'string' && /^data:image\/(png|jpe?g|gif|webp|svg\+xml)[;,]/.test(piece.data)) {
      items.push({
        id: newId(), ...stamp(), title, nodes: [], edges: [],
        image: { dataUri: piece.data, width: Math.round(Number(piece.w) || 80), height: Math.round(Number(piece.h) || 80) },
      });
    }
  }
  return items;
}

// ─── Reading, writing ───────────────────────────────────────

/**
 * The pieces in a library file from another tool, or in one of this app's
 * own. Null when the file is none of these, or has nothing in it.
 */
export async function parseLibrary(text: string, fileName: string): Promise<{ name: string; items: LibraryItem[] } | null> {
  const name = fileName.replace(/\.(excalidrawlib|xml|drawio|boardlib\.json|json)$/i, '').trim() || 'Library';
  let items: LibraryItem[] = [];
  if (/<mxlibrary>/.test(text)) {
    items = await fromMxLibrary(text);
  } else {
    let doc: any;
    try {
      doc = JSON.parse(text);
    } catch {
      return null;
    }
    if (doc?.type === FORMAT) {
      const own = readOwn(doc, '');
      return own && own.items.length ? { name: own.name || name, items: own.items } : null;
    }
    if (doc?.type === 'excalidrawlib' || Array.isArray(doc?.libraryItems) || Array.isArray(doc?.library)) items = fromExcalidrawLib(doc);
  }
  return items.length ? { name, items } : null;
}

function readOwn(doc: any, path: string): ShapeLibrary | null {
  if (doc?.type !== FORMAT || !Array.isArray(doc.items)) return null;
  const items: LibraryItem[] = doc.items
    .filter((i: any) => i && typeof i.id === 'string' && Array.isArray(i.nodes ?? []))
    .map((i: any) => ({
      id: i.id,
      title: String(i.title ?? ''),
      // Only items with a place, and lines between two of them: a file synced
      // or edited by hand can hold anything, and one bad item broke the picker.
      nodes: (Array.isArray(i.nodes) ? i.nodes : []).filter((n: any) => n && typeof n === 'object' && typeof n.id === 'string' && typeof n.type === 'string'
        && Number.isFinite(Number(n.position?.x)) && Number.isFinite(Number(n.position?.y))),
      edges: (Array.isArray(i.edges) ? i.edges : []).filter((e: any) => e && typeof e.source === 'string' && typeof e.target === 'string'),
      ...(typeof i.added_at === 'string' ? { added_at: i.added_at } : {}),
      ...(i.image && typeof i.image.dataUri === 'string' && i.image.dataUri.startsWith('data:image/') ? { image: i.image } : {}),
    }))
    .filter((i: LibraryItem) => i.nodes.length || i.image);
  return { path, name: String(doc.name ?? ''), items };
}

export function serialise(library: ShapeLibrary): string {
  // `metadata.updated_at` is what a sync falls back to when it cannot merge.
  return JSON.stringify({ type: FORMAT, version: 1, name: library.name, metadata: { updated_at: new Date().toISOString() }, items: library.items }, null, 1);
}

/** A file name for a library: its name, without what a file name cannot hold, not already taken. */
export function libraryPath(name: string, taken: string[]): string {
  const base = name.replace(/[\\/:*?"<>|#^[\]]/g, ' ').replace(/\s+/g, ' ').trim().slice(0, 60) || 'Library';
  let path = `${LIBRARY_DIR}/${base}.boardlib.json`;
  for (let n = 2; taken.includes(path); n++) path = `${LIBRARY_DIR}/${base} ${n}.boardlib.json`;
  return path;
}

export async function listLibraries(vaultPath: string): Promise<ShapeLibrary[]> {
  const files = await invoke<{ path: string; content: string }[]>('list_board_libraries', { vaultPath });
  const out: ShapeLibrary[] = [];
  for (const f of files) {
    try {
      const lib = readOwn(JSON.parse(f.content), f.path);
      if (lib) out.push({ ...lib, name: lib.name || f.path.split('/').pop()!.replace(/\.boardlib\.json$/, '') });
    } catch {
      // A library that cannot be read is left out, not shown broken.
    }
  }
  return out;
}

export function saveLibrary(vaultPath: string, library: ShapeLibrary): Promise<void> {
  return invoke('write_board_library', { vaultPath, path: library.path, content: serialise(library) });
}

/** The library goes to the vault's trash, like anything else deleted. */
export function trashLibrary(vaultPath: string, path: string): Promise<string> {
  return invoke('trash_node_file', { vaultPath, relPath: path });
}

/** A piece made of items on a board: copies of them, moved to start at 0,0. */
export function itemFrom(title: string, nodes: WBNode[], edges: WBEdge[]): LibraryItem {
  const kept = new Set(nodes.map((n) => n.id));
  const copy = <T>(v: T): T => JSON.parse(JSON.stringify(v));
  const inside = edges.filter((e) => kept.has(e.source) && kept.has(e.target));
  return { id: newId(), ...stamp(), title, ...atOrigin(copy(nodes), copy(inside)) };
}
