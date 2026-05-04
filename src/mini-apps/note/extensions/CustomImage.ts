import Image from '@tiptap/extension-image';
import { VueNodeViewRenderer } from '@tiptap/vue-3';
import ImageNode from '../nodes/ImageNode.vue';

export const CustomImage = Image.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      width: {
        default: null,
        parseHTML: element => element.getAttribute('width'),
        renderHTML: attributes => {
          if (!attributes.width) return {};
          return { width: attributes.width };
        },
      },
      height: {
        default: null,
        parseHTML: element => element.getAttribute('height'),
        renderHTML: attributes => {
          if (!attributes.height) return {};
          return { height: attributes.height };
        },
      },
      align: {
        default: 'center',
        parseHTML: element => element.getAttribute('data-align') || 'center',
        renderHTML: attributes => {
          return { 'data-align': attributes.align };
        },
      },
      rotation: {
        default: 0,
        parseHTML: element => parseInt(element.getAttribute('data-rotation') || '0', 10),
        renderHTML: attributes => {
          return { 'data-rotation': attributes.rotation };
        },
      },
      caption: {
        default: '',
        parseHTML: element => element.getAttribute('data-caption') || '',
        renderHTML: attributes => {
          return { 'data-caption': attributes.caption };
        },
      },
    };
  },

  addStorage() {
    return {
      markdown: {
        serialize(state: any, node: any) {
          const { src, alt, title, width, height, align, rotation, caption } = node.attrs;
          
          const hasCustomAttrs = (width && width !== 'auto') || 
                                 (height && height !== 'auto') || 
                                 (align && align !== 'center') || 
                                 (rotation && rotation !== 0) || 
                                 (caption && caption !== '');
          
          if (!hasCustomAttrs) {
            // Standard markdown serialization
            const altText = alt ? alt.replace(/([\[\]])/g, '\\$1') : '';
            const titleText = title ? ` "${title.replace(/"/g, '\\"')}"` : '';
            state.write(`![${altText}](${src}${titleText})`);
          } else {
            // HTML serialization to preserve custom attributes
            let html = `<img src="${src}"`;
            if (alt) html += ` alt="${alt}"`;
            if (title) html += ` title="${title}"`;
            if (width && width !== 'auto') html += ` width="${width}"`;
            if (height && height !== 'auto') html += ` height="${height}"`;
            if (align && align !== 'center') html += ` data-align="${align}"`;
            if (rotation && rotation !== 0) html += ` data-rotation="${rotation}"`;
            if (caption && caption !== '') html += ` data-caption="${caption.replace(/"/g, '&quot;')}"`;
            html += ' />';
            
            state.write(html);
          }
        },
        parse: {
          setup() {
            // markdown-it parses HTML automatically, so parseHTML will pick it up
          }
        }
      }
    };
  },

  addNodeView() {
    return VueNodeViewRenderer(ImageNode);
  },
});
