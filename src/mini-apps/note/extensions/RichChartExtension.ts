import { Node, mergeAttributes } from '@tiptap/core';
import { VueNodeViewRenderer } from '@tiptap/vue-3';
import RichChartNodeView from '../nodes/RichChartNodeView.vue';
import { chartComment, richChartPlugin } from '../../../shared/rich-table/markdownIt';

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    richChart: {
      /** A chart of a named Rich Table's view, here. */
      insertRichChart: (of: string, view: string) => ReturnType;
    };
  }
}

/**
 * A chart of a Rich Table, anywhere in the note (design §8.2): the table is
 * named by its `name:`, the view by its name, and the chart is that view's —
 * drawn from the table as it is now, so it follows every edit. In the file
 * it is one comment line, which any other Markdown reader leaves invisible.
 */
export const RichChartExtension = Node.create({
  name: 'richChart',
  group: 'block',
  atom: true,
  selectable: true,
  draggable: false,

  addAttributes() {
    return {
      of: {
        default: '',
        parseHTML: (el) => el.getAttribute('data-of') ?? '',
        renderHTML: (attrs) => ({ 'data-of': attrs.of }),
      },
      view: {
        default: '',
        parseHTML: (el) => el.getAttribute('data-view') ?? '',
        renderHTML: (attrs) => ({ 'data-view': attrs.view }),
      },
      /** What else the comment said, kept to be written back. */
      rest: {
        default: '',
        parseHTML: (el) => el.getAttribute('data-rest') ?? '',
        renderHTML: (attrs) => (attrs.rest ? { 'data-rest': attrs.rest } : {}),
      },
    };
  },

  parseHTML() {
    return [{ tag: 'div[data-type="rich-chart"]' }];
  },

  renderHTML({ HTMLAttributes }) {
    return ['div', mergeAttributes(HTMLAttributes, { 'data-type': 'rich-chart' })];
  },

  addNodeView() {
    return VueNodeViewRenderer(RichChartNodeView as any, {
      stopEvent: ({ event }) => !!(event.target as Element | null)?.closest?.('[data-rt-interactive]'),
    });
  },

  addCommands() {
    return {
      insertRichChart:
        (of: string, view: string) =>
        ({ commands }) => commands.insertContent({ type: this.name, attrs: { of, view } }),
    };
  },

  addStorage() {
    return {
      markdown: {
        serialize(state: any, node: any) {
          state.write(chartComment(node.attrs.of, node.attrs.view, node.attrs.rest));
          state.closeBlock(node);
        },
        parse: {
          setup(markdownit: any) {
            richChartPlugin(markdownit);
          },
        },
      },
    };
  },
});
