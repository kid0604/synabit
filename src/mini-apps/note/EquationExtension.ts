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

const MATH_INSTALLED = Symbol('math');

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

  // An equation reads as its TeX wherever a node's text is taken. A table cell
  // with nothing else in it was saved empty — tiptap-markdown skips a cell
  // whose text is blank — and the equation was gone (found by the note
  // property test). Copying it as plain text gives the TeX too.
  extendNodeSchema(extension) {
    return extension.name === 'equation' ? { leafText: (node: any) => `$${node.attrs.latex}$` } : {};
  },

  addStorage() {
    return {
      markdown: {
        serialize(state: any, node: any) {
          // No space after: it came back as text, and each save added another
          // (found by the note property test).
          state.write(`$$${node.attrs.latex}$$`);
        },
        parse: {
          setup(markdownit: any) {
            // Every parse runs this on the same instance; a rule added each
            // time ran once per note opened. See `richTablePlugin`.
            if (markdownit[MATH_INSTALLED]) return;
            markdownit[MATH_INSTALLED] = true;
            markdownit.use((md: any) => {
              md.inline.ruler.after('escape', 'math', (state: any, silent: boolean) => {
                const start = state.pos;
                // Check if starting with $
                if (state.src.charCodeAt(start) !== 0x24) return false;
                
                let isDouble = false;
                let contentStart = start + 1;
                
                if (state.src.charCodeAt(start + 1) === 0x24) {
                  isDouble = true;
                  contentStart = start + 2;
                }
                
                const endToken = isDouble ? '$$' : '$';
                const endPos = state.src.indexOf(endToken, contentStart);
                
                if (endPos === -1) return false;
                
                if (!silent) {
                  const token = state.push('math_inline', 'span', 0);
                  token.content = state.src.slice(contentStart, endPos);
                }
                state.pos = endPos + (isDouble ? 2 : 1);
                return true;
              });

              md.renderer.rules.math_inline = (tokens: any, idx: number) => {
                const content = tokens[idx].content;
                return `<span data-latex="${md.utils.escapeHtml(content)}"></span>`;
              };
            });
          }
        }
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
