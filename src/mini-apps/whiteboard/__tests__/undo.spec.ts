import { describe, it, expect, vi, afterEach } from 'vitest';
import { ref } from 'vue';
import { useWhiteboardStore } from '../composables/useWhiteboardStore';
import { newBoardData } from '../boardFile';
import type { WBNode } from '../boardFile';

const node = (id: string): WBNode => ({
  id,
  type: 'shape',
  position: { x: 0, y: 0 },
  data: { label: id, color: '#000' },
});

/** A store holding one board with the given nodes already on it. */
const boardWith = (ids: string[]) => {
  const store = useWhiteboardStore(ref('/vault'));
  const data = newBoardData('Test');
  data.nodes = ids.map(node);
  store.currentBoardData.value = data;
  store.currentBoardId.value = 'Whiteboards/test.whiteboard.json';
  return store;
};

const labels = (store: ReturnType<typeof boardWith>) =>
  store.currentBoardData.value!.nodes.map((n) => n.data.label);

afterEach(() => {
  vi.useRealTimers();
});

describe('one step back per thing the user did', () => {
  it('folds a run of operations into a single step', () => {
    // Erasing across a drawing removes and rebuilds strokes per pointer
    // event. Each one used to be its own step, so a single wipe could push
    // everything else off a fifty-deep stack.
    const store = boardWith(['a', 'b', 'c']);

    store.beginUndoBatch();
    store.removeNode('a');
    store.removeNode('b');
    store.removeNode('c');
    store.endUndoBatch();

    expect(store.undoStack.value).toHaveLength(1);
    store.undo();
    expect(labels(store)).toEqual(['a', 'b', 'c']);
  });

  it('folds a stream of edits to one item into a single step', () => {
    // A colour slider emits on `input`: one event per pixel dragged.
    const store = boardWith(['a']);

    store.updateNodeData('a', { color: '#111' });
    store.updateNodeData('a', { color: '#222' });
    store.updateNodeData('a', { color: '#333' });

    expect(store.undoStack.value).toHaveLength(1);
    store.undo();
    expect(store.currentBoardData.value!.nodes[0].data.color).toBe('#000');
  });

  it('keeps edits to different items apart', () => {
    const store = boardWith(['a', 'b']);

    store.updateNodeData('a', { color: '#111' });
    store.updateNodeData('b', { color: '#222' });

    expect(store.undoStack.value).toHaveLength(2);
  });

  it('starts a new step once the user pauses', () => {
    vi.useFakeTimers();
    const store = boardWith(['a']);

    store.updateNodeData('a', { color: '#111' });
    vi.advanceTimersByTime(5000);
    store.updateNodeData('a', { color: '#222' });

    expect(store.undoStack.value).toHaveLength(2);
  });

  it('records nothing for an operation that changed nothing', () => {
    const store = boardWith(['a']);

    store.pushUndoState();
    store.pushUndoState();

    expect(store.undoStack.value).toHaveLength(1);
  });

  it('carries a step forward again on redo', () => {
    const store = boardWith(['a', 'b']);

    store.removeNode('b');
    expect(labels(store)).toEqual(['a']);

    store.undo();
    expect(labels(store)).toEqual(['a', 'b']);

    store.redo();
    expect(labels(store)).toEqual(['a']);
  });

  it('does not fold the next edit into the one it was just undone to', () => {
    vi.useFakeTimers();
    const store = boardWith(['a']);

    store.updateNodeData('a', { color: '#111' });
    store.undo();
    store.updateNodeData('a', { color: '#222' });

    // Both edits are on the same node inside the coalescing window, but the
    // undo between them is a boundary the user drew themselves.
    expect(store.undoStack.value).toHaveLength(1);
    store.undo();
    expect(store.currentBoardData.value!.nodes[0].data.color).toBe('#000');
  });
});

describe('change stamps', () => {
  it('marks an item as changed when its data is edited', () => {
    const store = boardWith(['a']);
    const before = store.currentBoardData.value!.nodes[0].updated;

    store.updateNodeData('a', { color: '#111' });

    expect(store.currentBoardData.value!.nodes[0].updated).not.toBe(before);
    expect(typeof store.currentBoardData.value!.nodes[0].updated).toBe('number');
  });

  it('marks an item as changed when it is moved', () => {
    const store = boardWith(['a']);
    store.currentBoardData.value!.nodes[0].updated = 0;

    store.stampNode('a');

    expect(store.currentBoardData.value!.nodes[0].updated).toBeGreaterThan(0);
  });

  it('stamps a new item as it is added', () => {
    const store = boardWith([]);
    store.addNode(node('fresh'));
    expect(store.currentBoardData.value!.nodes[0].updated).toBeGreaterThan(0);
  });
});
