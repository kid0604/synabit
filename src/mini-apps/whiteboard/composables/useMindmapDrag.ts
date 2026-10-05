import { type Ref } from 'vue';
import { useVueFlow } from '@vue-flow/core';
import { stampElement } from '../boardFile';

export function useMindmapDrag(
  store: any,
  vfNodes: Ref<any[]>,
  vfEdges: Ref<any[]>,
  scheduleSave: () => void,
) {
  let mindmapDragState: {
    nodeId: string;
    startPos: { x: number; y: number };
    descendants: { id: string; entry: any; startX: number; startY: number }[];
  } | null = null;

  /**
   * Record where the dragged node's branch starts, so it can move with it.
   *
   * The branch is followed through mind-map links only. An arrow drawn from a
   * mind-map node to a shape is not the shape becoming a child: following
   * every edge dragged unrelated things along, and on a loop of arrows it
   * reached back to the node being dragged.
   *
   * Group members are selected by the canvas before this runs.
   */
  const { findNode } = useVueFlow({ id: 'whiteboard-flow' });

  /**
   * What sits inside a frame: every item whose box lies wholly within it.
   * Worked out when the drag starts, from where things are, rather than kept
   * as a list — an item dropped into a frame belongs to it, one dragged out
   * no longer does, with nothing to keep in step.
   */
  function insideFrame(frame: any): string[] {
    const f = findNode(frame.id);
    const fw = f?.dimensions?.width || frame.data?.width || 0;
    const fh = f?.dimensions?.height || frame.data?.height || 0;
    const fx = frame.position.x;
    const fy = frame.position.y;
    return vfNodes.value
      .filter((n: any) => n.id !== frame.id && !n.selected && !n.data?.locked)
      .filter((n: any) => {
        const d = findNode(n.id)?.dimensions;
        // An item the canvas never drew measures 0×0: its own size, then.
        const w = d?.width || n.data?.width || 0;
        const h = d?.height || n.data?.height || 0;
        return n.position.x >= fx && n.position.y >= fy && n.position.x + w <= fx + fw && n.position.y + h <= fy + fh;
      })
      .map((n: any) => n.id);
  }

  /**
   * What a drag carries besides the items the canvas moves itself (the
   * selection): a frame's contents, a mind-map item's branch, the rest of the
   * group the item is in. Locked items stay where they are.
   */
  function handleNodeDragStart({ node, nodes }: any) {
    const byId = new Map<string, any>(vfNodes.value.map((n: any) => [n.id, n]));
    const carried = new Set<string>();
    // Everything the canvas is dragging carries what goes with it, not just
    // the item under the pointer: a frame dragged along with a sticky note
    // outside it used to leave its contents behind.
    const dragged: any[] = nodes?.length ? nodes : [node];
    const isMindmap = (id: string) => byId.get(id)?.type === 'mindmap';

    for (const item of dragged) {
      if (item.type === 'frame') insideFrame(item).forEach((id) => carried.add(id));

      if (item.type === 'mindmap') {
        const stack = [item.id];
        const seen = new Set<string>([item.id]);
        while (stack.length) {
          const parentId = stack.pop()!;
          for (const edge of vfEdges.value) {
            if (edge.source !== parentId || !isMindmap(edge.target) || seen.has(edge.target)) continue;
            seen.add(edge.target);
            carried.add(edge.target);
            stack.push(edge.target);
          }
        }
      }

      // Comments about what is dragged go with it.
      for (const n of vfNodes.value) if (n.type === 'comment' && n.data?.on === item.id) carried.add(n.id);

      const group = item.data?.groupId;
      if (group) {
        for (const n of vfNodes.value) if (n.data?.groupId === group && n.id !== item.id) carried.add(n.id);
      }
    }

    for (const n of vfNodes.value) if (n.selected && n.id !== node.id && !findNode(n.id)) carried.add(n.id);
    // What the canvas drags itself is not carried twice.
    for (const item of dragged) carried.delete(item.id);

    const along = [...carried]
      .map((id) => byId.get(id))
      // The canvas drags what it selected; selected ink it does not hold
      // (a large selection, kept on the ink layer) is carried from here.
      .filter((n: any) => n && (!n.selected || !findNode(n.id)) && !n.data?.locked);
    if (!along.length) return;
    mindmapDragState = {
      nodeId: node.id,
      startPos: { x: node.position.x, y: node.position.y },
      descendants: along.map((n: any) => ({ id: n.id, entry: n, startX: n.position?.x || 0, startY: n.position?.y || 0 })),
    };
  }

  function handleNodeDrag({ node }: any) {
    // Mindmap: apply delta to all descendants
    if (mindmapDragState && node.id === mindmapDragState.nodeId) {
      const dx = node.position.x - mindmapDragState.startPos.x;
      const dy = node.position.y - mindmapDragState.startPos.y;
      for (const desc of mindmapDragState.descendants) {
        const vfNode = desc.entry;
        if (vfNode) {
          vfNode.position = {
            x: desc.startX + dx,
            y: desc.startY + dy,
          };
        }
      }
    }
  }

  function handleNodeDragStop({ node }: any) {
    if (mindmapDragState && node.id === mindmapDragState.nodeId) {
      // Sync final positions to store
      const board = new Map<string, any>((store.currentBoardData.value?.nodes ?? []).map((n: any) => [n.id, n]));
      for (const desc of mindmapDragState.descendants) {
        const wbNode = board.get(desc.id);
        if (wbNode && desc.entry) {
          wbNode.position = { ...desc.entry.position };
          stampElement(wbNode);
        }
      }
      mindmapDragState = null;
      scheduleSave();
    }
  }

  return {
    handleNodeDragStart,
    handleNodeDrag,
    handleNodeDragStop,
  };
}
