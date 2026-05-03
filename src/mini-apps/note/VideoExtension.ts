import { Node, mergeAttributes } from '@tiptap/core';

function getYouTubeEmbedUrl(url: string): string {
  try {
    const urlObj = new URL(url);
    if (urlObj.hostname.includes('youtube.com') || urlObj.hostname.includes('youtu.be')) {
      let videoId = '';
      if (urlObj.hostname.includes('youtu.be')) {
        videoId = urlObj.pathname.slice(1);
      } else if (urlObj.pathname === '/watch') {
        videoId = urlObj.searchParams.get('v') || '';
      } else if (urlObj.pathname.startsWith('/embed/')) {
        return url; // Already an embed URL
      }
      
      if (videoId) {
        const params = new URLSearchParams(urlObj.search);
        params.delete('v');
        params.set('autoplay', '0');
        const paramStr = params.toString();
        return `https://www.youtube.com/embed/${videoId}${paramStr ? '?' + paramStr : ''}`;
      }
    }
  } catch (e) {
    // Fallback if URL parsing fails
  }
  return url;
}

export interface VideoOptions {
  HTMLAttributes: Record<string, any>;
}

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    video: {
      setVideo: (options: { src: string }) => ReturnType;
    };
  }
}

export const VideoExtension = Node.create<VideoOptions>({
  name: 'video',

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
    };
  },

  parseHTML() {
    return [
      {
        tag: 'video[src]',
      },
      {
        tag: 'iframe[src*="youtube.com"]',
      },
      {
        tag: 'iframe[src*="youtu.be"]',
      },
    ];
  },

  renderHTML({ HTMLAttributes }) {
    const src = HTMLAttributes.src || '';
    if (src.includes('youtube.com') || src.includes('youtu.be')) {
      const embedSrc = getYouTubeEmbedUrl(src);
      return ['div', { class: 'video-wrapper aspect-video w-full' }, 
        ['iframe', mergeAttributes(this.options.HTMLAttributes, HTMLAttributes, {
          src: embedSrc,
          frameborder: '0',
          allowfullscreen: 'true',
          allow: 'accelerometer; clipboard-write; encrypted-media; gyroscope; picture-in-picture',
          class: 'w-full h-full rounded-lg shadow-sm border border-gray-200 dark:border-zinc-700'
        })]
      ];
    } else {
      const attrs = mergeAttributes(this.options.HTMLAttributes, HTMLAttributes, {
        controls: 'true',
        preload: 'metadata',
        class: 'w-full rounded-lg shadow-sm border border-gray-200 dark:border-zinc-700 max-h-[600px] object-contain bg-black/5'
      });
      delete attrs.autoplay;
      
      // WebKit/WebView first-frame rendering hack
      if (attrs.src && !attrs.src.includes('#t=')) {
        attrs.src = attrs.src + '#t=0.001';
      }
      
      return ['video', attrs];
    }
  },

  addCommands() {
    return {
      setVideo: (options: { src: string }) => ({ commands }) => {
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
          const src = node.attrs.src || '';
          if (src.includes('youtube.com') || src.includes('youtu.be')) {
            const embedSrc = getYouTubeEmbedUrl(src);
            state.write(`<iframe src="${embedSrc}" frameborder="0" allowfullscreen="true"></iframe>\n`);
          } else {
            // Revert absolute 'asset://...' or 'tauri://...' URL back to relative if needed, 
            // but the stripLocalAssets handles that after markdown serialization!
            state.write(`<video src="${src}" controls="true"></video>\n`);
          }
        },
        parse: {
          setup(markdownit: any) {
            // markdown-it parses HTML automatically, so parseHTML will pick it up
          }
        }
      }
    };
  }
});
