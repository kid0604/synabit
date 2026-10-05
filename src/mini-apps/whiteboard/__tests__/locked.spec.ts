import { describe, expect, it, vi } from 'vitest';
import { ref } from 'vue';
import { useShapeMenu } from '../composables/useShapeMenu';
import { useTextMenu } from '../composables/useTextMenu';

const storeWith = (locked: boolean) => ({
  currentBoardData: ref({ nodes: [{ id: 'a', type: 'shape', position: { x: 0, y: 0 }, data: { locked } }, { id: 't', type: 'text', position: { x: 0, y: 0 }, data: { locked } }] }),
});

describe('a locked item, from its panel', () => {
  it('can be unlocked, and nothing else', () => {
    const update = vi.fn();
    const remove = vi.fn();
    const shape = useShapeMenu(storeWith(true), update, remove);
    shape.handleShapeUpdate('a', { color: '#f00' });
    shape.handleShapeDelete('a');
    expect(update).not.toHaveBeenCalled();
    expect(remove).not.toHaveBeenCalled();
    shape.handleShapeUpdate('a', { locked: false });
    expect(update).toHaveBeenCalledWith('a', { locked: false });

    const text = useTextMenu(storeWith(true), ref([]), update, remove);
    text.handleTextUpdate('t', { fontSize: 30 });
    text.handleTextDelete('t');
    expect(update).toHaveBeenCalledTimes(1);
    expect(remove).not.toHaveBeenCalled();
  });

  it('is changed as before once unlocked', () => {
    const update = vi.fn();
    const remove = vi.fn();
    const shape = useShapeMenu(storeWith(false), update, remove);
    shape.handleShapeUpdate('a', { color: '#f00' });
    shape.handleShapeDelete('a');
    expect(update).toHaveBeenCalled();
    expect(remove).toHaveBeenCalledWith(['a']);
  });
});
