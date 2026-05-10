import { Node, mergeAttributes } from '@tiptap/core';
import { VueNodeViewRenderer } from '@tiptap/vue-3';
import LocationNodeView from './nodes/LocationNodeView.vue';

export interface LocationOptions {
  HTMLAttributes: Record<string, any>;
}

declare module '@tiptap/core' {
  interface Commands<ReturnType> {
    location: {
      setLocation: (options: { lat: number; lng: number; label?: string; zoom?: number; provider?: string }) => ReturnType;
      setRoute: (options: { routeUrl: string; provider?: string; label?: string }) => ReturnType;
    };
  }
}

export const LocationExtension = Node.create<LocationOptions>({
  name: 'location',

  group: 'block',
  atom: true,

  addOptions() {
    return {
      HTMLAttributes: {},
    };
  },

  addAttributes() {
    return {
      // Mode: 'pin' (single marker) or 'route' (directions embed)
      mode: {
        default: 'pin',
        parseHTML: (el) => el.getAttribute('data-mode') || 'pin',
        renderHTML: (attrs) => ({ 'data-mode': attrs.mode }),
      },
      // Pin mode coords
      lat: {
        default: 0,
        parseHTML: (el) => parseFloat(el.getAttribute('data-lat') || '0'),
        renderHTML: (attrs) => ({ 'data-lat': attrs.lat }),
      },
      lng: {
        default: 0,
        parseHTML: (el) => parseFloat(el.getAttribute('data-lng') || '0'),
        renderHTML: (attrs) => ({ 'data-lng': attrs.lng }),
      },
      label: {
        default: '',
        parseHTML: (el) => el.getAttribute('data-label') || '',
        renderHTML: (attrs) => ({ 'data-label': attrs.label }),
      },
      // Route mode: original URL
      routeUrl: {
        default: '',
        parseHTML: (el) => el.getAttribute('data-route-url') || '',
        renderHTML: (attrs) => attrs.routeUrl ? { 'data-route-url': attrs.routeUrl } : {},
      },
      zoom: {
        default: 15,
        parseHTML: (el) => parseInt(el.getAttribute('data-zoom') || '15'),
        renderHTML: (attrs) => ({ 'data-zoom': attrs.zoom }),
      },
      provider: {
        default: 'osm',
        parseHTML: (el) => el.getAttribute('data-provider') || 'osm',
        renderHTML: (attrs) => ({ 'data-provider': attrs.provider }),
      },
      width: {
        default: '100%',
        parseHTML: (el) => el.getAttribute('data-width') || '100%',
        renderHTML: (attrs) => ({ 'data-width': attrs.width }),
      },
      height: {
        default: '200px',
        parseHTML: (el) => el.getAttribute('data-height') || '200px',
        renderHTML: (attrs) => ({ 'data-height': attrs.height }),
      },
      align: {
        default: 'center',
        parseHTML: (el) => el.getAttribute('data-align') || 'center',
        renderHTML: (attrs) => ({ 'data-align': attrs.align }),
      },
    };
  },

  parseHTML() {
    return [
      {
        tag: 'div[data-type="location"]',
      },
    ];
  },

  renderHTML({ HTMLAttributes }) {
    return [
      'div',
      mergeAttributes(this.options.HTMLAttributes, HTMLAttributes, {
        'data-type': 'location',
        class: 'location-block',
      }),
    ];
  },

  addNodeView() {
    return VueNodeViewRenderer(LocationNodeView);
  },

  addCommands() {
    return {
      setLocation:
        (options: { lat: number; lng: number; label?: string; zoom?: number; provider?: string }) =>
        ({ commands }) => {
          return commands.insertContent({
            type: this.name,
            attrs: {
              mode: 'pin',
              lat: options.lat,
              lng: options.lng,
              label: options.label || '',
              zoom: options.zoom || 15,
              provider: options.provider || 'osm',
            },
          });
        },
      setRoute:
        (options: { routeUrl: string; provider?: string; label?: string }) =>
        ({ commands }) => {
          return commands.insertContent({
            type: this.name,
            attrs: {
              mode: 'route',
              routeUrl: options.routeUrl,
              label: options.label || 'Directions',
              provider: options.provider || 'google',
              height: '300px',
            },
          });
        },
    };
  },

  addStorage() {
    return {
      markdown: {
        serialize(state: any, node: any) {
          const a = node.attrs;
          let html = `<div data-type="location" data-mode="${a.mode || 'pin'}" data-lat="${a.lat}" data-lng="${a.lng}" data-label="${a.label || ''}" data-zoom="${a.zoom}" data-provider="${a.provider || 'osm'}" data-width="${a.width || '100%'}" data-height="${a.height || '200px'}" data-align="${a.align || 'center'}"`;
          if (a.mode === 'route' && a.routeUrl) {
            html += ` data-route-url="${a.routeUrl}"`;
          }
          html += `></div>\n`;
          state.write(html);
        },
        parse: {
          setup(_markdownit: any) {
            // markdown-it parses HTML automatically, parseHTML() picks up div[data-type="location"]
          },
        },
      },
    };
  },
});
