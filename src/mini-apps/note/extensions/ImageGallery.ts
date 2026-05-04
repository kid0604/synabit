import { Node, mergeAttributes } from '@tiptap/core';
import { VueNodeViewRenderer } from '@tiptap/vue-3';
import ImageGalleryNode from '../nodes/ImageGalleryNode.vue';

export interface GalleryImage {
  src: string;
  alt: string;
  caption: string;
}

export interface ImageGalleryOptions {
  HTMLAttributes: Record<string, any>;
  vaultPath: string;
}

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    imageGallery: {
      setImageGallery: (options: { images: GalleryImage[]; template?: string; caption?: string }) => ReturnType;
    };
  }
}

export const ImageGallery = Node.create<ImageGalleryOptions>({
  name: 'imageGallery',

  group: 'block',
  atom: true,

  addOptions() {
    return {
      HTMLAttributes: {
        class: 'synabit-gallery',
      },
      vaultPath: '',
    };
  },

  addAttributes() {
    return {
      images: {
        default: [],
      },
      template: {
        default: 'classic',
      },
      caption: {
        default: '',
      },
    };
  },

  parseHTML() {
    return [
      {
        tag: 'div.synabit-gallery',
        getAttrs: (element) => {
          if (typeof element === 'string') return {};
          
          const oldLayout = element.getAttribute('data-layout');
          let template = element.getAttribute('data-template') || 'classic';
          if (oldLayout && !element.hasAttribute('data-template')) {
             template = 'classic'; // fallback old grid-2/grid-3 to classic
          }
          const caption = element.getAttribute('data-caption') || '';
          
          const images: GalleryImage[] = [];
          const imgElements = element.querySelectorAll('img');
          imgElements.forEach(img => {
            images.push({
              src: img.getAttribute('src') || '',
              alt: img.getAttribute('alt') || '',
              caption: img.getAttribute('data-caption') || '',
            });
          });
          
          return { template, caption, images };
        },
      },
    ];
  },

  renderHTML({ HTMLAttributes, node }) {
    const { template, caption, images } = node.attrs;
    
    // Create the outer wrapper
    const wrapperArgs: any[] = ['div', mergeAttributes(this.options.HTMLAttributes, HTMLAttributes, {
      'data-template': template,
      'data-caption': caption,
    })];
    
    // Add child img elements
    (images as GalleryImage[]).forEach(img => {
      wrapperArgs.push(['img', {
        src: img.src,
        alt: img.alt,
        'data-caption': img.caption,
      }]);
    });
    
    return wrapperArgs as any;
  },

  addCommands() {
    return {
      setImageGallery: (options) => ({ commands }) => {
        return commands.insertContent({
          type: this.name,
          attrs: options,
        });
      },
    };
  },

  addStorage() {
    return {
      markdown: {
        serialize(state: any, node: any) {
          const { template, caption, images } = node.attrs;
          
          let html = `<div class="synabit-gallery" data-template="${template}"`;
          if (caption) {
            html += ` data-caption="${caption.replace(/"/g, '&quot;')}"`;
          }
          html += `>\n`;
          
          (images as GalleryImage[]).forEach(img => {
            html += `  <img src="${img.src}" alt="${img.alt ? img.alt.replace(/"/g, '&quot;') : ''}"`;
            if (img.caption) {
              html += ` data-caption="${img.caption.replace(/"/g, '&quot;')}"`;
            }
            html += ` />\n`;
          });
          
          html += `</div>\n`;
          state.write(html);
        },
        parse: {
          setup() {
            // HTML is automatically parsed by markdown-it
          }
        }
      }
    };
  },

  addNodeView() {
    return VueNodeViewRenderer(ImageGalleryNode);
  },
});
