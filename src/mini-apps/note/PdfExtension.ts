import { Node, mergeAttributes } from '@tiptap/core';

export interface PdfOptions {
  HTMLAttributes: Record<string, any>;
}

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    pdf: {
      setPdf: (options: { src: string; title?: string }) => ReturnType;
    };
  }
}

export const PdfExtension = Node.create<PdfOptions>({
  name: 'pdf',

  group: 'block',
  atom: true,

  addOptions() {
    return {
      HTMLAttributes: {},
    };
  },

  addAttributes() {
    return {
      src: {
        default: null,
      },
      title: {
        default: 'PDF Document',
      },
    };
  },

  parseHTML() {
    return [
      {
        tag: 'div[data-pdf-embed]',
        getAttrs: (dom: HTMLElement) => ({
          src: dom.getAttribute('data-src'),
          title: dom.getAttribute('data-title') || 'PDF Document',
        }),
      },
    ];
  },

  renderHTML({ HTMLAttributes }) {
    const src = HTMLAttributes.src || '';
    const title = HTMLAttributes.title || 'PDF Document';

    return ['div', mergeAttributes(this.options.HTMLAttributes, {
      'data-pdf-embed': 'true',
      'data-src': src,
      'data-title': title,
      class: 'pdf-embed-card',
    }), [
      'div', { class: 'pdf-embed-inner' }, [
        'div', { class: 'pdf-embed-icon' }, '📄'
      ], [
        'div', { class: 'pdf-embed-info' }, [
          'span', { class: 'pdf-embed-title' }, title
        ], [
          'span', { class: 'pdf-embed-path' }, src
        ]
      ]
    ]];
  },

  addCommands() {
    return {
      setPdf: (options: { src: string; title?: string }) => ({ commands }) => {
        return commands.insertContent({
          type: this.name,
          attrs: {
            src: options.src,
            title: options.title || options.src.split('/').pop()?.replace(/\.pdf$/i, '') || 'PDF Document',
          },
        });
      },
    };
  },

  addStorage() {
    return {
      markdown: {
        serialize(state: any, node: any) {
          const src = node.attrs.src || '';
          const title = node.attrs.title || 'PDF Document';
          state.write(`<div data-pdf-embed="true" data-src="${src}" data-title="${title}"></div>\n`);
        },
        parse: {
          setup(_markdownit: any) {
            // HTML div is parsed automatically by markdown-it
          }
        }
      }
    };
  }
});
