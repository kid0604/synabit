import { ref, computed } from 'vue';
import type { WBNode } from './useWhiteboardStore';

export function useShapeMenu(
  store: any,
  updateNodeData: (id: string, data: Record<string, any>) => void,
  deleteNodes: (ids: string[]) => void,
) {
  const selectedShapeNodeId = ref<string | null>(null);
  const shapeMenuPos = ref({ x: 0, y: 0 });

  const selectedShapeData = computed(() => {
    if (!selectedShapeNodeId.value || !store.currentBoardData.value) return null;
    const node = store.currentBoardData.value.nodes.find((n: WBNode) => n.id === selectedShapeNodeId.value);
    if (!node || node.type !== 'shape') return null;
    return {
      shapeType: node.data.shapeType || 'rectangle',
      label: node.data.label || '',
      color: node.data.color || '#7c3aed',
      fillColor: node.data.fillColor || '',
      borderWidth: node.data.borderWidth || 2,
      dashStyle: node.data.dashStyle || 'solid',
      opacity: node.data.opacity ?? 100,
      fontSize: node.data.fontSize || 13,
      locked: !!node.data.locked,
    };
  });

  /**
   * Whether an item is locked: then the panel may unlock it and do nothing else.
   * Locking was honoured by the menu, the eraser and the selection operations,
   * but the panel still restyled and deleted a locked item.
   */
  const isLocked = (id: string) => !!store.currentBoardData.value?.nodes.find((n: WBNode) => n.id === id)?.data?.locked;

  function handleShapeUpdate(nodeId: string, data: Record<string, any>) {
    if (isLocked(nodeId) && Object.keys(data).some((k) => k !== 'locked')) return;
    updateNodeData(nodeId, data);
  }

  function handleShapeDelete(nodeId: string) {
    if (isLocked(nodeId)) return;
    deleteNodes([nodeId]);
    selectedShapeNodeId.value = null;
  }

  function closeShapeMenu() {
    selectedShapeNodeId.value = null;
  }

  return {
    selectedShapeNodeId,
    shapeMenuPos,
    selectedShapeData,
    handleShapeUpdate,
    handleShapeDelete,
    closeShapeMenu,
  };
}
