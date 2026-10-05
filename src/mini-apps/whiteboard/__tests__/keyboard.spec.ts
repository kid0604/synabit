import { describe, expect, it, vi } from 'vitest';
import { ref } from 'vue';
import { useWhiteboardKeyboard } from '../composables/useWhiteboardKeyboard';
import { escapeAttribute } from '../../note/WhiteboardExtension';

const setup = (active = true) => {
  const store = {
    activeTool: ref('select'),
    drawSubTool: ref('pen'),
    undo: vi.fn(),
    redo: vi.fn(),
    saveCurrentBoard: vi.fn(),
  };
  const isActive = ref(active);
  const placeWithTool = vi.fn(() => ['shape', 'sticky'].includes(store.activeTool.value));
  const openMenu = vi.fn();
  const openShortcuts = vi.fn();
  const deleteLooseSelection = vi.fn(() => true);
  const vfNodes = ref<any[]>([]);
  const selection: string[] = [];
  const nudge = vi.fn();
  const handleMindmapAddChild = vi.fn();
  const { handleKeydown, handleKeydownCapture } = useWhiteboardKeyboard({
    isActive: () => isActive.value,
    startEditing: vi.fn(),
    store,
    vfNodes,
    vfEdges: ref([]),
    deleteNodes: vi.fn(),
    syncToVueFlow: vi.fn(),
    scheduleSave: vi.fn(),
    arrange: {
      selectedIds: vi.fn(() => selection),
      duplicate: vi.fn(),
      selectAll: vi.fn(),
      nudge,
      reorder: vi.fn(),
      toggleLock: vi.fn(),
      frameSelection: vi.fn(),
      copyStyle: vi.fn(),
      pasteStyle: vi.fn(),
    },
    focusMindmapNode: vi.fn(),
    handleMindmapAddChild,
    handleMindmapAddSibling: vi.fn(),
    handleMindmapRemoveNode: vi.fn(),
    handleMultiGroup: vi.fn(),
    handleMultiUngroup: vi.fn(),
    closeEdgeMenu: vi.fn(),
    closeShapeMenu: vi.fn(),
    closeTextMenu: vi.fn(),
    placeWithTool,
    openMenu,
    openShortcuts,
    deleteLooseSelection,
  });

  /** Press a key with focus on `on` (the page body unless given). */
  const press = (init: KeyboardEventInit, on: HTMLElement = document.body) => {
    const event = new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init });
    const listener = (e: Event) => handleKeydown(e as KeyboardEvent);
    on.addEventListener('keydown', listener);
    on.dispatchEvent(event);
    on.removeEventListener('keydown', listener);
    return event;
  };
  return { store, isActive, press, placeWithTool, openMenu, openShortcuts, deleteLooseSelection, handleKeydownCapture, vfNodes, handleMindmapAddChild, selection, nudge };
};

describe('board shortcuts', () => {
  it('nudges the selection a pixel before the canvas can move it its own way', () => {
    const { handleKeydownCapture, selection, nudge } = setup();
    selection.push('a');
    const node = document.createElement('div');
    node.className = 'vue-flow__node';
    const canvas = document.createElement('div');
    canvas.className = 'wb-canvas';
    canvas.appendChild(node);
    document.body.appendChild(canvas);
    const theirs = vi.fn((e: Event) => e.preventDefault());
    node.addEventListener('keydown', theirs);
    window.addEventListener('keydown', handleKeydownCapture, true);
    node.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true, cancelable: true }));
    window.removeEventListener('keydown', handleKeydownCapture, true);
    canvas.remove();
    expect(theirs).not.toHaveBeenCalled();
    expect(nudge).toHaveBeenCalledWith(1, 0);
  });

  it('lets Tab move on when focus is not on the selected mind-map item', () => {
    const { press, vfNodes, handleMindmapAddChild } = setup();
    vfNodes.value = [{ id: 'm', type: 'mindmap', selected: true, data: {} }];
    const elsewhere = document.createElement('div');
    elsewhere.setAttribute('role', 'group');
    document.body.appendChild(elsewhere);
    const event = press({ key: 'Tab' }, elsewhere);
    elsewhere.remove();
    expect(event.defaultPrevented).toBe(false);
    expect(handleMindmapAddChild).not.toHaveBeenCalled();
    // On the board itself it adds a branch, as before.
    press({ key: 'Tab' });
    expect(handleMindmapAddChild).toHaveBeenCalledOnce();
  });

  it('deletes selected ink before the canvas can claim the Delete key', () => {
    const { deleteLooseSelection, handleKeydownCapture } = setup();
    // The canvas listens on the document and takes the key for its own items.
    const canvas = vi.fn((e: Event) => e.preventDefault());
    window.addEventListener('keydown', handleKeydownCapture, true);
    document.addEventListener('keydown', canvas);
    const event = new KeyboardEvent('keydown', { key: 'Delete', bubbles: true, cancelable: true });
    document.body.dispatchEvent(event);
    window.removeEventListener('keydown', handleKeydownCapture, true);
    document.removeEventListener('keydown', canvas);
    expect(deleteLooseSelection).toHaveBeenCalledOnce();
    // The whole selection went in one step; the canvas never saw the key.
    expect(canvas).not.toHaveBeenCalled();
  });

  it('switches tools only with focus on the board, and never with Shift', () => {
    // WCAG 2.1.4: a single letter must not act from anywhere it was not meant for.
    const { store, press } = setup();
    const sidebar = document.createElement('button');
    document.body.appendChild(sidebar);
    press({ key: 's' }, sidebar);
    expect(store.activeTool.value).toBe('select');

    press({ key: 'S', shiftKey: true });
    expect(store.activeTool.value).toBe('select');

    const canvas = document.createElement('div');
    canvas.className = 'wb-canvas';
    const onCanvas = document.createElement('button');
    canvas.appendChild(onCanvas);
    document.body.appendChild(canvas);
    press({ key: 's' }, onCanvas);
    expect(store.activeTool.value).toBe('shape');
    sidebar.remove();
    canvas.remove();
  });

  it('opens the list of shortcuts with ?', () => {
    const { press, openShortcuts } = setup();
    const event = press({ key: '?', shiftKey: true });
    expect(openShortcuts).toHaveBeenCalledOnce();
    expect(event.defaultPrevented).toBe(true);
  });

  it('switches tools from the canvas', () => {
    const { store, press } = setup();
    press({ key: 't' });
    expect(store.activeTool.value).toBe('text');
  });

  it('leaves keys typed into a rich-text editor alone', () => {
    // A note's editor is a contenteditable element, not an input: typing
    // "t" in it used to switch the board's tool.
    const { store, press } = setup();
    const editor = document.createElement('div');
    editor.setAttribute('contenteditable', 'true');
    document.body.appendChild(editor);

    const event = press({ key: 't' }, editor);
    press({ key: 'z', metaKey: true }, editor);

    expect(store.activeTool.value).toBe('select');
    expect(store.undo).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(false);
    editor.remove();
  });

  it('does nothing while another app is on screen', () => {
    const { store, press } = setup(false);
    const event = press({ key: 'z', ctrlKey: true });
    press({ key: 'd' });

    expect(store.undo).not.toHaveBeenCalled();
    expect(store.activeTool.value).toBe('select');
    expect(event.defaultPrevented).toBe(false);
  });

  it('redoes with Shift held, which reports a capital Z, and with Ctrl+Y', () => {
    const { store, press } = setup();
    press({ key: 'Z', ctrlKey: true, shiftKey: true });
    press({ key: 'y', ctrlKey: true });

    expect(store.redo).toHaveBeenCalledTimes(2);
    expect(store.undo).not.toHaveBeenCalled();
  });

  it('undoes with Caps Lock on', () => {
    const { store, press } = setup();
    press({ key: 'Z', metaKey: true });
    expect(store.undo).toHaveBeenCalledTimes(1);
  });
});

describe('board embeds in a note', () => {
  it('escape what goes inside an attribute, and read back as written', () => {
    const title = 'Plan "B" <draft> & co';
    const html = `<div data-title="${escapeAttribute(title)}"></div>`;
    const parsed = new DOMParser().parseFromString(html, 'text/html').querySelector('div')!;
    expect(parsed.getAttribute('data-title')).toBe(title);
  });
});

describe('the keyboard reaches what a click does', () => {
  it('opens the board menu with Shift+F10 or the menu key', () => {
    const { press, openMenu } = setup();
    press({ key: 'F10', shiftKey: true });
    press({ key: 'ContextMenu' });
    expect(openMenu).toHaveBeenCalledTimes(2);
  });

  it('puts down what the active tool makes on Enter, and leaves Enter alone otherwise', () => {
    const { store, press, placeWithTool } = setup();
    store.activeTool.value = 'sticky';
    expect(press({ key: 'Enter' }).defaultPrevented).toBe(true);
    store.activeTool.value = 'select';
    expect(press({ key: 'Enter' }).defaultPrevented).toBe(false);
    expect(placeWithTool).toHaveBeenCalledTimes(2);
  });
});
