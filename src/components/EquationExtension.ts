import { Node, mergeAttributes, InputRule } from '@tiptap/core';
import { VueNodeViewRenderer } from '@tiptap/vue-3';
import EquationNodeView from './EquationNodeView.vue';

export interface EquationOptions {
  HTMLAttributes: Record<string, any>;
}

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    equation: {
      insertEquation: (options?: { latex: string }) => ReturnType;
    };
  }
}

// Regex for $$...$$
const extractMathRule = /(?:\$\$)([^\$]+)(?:\$\$)\s$/;

export const EquationExtension = Node.create<EquationOptions>({
  name: 'equation',

  group: 'inline',
  inline: true,
  atom: true,

  addOptions() {
    return {
      HTMLAttributes: {
        class: 'equation'
      },
    };
  },

  addAttributes() {
    return {
      latex: {
        default: '',
        parseHTML: element => element.getAttribute('data-latex'),
        renderHTML: attributes => {
          return {
            'data-latex': attributes.latex,
          };
        },
      },
    };
  },

  addStorage() {
    return {
      markdown: {
        serialize(state: any, node: any) {
          state.write(`$$${node.attrs.latex}$$ `);
        },
      },
    };
  },

  parseHTML() {
    return [
      {
        tag: 'span[data-latex]',
      },
    ];
  },

  renderHTML({ HTMLAttributes }) {
    return ['span', mergeAttributes(this.options.HTMLAttributes, HTMLAttributes)];
  },

  addNodeView() {
    return VueNodeViewRenderer(EquationNodeView);
  },

  addCommands() {
    return {
      insertEquation:
        (options) =>
        ({ commands }) => {
          return commands.insertContent({
            type: this.name,
            attrs: options,
          });
        },
    };
  },

  addInputRules() {
    return [
      new InputRule({
        find: extractMathRule,
        handler: ({ state, range, match }) => {
          const { tr } = state;
          const start = range.from;
          const end = range.to;
          
          const latex = match[1];

          tr.replaceWith(start, end, this.type.create({ latex }));
        },
      }),
    ];
  },
});
