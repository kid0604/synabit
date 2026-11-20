import { describe, it, expect } from 'vitest';
import { ref } from 'vue';
import { useWhiteboardStore, MINDMAP_COLORS } from '../composables/useWhiteboardStore';
import { newBoardData } from '../boardFile';

/** A store holding one board with a central idea on it. */
const withRoot = () => {
  const store = useWhiteboardStore(ref('/vault'));
  const data = newBoardData('Map');
  data.nodes = [{ id: 'root', type: 'mindmap', position: { x: 0, y: 0 }, data: { label: 'Root', level: 0, color: MINDMAP_COLORS[0] } }];
  store.currentBoardData.value = data;
  store.currentBoardId.value = 'Whiteboards/map.whiteboard.json';
  return store;
};
const colourOf = (store: ReturnType<typeof withRoot>, id: string) => store.currentBoardData.value!.nodes.find((n) => n.id === id)!.data.color;

describe('a mind map made by hand', () => {
  it('colours each main branch its own, and what is below it the same', () => {
    const store = withRoot();
    const a = store.addMindmapChild('root', 'right')!;
    const b = store.addMindmapChild('root', 'right')!;
    const a1 = store.addMindmapChild(a, 'right')!;
    expect(colourOf(store, a)).not.toBe(colourOf(store, b));
    expect(colourOf(store, a)).not.toBe(MINDMAP_COLORS[0]);
    expect(colourOf(store, a1)).toBe(colourOf(store, a));
  });

  it('adds no sibling to a central idea', () => {
    const store = withRoot();
    expect(store.addMindmapSibling('root')).toBeUndefined();
    expect(store.currentBoardData.value!.nodes).toHaveLength(1);
  });

  it('adds a sibling under the tree’s parent, not under an arrow drawn in', () => {
    const store = withRoot();
    const board = store.currentBoardData.value!;
    const a = store.addMindmapChild('root', 'right')!;
    board.nodes.push({ id: 'note', type: 'sticky', position: { x: -400, y: 0 }, data: { label: 'Goal' } });
    board.edges.unshift({ id: 'arrow', source: 'note', target: a, type: 'default', data: {} });
    const sibling = store.addMindmapSibling(a)!;
    expect(board.edges.find((e) => e.target === sibling)!.source).toBe('root');
    expect(store.addMindmapSibling('root')).toBeUndefined();
  });
});
