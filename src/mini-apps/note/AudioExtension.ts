import { Node, mergeAttributes } from '@tiptap/core';

function getAudioEmbedUrl(url: string): string {
  try {
    const urlObj = new URL(url);
    if (urlObj.hostname.includes('spotify.com')) {
      if (!urlObj.pathname.startsWith('/embed/')) {
        return `https://open.spotify.com/embed${urlObj.pathname}${urlObj.search}`;
      }
    } else if (urlObj.hostname.includes('soundcloud.com') && !urlObj.hostname.includes('w.soundcloud.com')) {
      return `https://w.soundcloud.com/player/?url=${encodeURIComponent(url)}&color=%23ff5500&auto_play=false&hide_related=false&show_comments=true&show_user=true&show_reposts=false&show_teaser=true&visual=true`;
    }
  } catch (e) {
    // Fallback if URL parsing fails
  }
  return url;
}

export interface AudioOptions {
  HTMLAttributes: Record<string, any>;
}

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    audio: {
      setAudio: (options: { src: string }) => ReturnType;
    };
  }
}

export const AudioExtension = Node.create<AudioOptions>({
  name: 'audio',

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
        tag: 'audio[src]',
      },
      {
        tag: 'iframe[src*="spotify.com"]',
      },
      {
        tag: 'iframe[src*="soundcloud.com"]',
      },
    ];
  },

  renderHTML({ HTMLAttributes }) {
    const src = HTMLAttributes.src || '';
    if (src.includes('spotify.com') || src.includes('soundcloud.com')) {
      const embedSrc = getAudioEmbedUrl(src);
      
      // Spotify track is 152px, playlist/album is 352px. SoundCloud visual is 300px.
      const height = src.includes('spotify.com') 
        ? (src.includes('/track/') ? '152' : '352') 
        : '300';
      
      return ['div', { class: 'audio-wrapper w-full my-2' }, 
        ['iframe', mergeAttributes(this.options.HTMLAttributes, HTMLAttributes, {
          src: embedSrc,
          width: '100%',
          height: height,
          frameborder: '0',
          allowtransparency: 'true',
          allow: 'encrypted-media',
          style: src.includes('spotify.com') ? 'border-radius: 12px;' : '',
          class: 'shadow-sm' // removed custom borders that interfered with iframe inner height
        })]
      ];
    } else {
      return ['audio', mergeAttributes(this.options.HTMLAttributes, HTMLAttributes, {
        controls: 'true',
        class: 'w-full my-2 outline-none rounded-lg'
      })];
    }
  },

  addCommands() {
    return {
      setAudio: (options: { src: string }) => ({ commands }) => {
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
          if (src.includes('spotify.com') || src.includes('soundcloud.com')) {
            const embedSrc = getAudioEmbedUrl(src);
            const height = src.includes('spotify.com') 
              ? (src.includes('/track/') ? '152' : '352') 
              : '300';
            state.write(`<iframe src="${embedSrc}" width="100%" height="${height}" frameborder="0" allowtransparency="true" allow="encrypted-media" style="${src.includes('spotify.com') ? 'border-radius: 12px;' : ''}"></iframe>\n`);
          } else {
            // Revert absolute 'asset://...' or 'tauri://...' URL back to relative if needed, 
            // but the stripLocalAssets handles that after markdown serialization!
            state.write(`<audio src="${src}" controls="true"></audio>\n`);
          }
        },
        parse: {
          setup(_markdownit: any) {
            // markdown-it parses HTML automatically, so parseHTML will pick it up
          }
        }
      }
    };
  }
});
