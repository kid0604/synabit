import { computed, type Ref } from 'vue';
import { useVueFlow } from '@vue-flow/core';

/**
 * Whether this item is the only thing selected.
 *
 * An item's own tools — the turn handle, a sticky note's colours — belong to
 * one item at a time. With four sticky notes selected, four sets of tools
 * crowded the canvas while the selection panel already offered what applies
 * to all of them.
 */
export function useOnlySelected(selected: Ref<boolean | undefined> | (() => boolean | undefined)) {
  const { getSelectedNodes } = useVueFlow();
  const read = typeof selected === 'function' ? selected : () => selected.value;
  return computed(() => !!read() && getSelectedNodes.value.length === 1);
}
