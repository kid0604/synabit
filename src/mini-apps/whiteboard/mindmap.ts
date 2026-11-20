/**
 * Mind maps as trees: their shape, their layout, and their text.
 *
 * A mind map on a board is mind-map items joined by lines from parent to
 * child. Nothing else marks it as one, so everything here works that tree out
 * from the items and lines each time it is asked. A line drawn by hand
 * between two of them is a relationship (`data.relation`), not a branch.
 */
import type { WBEdge, WBNode } from './boardFile';

export interface Size { width: number; height: number }

/**
 * A mind map's colours: the first is a central idea's, the rest its main
 * branches' in turn. Syn's maps take the same ones (`syn/mindmap.rs`).
 */
export const MINDMAP_COLORS = [
  '#7c3aed', '#3b82f6', '#10b981', '#f59e0b', '#ef4444',
  '#ec4899', '#8b5cf6', '#06b6d4', '#84cc16', '#f97316',
];

/** Space between a parent and its children, and between siblings. */
const H_GAP = 64;
const V_GAP = 16;

/**
 * Whether a line is a relationship between two ideas rather than a branch.
 *
 * Every line between two mind-map items used to be a branch: one drawn with
 * the connector to say "these two are related" gave the second a parent it
 * did not have, and the next tidy moved it there.
 */
export function isRelation(edge: WBEdge): boolean {
  return !!edge.data?.relation;
}

/** Each mind-map item's children, in the order their lines were drawn. */
export function childrenOf(nodes: WBNode[], edges: WBEdge[]): Map<string, string[]> {
  const isMind = new Set(nodes.filter((n) => n.type === 'mindmap').map((n) => n.id));
  const children = new Map<string, string[]>();
  for (const e of edges) {
    if (!isMind.has(e.source) || !isMind.has(e.target) || e.source === e.target || isRelation(e)) continue;
    const list = children.get(e.source) ?? [];
    if (!list.includes(e.target)) list.push(e.target);
    children.set(e.source, list);
  }
  return children;
}

/**
 * Each mind-map item's parent, by the lines of the tree only.
 *
 * An arrow drawn to a mind-map item from a sticky note is not the note
 * becoming its parent: found by any line, Enter on a central idea an arrow
 * pointed at grew a branch off the note.
 */
export function parentsOf(nodes: WBNode[], edges: WBEdge[]): Map<string, string> {
  const parent = new Map<string, string>();
  for (const [p, kids] of childrenOf(nodes, edges)) for (const k of kids) if (!parent.has(k)) parent.set(k, p);
  return parent;
}

/** The root of the tree an item is in: follow parents up until there are none. */
export function rootOf(id: string, nodes: WBNode[], edges: WBEdge[]): string {
  const parent = parentsOf(nodes, edges);
  const seen = new Set<string>();
  let at = id;
  while (parent.has(at) && !seen.has(at)) {
    seen.add(at);
    at = parent.get(at)!;
  }
  return at;
}

/** Everything below an item, however deep. */
export function descendantsOf(id: string, children: Map<string, string[]>): string[] {
  const out: string[] = [];
  const stack = [...(children.get(id) ?? [])];
  const seen = new Set([id]);
  while (stack.length) {
    const next = stack.pop()!;
    if (seen.has(next)) continue;
    seen.add(next);
    out.push(next);
    stack.push(...(children.get(next) ?? []));
  }
  return out;
}

/** Items hidden because a branch above them is folded. */
export function hiddenByCollapse(nodes: WBNode[], edges: WBEdge[]): Set<string> {
  const children = childrenOf(nodes, edges);
  const hidden = new Set<string>();
  for (const n of nodes) {
    if (n.type === 'mindmap' && n.data?.collapsed) for (const d of descendantsOf(n.id, children)) hidden.add(d);
  }
  return hidden;
}

/** The widest a mind-map item grows before its words wrap (`.wb-mindmap-node`). */
export const MAX_ITEM_WIDTH = 320;

/** A guess at an item's size before it has been drawn and can be measured. */
export function estimateSize(node: WBNode): Size {
  const { text, tags } = tagsOf(String(node.data?.label ?? ''));
  const lines = text.split('\n');
  const level = node.data?.level ?? 1;
  const perChar = level === 0 ? 9 : 8;
  const longest = Math.max(...lines.map((l) => l.length));
  // A checkbox and a mark sit before the words, a count of tasks after them.
  const extra = (node.data?.task ? 22 : 0) + (node.data?.icon ? 22 : 0);
  const width = Math.max(level === 0 ? 140 : 100, Math.min(MAX_ITEM_WIDTH, longest * perChar + 40 + extra));
  const wrapped = lines.reduce((sum, l) => sum + Math.max(1, Math.ceil((l.length * perChar) / (MAX_ITEM_WIDTH - 40 - extra))), 0);
  const line = level === 0 ? 20 : 18;
  return { width, height: (level === 0 ? 44 : 38) + (wrapped - 1) * line + (tags.length ? 22 : 0) };
}

/**
 * Where every item of a mind map goes, laid out as a tree from its root.
 *
 * Children sit in a column beside their parent — to the right, or to the left
 * for the root's left-hand branches — and each child takes as much height as
 * its own branch needs, so branches never overlap however deep they go. A
 * folded branch takes only the height of the item it is folded into.
 *
 * The root stays where it is, and so does an item placed by hand
 * (`data.pinned`, from a drag with Alt): it takes no room in its parent's
 * column, and its own branch is laid out from where it was put. Without that,
 * every add put back whatever had been moved.
 */
export function tidyTree(
  rootId: string,
  nodes: WBNode[],
  edges: WBEdge[],
  sizeOf: (node: WBNode) => Size,
): Map<string, { x: number; y: number }> {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const children = childrenOf(nodes, edges);
  const root = byId.get(rootId);
  const out = new Map<string, { x: number; y: number }>();
  if (!root) return out;
  if (layoutOf(rootId, nodes) === 'down') return tidyDown(root, byId, children, sizeOf);

  const kidsOf = (id: string): WBNode[] =>
    byId.get(id)?.data?.collapsed ? [] : (children.get(id) ?? []).map((k) => byId.get(k)).filter((n): n is WBNode => !!n);

  // How tall each branch is, worked out once from the leaves up.
  const heights = new Map<string, number>();
  const branchHeight = (node: WBNode, seen = new Set<string>()): number => {
    if (heights.has(node.id)) return heights.get(node.id)!;
    if (seen.has(node.id)) return sizeOf(node).height;
    seen.add(node.id);
    const kids = kidsOf(node.id).filter((k) => !k.data?.pinned);
    const own = sizeOf(node).height;
    const stacked = kids.reduce((sum, k) => sum + branchHeight(k, seen), 0) + V_GAP * Math.max(0, kids.length - 1);
    const h = Math.max(own, stacked);
    heights.set(node.id, h);
    return h;
  };

  const place = (parent: WBNode, at: { x: number; y: number }, all: WBNode[], side: 'left' | 'right', seen: Set<string>) => {
    const parentSize = sizeOf(parent);
    // Placed by hand: left where they are, their branches laid out from there,
    // on the side of the parent they were put.
    for (const kid of all.filter((k) => k.data?.pinned)) {
      if (seen.has(kid.id)) continue;
      seen.add(kid.id);
      const own = sizeOf(kid);
      const left = kid.position.x + own.width / 2 < at.x + parentSize.width / 2;
      place(kid, kid.position, kidsOf(kid.id), left ? 'left' : 'right', seen);
    }
    const kids = all.filter((k) => !k.data?.pinned);
    if (!kids.length) return;
    const total = kids.reduce((sum, k) => sum + branchHeight(k), 0) + V_GAP * (kids.length - 1);
    let y = at.y + parentSize.height / 2 - total / 2;
    for (const kid of kids) {
      if (seen.has(kid.id)) continue;
      seen.add(kid.id);
      const size = sizeOf(kid);
      const branch = branchHeight(kid);
      const x = side === 'right' ? at.x + parentSize.width + H_GAP : at.x - H_GAP - size.width;
      const pos = { x: Math.round(x), y: Math.round(y + branch / 2 - size.height / 2) };
      out.set(kid.id, pos);
      place(kid, pos, kidsOf(kid.id), side, seen);
      y += branch + V_GAP;
    }
  };

  const seen = new Set([root.id]);
  const rootKids = kidsOf(root.id);
  place(root, root.position, rootKids.filter((k) => k.data?.direction !== 'left'), 'right', seen);
  place(root, root.position, rootKids.filter((k) => k.data?.direction === 'left'), 'left', seen);
  return out;
}

// ── Layouts ─────────────────────────────────────────────────

/**
 * How a map is laid out, kept on its central idea (`data.layout`): a map with
 * branches on both sides (the default, MindNode's and XMind's first), a logic
 * chart growing to the right, or an org chart growing downwards. Three, not
 * XMind's nine: these are the ones a map is read in; the rest are diagrams,
 * and `draw_board` draws diagrams.
 */
export type MapLayout = 'map' | 'right' | 'down';

export function layoutOf(rootId: string, nodes: WBNode[]): MapLayout {
  const l = nodes.find((n) => n.id === rootId)?.data?.layout;
  return l === 'down' || l === 'right' ? l : 'map';
}

/** The handles a branch joins its parent by, for the map's layout and the child's side. */
export function handlesFor(layout: MapLayout, side: 'left' | 'right'): { sourceHandle: string; targetHandle: string } {
  if (layout === 'down') return { sourceHandle: 'bottom-source', targetHandle: 'top-target' };
  return side === 'left'
    ? { sourceHandle: 'left-source', targetHandle: 'right-target' }
    : { sourceHandle: 'right-source', targetHandle: 'left-target' };
}

/** Each mind-map item's map's layout, for drawing its buttons: worked out once for the board. */
export function mapLayouts(nodes: WBNode[], edges: WBEdge[]): Map<string, MapLayout> {
  const parents = parentsOf(nodes, edges);
  const out = new Map<string, MapLayout>();
  const rootIdOf = (id: string) => {
    let at = id;
    for (const seen = new Set<string>(); parents.has(at) && !seen.has(at);) { seen.add(at); at = parents.get(at)!; }
    return at;
  };
  const byRoot = new Map<string, MapLayout>();
  for (const n of nodes) {
    if (n.type !== 'mindmap') continue;
    const r = rootIdOf(n.id);
    if (!byRoot.has(r)) byRoot.set(r, layoutOf(r, nodes));
    out.set(n.id, byRoot.get(r)!);
  }
  return out;
}

/** Space in an org chart: between a parent and the row below it, and between items in a row. */
const DOWN_V_GAP = 56;
const DOWN_H_GAP = 24;

/** `tidyTree` for a map laid out downwards: children in a row under their parent, centred on it. */
function tidyDown(
  root: WBNode,
  byId: Map<string, WBNode>,
  children: Map<string, string[]>,
  sizeOf: (node: WBNode) => Size,
): Map<string, { x: number; y: number }> {
  const out = new Map<string, { x: number; y: number }>();
  const kidsOf = (id: string): WBNode[] =>
    byId.get(id)?.data?.collapsed ? [] : (children.get(id) ?? []).map((k) => byId.get(k)).filter((n): n is WBNode => !!n);
  const widths = new Map<string, number>();
  const branchWidth = (node: WBNode, seen = new Set<string>()): number => {
    if (widths.has(node.id)) return widths.get(node.id)!;
    if (seen.has(node.id)) return sizeOf(node).width;
    seen.add(node.id);
    const kids = kidsOf(node.id).filter((k) => !k.data?.pinned);
    const row = kids.reduce((sum, k) => sum + branchWidth(k, seen), 0) + DOWN_H_GAP * Math.max(0, kids.length - 1);
    const w = Math.max(sizeOf(node).width, row);
    widths.set(node.id, w);
    return w;
  };
  const place = (parent: WBNode, at: { x: number; y: number }, seen: Set<string>) => {
    const own = sizeOf(parent);
    const all = kidsOf(parent.id);
    for (const kid of all.filter((k) => k.data?.pinned)) {
      if (seen.has(kid.id)) continue;
      seen.add(kid.id);
      place(kid, kid.position, seen);
    }
    const kids = all.filter((k) => !k.data?.pinned);
    if (!kids.length) return;
    const total = kids.reduce((sum, k) => sum + branchWidth(k), 0) + DOWN_H_GAP * (kids.length - 1);
    let x = at.x + own.width / 2 - total / 2;
    const y = at.y + own.height + DOWN_V_GAP;
    for (const kid of kids) {
      if (seen.has(kid.id)) continue;
      seen.add(kid.id);
      const w = branchWidth(kid);
      const pos = { x: Math.round(x + w / 2 - sizeOf(kid).width / 2), y: Math.round(y) };
      out.set(kid.id, pos);
      place(kid, pos, seen);
      x += w + DOWN_H_GAP;
    }
  };
  place(root, root.position, new Set([root.id]));
  return out;
}

/**
 * Join every branch of a map by the handles its layout wants: after an add, a
 * move or a change of layout, so no line leaves from a side its map no longer
 * grows on. Returns the lines changed.
 */
export function fixHandles(rootId: string, nodes: WBNode[], edges: WBEdge[]): WBEdge[] {
  const layout = layoutOf(rootId, nodes);
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const children = childrenOf(nodes, edges);
  const inMap = new Set([rootId, ...descendantsOf(rootId, children)]);
  const changed: WBEdge[] = [];
  for (const e of edges) {
    if (isRelation(e) || !inMap.has(e.source) || !inMap.has(e.target) || !children.get(e.source)?.includes(e.target)) continue;
    const side = byId.get(e.target)?.data?.direction === 'left' ? 'left' : 'right';
    const want = handlesFor(layout, side);
    if (e.sourceHandle === want.sourceHandle && e.targetHandle === want.targetHandle) continue;
    e.sourceHandle = want.sourceHandle;
    e.targetHandle = want.targetHandle;
    changed.push(e);
  }
  return changed;
}

/**
 * Lay a map out another way: the layout kept on its root, every item's side
 * set for it — shared between both sides for a map, all to the right
 * otherwise — and the lines joined to match. Items placed by hand go back into
 * the layout: a place chosen for one layout means nothing in another. Returns
 * the items changed; the caller lays the map out (`tidyTree`).
 */
export function setLayout(rootId: string, layout: MapLayout, nodes: WBNode[], edges: WBEdge[]): WBNode[] {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const root = byId.get(rootId);
  if (!root) return [];
  const children = childrenOf(nodes, edges);
  const changed: WBNode[] = [];
  const set = (n: WBNode, data: Record<string, any>) => {
    const next = { ...n.data, ...data };
    for (const k of Object.keys(data)) if (data[k] === undefined) delete next[k];
    if (Object.keys(data).every((k) => n.data?.[k] === next[k])) return;
    n.data = next;
    changed.push(n);
  };
  set(root, { layout: layout === 'map' ? undefined : layout, pinned: undefined });
  const mains = children.get(rootId) ?? [];
  const right = layout === 'map' && mains.length > 1 ? Math.ceil(mains.length / 2) : mains.length;
  const walk = (id: string, side: 'left' | 'right', seen: Set<string>) => {
    if (seen.has(id)) return;
    seen.add(id);
    const n = byId.get(id);
    if (!n) return;
    set(n, { direction: side, pinned: undefined });
    for (const k of children.get(id) ?? []) walk(k, side, seen);
  };
  const seen = new Set([rootId]);
  mains.forEach((k, i) => walk(k, i < right ? 'right' : 'left', seen));
  fixHandles(rootId, nodes, edges);
  return changed;
}

/** A mind map as an indented list, one item per line — what a note shows as an outline. */
export function toOutline(rootId: string, nodes: WBNode[], edges: WBEdge[]): string {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const children = childrenOf(nodes, edges);
  const lines: string[] = [];
  const seen = new Set<string>();
  const walk = (id: string, depth: number) => {
    if (seen.has(id)) return;
    seen.add(id);
    const node = byId.get(id);
    const label = String(node?.data?.label ?? '').replace(/\s+/g, ' ').trim();
    const link = typeof node?.data?.link === 'string' ? node.data.link : '';
    const box = node?.data?.task ? (node.data.done ? '[x] ' : '[ ] ') : '';
    const icon = typeof node?.data?.icon === 'string' && node.data.icon ? `${node.data.icon} ` : '';
    lines.push(`${'  '.repeat(depth)}- ${box}${icon}${link ? `[${label.replace(/[[\]]/g, '')}](${link})` : label}`);
    for (const k of children.get(id) ?? []) walk(k, depth + 1);
  };
  walk(rootId, 0);
  return lines.join('\n');
}

/**
 * One idea of an outline and what grows from it: `link` is where it points, a
 * note or a page; `task` makes it a checkbox, `done` ticked; `icon` its mark.
 */
export interface OutlineItem { label: string; children: OutlineItem[]; link?: string; task?: boolean; done?: boolean; icon?: string }

// ── Tasks, marks and tags ───────────────────────────────────

/**
 * The marks an idea can carry, before its words: priority, a question, done,
 * an idea, a warning. A short list chosen for what a map says, not an emoji
 * keyboard; any emoji typed into a label is still just a word in it.
 */
export const ICONS = ['⭐', '❗', '❓', '💡', '⚠️', '🔥', '🎯', '📌', '💰', '📅', '👤', '🚩', '1️⃣', '2️⃣', '3️⃣', '✅'];

/** A label's words and its `#tags`, apart: the tags are drawn as chips, the words as the label. */
export function tagsOf(label: string): { text: string; tags: string[] } {
  const tags: string[] = [];
  const text = label
    .replace(/(^|\s)#([\p{L}\p{N}_-]+)/gu, (_m, space: string, tag: string) => {
      if (!tags.includes(tag)) tags.push(tag);
      return space;
    })
    .replace(/[ \t]{2,}/g, ' ')
    .trim();
  return { text, tags };
}

/**
 * How far along each idea's tasks are: for an item with tasks below it, how
 * many of them are done. Counted from the tasks only — an idea that is not
 * one is a heading over them — and every level down, so a main branch shows
 * the whole of its part of the plan.
 */
export function taskProgress(nodes: WBNode[], edges: WBEdge[]): Map<string, { done: number; total: number }> {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const children = childrenOf(nodes, edges);
  const out = new Map<string, { done: number; total: number }>();
  const count = (id: string, seen: Set<string>): { done: number; total: number } => {
    if (out.has(id)) return out.get(id)!;
    const sum = { done: 0, total: 0 };
    if (seen.has(id)) return sum;
    seen.add(id);
    for (const k of children.get(id) ?? []) {
      const kid = byId.get(k);
      const below = count(k, seen);
      // A task with tasks under it is as done as they are; one without, as it is.
      if (kid?.data?.task && !below.total) { sum.total += 1; if (kid.data.done) sum.done += 1; }
      sum.done += below.done;
      sum.total += below.total;
    }
    if (sum.total) out.set(id, sum);
    return sum;
  };
  for (const n of nodes) if (n.type === 'mindmap') count(n.id, new Set());
  return out;
}

/**
 * An indented list read as a tree, or null when the text is not one.
 *
 * At least two list lines, so that a pasted sentence that happens to start
 * with a dash stays a sentence. Indentation by spaces or tabs, markers `-`,
 * `*`, `+` or `1.`; several top-level items share a root named by `title`.
 */
export function fromOutline(text: string, title = ''): OutlineItem | null {
  const rows = text.split(/\r?\n/).filter((l) => l.trim());
  const listed = rows
    .map((l) => /^(\s*)(?:[-*+]|\d+[.)])\s+(.*)$/.exec(l.replace(/\t/g, '  ')))
    .filter((m): m is RegExpExecArray => !!m);
  if (listed.length < 2 || listed.length < rows.length) return null;

  const top: OutlineItem = { label: title, children: [] };
  const stack: { indent: number; item: OutlineItem }[] = [{ indent: -1, item: top }];
  for (const m of listed) {
    const indent = m[1].length;
    const item = idea(m[2]);
    while (stack.length > 1 && stack[stack.length - 1].indent >= indent) stack.pop();
    stack[stack.length - 1].item.children.push(item);
    stack.push({ indent, item });
  }
  return top.children.length === 1 && !title ? top.children[0] : top;
}

/**
 * One line of an outline as an idea: its words without Markdown's marks, and
 * where it points when the line is a link — `[Plan](synabit://note/…)`,
 * `[[Plan]]`, a web page. A checkbox is left behind; it says nothing a map shows.
 */
function idea(raw: string): OutlineItem {
  const box = /^\[( |x)\]\s*/i.exec(raw.trim());
  let text = raw.trim().slice(box?.[0].length ?? 0).trim();
  const icon = ICONS.find((i) => text.startsWith(`${i} `));
  if (icon) text = text.slice(icon.length).trim();
  const extra: Partial<OutlineItem> = {
    ...(box ? { task: true, ...(box[1].toLowerCase() === 'x' ? { done: true } : {}) } : {}),
    ...(icon ? { icon } : {}),
  };
  let link: string | undefined;
  const md = /^\[([^\]]*)\]\(([^)\s]+)\)$/.exec(text);
  if (md) {
    text = md[1];
    link = md[2];
  }
  text = text
    .replace(/\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g, (_m, target: string, alias?: string) => alias ?? target)
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/(\*\*|__|~~|`)(.+?)\1/g, '$2')
    .replace(/(^|\W)[*_](\S(?:.*?\S)?)[*_](?=\W|$)/g, '$1$2')
    .trim();
  return { label: text, children: [], ...(link ? { link } : {}), ...extra };
}

/**
 * A markmap block's Markdown as a tree, or null when it holds none.
 *
 * What markmap reads, so the map in a note and the map on a board are one
 * tree: headings are levels (`#` the top), a list under a heading hangs below
 * it, and its front matter (`---` options) is not part of the map. Several
 * top-level ideas share a root named by `title`; one is the root itself. A list on its own is read
 * as `fromOutline` reads it, but one line is enough here: the block says it
 * is a map.
 */
export function fromMarkdown(text: string, title = ''): OutlineItem | null {
  const body = text.replace(/^\uFEFF?---\r?\n[\s\S]*?\r?\n---\r?\n/, '');
  const top: OutlineItem = { label: title, children: [] };
  // Each open idea and its depth: headings 1–6, a list item below its heading by its indent.
  const stack: { depth: number; item: OutlineItem }[] = [{ depth: -1, item: top }];
  let headingDepth = 0;
  let fenced = false;
  for (const raw of body.split(/\r?\n/)) {
    if (/^\s*(```|~~~)/.test(raw)) { fenced = !fenced; continue; }
    if (fenced || !raw.trim()) continue;
    const line = raw.replace(/\t/g, '  ');
    const heading = /^(#{1,6})\s+(.*?)\s*#*\s*$/.exec(line);
    const listed = /^(\s*)(?:[-*+]|\d+[.)])\s+(.*)$/.exec(line);
    let depth: number;
    let words: string;
    if (heading) {
      depth = heading[1].length * 100;
      headingDepth = depth;
      words = heading[2];
    } else if (listed) {
      depth = headingDepth + 1 + listed[1].length;
      words = listed[2];
    } else {
      // A paragraph under an idea is its note, not a branch of the map.
      continue;
    }
    const item = idea(words);
    if (!item.label) continue;
    while (stack.length > 1 && stack[stack.length - 1].depth >= depth) stack.pop();
    stack[stack.length - 1].item.children.push(item);
    stack.push({ depth, item });
  }
  if (!top.children.length) return null;
  return top.children.length === 1 ? top.children[0] : top;
}

/** How many ideas a tree holds, its root among them. */
export function outlineSize(tree: OutlineItem): number {
  return 1 + tree.children.reduce((sum, k) => sum + outlineSize(k), 0);
}

/**
 * A tree as mind-map items and the lines between them, laid out, its root at `at`.
 *
 * The root's branches are shared between its two sides, the first half on
 * the right — a map read from a list, a file or Syn is balanced, as MindNode
 * and XMind draw one; branches added by hand go where they are put. Each main
 * branch has a colour of its own (`palette` after its first, the root's).
 */
export function buildMindmap(
  tree: OutlineItem,
  at: { x: number; y: number },
  palette: string[],
  newId: (prefix: string) => string,
): { rootId: string; nodes: WBNode[]; edges: WBEdge[] } {
  const nodes: WBNode[] = [];
  const edges: WBEdge[] = [];
  const branches = palette.slice(1);
  const mains = tree.children.length;
  const rightCount = mains > 3 ? Math.ceil(mains / 2) : mains;
  const add = (item: OutlineItem, level: number, parentId: string | null, colour: string, side: 'left' | 'right'): string => {
    const id = newId('mind');
    nodes.push({
      id, type: 'mindmap', position: { ...at },
      data: {
        label: item.label, color: colour, level, direction: side,
        ...(item.link ? { link: item.link } : {}),
        ...(item.task ? { task: true } : {}), ...(item.done ? { done: true } : {}),
        ...(item.icon ? { icon: item.icon } : {}),
      },
    });
    if (parentId) {
      edges.push({
        id: newId('e'), source: parentId, target: id, type: 'default', data: {},
        sourceHandle: side === 'left' ? 'left-source' : 'right-source',
        targetHandle: side === 'left' ? 'right-target' : 'left-target',
      });
    }
    item.children.forEach((child, i) => {
      const childSide = level === 0 ? (i < rightCount ? 'right' : 'left') : side;
      add(child, level + 1, id, level === 0 ? branches[i % branches.length] : colour, childSide);
    });
    return id;
  };
  const rootId = add(tree, 0, null, palette[0], 'right');
  const placed = tidyTree(rootId, nodes, edges, estimateSize);
  for (const n of nodes) {
    const p = placed.get(n.id);
    if (p) n.position = p;
  }
  return { rootId, nodes, edges };
}

/** Text made safe inside an XML attribute. */
function xmlAttr(text: string): string {
  return text.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/\n/g, '&#10;');
}

/**
 * A mind map as OPML, the outline format other mind-map and outliner apps
 * open: MindNode, XMind, OmniOutliner, Logseq. Labels, links (`url`) and the
 * tree; nothing of how it is drawn.
 */
export function toOpml(rootId: string, nodes: WBNode[], edges: WBEdge[]): string {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const children = childrenOf(nodes, edges);
  const seen = new Set<string>();
  const out: string[] = [];
  const walk = (id: string, depth: number) => {
    if (seen.has(id)) return;
    seen.add(id);
    const node = byId.get(id);
    const label = String(node?.data?.label ?? '');
    const link = typeof node?.data?.link === 'string' && /^https?:/i.test(node.data.link) ? ` url="${xmlAttr(node.data.link)}"` : '';
    const kids = (children.get(id) ?? []).filter((k) => byId.has(k));
    const pad = '  '.repeat(depth + 2);
    if (!kids.length) { out.push(`${pad}<outline text="${xmlAttr(label)}"${link}/>`); return; }
    out.push(`${pad}<outline text="${xmlAttr(label)}"${link}>`);
    for (const k of kids) walk(k, depth + 1);
    out.push(`${pad}</outline>`);
  };
  walk(rootId, 0);
  const title = xmlAttr(String(byId.get(rootId)?.data?.label ?? ''));
  return `<?xml version="1.0" encoding="UTF-8"?>\n<opml version="2.0">\n  <head><title>${title}</title></head>\n  <body>\n${out.join('\n')}\n  </body>\n</opml>\n`;
}

/**
 * What deleting these items takes with it: every mind-map item below one.
 *
 * Deleting a branch's item used to leave what was under it on the board as
 * loose trees of their own, still coloured and sized as the branch they had
 * been part of — and under a folded item, shown again in a heap where they
 * were hidden. A mind-map tool deletes the branch.
 */
export function withBranches(ids: string[], nodes: WBNode[], edges: WBEdge[]): string[] {
  const mind = new Set(nodes.filter((n) => n.type === 'mindmap').map((n) => n.id));
  const out = new Set(ids);
  const children = childrenOf(nodes, edges);
  for (const id of ids) if (mind.has(id)) for (const d of descendantsOf(id, children)) out.add(d);
  return [...out];
}

/**
 * The colour for a new main branch of `rootId`: the first of `palette` none
 * of its branches has yet, after the root's own; then round again.
 *
 * Colours went by level, so every main branch was the same colour and the
 * branches could not be told apart — while the template coloured them one
 * each, and its first added item came out in another scheme.
 */
export function nextBranchColour(rootId: string, nodes: WBNode[], edges: WBEdge[], palette: string[]): string {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const own = String(byId.get(rootId)?.data?.color ?? palette[0]).toLowerCase();
  const choices = palette.filter((c) => c.toLowerCase() !== own);
  const kids = childrenOf(nodes, edges).get(rootId) ?? [];
  const taken = new Set(kids.map((k) => String(byId.get(k)?.data?.color ?? '').toLowerCase()));
  return choices.find((c) => !taken.has(c.toLowerCase())) ?? choices[kids.length % choices.length];
}

/** Where a dropped branch goes: under which item, at which place among its children, on which side. */
export interface DropPlan { parentId: string; index: number; direction: 'left' | 'right' }

/**
 * What dropping a mind-map item where it now is means for the tree.
 *
 * On another item (its middle over that item), the branch moves under it, as
 * its last child. Anywhere else it stays with its parent, in the place among
 * its brothers and sisters that its height says — and beside a central idea,
 * on the side it was dropped. A root is not restructured: dragging it moves
 * the map. Neither is an item onto its own branch.
 *
 * Before this a branch could not be moved or reordered at all, and the
 * layout that runs on every add put back whatever had been dragged by hand.
 */
export function dropPlan(id: string, nodes: WBNode[], edges: WBEdge[], sizeOf: (node: WBNode) => Size): DropPlan | null {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const me = byId.get(id);
  const parents = parentsOf(nodes, edges);
  const was = parents.get(id);
  if (!me || me.type !== 'mindmap' || !was) return null;
  const children = childrenOf(nodes, edges);
  const mine = new Set([id, ...descendantsOf(id, children)]);
  const hidden = hiddenByCollapse(nodes, edges);
  const middle = (n: WBNode) => {
    const s = sizeOf(n);
    return { x: n.position.x + s.width / 2, y: n.position.y + s.height / 2 };
  };
  const at = middle(me);
  const over = (n: WBNode) => {
    const s = sizeOf(n);
    return at.x >= n.position.x && at.x <= n.position.x + s.width && at.y >= n.position.y && at.y <= n.position.y + s.height;
  };
  const onto = nodes.find((n) => n.type === 'mindmap' && !mine.has(n.id) && !hidden.has(n.id) && !n.data?.locked && over(n));
  const parentId = onto?.id ?? was;
  const parent = byId.get(parentId)!;
  const central = !parents.has(parentId);
  const direction: 'left' | 'right' = central
    ? (at.x < middle(parent).x ? 'left' : 'right')
    : (parent.data?.direction === 'left' ? 'left' : 'right');
  const siblings = (children.get(parentId) ?? []).filter((k) => k !== id);
  if (onto) return { parentId, index: siblings.length, direction };
  // In an org chart a row runs across: the place is by how far along it is.
  let root = parentId;
  for (const seen = new Set<string>(); parents.has(root) && !seen.has(root);) { seen.add(root); root = parents.get(root)!; }
  if (layoutOf(root, nodes) === 'down') {
    const after = siblings.find((k) => middle(byId.get(k)!).x > at.x);
    return { parentId, index: after !== undefined ? siblings.indexOf(after) : siblings.length, direction: 'right' };
  }
  const side = (k: string) => !central || (byId.get(k)?.data?.direction === 'left' ? 'left' : 'right') === direction;
  const sameSide = siblings.filter(side);
  const below = sameSide.find((k) => middle(byId.get(k)!).y > at.y);
  const index = below !== undefined
    ? siblings.indexOf(below)
    : sameSide.length ? siblings.indexOf(sameSide[sameSide.length - 1]) + 1 : siblings.length;
  return { parentId, index, direction };
}

/**
 * Move a branch as `plan` says, in place: its line from its parent, its
 * place among the children, and its items' level, side and colour. Items
 * that took the branch's colour take the new one; one coloured by hand
 * keeps its own. Returns the items and lines changed.
 */
export function moveBranch(
  id: string,
  plan: DropPlan,
  nodes: WBNode[],
  edges: WBEdge[],
  palette: string[],
): { nodes: WBNode[]; edges: WBEdge[] } {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const parents = parentsOf(nodes, edges);
  const me = byId.get(id);
  const was = parents.get(id);
  const parent = byId.get(plan.parentId);
  const changed = { nodes: [] as WBNode[], edges: [] as WBEdge[] };
  if (!me || !was || !parent) return changed;
  // Dropped back where it was: nothing to change but the layout.
  const now = childrenOf(nodes, edges).get(was) ?? [];
  if (was === plan.parentId && now.indexOf(id) === plan.index && (me.data?.direction === 'left' ? 'left' : 'right') === plan.direction) {
    return changed;
  }
  const mind = (nid: string) => byId.get(nid)?.type === 'mindmap';
  const handles = (e: WBEdge, side: 'left' | 'right') => {
    e.sourceHandle = side === 'left' ? 'left-source' : 'right-source';
    e.targetHandle = side === 'left' ? 'right-target' : 'left-target';
  };

  // The line from the parent: taken out, pointed at the new one, and put
  // back among that one's lines where the child's place is.
  const from = edges.findIndex((e) => e.source === was && e.target === id);
  if (from < 0) return changed;
  const [line] = edges.splice(from, 1);
  line.source = plan.parentId;
  handles(line, plan.direction);
  const theirs = edges.map((e, i) => [e, i] as const).filter(([e]) => e.source === plan.parentId && mind(e.target));
  const to = plan.index < theirs.length
    ? theirs[plan.index][1]
    : theirs.length ? theirs[theirs.length - 1][1] + 1 : edges.length;
  edges.splice(to, 0, line);
  changed.edges.push(line);

  // The branch, from its top: a level below its parent, on its side, in its colour.
  const central = !parents.has(plan.parentId);
  const before = String(me.data?.color ?? '');
  const colour = central
    ? (parents.get(id) === plan.parentId && before ? before : nextBranchColour(plan.parentId, nodes, edges.filter((e) => e !== line), palette))
    : String(parent.data?.color ?? before);
  const children = childrenOf(nodes, edges);
  const top = (parent.data?.level ?? 0) + 1;
  const walk = (nid: string, depth: number, seen: Set<string>) => {
    if (seen.has(nid)) return;
    seen.add(nid);
    const n = byId.get(nid);
    if (!n) return;
    n.data = { ...n.data, level: top + depth, direction: plan.direction, ...(nid === id || n.data?.color === before ? { color: colour } : {}) };
    changed.nodes.push(n);
    for (const k of children.get(nid) ?? []) walk(k, depth + 1, seen);
  };
  walk(id, 0, new Set());
  const inBranch = new Set(changed.nodes.map((n) => n.id));
  for (const e of edges) {
    if (e !== line && inBranch.has(e.source) && inBranch.has(e.target)) {
      handles(e, plan.direction);
      changed.edges.push(e);
    }
  }
  return changed;
}

/**
 * How a branch's line is drawn when nothing was chosen for it: in its
 * branch's colour, thicker near the centre and thinner towards the leaves —
 * so a branch reads as one thing from the middle out. A relationship, or a
 * line to anything that is not a mind-map item, is drawn as any other line.
 */
export function branchLine(edge: WBEdge, nodeOf: (id: string) => WBNode | undefined): { color: string; width: number } | null {
  if (isRelation(edge)) return null;
  const from = nodeOf(edge.source);
  const to = nodeOf(edge.target);
  if (from?.type !== 'mindmap' || to?.type !== 'mindmap' || !to.data?.color) return null;
  const level = Number(to.data?.level ?? 1);
  return { color: String(to.data.color), width: level <= 1 ? 4 : level === 2 ? 3 : 2 };
}

/**
 * The item an arrow key goes to from a mind-map item, the way MindNode and
 * XMind go: outwards to the child nearest in height, inwards to the parent,
 * up and down among siblings on the same side. Nothing past a folded item.
 */
export function neighbour(id: string, key: string, nodes: WBNode[], edges: WBEdge[]): string | null {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const me = byId.get(id);
  if (!me || me.type !== 'mindmap') return null;
  const parents = parentsOf(nodes, edges);
  const children = childrenOf(nodes, edges);
  const parent = parents.get(id);
  const sideOf = (n: WBNode | undefined): 'left' | 'right' => (n?.data?.direction === 'left' ? 'left' : 'right');
  const kids = (side?: 'left' | 'right') =>
    me.data?.collapsed ? [] : (children.get(id) ?? []).filter((k) => !side || sideOf(byId.get(k)) === side);
  const nearest = (ids: string[]) =>
    ids.reduce<string | null>((best, k) => {
      const dy = (n: string) => Math.abs((byId.get(n)?.position.y ?? 0) - me.position.y);
      return best === null || dy(k) < dy(best) ? k : best;
    }, null);
  const outwards = (side: 'left' | 'right') => nearest(parent ? kids() : kids(side));

  // An org chart is read downwards: down to the child nearest across, up to
  // the parent, left and right along the row.
  let root = id;
  for (const seen = new Set<string>(); parents.has(root) && !seen.has(root);) { seen.add(root); root = parents.get(root)!; }
  if (layoutOf(root, nodes) === 'down') {
    if (key === 'ArrowUp') return parent ?? null;
    if (key === 'ArrowDown') {
      return kids().reduce<string | null>((best, k) => {
        const dx = (n: string) => Math.abs((byId.get(n)?.position.x ?? 0) - me.position.x);
        return best === null || dx(k) < dx(best) ? k : best;
      }, null);
    }
    if ((key === 'ArrowLeft' || key === 'ArrowRight') && parent) {
      const row = children.get(parent) ?? [];
      return row[row.indexOf(id) + (key === 'ArrowLeft' ? -1 : 1)] ?? null;
    }
    return null;
  }

  if (key === 'ArrowRight') return parent && sideOf(me) === 'left' ? parent : outwards('right');
  if (key === 'ArrowLeft') return parent && sideOf(me) === 'right' ? parent : outwards('left');
  if ((key === 'ArrowUp' || key === 'ArrowDown') && parent) {
    const central = !parents.has(parent);
    const row = (children.get(parent) ?? []).filter((k) => !central || sideOf(byId.get(k)) === sideOf(me));
    const at = row.indexOf(id) + (key === 'ArrowUp' ? -1 : 1);
    return row[at] ?? null;
  }
  return null;
}

/**
 * The items a new colour goes to: the one chosen, and below it every item
 * that had its colour — the branch, not an item coloured apart from it.
 */
export function branchToRecolour(id: string, nodes: WBNode[], edges: WBEdge[]): string[] {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const was = String(byId.get(id)?.data?.color ?? '').toLowerCase();
  const below = descendantsOf(id, childrenOf(nodes, edges));
  return [id, ...below.filter((k) => String(byId.get(k)?.data?.color ?? '').toLowerCase() === was)];
}
