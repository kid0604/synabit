import { describe, it, expect, vi } from 'vitest';
import { ref, nextTick } from 'vue';
import { useListKeyboard } from '../composables/useListKeyboard';

const press = (key: string) => {
  const event = new KeyboardEvent('keydown', { key, cancelable: true });
  return event;
};

const setup = (count = 3) => {
  const items = ref(Array.from({ length: count }, (_, i) => ({ id: `p${i}` })));
  const chosen = vi.fn();
  return { items, chosen, keys: useListKeyboard(items, chosen) };
};

describe('moving through the list', () => {
  it('starts on nothing and opens the list at the top', () => {
    const { keys } = setup();
    expect(keys.activeIndex.value).toBe(-1);
    keys.onKeydown(press('ArrowDown'));
    expect(keys.activeIndex.value).toBe(0);
  });

  it('opens at the bottom when arrowing up from nothing', () => {
    const { keys } = setup();
    keys.onKeydown(press('ArrowUp'));
    expect(keys.activeIndex.value).toBe(2);
  });

  it('stops at each end rather than wrapping round', () => {
    // A list that wraps is a list somebody scrolls past the end of without
    // noticing they have started again.
    const { keys } = setup();
    keys.onKeydown(press('ArrowDown'));
    keys.onKeydown(press('ArrowUp'));
    keys.onKeydown(press('ArrowUp'));
    expect(keys.activeIndex.value).toBe(0);

    for (let i = 0; i < 6; i++) keys.onKeydown(press('ArrowDown'));
    expect(keys.activeIndex.value).toBe(2);
  });

  it('jumps to either end', () => {
    const { keys } = setup();
    keys.onKeydown(press('End'));
    expect(keys.activeIndex.value).toBe(2);
    keys.onKeydown(press('Home'));
    expect(keys.activeIndex.value).toBe(0);
  });

  it('opens whoever is on with Enter or Space', () => {
    const { items, chosen, keys } = setup();
    keys.onKeydown(press('ArrowDown'));
    keys.onKeydown(press('Enter'));
    expect(chosen).toHaveBeenCalledWith(items.value[0], 0);

    keys.onKeydown(press('ArrowDown'));
    keys.onKeydown(press(' '));
    expect(chosen).toHaveBeenLastCalledWith(items.value[1], 1);
  });

  it('opens nothing when nothing is on', () => {
    const { chosen, keys } = setup();
    keys.onKeydown(press('Enter'));
    expect(chosen).not.toHaveBeenCalled();
  });
});

describe('what it leaves alone', () => {
  it('swallows the keys it handles and no others', () => {
    // Swallowing the rest would stop the page scrolling and stop somebody
    // typing in the search box above the list.
    const { keys } = setup();

    const handled = press('ArrowDown');
    keys.onKeydown(handled);
    expect(handled.defaultPrevented).toBe(true);

    for (const key of ['a', 'Tab', 'Escape', 'PageDown']) {
      const other = press(key);
      keys.onKeydown(other);
      expect(other.defaultPrevented, key).toBe(false);
    }
  });

  it('does nothing at all with an empty list', () => {
    const { keys } = setup(0);
    for (const key of ['ArrowDown', 'ArrowUp', 'Home', 'End', 'Enter']) {
      keys.onKeydown(press(key));
    }
    expect(keys.activeIndex.value).toBe(-1);
  });
});

describe('keeping up with a list that changes', () => {
  it('does not point past the end when the list shrinks', async () => {
    // Somebody types in the search box while the keyboard is on the last row.
    const { items, keys } = setup(5);
    keys.onKeydown(press('End'));
    expect(keys.activeIndex.value).toBe(4);

    items.value = items.value.slice(0, 2);
    await nextTick();
    expect(keys.activeIndex.value).toBe(1);
  });

  it('survives the list emptying entirely', async () => {
    const { items, keys } = setup(3);
    keys.onKeydown(press('End'));
    items.value = [];
    await nextTick();
    expect(keys.activeIndex.value).toBe(-1);
  });
});

describe('one tab stop, not two thousand', () => {
  it('makes only the active row reachable by Tab', () => {
    const { keys } = setup();
    // Before anything is chosen, the first row is the way in.
    expect(keys.tabIndexFor(0)).toBe(0);
    expect(keys.tabIndexFor(1)).toBe(-1);

    keys.onKeydown(press('ArrowDown'));
    keys.onKeydown(press('ArrowDown'));
    expect(keys.tabIndexFor(1)).toBe(0);
    expect(keys.tabIndexFor(0)).toBe(-1);
  });

  it('follows the mouse, so clicking and typing agree about where you are', () => {
    const { keys } = setup();
    keys.onRowFocus(2);
    expect(keys.activeIndex.value).toBe(2);
    keys.onKeydown(press('ArrowUp'));
    expect(keys.activeIndex.value).toBe(1);
  });
});
