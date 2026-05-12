import { mergeAttributes, Node, nodeInputRule } from '@tiptap/core';
import { VueNodeViewRenderer } from '@tiptap/vue-3';
import TransclusionNodeView from '../nodes/TransclusionNodeView.vue';

export const TransclusionExtension = Node.create({
  name: 'transclusion',

  group: 'block',

  atom: true,

  addAttributes() {
    return {
      target: {
        default: null,
        parseHTML: element => element.getAttribute('data-transclusion'),
        renderHTML: attributes => {
          if (!attributes.target) return {};
          return { 'data-transclusion': attributes.target };
        },
      },
      nodeId: {
        default: null,
        parseHTML: element => element.getAttribute('data-node-id'),
        renderHTML: attributes => {
          if (!attributes.nodeId) return {};
          return { 'data-node-id': attributes.nodeId };
        },
      },
    };
  },

  parseHTML() {
    return [
      {
        tag: 'span[data-transclusion]',
      },
    ];
  },

  renderHTML({ HTMLAttributes }) {
    return ['span', mergeAttributes(HTMLAttributes)];
  },

  addNodeView() {
    return VueNodeViewRenderer(TransclusionNodeView);
  },

  addInputRules() {
    return [
      nodeInputRule({
        find: /!\[\[(.*?)\]\]$/,
        type: this.type,
        getAttributes: match => {
          return { target: match[1] };
        },
      }),
    ];
  },
});
