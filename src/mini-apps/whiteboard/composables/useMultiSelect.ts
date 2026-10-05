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
  const { updateNodeData: vfUpdateNodeData, findNode } = useVueFlow({ id: 'whiteboard-flow' });

  /**
   * Change an item's data on screen. The canvas changes what it holds; ink it
   * does not hold (a selection of many strokes stays on the ink layer) is
   * changed in the list the ink layer draws from — the canvas's own update
   * does nothing for an item it does not have, so a colour or a group given to
   * sixty strokes was saved but not shown, and the group could not be picked
   * up as one.
   */
  let looseChanged = false;
  function show(n: any, data: Record<string, any>, replace = false) {
    if (findNode(n.id)) vfUpdateNodeData(n.id, data, replace ? { replace: true } : undefined);
    else {
      n.data = replace ? data : { ...n.data, ...data };
      looseChanged = true;
    }
  }
  function settle() {
    // After the canvas has taken in its own changes (see `selectItems`).
    if (looseChanged) setTimeout(() => { vfNodes.value = [...vfNodes.value]; }, 0);
    looseChanged = false;
  }

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
    store.beginUndoBatch();
    for (const n of selected) {
      // Use VueFlow's native API — doesn't reset selection
      show(n, { groupId });
      store.updateNodeData(n.id, { groupId });
    }
    store.endUndoBatch();
    settle();
    scheduleSave();
  }

  function handleMultiUngroup() {
    const selected = [...multiSelectedNodes.value];
    store.beginUndoBatch();
    for (const n of selected) {
      if (n.data?.groupId) {
        const { groupId, ...rest } = n.data;
        show(n, rest, true);
        // A merge of `rest` into the board would keep `groupId`: it has to
        // be cleared by name, or the group comes back on the next redraw.
        store.updateNodeData(n.id, { groupId: undefined });
      }
    }
    store.endUndoBatch();
    settle();
    scheduleSave();
  }

  function handleMultiDelete() {
    const ids = multiSelectedNodes.value.map((n: any) => n.id);
    deleteNodes(ids);
  }

  function handleMultiUpdateAll(data: Record<string, any>) {
    // Locked items keep how they look; the rest of the selection changes.
    const selected = multiSelectedNodes.value.filter((n: any) => !n.data?.locked);
    store.beginUndoBatch();
    for (const n of selected) {
      show(n, data);
      store.updateNodeData(n.id, data);
    }
    store.endUndoBatch();
    settle();
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
