import { type Ref } from 'vue';
import { MarkerType } from '@vue-flow/core';
import type { WBNode, WBEdge } from './useWhiteboardStore';

export function useNodeOperations(
  store: any,
  vfNodes: Ref<any[]>,
  vfEdges: Ref<any[]>,
  scheduleSave: () => void,
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
  const SIZED_TYPES = new Set(['shape', 'image']);
  const applySize = (vfNode: any, width?: number, height?: number) => {
    if (!SIZED_TYPES.has(vfNode.type)) return;
    const w = width || vfNode.data?.width || (vfNode.type === 'image' ? 320 : 160);
    const h = height || vfNode.data?.height || (vfNode.type === 'image' ? 240 : 80);
    vfNode.zIndex = computeShapeZIndex(w, h);

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
   * Exact logic from syncToVueFlow edge mapping (L115-143).
   */
  const buildVfEdge = (edge: WBEdge, _nodes: WBNode[]) => {
    const d = edge.data || {};
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
        stroke: d.color || undefined,
        strokeWidth: d.strokeWidth ? `${d.strokeWidth}px` : undefined,
        strokeDasharray: d.dashStyle === 'dashed' ? '8 4' : d.dashStyle === 'dotted' ? '2 4' : undefined,
      },
      data: d,
    };
    // Apply markers
    if (d.markerEnd === 'arrow') {
      edgeObj.markerEnd = { type: MarkerType.ArrowClosed, color: d.color || undefined };
    }
    if (d.markerStart === 'arrow') {
      edgeObj.markerStart = { type: MarkerType.ArrowClosed, color: d.color || undefined };
    }
    // Set edge z-index ABOVE all shape z-indices so edges are always clickable
    edgeObj.zIndex = 10001;
    return edgeObj;
  };

  return { computeShapeZIndex, applySize, deleteNodes, updateNodeData, buildVfEdge };
}
