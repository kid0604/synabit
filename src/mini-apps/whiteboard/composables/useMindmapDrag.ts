import { type Ref } from 'vue';
import { useVueFlow } from '@vue-flow/core';

export function useMindmapDrag(
  store: any,
  vfNodes: Ref<any[]>,
  vfEdges: Ref<any[]>,
  scheduleSave: () => void,
) {
  const { addSelectedNodes } = useVueFlow({ id: 'whiteboard-flow' });

  let mindmapDragState: {
    nodeId: string;
    startPos: { x: number; y: number };
    descendants: { id: string; startX: number; startY: number }[];
  } | null = null;

  function handleNodeDragStart({ node }: any) {
    // Auto-select all group members BEFORE drag starts
    if (node.data?.groupId) {
      const groupId = node.data.groupId;
      const groupMembers = vfNodes.value.filter(
        (n: any) => n.data?.groupId === groupId && n.id !== node.id
      );
      if (groupMembers.length > 0) {
        addSelectedNodes(groupMembers);
      }
    }

    // Mindmap: record initial positions of all descendants
    if (node.type === 'mindmap') {
      const descendantIds = new Set<string>();
      const findDescendants = (parentId: string) => {
        for (const edge of vfEdges.value) {
          if (edge.source === parentId && !descendantIds.has(edge.target)) {
            descendantIds.add(edge.target);
            findDescendants(edge.target);
          }
        }
      };
      findDescendants(node.id);

      if (descendantIds.size > 0) {
        mindmapDragState = {
          nodeId: node.id,
          startPos: { x: node.position.x, y: node.position.y },
          descendants: [...descendantIds].map(id => {
            const n = vfNodes.value.find((nd: any) => nd.id === id);
            return { id, startX: n?.position?.x || 0, startY: n?.position?.y || 0 };
          }),
        };
      }
    }
  }

  function handleNodeDrag({ node }: any) {
    // Mindmap: apply delta to all descendants
    if (mindmapDragState && node.id === mindmapDragState.nodeId) {
      const dx = node.position.x - mindmapDragState.startPos.x;
      const dy = node.position.y - mindmapDragState.startPos.y;
      for (const desc of mindmapDragState.descendants) {
        const vfNode = vfNodes.value.find((n: any) => n.id === desc.id);
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
      for (const desc of mindmapDragState.descendants) {
        const vfNode = vfNodes.value.find((n: any) => n.id === desc.id);
        if (vfNode) {
          const wbNode = store.currentBoardData.value?.nodes.find((n: any) => n.id === desc.id);
          if (wbNode) {
            wbNode.position = { ...vfNode.position };
          }
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
