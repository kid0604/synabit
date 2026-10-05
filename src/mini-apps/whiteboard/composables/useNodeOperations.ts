import { type Ref } from 'vue';
import type { WBNode, WBEdge } from './useWhiteboardStore';
import { paint } from '../ink';

/** Where connectors are stacked: above any node. */
export const EDGE_Z = 1_000_000;
/** A line with no colour of its own is drawn in the theme's grey. */
export const EDGE_GREY = 'var(--wb-edge, #8b8b8b)';

/** Where comments are stacked: over every item, under the connectors. */
export const COMMENT_Z = 500_000;

/** Where frames are stacked: below any item that is not one. */
export const FRAME_Z = -100_000;

export function useNodeOperations(
  store: any,
  vfNodes: Ref<any[]>,
  vfEdges: Ref<any[]>,
  scheduleSave: () => void,
  /** What a screen reader says for a line, from what it joins. */
  spokenEdge?: (edge: WBEdge, nodes: WBNode[]) => string,
) {
  /**
   * Compute z-index for shape nodes based on area.
   * Smaller shapes get higher z-index so they are always clickable
   * above larger shapes that contain them (like Miro).
   */
  const computeShapeZIndex = (w: number, h: number): number => {
    const area = w * h;
    // Max area ~1000×1000 = 1_000_000. Invert so small = high z.
    return Math.max(1, Math.round(10000 - area / 100));
  };

  /**
   * Give a node on the canvas the size it carries in the board file.
   *
   * Shapes and pictures are sized by the user rather than by their content,
   * and the size has to reach the canvas node itself: the resizer drags that
   * element, so a component sizing its own box instead would stay put until
   * the drag ended. The z-index is by area for the same reason it is for
   * shapes — whatever is smaller sits on top, so a picture inside a frame
   * stays clickable.
   */
  const SIZED_TYPES = new Set(['shape', 'image', 'sticky', 'frame', 'card']);
  /** The size an item has before anyone resizes it. */
  const DEFAULT_SIZE: Record<string, [number, number]> = {
    shape: [160, 80], image: [320, 240], sticky: [200, 200], frame: [480, 320], card: [260, 120],
  };

  /**
   * Draw a node the way the board says: its stacking, and whether it is
   * locked. A node brought to the front or sent to the back carries its place
   * in `data.z`; one that never was is stacked by size, as before. A locked
   * node stays selectable — that is how it gets unlocked — but cannot be
   * dragged or deleted from the canvas.
   */
  const applyState = (vfNode: any) => {
    const z = vfNode.data?.z;
    if (typeof z === 'number') vfNode.zIndex = z;
    // A comment sits over what it is about, never under the next item along.
    else if (vfNode.type === 'comment') vfNode.zIndex = COMMENT_Z;
    const locked = !!vfNode.data?.locked;
    vfNode.draggable = !locked;
    vfNode.deletable = !locked;
    vfNode.class = [locked && 'wb-locked', vfNode.data?.link && 'wb-has-link'].filter(Boolean).join(' ') || undefined;
  };

  const applySize = (vfNode: any, width?: number, height?: number) => {
    if (!SIZED_TYPES.has(vfNode.type)) {
      applyState(vfNode);
      return;
    }
    const [dw, dh] = DEFAULT_SIZE[vfNode.type] ?? [160, 80];
    const w = width || vfNode.data?.width || dw;
    const h = height || vfNode.data?.height || dh;
    // A frame is the ground its items stand on: always behind them.
    vfNode.zIndex = vfNode.type === 'frame' ? FRAME_Z : computeShapeZIndex(w, h);
    applyState(vfNode);

    if (vfNode.type === 'image') {
      // A style *function*, because a turned picture needs the whole node
      // turned — outline, resize handles and all — and the canvas writes the
      // node's `transform` itself, once per pan, to place it. The canvas
      // spreads this over its own style, so what is returned here wins; the
      // translate has to be reproduced, which means it has to be read at the
      // moment of drawing rather than baked in now.
      vfNode.style = (n: any) => {
        const size = {
          width: `${n.data?.width || 320}px`,
          height: `${n.data?.height || 240}px`,
        };
        const rotation = n.data?.rotation || 0;
        if (!rotation) return size;
        return {
          ...size,
          transform: `translate(${n.computedPosition.x}px, ${n.computedPosition.y}px) rotate(${rotation}deg)`,
        };
      };
      return;
    }

    vfNode.style = { ...vfNode.style, width: `${w}px`, height: `${h}px` };
  };

  /**
   * Delete multiple nodes by ID. For each: remove from store, filter out
   * from vfNodes, filter out edges that reference the node. Saves once at end.
   */
  const deleteNodes = (nodeIds: string[]) => {
    // Deleting a selection is one thing the user did, however many nodes it
    // covers, so it is one step to come back from.
    store.beginUndoBatch();
    for (const id of nodeIds) {
      store.removeNode(id);
    }
    store.endUndoBatch();
    vfNodes.value = vfNodes.value.filter((n: any) => !nodeIds.includes(n.id));
    vfEdges.value = vfEdges.value.filter((e: any) => !nodeIds.includes(e.source) && !nodeIds.includes(e.target));
    scheduleSave();
  };

  /**
   * Update data on a single node in both store and VueFlow refs.
   */
  const updateNodeData = (nodeId: string, data: Record<string, any>) => {
    if (!store.currentBoardData.value) return;
    const wbNode = store.currentBoardData.value.nodes.find((n: WBNode) => n.id === nodeId);
    if (!wbNode) return;
    // Through the store, which is what records the step back and stamps the
    // node as changed.
    store.updateNodeData(nodeId, data);

    // Sync to VueFlow
    const idx = vfNodes.value.findIndex((n: any) => n.id === nodeId);
    if (idx !== -1) {
      vfNodes.value[idx].data = { ...vfNodes.value[idx].data, ...data };
      vfNodes.value = [...vfNodes.value];
    }
    scheduleSave();
  };

  /**
   * Build a VueFlow edge object from a WBEdge (store model).
   * The one place an edge is drawn from what the board holds; the edge menu
   * rebuilds through it too, so a style change looks like a reload would.
   */
  const buildVfEdge = (edge: WBEdge, nodes: WBNode[]) => {
    const d = edge.data || {};
    const color = paint(d.color) || undefined;
    const edgeObj: any = {
      id: edge.id,
      source: edge.source,
      sourceHandle: edge.sourceHandle,
      target: edge.target,
      targetHandle: edge.targetHandle,
      type: edge.type || 'default',
      animated: !!d.animated,
      label: d.label || '',
      style: {
        stroke: color,
        strokeWidth: d.strokeWidth ? `${d.strokeWidth}px` : undefined,
        strokeDasharray: d.dashStyle === 'dashed' ? '8 4' : d.dashStyle === 'dotted' ? '2 4' : undefined,
      },
      data: d,
    };
    // Its ends are drawn by the edge itself, from `data` (see WaypointEdge and
    // edgeMarkers.ts) — not handed to the canvas as markers, which would draw
    // an empty marker of its own under the same id.
    // Above every node, however far forward a node has been brought, so a
    // connector stays visible and clickable over what it joins.
    edgeObj.zIndex = EDGE_Z;
    // Read by its ends, not as the canvas would: "Edge from shape_1759…".
    if (spokenEdge) edgeObj.ariaLabel = spokenEdge(edge, nodes);
    return edgeObj;
  };

  return { computeShapeZIndex, applySize, applyState, deleteNodes, updateNodeData, buildVfEdge };
}
