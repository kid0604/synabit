import { Node, mergeAttributes } from '@tiptap/core';
import { VueNodeViewRenderer } from '@tiptap/vue-3';
import WhiteboardNodeView from './nodes/WhiteboardNodeView.vue';
import { boardEmbedHtml, escapeAttribute } from './boardEmbed';

export { escapeAttribute };

export interface WhiteboardEmbedOptions {
  HTMLAttributes: Record<string, any>;
}

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    whiteboard: {
      setWhiteboard: (options: {
        boardId: string;
        boardPath: string;
        title: string;
      }) => ReturnType;
    };
  }
}

export const WhiteboardExtension = Node.create<WhiteboardEmbedOptions>({
  name: 'whiteboard',

  group: 'block',
  atom: true,

  addOptions() {
    return {
      HTMLAttributes: {},
    };
  },

  addAttributes() {
    return {
      boardId: {
        default: '',
        parseHTML: (el) => el.getAttribute('data-board-id') || '',
        renderHTML: (attrs) => ({ 'data-board-id': attrs.boardId }),
      },
      boardPath: {
        default: '',
        parseHTML: (el) => el.getAttribute('data-board-path') || '',
        renderHTML: (attrs) => ({ 'data-board-path': attrs.boardPath }),
      },
      title: {
        default: '',
        parseHTML: (el) => el.getAttribute('data-title') || '',
        renderHTML: (attrs) => ({ 'data-title': attrs.title }),
      },
      width: {
        default: '100%',
        parseHTML: (el) => el.getAttribute('data-width') || '100%',
        renderHTML: (attrs) => ({ 'data-width': attrs.width }),
      },
      height: {
        default: '240px',
        parseHTML: (el) => el.getAttribute('data-height') || '240px',
        renderHTML: (attrs) => ({ 'data-height': attrs.height }),
      },
      align: {
        default: 'center',
        parseHTML: (el) => el.getAttribute('data-align') || 'center',
        renderHTML: (attrs) => ({ 'data-align': attrs.align }),
      },
    };
  },

  parseHTML() {
    return [
      {
        tag: 'div[data-type="whiteboard"]',
      },
    ];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      'div',
      mergeAttributes(this.options.HTMLAttributes, HTMLAttributes, {
        'data-type': 'whiteboard',
        class: 'whiteboard-embed-block',
      }),
    ];
  },

  addNodeView() {
    return VueNodeViewRenderer(WhiteboardNodeView as any);
  },

  addCommands() {
    return {
      setWhiteboard:
        (options: { boardId: string; boardPath: string; title: string }) =>
        ({ commands }) => {
          return commands.insertContent({
            type: this.name,
            attrs: {
              boardId: options.boardId,
              boardPath: options.boardPath,
              title: options.title,
            },
          });
        },
    };
  },

  addStorage() {
    return {
      markdown: {
        serialize(state: any, node: any) {
          const html = boardEmbedHtml(node.attrs);
          state.write(html);
          state.closeBlock(node);
        },
        parse: {
          setup(_markdownit: any) {
            // markdown-it parses HTML automatically, parseHTML() picks up div[data-type="whiteboard"]
          },
        },
      },
    };
  },
});
