import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ref, computed, defineComponent, h } from 'vue';
import { mount } from '@vue/test-utils';
import { useTaskKeyboard, type TaskKeyboardActions } from '../composables/useTaskKeyboard';
import type { TaskMetadata } from '../types';

const task = (id: string): TaskMetadata => ({ id, path: id, title: id } as TaskMetadata);

const harness = (list: TaskMetadata[] = [task('a'), task('b'), task('c')]) => {
  const rows = ref(list);
  const hasSelection = ref(false);
  const suspended = ref(false);
  const actions: TaskKeyboardActions = {
    createTask: vi.fn(), openTask: vi.fn(), toggleStatus: vi.fn(), deleteTask: vi.fn(),
    selectOne: vi.fn(), selectRange: vi.fn(), clearSelection: vi.fn(),
    selectAllVisible: vi.fn(), focusSearch: vi.fn(), setViewMode: vi.fn(), showHelp: vi.fn(),
  };
  let api!: ReturnType<typeof useTaskKeyboard>;
  const wrapper = mount(defineComponent({
    setup() {
      api = useTaskKeyboard(computed(() => rows.value), computed(() => hasSelection.value), suspended, actions);
      return () => h('div');
    },
  }));
  return { api: () => api, rows, hasSelection, suspended, actions, wrapper };
};

const press = (key: string, init: KeyboardEventInit = {}, target?: EventTarget) => {
  const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init });
  if (target) Object.defineProperty(event, 'target', { value: target });
  window.dispatchEvent(event);
  return event;
};

let harnesses: { wrapper: { unmount: () => void } }[] = [];
const make = (...args: Parameters<typeof harness>) => {
  const h = harness(...args);
  harnesses.push(h);
  return h;
};
beforeEach(() => { harnesses = []; });
afterEach(() => { harnesses.forEach((h) => h.wrapper.unmount()); });

describe('moving the cursor', () => {
  it('starts at the top on the first press of down', () => {
    const h = make();
    press('j');
    expect(h.api().focusedId.value).toBe('a');
  });

  it('starts at the bottom on the first press of up', () => {
    const h = make();
    press('k');
    expect(h.api().focusedId.value).toBe('c');
  });

  it('walks down and back up', () => {
    const h = make();
    press('j'); press('j');
    expect(h.api().focusedId.value).toBe('b');
    press('k');
    expect(h.api().focusedId.value).toBe('a');
  });

  it('stops at the ends rather than wrapping round', () => {
    const h = make();
    press('j'); press('k'); press('k');
    expect(h.api().focusedId.value).toBe('a');
  });

  it('takes the arrow keys as well as j and k', () => {
    const h = make();
    press('ArrowDown');
    expect(h.api().focusedId.value).toBe('a');
  });

  /**
   * The list is rebuilt from the database on every watcher tick. An index
   * would come to rest on a different task each time; an id cannot.
   */
  it('keeps the cursor on the same task when the list is rebuilt', async () => {
    const h = make();
    press('j'); press('j');
    h.rows.value = [task('c'), task('b'), task('a')];
    await h.wrapper.vm.$nextTick();
    expect(h.api().focusedId.value).toBe('b');
  });

  it('moves to where the task was when it disappears', async () => {
    const h = make();
    press('j'); press('j');
    h.rows.value = [task('a'), task('c')];
    await h.wrapper.vm.$nextTick();
    expect(h.api().focusedId.value).toBe('c');
  });

  it('does nothing at all on an empty list', () => {
    const h = make([]);
    press('j');
    expect(h.api().focusedId.value).toBeNull();
  });
});

describe('acting on the focused task', () => {
  it('opens it', () => {
    const h = make();
    press('j'); press('Enter');
    expect(h.actions.openTask).toHaveBeenCalledWith(expect.objectContaining({ id: 'a' }));
  });

  it('toggles it done', () => {
    const h = make();
    press('j'); press(' ');
    expect(h.actions.toggleStatus).toHaveBeenCalled();
  });

  it('deletes it, on either delete key', () => {
    const h = make();
    press('j'); press('Backspace');
    press('Delete');
    expect(h.actions.deleteTask).toHaveBeenCalledTimes(2);
  });

  it('adds it to the selection', () => {
    const h = make();
    press('j'); press('x');
    expect(h.actions.selectOne).toHaveBeenCalledWith('a');
  });

  it('extends the selection with shift', () => {
    const h = make();
    press('j'); press('x', { shiftKey: true });
    expect(h.actions.selectRange).toHaveBeenCalledWith('a');
  });

  it('does nothing when the cursor is nowhere', () => {
    const h = make();
    press('Enter'); press(' '); press('x');
    expect(h.actions.openTask).not.toHaveBeenCalled();
    expect(h.actions.toggleStatus).not.toHaveBeenCalled();
  });
});

describe('the standalone keys', () => {
  it('makes a new task on n and on c', () => {
    const h = make();
    press('n'); press('c');
    expect(h.actions.createTask).toHaveBeenCalledTimes(2);
  });

  it('jumps to the search box', () => {
    const h = make();
    press('/');
    expect(h.actions.focusSearch).toHaveBeenCalled();
  });

  it('switches views on the number keys', () => {
    const h = make();
    press('1'); press('2'); press('3'); press('4');
    expect((h.actions.setViewMode as any).mock.calls.map((c: any[]) => c[0]))
      .toEqual(['list', 'board', 'table', 'matrix']);
  });

  it('shows the shortcut list', () => {
    const h = make();
    press('?');
    expect(h.actions.showHelp).toHaveBeenCalled();
  });

  it('clears the selection on escape', () => {
    const h = make();
    h.hasSelection.value = true;
    press('Escape');
    expect(h.actions.clearSelection).toHaveBeenCalled();
  });

  it('drops the cursor on escape when nothing is selected', () => {
    const h = make();
    press('j');
    press('Escape');
    expect(h.api().focusedId.value).toBeNull();
  });
});

/**
 * These are all bare letters, so this is the difference between a shortcut and
 * the app eating what you type.
 */
describe('staying out of the way', () => {
  it('ignores keys typed into a text field', () => {
    const h = make();
    const input = document.createElement('input');
    press('n', {}, input);
    press('j', {}, input);
    expect(h.actions.createTask).not.toHaveBeenCalled();
    expect(h.api().focusedId.value).toBeNull();
  });

  it('ignores keys typed into a textarea', () => {
    const h = make();
    press('n', {}, document.createElement('textarea'));
    expect(h.actions.createTask).not.toHaveBeenCalled();
  });

  it('ignores keys typed into a rich-text editor', () => {
    const h = make();
    const div = document.createElement('div');
    Object.defineProperty(div, 'isContentEditable', { value: true });
    press('n', {}, div);
    expect(h.actions.createTask).not.toHaveBeenCalled();
  });

  it('ignores keys while a dialog is open', () => {
    const h = make();
    const dialog = document.createElement('div');
    dialog.setAttribute('role', 'dialog');
    const inner = document.createElement('div');
    dialog.appendChild(inner);
    document.body.appendChild(dialog);
    press('n', {}, inner);
    expect(h.actions.createTask).not.toHaveBeenCalled();
    dialog.remove();
  });

  it('does nothing while suspended', () => {
    const h = make();
    h.suspended.value = true;
    press('n'); press('j');
    expect(h.actions.createTask).not.toHaveBeenCalled();
    expect(h.api().focusedId.value).toBeNull();
  });

  /** Cmd+N opens a window; the list has no business claiming it. */
  it('leaves modified keystrokes alone', () => {
    const h = make();
    press('n', { metaKey: true });
    press('j', { ctrlKey: true });
    expect(h.actions.createTask).not.toHaveBeenCalled();
    expect(h.api().focusedId.value).toBeNull();
  });

  it('claims select-all only while something is selected', () => {
    const h = make();
    press('a', { metaKey: true });
    expect(h.actions.selectAllVisible).not.toHaveBeenCalled();
    h.hasSelection.value = true;
    press('a', { metaKey: true });
    expect(h.actions.selectAllVisible).toHaveBeenCalled();
  });

  it('stops the browser acting on a key it has taken', () => {
    make();
    expect(press('j').defaultPrevented).toBe(true);
  });

  it('lets a key it does not use through', () => {
    make();
    expect(press('q').defaultPrevented).toBe(false);
  });
});

/**
 * The cursor is useless if it walks off the bottom of the screen: the keys go
 * on working on a task nobody can see. This matters more now that rows past
 * the fold defer their rendering — they still have a box to scroll to, which
 * is exactly what a virtual scroller would have taken away.
 */
describe('keeping the cursor in view', () => {
  const withRow = (id: string) => {
    const el = document.createElement('div');
    el.setAttribute('data-task-id', id);
    el.scrollIntoView = vi.fn();
    document.body.appendChild(el);
    return el;
  };

  it('scrolls the row it moves to into view', async () => {
    const h = make();
    const row = withRow('a');
    press('j');
    await h.wrapper.vm.$nextTick();
    expect(row.scrollIntoView).toHaveBeenCalled();
    row.remove();
  });

  /** The least scrolling that will do, so stepping does not jerk the list. */
  it('scrolls as little as possible', async () => {
    const h = make();
    const row = withRow('a');
    press('j');
    await h.wrapper.vm.$nextTick();
    expect(row.scrollIntoView).toHaveBeenCalledWith({ block: 'nearest' });
    row.remove();
  });

  it('does not fall over when the row is not in the DOM', async () => {
    const h = make();
    expect(() => press('j')).not.toThrow();
    await h.wrapper.vm.$nextTick();
    expect(h.api().focusedId.value).toBe('a');
  });
});
