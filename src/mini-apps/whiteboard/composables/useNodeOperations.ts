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
   * Delete multiple nodes by ID. For each: remove from store, filter out
   * from vfNodes, filter out edges that reference the node. Saves once at end.
   */
  const deleteNodes = (nodeIds: string[]) => {
    for (const id of nodeIds) {
      store.removeNode(id);
    }
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
    wbNode.data = { ...wbNode.data, ...data };

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

  return { computeShapeZIndex, deleteNodes, updateNodeData, buildVfEdge };
}
