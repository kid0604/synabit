import { describe, expect, it, vi } from 'vitest';
import { mount } from '@vue/test-utils';
import { createI18n } from 'vue-i18n';

const drawn = vi.fn(() => '<svg></svg>');
vi.mock('../../../shared/boardPreview', () => ({ boardPreview: () => drawn() }));
vi.mock('../iconCatalog', () => ({ loadIcons: () => new Promise(() => {}), findIcons: () => [] }));
import ShapePicker, { gridMove, columnsOf, libraryPreview } from '../components/ShapePicker.vue';
import type { LibraryItem, ShapeLibrary } from '../shapeLibraries';

describe('moving around a grid of pieces', () => {
  it('goes along a row, down and up a column, and stops at the edges', () => {
    // 10 buttons, 4 to a row: rows 0-3, 4-7, 8-9.
    expect(gridMove('ArrowRight', 2, 10, 4)).toBe(3);
    expect(gridMove('ArrowRight', 9, 10, 4)).toBe(9);
    expect(gridMove('ArrowLeft', 0, 10, 4)).toBe(0);
    expect(gridMove('ArrowDown', 1, 10, 4)).toBe(5);
    expect(gridMove('ArrowDown', 6, 10, 4)).toBe(6); // nothing below in a short last row
    expect(gridMove('ArrowUp', 9, 10, 4)).toBe(5);
    expect(gridMove('ArrowUp', 2, 10, 4)).toBe(2);
    expect(gridMove('Home', 7, 10, 4)).toBe(0);
    expect(gridMove('End', 1, 10, 4)).toBe(9);
  });

  it('leaves Enter, Space and Tab to the button and the page', () => {
    for (const key of ['Enter', ' ', 'Tab', 'a']) expect(gridMove(key, 1, 10, 4)).toBeNull();
  });

  it('counts the columns from where the buttons sit', () => {
    expect(columnsOf([0, 0, 0, 42, 42, 42, 84])).toBe(3);
    expect(columnsOf([0, 0])).toBe(2);
    expect(columnsOf([])).toBe(1);
  });
});

describe('library previews', () => {
  it('are drawn once per piece, however often the list is filtered', () => {
    drawn.mockClear();
    const item = { id: 'a', title: 'Server', nodes: [{ id: 'n' }], edges: [] } as unknown as LibraryItem;
    const empty = { id: 'b', title: 'Empty', nodes: [], edges: [] } as unknown as LibraryItem;
    expect(libraryPreview(item)).toBe('<svg></svg>');
    libraryPreview(item);
    libraryPreview(item);
    expect(libraryPreview(empty)).toBe('');
    expect(drawn).toHaveBeenCalledTimes(1);
  });

  it('are not redrawn when typing in the search box', async () => {
    drawn.mockClear();
    const lib = {
      path: 'lib/a.json', name: 'Cloud',
      items: ['Server', 'Database', 'Queue'].map((title, i) => ({ id: `i${i}`, title, nodes: [{ id: 'n' }], edges: [] })),
    } as unknown as ShapeLibrary;
    const i18n = createI18n({ legacy: false, locale: 'en', missingWarn: false, fallbackWarn: false, messages: { en: {} } });
    const wrapper = mount(ShapePicker, { props: { libraries: [lib] }, global: { plugins: [i18n] } });
    await wrapper.findAll('[role="tab"]')[2].trigger('click');
    expect(drawn).toHaveBeenCalledTimes(3);
    const input = wrapper.find('input[type="search"]');
    for (const q of ['s', 'se', 'ser', '']) await input.setValue(q);
    expect(drawn).toHaveBeenCalledTimes(3);
    wrapper.unmount();
  });
});

describe('the shapes grid', () => {
  it('is one Tab stop, and the arrow keys move that stop', async () => {
    const i18n = createI18n({ legacy: false, locale: 'en', missingWarn: false, fallbackWarn: false, messages: { en: {} } });
    const wrapper = mount(ShapePicker, { props: { libraries: [] }, global: { plugins: [i18n] }, attachTo: document.body });
    await new Promise((r) => setTimeout(r, 0)); // the search box takes focus on open
    const grid = wrapper.find('.wb-picker-grid');
    const buttons = () => grid.findAll('button');
    expect(buttons().filter((b) => b.attributes('tabindex') === '0')).toHaveLength(1);
    expect(buttons()[0].attributes('tabindex')).toBe('0');
    (buttons()[0].element as HTMLElement).focus();
    await buttons()[0].trigger('keydown', { key: 'ArrowRight' });
    expect(document.activeElement).toBe(buttons()[1].element);
    expect(buttons()[1].attributes('tabindex')).toBe('0');
    expect(buttons()[0].attributes('tabindex')).toBe('-1');
    await buttons()[1].trigger('keydown', { key: 'End' });
    expect(document.activeElement).toBe(buttons()[buttons().length - 1].element);
    wrapper.unmount();
  });
});
