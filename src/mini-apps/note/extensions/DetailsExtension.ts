import { Node, mergeAttributes } from '@tiptap/core';
import { VueNodeViewRenderer } from '@tiptap/vue-3';
import DetailsNodeView from '../nodes/DetailsNodeView.vue';

export interface DetailsOptions {
  HTMLAttributes: Record<string, any>;
}

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    details: {
      setDetails: (attrs?: { summary?: string }) => ReturnType;
    };
  }
}

export const DetailsExtension = Node.create<DetailsOptions>({
  name: 'details',

  group: 'block',

  // Allow nested block content (paragraphs, lists, code blocks, etc.)
  content: 'block+',

  defining: true,

  addOptions() {
    return {
      HTMLAttributes: {
        class: 'synabit-details',
      },
    };
  },

  addAttributes() {
    return {
      summary: {
        default: 'Toggle',
        parseHTML: (element: HTMLElement) => {
          const summaryEl = element.querySelector(':scope > summary');
          return summaryEl?.textContent || 'Toggle';
        },
      },
      open: {
        default: true,
        parseHTML: (element: HTMLElement) => element.hasAttribute('open'),
        renderHTML: (attributes: Record<string, any>) => {
          if (attributes.open) {
            return { open: 'open' };
          }
          return {};
        },
      },
    };
  },

  parseHTML() {
    return [
      {
        tag: 'details.synabit-details',
        contentElement: 'div.details-content',
      },
      {
        tag: 'details',
        contentElement: (node: HTMLElement) => {
          const wrapper = node.querySelector(':scope > div.details-content');
          return wrapper || node;
        },
      },
    ];
  },

  renderHTML({ HTMLAttributes, node }) {
    return [
      'details',
      mergeAttributes(this.options.HTMLAttributes, HTMLAttributes),
      ['summary', {}, node.attrs.summary || 'Toggle'],
      ['div', { class: 'details-content' }, 0],
    ];
  },

  addCommands() {
    return {
      setDetails:
        (attrs) =>
        ({ commands }) => {
          return commands.insertContent({
            type: this.name,
            attrs: { summary: attrs?.summary || 'Toggle', open: true },
            content: [{ type: 'paragraph' }],
          });
        },
    };
  },

  addStorage() {
    return {
      markdown: {
        serialize(state: any, node: any) {
          const summary = (node.attrs.summary || 'Toggle')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;');

          state.write(`<details class="synabit-details">\n<summary>${summary}</summary>\n<div class="details-content">\n\n`);
          state.renderContent(node);
          state.write(`\n</div>\n</details>\n`);
        },
        parse: {
          setup() {
            // HTML <details> tags are parsed by markdown-it with html: true
          },
        },
      },
    };
  },

  addNodeView() {
    return VueNodeViewRenderer(DetailsNodeView);
  },
});
