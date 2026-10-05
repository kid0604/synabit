import { type Ref } from 'vue';

/**
 * Whether a key pressed here belongs to something other than the canvas.
 *
 * Text being typed anywhere — an input, a textarea, a select, or a rich-text
 * editor, which is a contenteditable element rather than an input. Missing
 * that last one meant typing "t" or "d" in a note switched the board's tool.
 */
function isTypingIn(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  if (!el || !el.tagName) return false;
  return (
    el.tagName === 'INPUT' ||
    el.tagName === 'TEXTAREA' ||
    el.tagName === 'SELECT' ||
    el.isContentEditable ||
    !!el.closest?.('[contenteditable]:not([contenteditable="false"])')
  );
}

/** A control that Enter or Tab already means something to: press it, or move on from it. */
function isControl(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  return !!el?.closest?.('button, a[href], [role="button"], [role="menuitem"], [role="option"]');
}

/**
 * Whether focus is on the board itself — the canvas, its title bar and its
 * toolbar — or on nothing. A single letter switches the tool only then: with
 * focus in the sidebar, or anywhere else the board is not, "s" was a tool
 * change nobody asked for (WCAG 2.1.4, character key shortcuts).
 */
function onBoard(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  return !el || !el.closest || el === document.body || el === document.documentElement || !!el.closest('.wb-canvas');
}

const ARROWS: Record<string, [number, number]> = {
  ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1],
};

/** Items whose words the keyboard can open for editing. */
const EDITABLE = new Set(['shape', 'text', 'sticky', 'frame', 'comment']);

export function useWhiteboardKeyboard(ctx: {
  /**
   * Whether the board is the app on screen. The listener is on the window and
   * the board is kept alive behind other apps, so without this, Cmd+Z in a
   * note undid something on a board nobody could see.
   */
  isActive: () => boolean;
  /** Open the editor of an item's words, as a double-click would. */
  startEditing: (id: string) => void;
  store: any;
  vfNodes: Ref<any[]>;
  vfEdges: Ref<any[]>;
  deleteNodes: (ids: string[]) => void;
  syncToVueFlow: () => void;
  scheduleSave: () => void;
  /** The selection operations (see `useArrange`). */
  arrange: {
    selectedIds: () => string[];
    duplicate: () => void;
    selectAll: () => void;
    nudge: (dx: number, dy: number) => void;
    reorder: (where: 'front' | 'back') => void;
    toggleLock: () => void;
    frameSelection: () => void;
    copyStyle: () => void;
    pasteStyle: () => void;
  };
  focusMindmapNode: (id: string) => void;
  handleMindmapAddChild: (params: { parentId: string; direction: 'right' | 'left' }) => void;
  handleMindmapAddSibling: (id: string) => void;
  handleMindmapRemoveNode: (id: string) => void;
  handleMultiGroup: () => void;
  handleMultiUngroup: () => void;
  closeEdgeMenu: () => void;
  closeShapeMenu: () => void;
  closeTextMenu: () => void;
  /** Delete a selection that includes ink the canvas does not hold; whether it did. */
  deleteLooseSelection?: () => boolean;
  /** Put down what the active tool makes, in the middle of the view: the keyboard's click. */
  placeWithTool: () => boolean;
  /** The right-click menu, for the selection or the board, opened from the keyboard. */
  openMenu: () => void;
  /** Find words on the board. */
  openSearch?: () => void;
  /** The list of every key the board answers to. */
  openShortcuts?: () => void;
}) {
  function handleKeydown(e: KeyboardEvent) {
    if (!ctx.isActive() || e.defaultPrevented || isTypingIn(e.target)) return;

    // Shift turns "z" into "Z", and Caps Lock does it without Shift; the
    // letter is what is meant either way.
    const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
    const mod = e.ctrlKey || e.metaKey;

    // F6: into the open item's panel, which sits apart from the canvas at the
    // end of the page, where Tab from the item would never reach it.
    if (e.key === 'F6') {
      const panel = document.querySelector<HTMLElement>('.sp-panel, .ep-panel');
      const first = panel?.querySelector<HTMLElement>('button:not([disabled]), input, select, textarea');
      if (first) {
        e.preventDefault();
        first.focus();
        return;
      }
    }

    if (mod && key === 'f' && !e.shiftKey && ctx.openSearch) {
      e.preventDefault();
      ctx.openSearch();
      return;
    }

    if (e.key === '?' && !mod && !e.altKey && ctx.openShortcuts && onBoard(e.target)) {
      e.preventDefault();
      ctx.openShortcuts();
      return;
    }

    // The menu key, or Shift+F10: everything the right-click menu offers —
    // templates, vault cards, live frames, Syn — within reach of a keyboard.
    if (e.key === 'ContextMenu' || (e.key === 'F10' && e.shiftKey)) {
      e.preventDefault();
      ctx.openMenu();
      return;
    }

    // Enter with a tool that makes something (a shape, a sticky note…) makes
    // it, in the middle of the view. Otherwise only a click could.
    if (e.key === 'Enter' && !mod && !isControl(e.target) && ctx.placeWithTool()) {
      e.preventDefault();
      return;
    }

    // Mindmap shortcuts: Tab = child, Enter = sibling (when a mindmap node is
    // selected), unless the key is on a toolbar button, where it already means
    // "press this" or "move on".
    if ((e.key === 'Tab' || e.key === 'Enter') && !isControl(e.target)) {
      const selectedNode = ctx.vfNodes.value.find((n: any) => n.selected && n.type === 'mindmap');
      // Only with focus on that item, or on nothing: anywhere else Tab is how
      // focus moves on, and taking it there trapped the keyboard on the board.
      const el = e.target as HTMLElement | null;
      const onIt = !el || el === document.body || el.closest?.('.vue-flow__node')?.getAttribute('data-id') === selectedNode?.id;
      if (selectedNode && onIt) {
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

    // F2, or Enter on a selected item, edits its words — the keyboard's
    // double-click. (Enter on a mind-map item adds a sibling, above.)
    if ((e.key === 'F2' || e.key === 'Enter') && !mod && !e.shiftKey && !isControl(e.target)) {
      const chosen = ctx.vfNodes.value.filter((n: any) => n.selected);
      if (chosen.length === 1 && EDITABLE.has(chosen[0].type) && !chosen[0].data?.locked) {
        e.preventDefault();
        ctx.startEditing(chosen[0].id);
        return;
      }
    }

    // Tool shortcuts — a letter alone, with focus on the board.
    if (!mod && !e.altKey && !e.shiftKey && onBoard(e.target)) {
      if (key === 'v') { ctx.store.activeTool.value = 'select'; return; }
      if (key === 'h') { ctx.store.activeTool.value = 'pan'; return; }
      if (key === 'd') { ctx.store.activeTool.value = 'draw'; return; }
      if (key === 's') { ctx.store.activeTool.value = 'shape'; return; }
      if (key === 't') { ctx.store.activeTool.value = 'text'; return; }
      if (key === 'e') { ctx.store.activeTool.value = 'draw'; ctx.store.drawSubTool.value = 'eraser'; return; }
      if (key === 'm') { ctx.store.activeTool.value = 'mindmap'; return; }
      if (key === 'n') { ctx.store.activeTool.value = 'sticky'; return; }
      if (key === 'f') { ctx.store.activeTool.value = 'frame'; return; }
    }

    if (mod && key === 'z' && !e.shiftKey) {
      e.preventDefault();
      ctx.store.undo();
      ctx.syncToVueFlow();
      ctx.scheduleSave();
      return;
    }
    // Redo: Cmd/Ctrl+Shift+Z everywhere, and Ctrl+Y, which is what Windows
    // and Linux users reach for.
    if (mod && ((key === 'z' && e.shiftKey) || (key === 'y' && !e.shiftKey))) {
      e.preventDefault();
      ctx.store.redo();
      ctx.syncToVueFlow();
      ctx.scheduleSave();
      return;
    }
    if (mod && key === 's') {
      e.preventDefault();
      ctx.store.saveCurrentBoard();
      return;
    }

    // Ctrl+C, Ctrl+X and Ctrl+V are deliberately not here. Cancelling the key
    // stops the browser from raising the `copy`, `cut` and `paste` events, and
    // only inside those may the page read and write the system clipboard — so
    // the canvas listens for the events themselves.

    // Keys named by where they are rather than what they type: Shift turns
    // "]" into "}", and on a Mac Option turns "c" into "ç".
    const code = e.code;

    // Copy and paste a look: Ctrl/Cmd+Alt+C, Ctrl/Cmd+Alt+V.
    if (mod && e.altKey && (code === 'KeyC' || code === 'KeyV')) {
      e.preventDefault();
      if (code === 'KeyC') ctx.arrange.copyStyle();
      else ctx.arrange.pasteStyle();
      return;
    }

    // Put the selection in a frame: Ctrl/Cmd+Alt+G, as in Figma.
    if (mod && e.altKey && code === 'KeyG') {
      e.preventDefault();
      ctx.arrange.frameSelection();
      return;
    }

    if (mod && !e.shiftKey && !e.altKey && key === 'd') {
      e.preventDefault();
      ctx.arrange.duplicate();
      return;
    }
    if (mod && !e.shiftKey && !e.altKey && key === 'a') {
      e.preventDefault();
      ctx.arrange.selectAll();
      return;
    }

    // To the front / to the back: Ctrl/Cmd+Shift+] and [.
    if (mod && e.shiftKey && (code === 'BracketRight' || code === 'BracketLeft')) {
      e.preventDefault();
      ctx.arrange.reorder(code === 'BracketRight' ? 'front' : 'back');
      return;
    }

    if (mod && e.shiftKey && key === 'l') {
      e.preventDefault();
      ctx.arrange.toggleLock();
      return;
    }

    // Ctrl+G → group selected nodes
    if (mod && key === 'g' && !e.shiftKey) {
      e.preventDefault();
      ctx.handleMultiGroup();
      return;
    }

    // Ctrl+Shift+G → ungroup selected nodes
    if (mod && key === 'g' && e.shiftKey) {
      e.preventDefault();
      ctx.handleMultiUngroup();
      return;
    }
  }

  /**
   * Keys that must be seen before the canvas sees them, on the window in the
   * capture phase.
   *
   * Delete with ink selected that the canvas does not hold (a selection of
   * more than the ink layer hands over): the canvas's own Delete listener is
   * on the document and claims the key for the items it holds, so a handler
   * after it found the key taken and the ink stayed on the board. Here the
   * whole selection goes, as one step, and the canvas never sees the key.
   */
  function handleKeydownCapture(e: KeyboardEvent) {
    if (!ctx.isActive() || e.defaultPrevented || isTypingIn(e.target)) return;
    if ((e.key === 'Delete' || e.key === 'Backspace') && !(e.ctrlKey || e.metaKey) && ctx.deleteLooseSelection?.()) {
      e.preventDefault();
      e.stopPropagation();
      return;
    }
    // Arrows move the selection a pixel; with Shift, ten. Taken here, before
    // the canvas: a clicked item has focus, and the canvas's own handler on it
    // moved the selection five pixels at a time, left the bends of the lines
    // between moved items behind, and left selected ink where it was.
    if (ARROWS[e.key] && !(e.ctrlKey || e.metaKey || e.altKey) && onBoard(e.target) && !isControl(e.target) && ctx.arrange.selectedIds().length) {
      e.preventDefault();
      e.stopPropagation();
      const step = e.shiftKey ? 10 : 1;
      ctx.arrange.nudge(ARROWS[e.key][0] * step, ARROWS[e.key][1] * step);
    }
  }

  return { handleKeydown, handleKeydownCapture };
}
