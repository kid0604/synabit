import { describe, it, expect, vi, afterEach } from 'vitest';
import { mount, type VueWrapper } from '@vue/test-utils';
import { ref, markRaw } from 'vue';
import { Editor } from '@tiptap/vue-3';
import StarterKit from '@tiptap/starter-kit';
import TaskList from '@tiptap/extension-task-list';
import TaskItem from '@tiptap/extension-task-item';
import EditorToolbar from '../toolbar/EditorToolbar.vue';

vi.mock('../../../../composables/usePlatform', () => ({
  usePlatform: () => ({ isMac: ref(true) }),
}));
vi.mock('vue-i18n', async (importOriginal) => ({
  ...(await importOriginal<typeof import('vue-i18n')>()),
  useI18n: () => ({ t: (key: string, named?: Record<string, unknown>) => (named ? `${key}:${JSON.stringify(named)}` : key) }),
}));

let wrapper: VueWrapper | null = null;
let editor: Editor | null = null;

afterEach(() => {
  wrapper?.unmount();
  editor?.destroy();
  wrapper = null;
  editor = null;
});

function setup(content = '<p>hello</p>') {
  editor = new Editor({ extensions: [StarterKit, TaskList, TaskItem], content });
  const insertCommand = vi.fn();
  wrapper = mount(EditorToolbar, {
    attachTo: document.body,
    props: {
      editor: markRaw(editor) as any,
      placement: 'top',
      insertItems: [{ title: 'Table', titleKey: 'note.slash.table.title', descriptionKey: 'd', icon: { render: () => null }, command: insertCommand }],
    },
  });
  return { editor, wrapper, insertCommand };
}

describe('EditorToolbar', () => {
  it('is one toolbar with a single tab stop, named buttons and Mac shortcuts', () => {
    const { wrapper } = setup();
    const bar = wrapper.get('[role="toolbar"]');
    const buttons = bar.findAll('[data-toolbar-item]');
    expect(buttons.map((b) => b.attributes('tabindex')).filter((t) => t === '0')).toHaveLength(1);
    const bold = buttons.find((b) => b.attributes('aria-label')?.startsWith('note.editor.bold'))!;
    expect(bold.attributes('aria-label')).toBe('note.editor.bold (⌘B)');
    expect(bold.attributes('title')).toBe('note.editor.bold (⌘B)');
    expect(bold.attributes('aria-pressed')).toBe('false');
  });

  it('runs the command and reports the mark as pressed', async () => {
    const { editor, wrapper } = setup();
    editor.commands.selectAll();
    const bold = wrapper.findAll('[data-toolbar-item]').find((b) => b.attributes('aria-label')?.startsWith('note.editor.bold'))!;
    await bold.trigger('click');
    expect(editor.getHTML()).toContain('<strong>hello</strong>');
    await wrapper.vm.$forceUpdate();
    expect(bold.attributes('aria-pressed')).toBe('true');
  });

  it('moves focus along the row with the arrow keys', async () => {
    const { wrapper } = setup();
    const buttons = wrapper.findAll('[data-toolbar-item]');
    (buttons[0].element as HTMLElement).focus();
    await wrapper.get('[role="toolbar"]').trigger('keydown', { key: 'ArrowRight' });
    expect(document.activeElement).toBe(buttons[1].element);
    await wrapper.get('[role="toolbar"]').trigger('keydown', { key: 'End' });
    expect(document.activeElement).toBe(buttons[buttons.length - 1].element);
  });

  it('sets a heading from the text style menu', async () => {
    const { editor, wrapper } = setup();
    await wrapper.findAll('[data-toolbar-item]')[0].trigger('click');
    const items = wrapper.findAll('[role="menuitemradio"]');
    expect(items).toHaveLength(4);
    expect(items[0].attributes('aria-checked')).toBe('true');
    await items[2].trigger('click');
    expect(editor.getHTML()).toContain('<h2>hello</h2>');
    expect(wrapper.find('[role="menu"]').exists()).toBe(false);
  });

  it('runs an insert item at the caret with an empty range', async () => {
    const { editor, wrapper, insertCommand } = setup();
    editor.commands.setTextSelection(3);
    const buttons = wrapper.findAll('[data-toolbar-item]');
    await buttons[buttons.length - 1].trigger('click');
    await wrapper.get('[role="menuitem"]').trigger('click');
    expect(insertCommand).toHaveBeenCalledWith({ editor: expect.anything(), range: { from: 3, to: 3 } });
  });
});
