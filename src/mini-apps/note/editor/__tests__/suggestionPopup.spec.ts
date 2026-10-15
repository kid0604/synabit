import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { Editor } from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import { defineComponent, h } from 'vue';
import { SlashCommands } from '../extensions/slashCommands';
import { suggestionPopup } from '../suggestionPopup';

/** Shaped like SlashCommandMenu: a root that stays rendered, hidden when empty. */
const Menu = defineComponent({
  props: { items: { type: Array, required: true }, command: Function },
  setup(props, { expose }) {
    expose({ onKeyDown: () => false });
    return () => h(
      'div',
      { class: 'test-menu', style: props.items.length ? undefined : { display: 'none' } },
      props.items.map((item: any) => h('button', item)),
    );
  },
});

let editor: Editor;

const flush = () => new Promise(resolve => setTimeout(resolve, 0));
const menu = () => document.querySelector<HTMLElement>('[data-tippy-root] .test-menu');
const popupShown = () => document.querySelector('[data-tippy-root] .tippy-box')?.getAttribute('data-state') === 'visible';

beforeEach(() => {
  editor = new Editor({
    element: document.body.appendChild(document.createElement('div')),
    extensions: [
      StarterKit,
      SlashCommands.configure({
        suggestion: {
          char: '/',
          items: ({ query }: { query: string }) =>
            ['Heading', 'Table', 'Quote'].filter(t => t.toLowerCase().includes(query.toLowerCase())),
          render: suggestionPopup(Menu),
        },
      }),
    ],
    content: '<p></p>',
  });
  editor.commands.focus('end');
});

afterEach(() => {
  editor.destroy();
  document.body.innerHTML = '';
});

describe('suggestionPopup', () => {
  // @tiptap/suggestion 3.31 starts with no items and fetches them after. The
  // menu used to be built from that empty start, and the popup never showed.
  it('shows the menu once a slash is typed', async () => {
    editor.commands.insertContent('/');
    await flush();
    expect(menu()?.textContent).toBe('HeadingTableQuote');
    expect(popupShown()).toBe(true);
  });

  it('narrows the menu as the name is typed', async () => {
    editor.commands.insertContent('/');
    await flush();
    editor.commands.insertContent('ta');
    await flush();
    expect(menu()?.textContent).toBe('Table');
    expect(popupShown()).toBe(true);
  });

  it('hides when nothing matches', async () => {
    editor.commands.insertContent('/');
    await flush();
    editor.commands.insertContent('zzz');
    await flush();
    expect(popupShown()).toBe(false);
  });
});
