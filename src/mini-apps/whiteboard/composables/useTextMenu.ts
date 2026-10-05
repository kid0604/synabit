import { ref, computed, type Ref } from 'vue';
import type { WBNode } from './useWhiteboardStore';

export function useTextMenu(
  store: any,
  vfNodes: Ref<any[]>,
  updateNodeData: (id: string, data: Record<string, any>) => void,
  deleteNodes: (ids: string[]) => void,
) {
  const selectedTextNodeId = ref<string | null>(null);

  const selectedTextData = computed(() => {
    if (!selectedTextNodeId.value || !store.currentBoardData.value) return null;
    const node = store.currentBoardData.value.nodes.find((n: WBNode) => n.id === selectedTextNodeId.value);
    if (!node || node.type !== 'text') return null;
    return {
      label: node.data.label || '',
      fontSize: node.data.fontSize || 14,
      fontWeight: node.data.fontWeight || 'normal',
      fontStyle: node.data.fontStyle || 'normal',
      textAlign: node.data.textAlign || 'left',
      color: node.data.color || '#1e1e1e',
      backgroundColor: node.data.backgroundColor || '',
      opacity: node.data.opacity ?? 100,
      width: node.data.width || 240,
      locked: !!node.data.locked,
    };
  });

  /** A locked item: the panel may unlock it and do nothing else. */
  const isLocked = (id: string) => !!store.currentBoardData.value?.nodes.find((n: any) => n.id === id)?.data?.locked;

  function handleTextUpdate(nodeId: string, data: Record<string, any>) {
    if (isLocked(nodeId) && Object.keys(data).some((k) => k !== 'locked')) return;
    updateNodeData(nodeId, data);
  }

  function handleTextDelete(nodeId: string) {
    if (isLocked(nodeId)) return;
    deleteNodes([nodeId]);
    selectedTextNodeId.value = null;
  }

  function closeTextMenu() {
    selectedTextNodeId.value = null;
  }

  return {
    selectedTextNodeId,
    selectedTextData,
    handleTextUpdate,
    handleTextDelete,
    closeTextMenu,
  };
}
