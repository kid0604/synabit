import { describe, it, expect } from 'vitest';
import { defineComponent, h, ref } from 'vue';
import { mount } from '@vue/test-utils';

// `vite/client` declares `*?raw`, so reading these needs no Node types in a
// config that has none.
import editorSource from '../../mini-apps/note/TiptapEditor.vue?raw';
import panelSource from '../views/ObjectDetail.vue?raw';

/**
 * Whether leaving the body editor saves what was typed in it.
 *
 * `ObjectDetail` writes `@blur` on `TiptapEditor`, and Things' `saveOrCreate`
 * runs on nothing else — `closeNode` clears state without saving, and opening
 * another node does not save the one being left. So the whole of a node's body
 * rested on that one listener firing, and it did not: `TiptapEditor` declared
 * no `blur` emit, so the listener fell through onto its root `<div>`, and blur
 * does not bubble. Type a paragraph into a `book`, click the next row, and the
 * paragraph was gone.
 *
 * Two tests, because there are two ways to break it again: the mechanism, and
 * the declaration that works around it.
 */

/** The shape TiptapEditor has: a plain wrapper div around the focusable part. */
const WrapperRooted = defineComponent({
  name: 'WrapperRooted',
  emits: ['update:modelValue'],
  setup: () => () => h('div', { class: 'tiptap-wrapper' }, [h('div', { contenteditable: 'true' })]),
});

/** The same, but declaring the emit its parent is listening for. */
const EmitsBlur = defineComponent({
  name: 'EmitsBlur',
  emits: ['update:modelValue', 'blur'],
  setup: (_, { emit }) => () =>
    h('div', { class: 'tiptap-wrapper' }, [
      h('div', { contenteditable: 'true', onBlur: () => emit('blur') }),
    ]),
});

const parentOf = (child: unknown) => {
  const saves = ref(0);
  const host = mount(
    defineComponent({
      setup: () => () => h(child as never, { onBlur: () => (saves.value += 1) }),
    }),
    { attachTo: document.body },
  );
  return { saves, host };
};

/** Focus the inner editable, then take focus away from it. */
const leaveTheEditor = async (host: ReturnType<typeof mount>) => {
  const editable = host.element.querySelector('[contenteditable]') as HTMLElement;
  editable.tabIndex = 0;
  editable.focus();
  editable.blur();
  await host.vm.$nextTick();
};

describe('leaving the body editor', () => {
  it('cannot reach the parent through fallthrough alone, because blur does not bubble', async () => {
    const { saves, host } = parentOf(WrapperRooted);
    await leaveTheEditor(host);
    expect(saves.value, 'the mechanism this whole fix works around').toBe(0);
    host.unmount();
  });

  it('reaches the parent once the editor declares the emit', async () => {
    const { saves, host } = parentOf(EmitsBlur);
    await leaveTheEditor(host);
    expect(saves.value).toBe(1);
    host.unmount();
  });
});

describe('the editor and the panel still agree', () => {
  const editor = editorSource;
  const panel = panelSource;

  it('declares the blur its only saver depends on', () => {
    expect(
      editor,
      'ObjectDetail’s @blur silently stops firing without this line',
    ).toContain("(e: 'blur'): void;");
    expect(panel).toContain('@blur="emit(\'save\')"');
  });

  /**
   * Order, not presence. The listener saves the node, and what it saves is
   * whatever the last `update:modelValue` left in the parent — so announcing
   * the blur before flushing writes the document as it stood one keystroke
   * ago, every single time, while looking exactly like a working save.
   */
  it('flushes the document before announcing the blur', () => {
    const body = editor.slice(editor.indexOf('onBlur: () => {'));
    const flush = body.indexOf('flushSerialize();');
    const announce = body.indexOf("emit('blur');");

    expect(flush, 'onBlur no longer flushes').toBeGreaterThan(-1);
    expect(announce, 'onBlur no longer announces the blur').toBeGreaterThan(-1);
    expect(flush, 'the blur is announced before the document is flushed').toBeLessThan(announce);
  });
});
