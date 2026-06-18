import { type Ref } from 'vue';

export function useWhiteboardKeyboard(ctx: {
  store: any;
  vfNodes: Ref<any[]>;
  vfEdges: Ref<any[]>;
  deleteNodes: (ids: string[]) => void;
  syncToVueFlow: () => void;
  scheduleSave: () => void;
  copySelected: () => void;
  pasteClipboard: () => void;
  focusMindmapNode: (id: string) => void;
  handleMindmapAddChild: (params: { parentId: string; direction: 'right' | 'left' }) => void;
  handleMindmapAddSibling: (id: string) => void;
  handleMindmapRemoveNode: (id: string) => void;
  handleMultiGroup: () => void;
  handleMultiUngroup: () => void;
  closeEdgeMenu: () => void;
  closeShapeMenu: () => void;
  closeTextMenu: () => void;
}) {
  function handleKeydown(e: KeyboardEvent) {
    // Don't capture when editing text
    const target = e.target as HTMLElement;
    if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA') return;

    // Mindmap shortcuts: Tab = child, Enter = sibling (when a mindmap node is selected)
    if (e.key === 'Tab' || e.key === 'Enter') {
      const selectedNode = ctx.vfNodes.value.find((n: any) => n.selected && n.type === 'mindmap');
      if (selectedNode) {
        e.preventDefault();
        if (e.key === 'Tab') {
          const dir = selectedNode.data?.direction || 'right';
          ctx.handleMindmapAddChild({ parentId: selectedNode.id, direction: dir });
        } else {
          ctx.handleMindmapAddSibling(selectedNode.id);
        }
        return;
      }
    }

    // Tool shortcuts — only when no modifier key is held
    if (!e.ctrlKey && !e.metaKey) {
      if (e.key === 'v' || e.key === 'V') { ctx.store.activeTool.value = 'select'; return; }
      if (e.key === 'h' || e.key === 'H') { ctx.store.activeTool.value = 'pan'; return; }
      if (e.key === 'd' || e.key === 'D') { ctx.store.activeTool.value = 'draw'; return; }
      if (e.key === 's') { ctx.store.activeTool.value = 'shape'; return; }
      if (e.key === 't' || e.key === 'T') { ctx.store.activeTool.value = 'text'; return; }
      if (e.key === 'e' || e.key === 'E') { ctx.store.activeTool.value = 'draw'; ctx.store.drawSubTool.value = 'eraser'; return; }
      if (e.key === 'm' || e.key === 'M') { ctx.store.activeTool.value = 'mindmap'; return; }
    }

    if ((e.ctrlKey || e.metaKey) && e.key === 'z' && !e.shiftKey) {
      e.preventDefault();
      ctx.store.undo();
      ctx.syncToVueFlow();
      ctx.scheduleSave();
      return;
    }
    if ((e.ctrlKey || e.metaKey) && e.key === 'z' && e.shiftKey) {
      e.preventDefault();
      ctx.store.redo();
      ctx.syncToVueFlow();
      ctx.scheduleSave();
      return;
    }
    if ((e.ctrlKey || e.metaKey) && (e.key === 's' || e.key === 'S')) {
      e.preventDefault();
      ctx.store.saveCurrentBoard();
      return;
    }

    // Ctrl+C → copy selected node
    if ((e.ctrlKey || e.metaKey) && (e.key === 'c' || e.key === 'C') && !e.shiftKey) {
      const selectedNode = ctx.vfNodes.value.find((n: any) => n.selected);
      if (selectedNode) {
        e.preventDefault();
        ctx.copySelected();
      }
      return;
    }

    // Ctrl+V → paste copied node with offset
    if ((e.ctrlKey || e.metaKey) && (e.key === 'v' || e.key === 'V') && !e.shiftKey) {
      e.preventDefault();
      ctx.pasteClipboard();
      return;
    }

    // Ctrl+G → group selected nodes
    if ((e.ctrlKey || e.metaKey) && (e.key === 'g' || e.key === 'G') && !e.shiftKey) {
      e.preventDefault();
      ctx.handleMultiGroup();
      return;
    }

    // Ctrl+Shift+G → ungroup selected nodes
    if ((e.ctrlKey || e.metaKey) && (e.key === 'g' || e.key === 'G') && e.shiftKey) {
      e.preventDefault();
      ctx.handleMultiUngroup();
      return;
    }
  }

  return { handleKeydown };
}
