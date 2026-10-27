import { Node, mergeAttributes } from '@tiptap/core';
import { VueNodeViewRenderer } from '@tiptap/vue-3';
import SynVisualNodeView from './nodes/SynVisualNodeView.vue';

/**
 * A visual Syn drew, kept in a note: one line in the note's markdown,
 * `<div data-type="syn-visual" data-visual="<id>"></div>`, pointing at the
 * visual's own node (`SynVisuals/<id>.md`), as a whiteboard block points at its
 * board. Shown by the same `SynVisual` the chat uses.
 *
 * Without this, a note kept from the chat would hold a block the editor does
 * not know — and an unknown block is dropped on the next save.
 */
export const SynVisualExtension = Node.create({
  name: 'synVisual',
  group: 'block',
  atom: true,
  selectable: true,
  draggable: true,

  addAttributes() {
    return {
      visual: {
        default: '',
        // Only an id: 32 hex digits. Anything else in the attribute is not one.
        parseHTML: (el) => (el.getAttribute('data-visual') || '').replace(/[^0-9a-f]/g, '').slice(0, 32),
        renderHTML: (attrs) => ({ 'data-visual': attrs.visual }),
      },
    };
  },

  parseHTML() {
    return [{ tag: 'div[data-type="syn-visual"]' }];
  },

  renderHTML({ HTMLAttributes }) {
    return ['div', mergeAttributes(HTMLAttributes, { 'data-type': 'syn-visual' })];
  },

  addNodeView() {
    return VueNodeViewRenderer(SynVisualNodeView as any);
  },

  addStorage() {
    return {
      vaultPath: '',
      markdown: {
        serialize(state: any, node: any) {
          const id = String(node.attrs.visual ?? '').replace(/[^0-9a-f]/g, '');
          state.write(`<div data-type="syn-visual" data-visual="${id}"></div>\n`);
          state.closeBlock(node);
        },
        parse: {
          setup() {
            // markdown-it passes the HTML through; parseHTML() takes it from there.
          },
        },
      },
    };
  },
});
