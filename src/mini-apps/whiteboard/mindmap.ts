/**
 * Mind maps as trees: their shape, their layout, and their text.
 *
 * A mind map on a board is mind-map items joined by lines from parent to
 * child. Nothing else marks it as one, so everything here works that tree out
 * from the items and lines each time it is asked.
 */
import type { WBEdge, WBNode } from './boardFile';

export interface Size { width: number; height: number }

/** Space between a parent and its children, and between siblings. */
const H_GAP = 64;
const V_GAP = 16;

/** Each mind-map item's children, in the order their lines were drawn. */
export function childrenOf(nodes: WBNode[], edges: WBEdge[]): Map<string, string[]> {
  const isMind = new Set(nodes.filter((n) => n.type === 'mindmap').map((n) => n.id));
  const children = new Map<string, string[]>();
  for (const e of edges) {
    if (!isMind.has(e.source) || !isMind.has(e.target) || e.source === e.target) continue;
    const list = children.get(e.source) ?? [];
    if (!list.includes(e.target)) list.push(e.target);
    children.set(e.source, list);
  }
  return children;
}

/** The root of the tree an item is in: follow parents up until there are none. */
export function rootOf(id: string, nodes: WBNode[], edges: WBEdge[]): string {
  const parent = new Map<string, string>();
  for (const [p, kids] of childrenOf(nodes, edges)) for (const k of kids) if (!parent.has(k)) parent.set(k, p);
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

/** A guess at an item's size before it has been drawn and can be measured. */
export function estimateSize(node: WBNode): Size {
  const label = String(node.data?.label ?? '');
  const level = node.data?.level ?? 1;
  const perChar = level === 0 ? 9 : 8;
  return { width: Math.max(level === 0 ? 140 : 100, Math.min(320, label.length * perChar + 40)), height: level === 0 ? 44 : 38 };
}

/**
 * Where every item of a mind map goes, laid out as a tree from its root.
 *
 * Children sit in a column beside their parent — to the right, or to the left
 * for the root's left-hand branches — and each child takes as much height as
 * its own branch needs, so branches never overlap however deep they go. A
 * folded branch takes only the height of the item it is folded into.
 *
 * The root stays where it is.
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

  const kidsOf = (id: string): WBNode[] =>
    byId.get(id)?.data?.collapsed ? [] : (children.get(id) ?? []).map((k) => byId.get(k)).filter((n): n is WBNode => !!n);

  // How tall each branch is, worked out once from the leaves up.
  const heights = new Map<string, number>();
  const branchHeight = (node: WBNode, seen = new Set<string>()): number => {
    if (heights.has(node.id)) return heights.get(node.id)!;
    if (seen.has(node.id)) return sizeOf(node).height;
    seen.add(node.id);
    const kids = kidsOf(node.id);
    const own = sizeOf(node).height;
    const stacked = kids.reduce((sum, k) => sum + branchHeight(k, seen), 0) + V_GAP * Math.max(0, kids.length - 1);
    const h = Math.max(own, stacked);
    heights.set(node.id, h);
    return h;
  };

  const place = (parent: WBNode, at: { x: number; y: number }, kids: WBNode[], side: 'left' | 'right', seen: Set<string>) => {
    if (!kids.length) return;
    const parentSize = sizeOf(parent);
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

/** A mind map as an indented list, one item per line — what a note shows as an outline. */
export function toOutline(rootId: string, nodes: WBNode[], edges: WBEdge[]): string {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const children = childrenOf(nodes, edges);
  const lines: string[] = [];
  const seen = new Set<string>();
  const walk = (id: string, depth: number) => {
    if (seen.has(id)) return;
    seen.add(id);
    const label = String(byId.get(id)?.data?.label ?? '').replace(/\s+/g, ' ').trim();
    lines.push(`${'  '.repeat(depth)}- ${label}`);
    for (const k of children.get(id) ?? []) walk(k, depth + 1);
  };
  walk(rootId, 0);
  return lines.join('\n');
}

export interface OutlineItem { label: string; children: OutlineItem[] }

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
    const item: OutlineItem = { label: m[2].replace(/\[( |x)\]\s*/i, '').trim(), children: [] };
    while (stack.length > 1 && stack[stack.length - 1].indent >= indent) stack.pop();
    stack[stack.length - 1].item.children.push(item);
    stack.push({ indent, item });
  }
  return top.children.length === 1 && !title ? top.children[0] : top;
}
