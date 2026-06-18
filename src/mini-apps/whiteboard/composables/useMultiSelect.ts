import { computed, type Ref } from 'vue';
import { useVueFlow } from '@vue-flow/core';

export function useMultiSelect(
  store: any,
  vfNodes: Ref<any[]>,
  vfEdges: Ref<any[]>,
  deleteNodes: (ids: string[]) => void,
  updateNodeData: (id: string, data: Record<string, any>) => void,
  scheduleSave: () => void,
) {
  const { updateNodeData: vfUpdateNodeData } = useVueFlow({ id: 'whiteboard-flow' });

  const multiSelectedNodes = computed(() =>
    vfNodes.value.filter((n: any) => n.selected)
  );
  const showMultiSelectMenu = computed(() =>
    multiSelectedNodes.value.length >= 2
  );

  function handleMultiGroup() {
    const selected = [...multiSelectedNodes.value];
    if (selected.length < 2) return;
    const groupId = `grp_${Date.now()}`;
    for (const n of selected) {
      // Use VueFlow's native API — doesn't reset selection
      vfUpdateNodeData(n.id, { groupId });
      store.updateNodeData(n.id, { groupId });
    }
    scheduleSave();
  }

  function handleMultiUngroup() {
    const selected = [...multiSelectedNodes.value];
    for (const n of selected) {
      if (n.data?.groupId) {
        const { groupId, ...rest } = n.data;
        vfUpdateNodeData(n.id, rest, { replace: true });
        store.updateNodeData(n.id, rest);
      }
    }
    scheduleSave();
  }

  function handleMultiDelete() {
    const ids = multiSelectedNodes.value.map((n: any) => n.id);
    deleteNodes(ids);
  }

  function handleMultiUpdateAll(data: Record<string, any>) {
    const selected = [...multiSelectedNodes.value];
    for (const n of selected) {
      vfUpdateNodeData(n.id, data);
      store.updateNodeData(n.id, data);
    }
    scheduleSave();
  }

  function closeMultiSelectMenu() {
    // Deselect all nodes
    for (const n of vfNodes.value) {
      n.selected = false;
    }
    vfNodes.value = [...vfNodes.value];
  }

  return {
    multiSelectedNodes,
    showMultiSelectMenu,
    handleMultiGroup,
    handleMultiUngroup,
    handleMultiDelete,
    handleMultiUpdateAll,
    closeMultiSelectMenu,
  };
}
