import { ref, computed, type Ref } from 'vue';
import { MarkerType, useVueFlow } from '@vue-flow/core';
import type { WBNode, WBEdge } from './useWhiteboardStore';

export function useEdgeMenu(
  store: any,
  vfEdges: Ref<any[]>,
  vfNodes: Ref<any[]>,
  buildVfEdge: (edge: WBEdge, nodes: WBNode[]) => any,
  scheduleSave: () => void,
) {
  const { removeEdges, addEdges } = useVueFlow({ id: 'whiteboard-flow' });

  const selectedEdgeId = ref<string | null>(null);
  const edgeMenuPos = ref({ x: 0, y: 0 });

  const selectedEdgeData = computed(() => {
    if (!selectedEdgeId.value || !store.currentBoardData.value) return null;
    const edge = store.currentBoardData.value.edges.find((e: WBEdge) => e.id === selectedEdgeId.value);
    if (!edge) return null;
    return {
      type: edge.type || 'default',
      color: edge.data?.color || '',
      strokeWidth: edge.data?.strokeWidth || 2,
      animated: edge.data?.animated || false,
      label: edge.data?.label || '',
      markerEnd: edge.data?.markerEnd || 'none',
      markerStart: edge.data?.markerStart || 'none',
      dashStyle: edge.data?.dashStyle || 'solid',
    };
  });

  function handleEdgeClick({ edge, event }: any) {
    if (store.activeTool.value !== 'select') return;
    selectedEdgeId.value = edge.id;
    edgeMenuPos.value = { x: event.clientX, y: event.clientY };
  }

  // Guard flag to prevent handleEdgesChange from deleting the edge during update
  let updatingEdgeId: string | null = null;

  function handleEdgeUpdate(edgeId: string, data: Record<string, any>) {
    if (!store.currentBoardData.value) return;
    const wbEdge = store.currentBoardData.value.edges.find((e: WBEdge) => e.id === edgeId);
    if (!wbEdge) return;

    // Update store
    wbEdge.type = data.type || wbEdge.type;
    wbEdge.data = { ...wbEdge.data, ...data };

    // Rebuild the full VueFlow edge object
    const d = wbEdge.data || {};
    const newVfEdge: any = {
      id: wbEdge.id,
      source: wbEdge.source,
      sourceHandle: wbEdge.sourceHandle,
      target: wbEdge.target,
      targetHandle: wbEdge.targetHandle,
      type: wbEdge.type || 'default',
      animated: !!d.animated,
      label: d.label || '',
      style: {
        stroke: d.color || undefined,
        strokeWidth: d.strokeWidth ? `${d.strokeWidth}px` : undefined,
        strokeDasharray: d.dashStyle === 'dashed' ? '8 4' : d.dashStyle === 'dotted' ? '2 4' : undefined,
      },
      data: d,
      zIndex: 10001,
    };
    if (d.markerEnd === 'arrow') {
      newVfEdge.markerEnd = { type: MarkerType.ArrowClosed, color: d.color || undefined };
    }
    if (d.markerStart === 'arrow') {
      newVfEdge.markerStart = { type: MarkerType.ArrowClosed, color: d.color || undefined };
    }

    // Set guard flag, remove old edge, add new edge, clear flag
    updatingEdgeId = edgeId;
    removeEdges([edgeId]);
    addEdges([newVfEdge]);
    updatingEdgeId = null;

    scheduleSave();
  }

  function handleEdgeDelete(edgeId: string) {
    store.removeEdge(edgeId);
    vfEdges.value = vfEdges.value.filter((e: any) => e.id !== edgeId);
    selectedEdgeId.value = null;
    scheduleSave();
  }

  function closeEdgeMenu() {
    selectedEdgeId.value = null;
  }

  function updateEdgeWaypoints(edgeId: string, waypoints: any[]) {
    if (!store.currentBoardData.value) return;
    const wbEdge = store.currentBoardData.value.edges.find((e: WBEdge) => e.id === edgeId);
    if (wbEdge) {
      wbEdge.data = { ...wbEdge.data, waypoints };
    }

    const vfEdge = vfEdges.value.find((e: any) => e.id === edgeId);
    if (vfEdge) {
      if (!vfEdge.data) vfEdge.data = {};
      vfEdge.data.waypoints = waypoints;
    }

    scheduleSave();
  }

  /** Getter for the updatingEdgeId guard flag (used by handleEdgesChange) */
  function getUpdatingEdgeId() {
    return updatingEdgeId;
  }

  return {
    selectedEdgeId,
    edgeMenuPos,
    selectedEdgeData,
    handleEdgeClick,
    handleEdgeUpdate,
    handleEdgeDelete,
    closeEdgeMenu,
    updateEdgeWaypoints,
    getUpdatingEdgeId,
  };
}
