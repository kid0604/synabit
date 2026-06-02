<script setup lang="ts">
import { watch, onBeforeUnmount, onMounted, ref, computed } from 'vue';
import { useEditor, EditorContent } from '@tiptap/vue-3';
import { VueRenderer, VueNodeViewRenderer } from '@tiptap/vue-3';
import StarterKit from '@tiptap/starter-kit';
import Blockquote from '@tiptap/extension-blockquote';
import Placeholder from '@tiptap/extension-placeholder';
import { CustomImage } from './extensions/CustomImage';
import { ImageCopyFix } from './extensions/ImageCopyFix';
import { ImageGallery, type GalleryImage } from './extensions/ImageGallery';
import TaskList from '@tiptap/extension-task-list';
import TaskItem from '@tiptap/extension-task-item';
import Link from '@tiptap/extension-link';
import Underline from '@tiptap/extension-underline';
import Highlight from '@tiptap/extension-highlight';
import CodeBlockLowlight from '@tiptap/extension-code-block-lowlight';
import { Table, TableRow, TableCell, TableHeader } from '@tiptap/extension-table';
import TextAlign from '@tiptap/extension-text-align';
import { TextStyle } from '@tiptap/extension-text-style';
import { Color } from '@tiptap/extension-color';
import { common, createLowlight } from 'lowlight';
import { Markdown } from 'tiptap-markdown';
import { EquationExtension } from './EquationExtension';
import { VideoExtension } from './VideoExtension';
import { AudioExtension } from './AudioExtension';
import { PdfExtension } from './PdfExtension';
import { LocationExtension } from './LocationExtension';
import { WhiteboardExtension } from './WhiteboardExtension';
import { TransclusionExtension } from './extensions/TransclusionExtension';
import { DetailsExtension } from './extensions/DetailsExtension';
import { BlockIdHider } from './extensions/BlockIdHider';
import EmbedPickerModal from './EmbedPickerModal.vue';
import 'katex/dist/katex.min.css';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { Extension, textInputRule } from '@tiptap/core';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import Suggestion from '@tiptap/suggestion';
import tippy, { type Instance as TippyInstance } from 'tippy.js';
import SlashCommandMenu from './SlashCommandMenu.vue';
import type { SlashCommandItem } from './SlashCommandMenu.vue';
import NoteMentionMenu from './NoteMentionMenu.vue';
import EmojiSuggestionMenu from './EmojiSuggestionMenu.vue';
import { emojiData, emojiCategories, type EmojiItem } from './emojiData';
import CodeBlockComponent from './CodeBlockComponent.vue';
import {
  Heading1, Heading2, Heading3,
  List, ListOrdered, ListChecks,
  Quote, Code2, Minus, Type, Table2,
  Image as ImageIcon, Images, Sigma, Video as VideoIcon,
  Music as MusicIcon, MapPin as MapPinIcon,
  Smile as SmileIcon, Navigation as NavigationIcon,
  PenTool as PenToolIcon,
  Link2 as EmbedIcon,
  BookOpen as BookOpenIcon,
  Network as MarkmapIcon,
  ChevronRight as ChevronRightIcon
} from 'lucide-vue-next';
import {
  Bold as BoldIcon,
  Italic as ItalicIcon,
  Underline as UnderlineIcon,
  Strikethrough as StrikeThroughIcon,
  Highlighter,
  Code,
  Link as LinkIcon,
  Plus,
  GripVertical,
  AlignLeft,
  AlignCenter,
  AlignRight,
  AlignJustify,
  Palette
} from 'lucide-vue-next';
import { open } from '@tauri-apps/plugin-dialog';
import { readFile } from '@tauri-apps/plugin-fs';
import { useSettings } from '../../composables/useSettings';
import { logger } from '../../utils/logger';

const CustomTableCell = TableCell.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      backgroundColor: {
        default: null,
        parseHTML: element => element.getAttribute('data-background-color') || element.style.backgroundColor || null,
        renderHTML: attributes => {
          if (!attributes.backgroundColor) {
            return {};
          }
          return {
            'data-background-color': attributes.backgroundColor,
            style: `background-color: ${attributes.backgroundColor}`,
          };
        },
      },
    };
  },
});

const CustomTableHeader = TableHeader.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      backgroundColor: {
        default: null,
        parseHTML: element => element.getAttribute('data-background-color') || element.style.backgroundColor || null,
        renderHTML: attributes => {
          if (!attributes.backgroundColor) {
            return {};
          }
          return {
            'data-background-color': attributes.backgroundColor,
            style: `background-color: ${attributes.backgroundColor}`,
          };
        },
      },
    };
  },
});

const allNodes = ref<any[]>([]);

onMounted(async () => {
    try {
        allNodes.value = await invoke<any[]>('get_all_nodes');
    } catch(e) {
        logger.error('Failed to fetch all nodes for mention menu', e);
    }
});

const { nestedNumberListStyle } = useSettings();

const lowlight = createLowlight(common);

const props = defineProps<{
  modelValue: string;
  vaultPath: string;
  notes?: any[];
  zenMode?: boolean;
  currentNoteId?: string;
  minHeightClass?: string;
}>();

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
  (e: 'open-internal-note', payload: any): void;
}>();

// --- Asset path helpers ---
const injectLocalAssets = (md: string) => {
   if (!props.vaultPath) return md;
   let processed = md;
   
   // Preserve <img> tags but convert src to absolute asset URL
    processed = processed.replace(/<img\s+([^>]*)src="assets\/([^"]+)"([^>]*)>/g, (_m, before, filename, after) => {
      const sep = props.vaultPath.includes('\\') ? '\\' : '/';
      const decodedName = decodeURIComponent(filename);
      const absPath = `${props.vaultPath}${sep}assets${sep}${decodedName}`;
      const assetUrl = convertFileSrc(absPath); 
      return `<img ${before}src="${assetUrl}"${after}>`;
   });
      processed = processed.replace(/<video\s+([^>]*)src="assets\/([^"]+)"([^>]*)>/g, (_m, before, filename, after) => {
      const sep = props.vaultPath.includes('\\') ? '\\' : '/';
      const decodedName = decodeURIComponent(filename);
      const absPath = `${props.vaultPath}${sep}assets${sep}${decodedName}`;
      const assetUrl = convertFileSrc(absPath); 
      return `<video ${before}src="${assetUrl}"${after}>`;
   });
      processed = processed.replace(/<audio\s+([^>]*)src="assets\/([^"]+)"([^>]*)>/g, (_m, before, filename, after) => {
      const sep = props.vaultPath.includes('\\') ? '\\' : '/';
      const decodedName = decodeURIComponent(filename);
      const absPath = `${props.vaultPath}${sep}assets${sep}${decodedName}`;
      const assetUrl = convertFileSrc(absPath); 
      return `<audio ${before}src="${assetUrl}"${after}>`;
   });
    processed = processed.replace(/\[([^\]]*)\]\(synabit:\/\/(note|node|person|task|quickcap)\/([^)]+)\)/g, (_match, label, type, uri) => {
      const decoded = decodeURIComponent(uri);
      return `[${label}](synabit://${type}/${encodeURIComponent(decoded)})`;
   });
   
   return processed.replace(/\]\(assets\/([^\)]+)\)/g, (_m: string, filename: string) => {
      const sep = props.vaultPath.includes('\\') ? '\\' : '/';
      // Decode URI in case it was encoded (e.g. spaces as %20)
      const decodedName = decodeURIComponent(filename);
      const absPath = `${props.vaultPath}${sep}assets${sep}${decodedName}`;
      const assetUrl = convertFileSrc(absPath); 
      return `](${assetUrl})`;
   });
};

const stripLocalAssets = (md: string) => {
   let processed = md;
   
   // Preserve <img> tags and their inline styles/width/height but make src relative.
   // Ensure it ends with /> to prevent markdown-it from swallowing text.
    processed = processed.replace(/<img\s+([^>]*)src="([^"]+)"([^>]*)>/gi, (_m, before, src, after) => {
      const match = src.match(/(?:https?:\/\/asset\.localhost|asset:\/\/localhost|tauri:\/\/localhost)[^\"]+(?:\/|%2F)assets(?:\/|%2F)([^\"]+)/);
      let newSrc = src;
      if (match) {
         const decodedName = decodeURIComponent(match[1]);
         newSrc = `assets/${encodeURI(decodedName)}`;
      }
      
      let newAfter = after;
      if (!newAfter.trim().endsWith('/')) {
         newAfter = newAfter + '/';
      }
      return `<img ${before}src="${newSrc}"${newAfter}>`;
   });

   processed = processed.replace(/<video\s+([^>]*)src="([^"]+)"([^>]*)>/g, (m, before, src, after) => {
      const match = src.match(/(?:https?:\/\/asset\.localhost|asset:\/\/localhost|tauri:\/\/localhost)[^\"]+(?:\/|%2F)assets(?:\/|%2F)([^\"]+)/);
      if (match) {
         const decodedName = decodeURIComponent(match[1]);
         return `<video ${before}src="assets/${encodeURI(decodedName)}"${after}>`;
      }
      return m;
   });

   processed = processed.replace(/<audio\s+([^>]*)src="([^"]+)"([^>]*)>/g, (m, before, src, after) => {
      const match = src.match(/(?:https?:\/\/asset\.localhost|asset:\/\/localhost|tauri:\/\/localhost)[^\"]+(?:\/|%2F)assets(?:\/|%2F)([^\"]+)/);
      if (match) {
         const decodedName = decodeURIComponent(match[1]);
         return `<audio ${before}src="assets/${encodeURI(decodedName)}"${after}>`;
      }
      return m;
   });

   return processed.replace(/\]\((?:https?:\/\/asset\.localhost|asset:\/\/localhost|tauri:\/\/localhost)[^\)]+(?:\/|%2F)assets(?:\/|%2F)([^\)]+)\)/g, (_m: string, filename: string) => {
      // Decode first to get real filename, then encode for valid Markdown URL
      const decodedName = decodeURIComponent(filename);
      return `](assets/${encodeURI(decodedName)})`;
   });
};

// --- Link prompt (uses reactive modal instead of window.prompt for mobile compat) ---
const linkModal = ref<{ show: boolean; url: string }>({ show: false, url: '' });

const setLink = () => {
  if (!editor.value) return;
  const previousUrl = editor.value.getAttributes('link').href;
  linkModal.value = { show: true, url: previousUrl || 'https://' };
};

const confirmLink = () => {
  linkModal.value.show = false;
  if (!editor.value) return;
  const url = linkModal.value.url;
  if (!url || url === '') {
    editor.value.chain().focus().extendMarkRange('link').unsetLink().run();
    return;
  }
  editor.value.chain().focus().extendMarkRange('link').setLink({ href: url }).run();
};

// --- Video prompt ---
const videoModal = ref<{ show: boolean; url: string }>({ show: false, url: '' });

const confirmVideo = () => {
  videoModal.value.show = false;
  if (!editor.value) return;
  const url = videoModal.value.url;
  if (!url || url === '') return;
  
  let finalUrl = url;
  if (url.startsWith('assets/')) {
      const sep = props.vaultPath.includes('\\') ? '\\' : '/';
      const filename = url.substring(7);
      const absPath = `${props.vaultPath}${sep}assets${sep}${decodeURIComponent(filename)}`;
      finalUrl = convertFileSrc(absPath);
  }
  
  editor.value.commands.setVideo({ src: finalUrl });
};

const selectLocalVideo = async () => {
  try {
    const selectedPath = await open({
      multiple: false,
      filters: [{
        name: 'Video',
        extensions: ['mp4', 'webm', 'mov', 'mkv', 'ogg']
      }]
    });
    
    if (selectedPath && !Array.isArray(selectedPath) && props.vaultPath) {
      const pathStr = selectedPath as string;
      const match = pathStr.match(/[\\\/]([^\\\/]+)$/);
      const filename = match ? match[1] : `video-${Date.now()}.mp4`;
      const buffer = await readFile(pathStr);
      
      const relativePath = await invoke<string>('save_asset', {
          vaultPath: props.vaultPath,
          filename: filename,
          bytes: Array.from(buffer)
      });
      const sep = props.vaultPath.includes('\\') ? '\\' : '/';
      const absPath = `${props.vaultPath}${sep}${relativePath}`;
      const renderUrl = convertFileSrc(absPath);
      
      videoModal.value.show = false;
      editor.value?.commands.setVideo({ src: renderUrl });
    }
  } catch (e) {
    logger.error("Failed to insert local video", e);
  }
};

// --- Audio prompt ---
const audioModal = ref<{ show: boolean; url: string }>({ show: false, url: '' });

const confirmAudio = () => {
  audioModal.value.show = false;
  if (!editor.value) return;
  const url = audioModal.value.url;
  if (!url || url === '') return;
  
  let finalUrl = url;
  if (url.startsWith('assets/')) {
      const sep = props.vaultPath.includes('\\') ? '\\' : '/';
      const filename = url.substring(7);
      const absPath = `${props.vaultPath}${sep}assets${sep}${decodeURIComponent(filename)}`;
      finalUrl = convertFileSrc(absPath);
  }
  
  editor.value.commands.setAudio({ src: finalUrl });
};

const selectLocalAudio = async () => {
  try {
    const selectedPath = await open({
      multiple: false,
      filters: [{
        name: 'Audio',
        extensions: ['mp3', 'wav', 'ogg', 'm4a', 'aac']
      }]
    });
    
    if (selectedPath && !Array.isArray(selectedPath) && props.vaultPath) {
      const pathStr = selectedPath as string;
      const match = pathStr.match(/[\\\/]([^\\\/]+)$/);
      const filename = match ? match[1] : `audio-${Date.now()}.mp3`;
      const buffer = await readFile(pathStr);
      
      const relativePath = await invoke<string>('save_asset', {
          vaultPath: props.vaultPath,
          filename: filename,
          bytes: Array.from(buffer)
      });
      const sep = props.vaultPath.includes('\\') ? '\\' : '/';
      const absPath = `${props.vaultPath}${sep}${relativePath}`;
      const renderUrl = convertFileSrc(absPath);
      
      audioModal.value.show = false;
      editor.value?.commands.setAudio({ src: renderUrl });
    }
  } catch (e) {
    logger.error("Failed to insert local audio", e);
  }
};

// --- Location prompt ---
const locationModal = ref<{
  show: boolean;
  input: string;
  lat: number | null;
  lng: number | null;
  label: string;
  provider: 'osm' | 'google';
  searching: boolean;
  suggestions: { display: string; lat: number; lng: number }[];
  error: string;
}>({
  show: false, input: '', lat: null, lng: null, label: '',
  provider: 'osm', searching: false, suggestions: [], error: ''
});

let geocodeTimer: ReturnType<typeof setTimeout> | null = null;

/** Parse Google Maps / OSM URL → {lat, lng, provider} */
const parseMapUrl = (url: string): { lat: number; lng: number; label?: string; provider: 'osm' | 'google' } | null => {
  try {
    // Google Maps: various formats
    // https://www.google.com/maps?q=LAT,LNG
    // https://www.google.com/maps/@LAT,LNG,ZOOMz
    // https://maps.google.com/?ll=LAT,LNG
    // https://goo.gl/maps/... (short link, won't parse)
    const u = new URL(url);
    if (u.hostname.includes('google.com') || u.hostname.includes('maps.google')) {
      const q = u.searchParams.get('q');
      if (q) {
        const parts = q.split(',').map(s => parseFloat(s.trim()));
        if (parts.length >= 2 && !isNaN(parts[0]) && !isNaN(parts[1])) {
          return { lat: parts[0], lng: parts[1], provider: 'google' };
        }
      }
      const atMatch = u.pathname.match(/@(-?\d+\.\d+),(-?\d+\.\d+)/);
      if (atMatch) {
        return { lat: parseFloat(atMatch[1]), lng: parseFloat(atMatch[2]), provider: 'google' };
      }
      const placeMatch = u.pathname.match(/place\/([^/]+)\//);
      if (placeMatch && atMatch) {
        return { lat: parseFloat(atMatch[1]), lng: parseFloat(atMatch[2]), label: decodeURIComponent(placeMatch[1].replace(/\+/g, ' ')), provider: 'google' };
      }
    }
    // OpenStreetMap: https://www.openstreetmap.org/?mlat=LAT&mlon=LNG
    if (u.hostname.includes('openstreetmap.org')) {
      const mlat = u.searchParams.get('mlat');
      const mlon = u.searchParams.get('mlon');
      if (mlat && mlon) {
        return { lat: parseFloat(mlat), lng: parseFloat(mlon), provider: 'osm' };
      }
      const hash = u.hash; // #map=ZOOM/LAT/LNG
      const hashMatch = hash.match(/#map=\d+\/(-?\d+\.\d+)\/(-?\d+\.\d+)/);
      if (hashMatch) {
        return { lat: parseFloat(hashMatch[1]), lng: parseFloat(hashMatch[2]), provider: 'osm' };
      }
    }
  } catch { /* not a URL */ }
  return null;
};

/** Parse raw lat,lng string */
const parseLatLng = (input: string): { lat: number; lng: number } | null => {
  const match = input.trim().match(/^(-?\d+\.?\d*)\s*[,\s]\s*(-?\d+\.?\d*)$/);
  if (match) {
    const lat = parseFloat(match[1]);
    const lng = parseFloat(match[2]);
    if (lat >= -90 && lat <= 90 && lng >= -180 && lng <= 180) {
      return { lat, lng };
    }
  }
  return null;
};

/** Nominatim geocoding (free, privacy-first, 1 req/s) */
const geocodeAddress = async (query: string) => {
  if (query.length < 3) {
    locationModal.value.suggestions = [];
    return;
  }
  locationModal.value.searching = true;
  locationModal.value.error = '';
  try {
    const res = await fetch(
      `https://nominatim.openstreetmap.org/search?format=json&q=${encodeURIComponent(query)}&limit=5&addressdetails=1`,
      { headers: { 'User-Agent': 'Synabit/0.4.1 (https://synabit.app)' } }
    );
    if (!res.ok) throw new Error('Geocoding failed');
    const data = await res.json();
    locationModal.value.suggestions = data.map((item: any) => ({
      display: item.display_name,
      lat: parseFloat(item.lat),
      lng: parseFloat(item.lon),
    }));
  } catch (e: any) {
    locationModal.value.error = 'Could not search. Check your internet connection.';
    locationModal.value.suggestions = [];
  } finally {
    locationModal.value.searching = false;
  }
};

const onLocationInput = (val: string) => {
  locationModal.value.input = val;
  locationModal.value.error = '';
  locationModal.value.suggestions = [];

  // Try URL first
  const urlResult = parseMapUrl(val);
  if (urlResult) {
    locationModal.value.lat = urlResult.lat;
    locationModal.value.lng = urlResult.lng;
    locationModal.value.provider = urlResult.provider;
    if (urlResult.label) locationModal.value.label = urlResult.label;
    return;
  }

  // Try lat,lng
  const coordResult = parseLatLng(val);
  if (coordResult) {
    locationModal.value.lat = coordResult.lat;
    locationModal.value.lng = coordResult.lng;
    return;
  }

  // Otherwise treat as address search (debounced)
  locationModal.value.lat = null;
  locationModal.value.lng = null;
  if (geocodeTimer) clearTimeout(geocodeTimer);
  geocodeTimer = setTimeout(() => geocodeAddress(val), 500);
};

const selectSuggestion = (s: { display: string; lat: number; lng: number }) => {
  locationModal.value.lat = s.lat;
  locationModal.value.lng = s.lng;
  locationModal.value.label = s.display.split(',').slice(0, 2).join(',').trim();
  locationModal.value.suggestions = [];
  locationModal.value.input = s.display;
};

const confirmLocation = () => {
  if (!editor.value || locationModal.value.lat === null || locationModal.value.lng === null) return;
  editor.value.commands.setLocation({
    lat: locationModal.value.lat,
    lng: locationModal.value.lng,
    label: locationModal.value.label || '',
    zoom: 15,
    provider: locationModal.value.provider,
  });
  locationModal.value = {
    show: false, input: '', lat: null, lng: null, label: '',
    provider: 'osm', searching: false, suggestions: [], error: ''
  };
};

// --- Route Modal ---
const routeModal = ref<{
  show: boolean;
  urlInput: string;
  error: string;
  label: string;
}>({
  show: false, urlInput: '', error: '', label: ''
});

const isValidRouteUrl = computed(() => {
  try {
    const u = new URL(routeModal.value.urlInput.trim());
    const isGoogle = (u.hostname.includes('google.com') || u.hostname.includes('maps.google') || u.hostname.includes('goo.gl'))
      && (u.pathname.includes('/dir') || u.pathname.includes('/maps'));
    const isOSM = u.hostname.includes('openstreetmap.org') && (u.pathname.includes('/directions') || u.searchParams.has('route'));
    return isGoogle || isOSM;
  } catch { return false; }
});

/** Detect provider from URL */
const detectRouteProvider = (url: string): 'google' | 'osm' => {
  try {
    const u = new URL(url);
    if (u.hostname.includes('openstreetmap.org')) return 'osm';
  } catch { /* default */ }
  return 'google';
};

const confirmRoute = () => {
  const r = routeModal.value;
  if (!editor.value || !isValidRouteUrl.value) return;
  const url = r.urlInput.trim();
  const provider = detectRouteProvider(url);

  // Extract label from URL
  let label = r.label || 'Directions';
  if (!r.label) {
    try {
      const u = new URL(url);
      if (provider === 'google') {
        const parts = u.pathname.replace(/^\/maps\/dir\/?/, '').split('/').filter(p => p && !p.startsWith('@') && !p.startsWith('data'));
        if (parts.length >= 2) {
          const o = decodeURIComponent(parts[0]).replace(/\+/g, ' ');
          const d = decodeURIComponent(parts[1]).replace(/\+/g, ' ');
          label = `${o.split(',')[0].trim()} → ${d.split(',')[0].trim()}`;
        }
      } else {
        // OSM: route=lat1,lng1;lat2,lng2
        const route = u.searchParams.get('route');
        if (route) {
          const pts = route.split(';');
          if (pts.length >= 2) label = `${pts[0]} → ${pts[pts.length - 1]}`;
        }
      }
    } catch { /* use default */ }
  }
  editor.value.commands.setRoute({ routeUrl: url, label, provider });
  routeModal.value = { show: false, urlInput: '', error: '', label: '' };
};

// --- Emoji Picker (full panel from /emoji) ---
const emojiPicker = ref({ show: false, search: '', activeCategory: 'smileys' });

// --- Whiteboard Picker Modal ---
const whiteboardPickerModal = ref<{ show: boolean; boards: any[]; loading: boolean; search: string }>({
  show: false, boards: [], loading: false, search: ''
});

const filteredWhiteboards = computed(() => {
  const q = whiteboardPickerModal.value.search.toLowerCase().trim();
  if (!q) return whiteboardPickerModal.value.boards;
  return whiteboardPickerModal.value.boards.filter((b: any) =>
    (b.title || '').toLowerCase().includes(q)
  );
});

const confirmWhiteboard = (board: any) => {
  if (!editor.value) return;
  editor.value.commands.setWhiteboard({
    boardId: board.id || board.path,
    boardPath: board.path,
    title: board.title || 'Untitled Board',
  });
  whiteboardPickerModal.value = { show: false, boards: [], loading: false, search: '' };
};

// --- Embed Picker Modal (Transclusion 2.0) ---
const embedPickerModal = ref(false);

// --- PDF Embed Modal ---
const pdfModal = ref<{ show: boolean }>({ show: false });

const selectPdfFile = async () => {
  try {
    const selectedPath = await open({
      multiple: false,
      filters: [{
        name: 'PDF',
        extensions: ['pdf']
      }]
    });

    if (selectedPath && !Array.isArray(selectedPath) && props.vaultPath) {
      const pathStr = selectedPath as string;
      const match = pathStr.match(/[\\\/]([^\\\/]+)$/);
      const filename = match ? match[1] : `document-${Date.now()}.pdf`;

      // Copy to vault assets
      const relativePath = await invoke<string>('copy_asset_to_vault', {
        vaultPath: props.vaultPath,
        sourcePath: pathStr,
      });

      pdfModal.value.show = false;

      if (editor.value) {
        editor.value.commands.setPdf({
          src: relativePath,
          title: filename.replace(/\.pdf$/i, ''),
        });
      }
    }
  } catch (e) {
    logger.error('Failed to embed PDF', e);
  }
};

const confirmEmbed = (payload: { nodeId: string; blockId?: string; noteTitle: string }) => {
  if (!editor.value) return;
  const target = payload.blockId
    ? `${payload.nodeId}#${payload.blockId}`
    : payload.noteTitle;
  editor.value.commands.insertContent({
    type: 'transclusion',
    attrs: { target, nodeId: payload.nodeId },
  });
  embedPickerModal.value = false;
};

// --- Block Context Menu (right-click → Copy Block Link) ---
const blockCtxMenu = ref<{ show: boolean; top: number; left: number; text: string }>({
  show: false, top: 0, left: 0, text: ''
});

const openBlockContextMenu = (event: MouseEvent, text: string) => {
  const wrapper = (event.target as HTMLElement).closest('.tiptap-wrapper');
  const wrapperRect = wrapper ? wrapper.getBoundingClientRect() : { top: 0, left: 0 };
  blockCtxMenu.value = {
    show: true,
    top: event.clientY - wrapperRect.top,
    left: event.clientX - wrapperRect.left,
    text,
  };
};

const copyBlockLink = async () => {
  if (!props.currentNoteId || !blockCtxMenu.value.text) return;
  try {
    const blockId = await invoke<string>('create_block_reference', {
      vaultPath: props.vaultPath,
      nodeId: props.currentNoteId,
      contentSnippet: blockCtxMenu.value.text.trim(),
    });
    const uri = `synabit://block/${props.currentNoteId}#${blockId}`;
    await navigator.clipboard.writeText(uri);
    blockCtxMenu.value.show = false;
  } catch (err) {
    console.error('Failed to copy block link:', err);
    blockCtxMenu.value.show = false;
  }
};

const insertEmoji = (emoji: string) => {
  if (!editor.value) return;
  editor.value.chain().focus().insertContent(emoji).run();
  emojiPicker.value.show = false;
  emojiPicker.value.search = '';
};

const filteredPickerEmojis = computed(() => {
  const q = emojiPicker.value.search.toLowerCase().trim();
  if (q) {
    return emojiData.filter(e =>
      e.shortcode.includes(q) ||
      e.emoji.includes(q) ||
      e.keywords.some(k => k.includes(q))
    );
  }
  return emojiData.filter(e => e.category === emojiPicker.value.activeCategory);
});

// --- Floating Toolbar (manual implementation) ---
const bubbleMenuRef = ref<HTMLElement | null>(null);
const showBubble = ref(false);
const bubblePos = ref({ top: 0, left: 0 });

const BUBBLE_MENU_WIDTH = 290; // approximate width of the toolbar
const BUBBLE_MENU_HEIGHT = 40;
const BUBBLE_PADDING = 8; // min gap from viewport edges

const updateBubbleMenu = () => {
  if (!editor.value) return;
  const { from, to, empty } = editor.value.state.selection;
  
  if (
    empty || 
    from === to || 
    editor.value.isActive('image') || 
    editor.value.isActive('imageGallery') ||
    editor.value.isActive('video') ||
    editor.value.isActive('audio') ||
    editor.value.isActive('whiteboard') ||
    'node' in editor.value.state.selection
  ) {
    showBubble.value = false;
    return;
  }
  
  const view = editor.value.view;
  const start = view.coordsAtPos(from);
  const end = view.coordsAtPos(to);
  
  // Get the editor container's visible bounds (accounts for sidebars)
  const editorRect = view.dom.getBoundingClientRect();
  const areaLeft = editorRect.left;
  const areaRight = editorRect.right;
  
  // Calculate center in viewport coordinates (fixed positioning)
  let centerX = (start.left + end.right) / 2;
  let topY = Math.min(start.top, end.top) - BUBBLE_MENU_HEIGHT - 8;
  
  // Clamp horizontal: keep fully visible within editor content area
  const halfW = BUBBLE_MENU_WIDTH / 2;
  centerX = Math.max(areaLeft + halfW + BUBBLE_PADDING, Math.min(centerX, areaRight - halfW - BUBBLE_PADDING));
  
  // If would go above viewport, show below selection instead
  if (topY < BUBBLE_PADDING) {
    topY = Math.max(end.bottom, start.bottom) + 8;
  }
  
  bubblePos.value = { top: topY, left: centerX };
  showBubble.value = true;
};

// --- Table Controls (Confluence-style) ---
const isInTable = ref(false);
const activeTableEl = ref<HTMLElement | null>(null);
const tableRect = ref({ top: 0, left: 0, width: 0, height: 0, bottom: 0, right: 0 });
const colPositions = ref<{ left: number; width: number }[]>([]);
const rowPositions = ref<{ top: number; height: number }[]>([]);
const activeRowIdx = ref(-1);
const activeColIdx = ref(-1);

// Context menu
const showCtxMenu = ref(false);
const ctxMenuPos = ref({ top: 0, left: 0 });
const canMerge = ref(false);
const canSplit = ref(false);

// Saved CellSelection for merge/split (tracked on selectionUpdate, restored on action)
let lastCellSelection: any = null;
let lastCanMerge = false;
let lastCanSplit = false;

// Track CellSelection on every selection change
// Key insight: right-click fires mousedown → creates TextSelection → onSelectionUpdate fires again
// We must NOT clear savedCellSelection in that case (user is still in table, just lost CellSelection)
const trackCellSelection = () => {
  if (!editor.value) return;
  const sel = editor.value.state.selection;
  if ((sel as any).$anchorCell) {
    // Active CellSelection — save it
    lastCellSelection = sel;
    lastCanMerge = editor.value.can().mergeCells();
    lastCanSplit = editor.value.can().splitCell();
  } else if (!editor.value.isActive('table')) {
    // User left the table entirely — clear saved state
    lastCellSelection = null;
    lastCanMerge = false;
    lastCanSplit = false;
  }
  // If user is in table but no CellSelection (e.g. after right-click),
  // we intentionally KEEP lastCellSelection so merge can restore it
};

const updateTableControls = () => {
  if (!editor.value) { isInTable.value = false; return; }
  const inTable = editor.value.isActive('table');
  isInTable.value = inTable;
  if (!inTable) { activeTableEl.value = null; return; }
  
  canMerge.value = editor.value.can().mergeCells() || lastCanMerge;
  canSplit.value = editor.value.can().splitCell() || lastCanSplit;

  // Find the actual table DOM element
  const { from } = editor.value.state.selection;
  const domAtPos = editor.value.view.domAtPos(from);
  let el = domAtPos.node as HTMLElement;
  while (el && el.tagName !== 'TABLE') {
    el = el.parentElement as HTMLElement;
  }
  if (!el) return;
  activeTableEl.value = el;

  const wrapper = el.closest('.tiptap-wrapper');
  const wrapperRect = wrapper ? wrapper.getBoundingClientRect() : { top: 0, left: 0 };

  const rect = el.getBoundingClientRect();
  tableRect.value = { 
    top: rect.top - wrapperRect.top, 
    left: rect.left - wrapperRect.left, 
    width: rect.width, 
    height: rect.height, 
    bottom: rect.bottom - wrapperRect.top, 
    right: rect.right - wrapperRect.left 
  };

  // Read column positions from first row
  const firstRow = el.querySelector('tr');
  if (firstRow) {
    const cells = firstRow.querySelectorAll('td, th');
    colPositions.value = Array.from(cells).map(c => {
      const cr = c.getBoundingClientRect();
      return { left: cr.left - wrapperRect.left, width: cr.width };
    });
  }

  // Read row positions
  const rows = el.querySelectorAll('tr');
  rowPositions.value = Array.from(rows).map(r => {
    const rr = r.getBoundingClientRect();
    return { top: rr.top - wrapperRect.top, height: rr.height };
  });

  // Determine active row and col for showing specific handles
  let cell = domAtPos.node as HTMLElement;
  if (cell && cell.nodeType === Node.TEXT_NODE) {
    cell = cell.parentElement as HTMLElement;
  }
  while (cell && cell.tagName !== 'TD' && cell.tagName !== 'TH' && cell !== activeTableEl.value) {
    cell = cell.parentElement as HTMLElement;
  }
  if (cell && (cell.tagName === 'TD' || cell.tagName === 'TH')) {
    const row = cell.parentElement as HTMLTableRowElement;
    if (row && activeTableEl.value) {
      const allRows = Array.from(activeTableEl.value.querySelectorAll('tr'));
      activeRowIdx.value = allRows.indexOf(row);
      const allCells = Array.from(row.querySelectorAll('td, th'));
      activeColIdx.value = allCells.indexOf(cell);
    }
  } else {
    activeRowIdx.value = -1;
    activeColIdx.value = -1;
  }
};

const openContextMenu = (e: MouseEvent) => {
  if (!editor.value || !editor.value.isActive('table')) return;
  e.preventDefault();
  const wrapper = activeTableEl.value?.closest('.tiptap-wrapper');
  const wrapperRect = wrapper ? wrapper.getBoundingClientRect() : { top: 0, left: 0 };
  
  ctxMenuPos.value = { top: e.clientY - wrapperRect.top, left: e.clientX - wrapperRect.left };
  showCtxMenu.value = true;
};

const closeCtxMenu = () => { showCtxMenu.value = false; };

const ctxAction = (action: string) => {
  if (!editor.value) return;
  
  // For merge: restore saved CellSelection first
  if (action === 'mergeCells' && lastCellSelection) {
    try {
      const tr = editor.value.state.tr.setSelection(lastCellSelection);
      editor.value.view.dispatch(tr);
    } catch (e) { /* positions may be stale */ }
    editor.value.commands.mergeCells();
    lastCellSelection = null;
    lastCanMerge = false;
    closeCtxMenu();
    setTimeout(updateTableControls, 50);
    return;
  }
  
  const chain = editor.value.chain().focus();
  switch (action) {
    case 'addRowAbove': chain.addRowBefore().run(); break;
    case 'addRowBelow': chain.addRowAfter().run(); break;
    case 'deleteRow': chain.deleteRow().run(); break;
    case 'addColLeft': chain.addColumnBefore().run(); break;
    case 'addColRight': chain.addColumnAfter().run(); break;
    case 'deleteCol': chain.deleteColumn().run(); break;
    case 'splitCell': chain.splitCell().run(); break;
    case 'toggleHeaderRow': chain.toggleHeaderRow().run(); break;
    case 'toggleHeaderCol': chain.toggleHeaderColumn().run(); break;
    case 'deleteTable': chain.deleteTable().run(); break;
  }
  closeCtxMenu();
  setTimeout(updateTableControls, 50);
};

const setCellColor = (color: string | null, close = true) => {
  if (!editor.value) return;
  
  if (lastCellSelection) {
    try {
      const tr = editor.value.state.tr.setSelection(lastCellSelection);
      editor.value.view.dispatch(tr);
    } catch (e) { /* positions may be stale */ }
    lastCellSelection = null;
    lastCanMerge = false;
  }
  
  if (color) {
    editor.value.chain().focus().setCellAttribute('backgroundColor', color).run();
  } else {
    editor.value.chain().focus().setCellAttribute('backgroundColor', null).run();
  }
  if (close) {
    closeCtxMenu();
  }
  setTimeout(updateTableControls, 50);
};

// Focus a specific cell to position cursor there before operations

const addRowAtBottom = () => {
  if (!editor.value || !activeTableEl.value) return;
  // Focus last row, then add after
  const rows = activeTableEl.value.querySelectorAll('tr');
  if (rows.length > 0) {
    const lastRow = rows[rows.length - 1];
    const cell = lastRow.querySelector('td, th');
    if (cell) {
      const pos = editor.value.view.posAtDOM(cell, 0);
      editor.value.chain().setTextSelection(pos).addRowAfter().run();
    }
  }
};

const addColAtRight = () => {
  if (!editor.value || !activeTableEl.value) return;
  const firstRow = activeTableEl.value.querySelector('tr');
  if (firstRow) {
    const cells = firstRow.querySelectorAll('td, th');
    const lastCell = cells[cells.length - 1];
    if (lastCell) {
      const pos = editor.value.view.posAtDOM(lastCell, 0);
      editor.value.chain().setTextSelection(pos).addColumnAfter().run();
    }
  }
};

const getCellPos = (domNode: Element) => {
  if (!editor.value) return 0;
  const pos = editor.value.view.posAtDOM(domNode, 0);
  const $pos = editor.value.state.doc.resolve(pos);
  for (let d = $pos.depth; d > 0; d--) {
    const name = $pos.node(d).type.name;
    if (name === 'tableCell' || name === 'tableHeader') {
      return $pos.before(d);
    }
  }
  return pos - 1;
};

const selectWholeTable = () => {
  if (!editor.value || !activeTableEl.value) return;
  const rows = activeTableEl.value.querySelectorAll('tr');
  if (rows.length > 0) {
    const firstCell = rows[0].querySelector('td, th');
    const lastRow = rows[rows.length - 1];
    const lastCell = lastRow.children[lastRow.children.length - 1];
    if (firstCell && lastCell) {
      const anchorPos = getCellPos(firstCell);
      const headPos = getCellPos(lastCell);
      editor.value.chain().setCellSelection({ anchorCell: anchorPos, headCell: headPos }).run();
    }
  }
};

const selectColumn = (colIdx: number, e?: MouseEvent) => {
  if (!editor.value || !activeTableEl.value) return;
  const rows = activeTableEl.value.querySelectorAll('tr');
  if (rows.length > 0) {
    const firstCell = rows[0].querySelectorAll('td, th')[colIdx];
    const lastCell = rows[rows.length - 1].querySelectorAll('td, th')[colIdx];
    if (firstCell && lastCell) {
      const anchorPos = getCellPos(firstCell);
      const headPos = getCellPos(lastCell);
      if (e?.shiftKey && lastCellSelection) {
        editor.value.chain().setCellSelection({ anchorCell: lastCellSelection.$anchorCell.pos, headCell: headPos }).run();
      } else {
        editor.value.chain().setCellSelection({ anchorCell: anchorPos, headCell: headPos }).run();
      }
    }
  }
};

const selectRow = (rowIdx: number, e?: MouseEvent) => {
  if (!editor.value || !activeTableEl.value) return;
  const row = activeTableEl.value.querySelectorAll('tr')[rowIdx];
  if (row) {
    const firstCell = row.children[0];
    const lastCell = row.children[row.children.length - 1];
    if (firstCell && lastCell) {
      const anchorPos = getCellPos(firstCell);
      const headPos = getCellPos(lastCell);
      if (e?.shiftKey && lastCellSelection) {
        editor.value.chain().setCellSelection({ anchorCell: lastCellSelection.$anchorCell.pos, headCell: headPos }).run();
      } else {
        editor.value.chain().setCellSelection({ anchorCell: anchorPos, headCell: headPos }).run();
      }
    }
  }
};

// --- Slash Commands definition ---
const slashCommandItems = (): SlashCommandItem[] => [
  {
    title: 'Text',
    description: 'Plain text paragraph',
    icon: Type,
    command: ({ editor, range }: any) => {
      editor.chain().focus().deleteRange(range).setParagraph().run();
    },
  },
  {
    title: 'Heading 1',
    description: 'Large section heading',
    icon: Heading1,
    command: ({ editor, range }: any) => {
      editor.chain().focus().deleteRange(range).setHeading({ level: 1 }).run();
    },
  },
  {
    title: 'Heading 2',
    description: 'Medium section heading',
    icon: Heading2,
    command: ({ editor, range }: any) => {
      editor.chain().focus().deleteRange(range).setHeading({ level: 2 }).run();
    },
  },
  {
    title: 'Heading 3',
    description: 'Small section heading',
    icon: Heading3,
    command: ({ editor, range }: any) => {
      editor.chain().focus().deleteRange(range).setHeading({ level: 3 }).run();
    },
  },
  {
    title: 'Bullet List',
    description: 'Unordered list of items',
    icon: List,
    command: ({ editor, range }: any) => {
      editor.chain().focus().deleteRange(range).toggleBulletList().run();
    },
  },
  {
    title: 'Numbered List',
    description: 'Ordered list of items',
    icon: ListOrdered,
    command: ({ editor, range }: any) => {
      editor.chain().focus().deleteRange(range).toggleOrderedList().run();
    },
  },
  {
    title: 'Task List',
    description: 'Checkbox task list',
    icon: ListChecks,
    command: ({ editor, range }: any) => {
      editor.chain().focus().deleteRange(range).toggleTaskList().run();
    },
  },
  {
    title: 'Blockquote',
    description: 'Quoted text block',
    icon: Quote,
    command: ({ editor, range }: any) => {
      editor.chain().focus().deleteRange(range).setBlockquote().run();
    },
  },
  {
    title: 'Code Block',
    description: 'Fenced code snippet',
    icon: Code2,
    command: ({ editor, range }: any) => {
      editor.chain().focus().deleteRange(range).setCodeBlock().run();
    },
  },
  {
    title: 'Markmap',
    description: 'Interactive mindmap from markdown',
    icon: MarkmapIcon,
    command: ({ editor, range }: any) => {
      editor.chain().focus().deleteRange(range).setCodeBlock({ language: 'markmap' }).run();
    },
  },
  {
    title: 'Divider',
    description: 'Horizontal separator line',
    icon: Minus,
    command: ({ editor, range }: any) => {
      editor.chain().focus().deleteRange(range).setHorizontalRule().run();
    },
  },
  {
    title: 'Image',
    description: 'Upload an image',
    icon: ImageIcon,
    command: async ({ editor, range }: any) => {
      editor.chain().focus().deleteRange(range).run();
      try {
        const selectedPath = await open({
          multiple: false,
          filters: [{
            name: 'Image',
            extensions: ['png', 'jpeg', 'jpg', 'gif', 'webp', 'svg']
          }]
        });
        
        if (selectedPath && !Array.isArray(selectedPath) && props.vaultPath) {
          const pathStr = selectedPath as string;
          const match = pathStr.match(/[\\\/]([^\\\/]+)$/);
          const filename = match ? match[1] : `image-${Date.now()}.png`;
          const buffer = await readFile(pathStr);
          
          const relativePath = await invoke<string>('save_asset', {
              vaultPath: props.vaultPath,
              filename: filename,
              bytes: Array.from(buffer)
          });
          const sep = props.vaultPath.includes('\\') ? '\\' : '/';
          const absPath = `${props.vaultPath}${sep}${relativePath}`;
          const renderUrl = convertFileSrc(absPath);
          
          editor.commands.setImage({ src: renderUrl, alt: filename });
        }
      } catch (e) {
        logger.error("Failed to insert image", e);
      }
    },
  },
  {
    title: 'Image Collection',
    description: 'Upload multiple images into a grid',
    icon: Images,
    command: async ({ editor, range }: any) => {
      editor.chain().focus().deleteRange(range).run();
      try {
        const selectedPaths = await open({
          multiple: true,
          filters: [{
            name: 'Image',
            extensions: ['png', 'jpeg', 'jpg', 'gif', 'webp', 'svg']
          }]
        });
        
        if (selectedPaths && Array.isArray(selectedPaths) && props.vaultPath) {
          const newImages: GalleryImage[] = [];
          for (const pathStr of selectedPaths) {
            const match = pathStr.match(/[\\\/]([^\\\/]+)$/);
            const filename = match ? match[1] : `image-${Date.now()}.png`;
            const buffer = await readFile(pathStr);
            
            const relativePath = await invoke<string>('save_asset', {
                vaultPath: props.vaultPath,
                filename: filename,
                bytes: Array.from(buffer)
            });
            const sep = props.vaultPath.includes('\\') ? '\\' : '/';
            const absPath = `${props.vaultPath}${sep}${relativePath}`;
            const renderUrl = convertFileSrc(absPath);
            
            newImages.push({
              src: renderUrl,
              alt: filename,
              caption: ''
            });
          }
          if (newImages.length > 0) {
            const layout = newImages.length >= 3 ? 'grid-3' : 'grid-2';
            editor.commands.setImageGallery({ images: newImages, layout });
          }
        }
      } catch (e) {
        logger.error("Failed to insert image collection", e);
      }
    },
  },
  {
    title: 'Video',
    description: 'Embed YouTube or local video',
    icon: VideoIcon,
    command: ({ editor, range }: any) => {
      editor.chain().focus().deleteRange(range).run();
      videoModal.value = { show: true, url: '' };
    },
  },
  {
    title: 'Audio',
    description: 'Embed Spotify, SoundCloud or local audio',
    icon: MusicIcon,
    command: ({ editor, range }: any) => {
      editor.chain().focus().deleteRange(range).run();
      audioModal.value = { show: true, url: '' };
    },
  },
  {
    title: 'Table',
    description: 'Insert a table',
    icon: Table2,
    command: ({ editor, range }: any) => {
      editor.chain().focus().deleteRange(range)
        .insertTable({ rows: 3, cols: 3, withHeaderRow: true })
        .run();
    },
  },
  {
    title: 'Equation',
    description: 'LaTeX/KaTeX Math formula',
    icon: Sigma,
    command: ({ editor, range }: any) => {
      editor.chain().focus().deleteRange(range).insertContent({ type: 'equation', attrs: { latex: '' } }).run();
    },
  },
  {
    title: 'Location',
    description: 'Embed a map location',
    icon: MapPinIcon,
    command: ({ editor, range }: any) => {
      editor.chain().focus().deleteRange(range).run();
      locationModal.value = {
        show: true, input: '', lat: null, lng: null, label: '',
        provider: 'osm', searching: false, suggestions: [], error: ''
      };
    },
  },
  {
    title: 'Route',
    description: 'Embed a route/directions map',
    icon: NavigationIcon,
    command: ({ editor, range }: any) => {
      editor.chain().focus().deleteRange(range).run();
      routeModal.value = { show: true, urlInput: '', error: '', label: '' };
    },
  },
  {
    title: 'Emoji',
    description: 'Open emoji picker',
    icon: SmileIcon,
    command: ({ editor, range }: any) => {
      editor.chain().focus().deleteRange(range).run();
      emojiPicker.value = { show: true, search: '', activeCategory: 'smileys' };
    },
  },
  {
    title: 'Whiteboard',
    description: 'Embed an existing whiteboard',
    icon: PenToolIcon,
    command: async ({ editor, range }: any) => {
      editor.chain().focus().deleteRange(range).run();
      whiteboardPickerModal.value = { show: true, boards: [], loading: true, search: '' };
      try {
        const boards = await invoke<any[]>('scan_whiteboards', { vaultPath: props.vaultPath });
        whiteboardPickerModal.value.boards = boards;
      } catch (e) {
        logger.error('Failed to scan whiteboards', e);
      } finally {
        whiteboardPickerModal.value.loading = false;
      }
    },
  },
  {
    title: 'Embed',
    description: 'Embed content from another note',
    icon: EmbedIcon,
    command: ({ editor, range }: any) => {
      editor.chain().focus().deleteRange(range).run();
      embedPickerModal.value = true;
    },
  },
  {
    title: 'PDF',
    description: 'Embed a PDF document',
    icon: BookOpenIcon,
    command: async ({ editor, range }: any) => {
      editor.chain().focus().deleteRange(range).run();
      pdfModal.value = { show: true };
    },
  },
  {
    title: 'Toggle list',
    description: 'Toggles can hide and show content inside',
    icon: ChevronRightIcon,
    command: ({ editor, range }: any) => {
      editor.chain().focus().deleteRange(range).setDetails({ summary: 'Toggle heading' }).run();
    },
  },
];

// --- Slash Command Extension ---
const SlashCommands = Extension.create({
  name: 'slashCommands',

  addOptions() {
    return {
      suggestion: {
        char: '/',
        command: ({ editor, range, props }: any) => {
          props.command({ editor, range });
        },
      },
    };
  },

  addProseMirrorPlugins() {
    return [
      Suggestion({
        editor: this.editor,
        ...this.options.suggestion,
      }),
    ];
  },
});

// --- Emoji Suggestion Extension (triggered by :) ---
const EmojiSuggestion = Extension.create({
  name: 'emojiSuggestion',

  addOptions() {
    return {
      suggestion: {
        char: ':',
        pluginKey: new PluginKey('emojiSuggestion'),
        // Only activate after at least 2 chars typed (avoid false triggers on plain colons)
        allowSpaces: false,
        command: ({ editor, range, props }: any) => {
          editor.chain().focus().deleteRange(range).insertContent(props.emoji).run();
        },
      },
    };
  },

  addProseMirrorPlugins() {
    return [
      Suggestion({
        editor: this.editor,
        ...this.options.suggestion,
      }),
    ];
  },
});

// --- Tab Indent Extension ---
const TabIndentExtension = Extension.create({
  name: 'tabIndent',
  addKeyboardShortcuts() {
    return {
      Tab: () => {
        if (this.editor.commands.sinkListItem('listItem')) return true;
        if (this.editor.commands.sinkListItem('taskItem')) return true;
        // Fallback for regular paragraph (insert spaces), BUT NOT IN A TABLE
        if (this.editor.isActive('paragraph') && !this.editor.isActive('table')) {
          return this.editor.commands.insertContent('    ');
        }
        return false;
      },
      'Shift-Tab': () => {
        if (this.editor.commands.liftListItem('listItem')) return true;
        if (this.editor.commands.liftListItem('taskItem')) return true;
        return false;
      },
    };
  },
});

// --- Arrow Typography Extension ---
const ArrowExtension = Extension.create({
  name: 'arrows',
  addInputRules() {
    return [
      textInputRule({
        find: /->$/,
        replace: '→',
      }),
      textInputRule({
        find: /<-$/,
        replace: '←',
      }),
      textInputRule({
        find: /←>$/,
        replace: '↔',
      }),
      // Support direct <-> typing without individual triggering just in case
      textInputRule({
        find: /<->$/,
        replace: '↔',
      }),
    ];
  },
});

// --- Custom Blockquote to remove "> " shortcut ---
const CustomBlockquote = Blockquote.extend({
  addInputRules() {
    return [];
  }
});

// --- Editor ---
const editor = useEditor({
  content: injectLocalAssets(props.modelValue),
  extensions: [
    StarterKit.configure({
      codeBlock: false, // replaced by CodeBlockLowlight
      blockquote: false, // replaced by CustomBlockquote
    }),
    CustomBlockquote,
    TabIndentExtension,
    Markdown.configure({ html: true }),
    ArrowExtension,
    CustomImage,
    ImageCopyFix,
    ImageGallery.configure({ vaultPath: props.vaultPath }),
    TaskList,
    TaskItem.configure({ nested: true }),
    Link.configure({
      openOnClick: false,
      autolink: true,
      linkOnPaste: true,
      protocols: ['http', 'https', 'ftp', 'mailto', 'synabit'],
      HTMLAttributes: {
        title: 'Cmd/Ctrl + Click to open link',
        class: 'synabit-link',
      },
    }),
    Underline,
    Highlight.configure({ multicolor: false }),
    CodeBlockLowlight.extend({
      addNodeView() {
        return VueNodeViewRenderer(CodeBlockComponent);
      },
    }).configure({
      lowlight,
    }),
    EquationExtension,
    LocationExtension,
    WhiteboardExtension.configure({
      HTMLAttributes: {},
    }).extend({
      addStorage() {
        return {
          ...this.parent?.(),
          vaultPath: props.vaultPath,
        };
      },
    }),
    VideoExtension,
    AudioExtension,
    PdfExtension,
    DetailsExtension,
    Table.configure({
      resizable: true,
      allowTableNodeSelection: true,
    }),
    TableRow,
    CustomTableCell,
    CustomTableHeader,
    TextAlign.configure({
      types: ['heading', 'paragraph'],
    }),
    TextStyle,
    Color,
    Placeholder.configure({
      placeholder: 'Type / for commands...',
    }),
    SlashCommands.configure({
      suggestion: {
        char: '/',
        items: ({ query }: { query: string }) => {
          return slashCommandItems().filter(item =>
            item.title.toLowerCase().includes(query.toLowerCase())
          );
        },
        render: () => {
          let component: VueRenderer;
          let popup: TippyInstance;

          return {
            onStart: (props: any) => {
              component = new VueRenderer(SlashCommandMenu, {
                props,
                editor: props.editor,
              });

              if (!props.clientRect) return;

              popup = tippy(document.body, {
                getReferenceClientRect: props.clientRect,
                appendTo: () => document.body,
                content: component.element as Element,
                showOnCreate: true,
                interactive: true,
                trigger: 'manual',
                placement: 'bottom-start',
              });
            },
            onUpdate: (props: any) => {
              component?.updateProps(props);
              if (props.clientRect) {
                popup?.setProps({
                  getReferenceClientRect: props.clientRect,
                });
              }
            },
            onKeyDown: (props: any) => {
              if (props.event.key === 'Escape') {
                popup?.hide();
                return true;
              }
              return component?.ref?.onKeyDown(props.event);
            },
            onExit: () => {
              popup?.destroy();
              component?.destroy();
            },
          };
        },
      },
    }),
    Extension.create({
      name: 'noteMentionExtension',
      addProseMirrorPlugins() {
        return [
          Suggestion({
            editor: this.editor,
            pluginKey: new PluginKey('noteMentionSuggestion'),
            char: '@',
            command: ({ editor, range, props }) => {
              editor
                .chain()
                .focus()
                .deleteRange(range)
                .insertContent({
                  type: 'text',
                  marks: [
                    {
                      type: 'link',
                      attrs: { href: `synabit://${props.node_type || 'node'}/${props.id}` }
                    }
                  ],
                  text: props.title
                })
                .insertContent(' ')
                .run();
            },
            items: ({ query }) => {
              if (allNodes.value.length === 0) return [];
              const lowerQuery = query.toLowerCase();
              return allNodes.value
                .filter(n => n.title.toLowerCase().includes(lowerQuery) || (n.content && n.content.toLowerCase().includes(lowerQuery)))
                .slice(0, 5)
                .map(n => ({
                  id: n.id,
                  title: n.title,
                  summary: n.content ? n.content.substring(0, 50).trim() : '',
                  node_type: n.node_type || 'note'
                }));
            },
            render: () => {
              let component: any;
              let popup: TippyInstance | undefined;

              return {
                onStart: (suggestionProps: any) => {
                  component = new VueRenderer(NoteMentionMenu, {
                    props: suggestionProps,
                    editor: suggestionProps.editor,
                  });

                  if (!suggestionProps.clientRect) return;

                  popup = tippy(document.body, {
                    getReferenceClientRect: suggestionProps.clientRect,
                    appendTo: () => document.body,
                    content: component.element as Element,
                    showOnCreate: true,
                    interactive: true,
                    trigger: 'manual',
                    placement: 'bottom-start',
                  });
                },
                onUpdate: (suggestionProps: any) => {
                  component?.updateProps(suggestionProps);
                  if (suggestionProps.clientRect) {
                    popup?.setProps({
                      getReferenceClientRect: suggestionProps.clientRect,
                    });
                  }
                },
                onKeyDown: (suggestionProps: any) => {
                  if (suggestionProps.event.key === 'Escape') {
                    popup?.hide();
                    return true;
                  }
                  return component?.ref?.onKeyDown(suggestionProps.event);
                },
                onExit: () => {
                  popup?.destroy();
                  component?.destroy();
                },
              };
            },
          }),
        ];
      },
    }),
    EmojiSuggestion.configure({
      suggestion: {
        char: ':',
        pluginKey: new PluginKey('emojiSuggestion'),
        allowSpaces: false,
        items: ({ query }: { query: string }) => {
          if (!query || query.length < 2) return [];
          const q = query.toLowerCase();
          return emojiData.filter(e =>
            e.shortcode.includes(q) ||
            e.keywords.some(k => k.includes(q))
          ).slice(0, 8);
        },
        command: ({ editor, range, props }: any) => {
          editor.chain().focus().deleteRange(range).insertContent(props.emoji).run();
        },
        render: () => {
          let component: VueRenderer;
          let popup: TippyInstance;

          const createPopup = (props: any) => {
            component = new VueRenderer(EmojiSuggestionMenu, {
              props,
              editor: props.editor,
            });

            if (!props.clientRect) return;

            popup = tippy(document.body, {
              getReferenceClientRect: props.clientRect,
              appendTo: () => document.body,
              content: component.element as Element,
              showOnCreate: props.items.length > 0,
              interactive: true,
              trigger: 'manual',
              placement: 'bottom-start',
            });
          };

          return {
            onStart: (props: any) => {
              createPopup(props);
            },
            onUpdate: (props: any) => {
              // Lazy init: if onStart didn't create popup (e.g. no clientRect)
              if (!component) {
                createPopup(props);
                return;
              }
              component.updateProps(props);
              if (!props.items.length) {
                popup?.hide();
                return;
              }
              popup?.show();
              if (props.clientRect) {
                popup?.setProps({
                  getReferenceClientRect: props.clientRect,
                });
              }
            },
            onKeyDown: (props: any) => {
              if (props.event.key === 'Escape') {
                popup?.hide();
                return true;
              }
              return component?.ref?.onKeyDown(props.event);
            },
            onExit: () => {
              popup?.destroy();
              component?.destroy();
            },
          };
        },
      },
    }),
    TransclusionExtension,
    BlockIdHider,
  ],
  onUpdate: ({ editor: ed }) => {
    let md = (ed.storage as any).markdown.getMarkdown();
    // Convert Transclusion HTML tags back to ![[Target]]
    // Handle spans with data-transclusion (and optional data-node-id) in any attribute order
    md = md.replace(/<span[^>]*data-transclusion="([^"]+)"[^>]*>.*?<\/span>/g, (_m: string, target: string) => `![[${target}]]`);
    emit('update:modelValue', stripLocalAssets(md));
    // Update bubble menu position on content change
    setTimeout(updateBubbleMenu, 10);
  },
  onSelectionUpdate: ({ editor: ed }) => {
    trackCellSelection(); // capture CellSelection before any right-click can destroy it
    setTimeout(updateBubbleMenu, 10);
    setTimeout(updateTableControls, 10);
    
    // Typewriter scrolling in Zen Mode
    if (props.zenMode && ed.view.state.selection.empty) {
      const view = ed.view;
      const coords = view.coordsAtPos(view.state.selection.from);
      const scrollContainer = view.dom.closest('.overflow-y-auto');
      if (scrollContainer) {
         const containerRect = scrollContainer.getBoundingClientRect();
         const targetTop = scrollContainer.scrollTop + (coords.top - containerRect.top) - (containerRect.height / 2) + 20;
         scrollContainer.scrollTo({ top: targetTop, behavior: 'smooth' });
      }
    }
  },
  onBlur: () => {
    setTimeout(() => { showBubble.value = false; }, 200);
  },
  editorProps: {
    handleDOMEvents: {
      contextmenu: (view, event) => {
        const target = event.target as HTMLElement;
        if (target.closest('td, th') && target.closest('table')) {
          event.preventDefault();
          updateTableControls();
          openContextMenu(event);
          return true;
        }
        // Block context menu for paragraphs/headings
        const blockEl = target.closest('p, h1, h2, h3, h4, h5, h6');
        if (blockEl && props.currentNoteId && !target.closest('table')) {
          const text = blockEl.textContent?.trim();
          if (text) {
            event.preventDefault();
            openBlockContextMenu(event, text);
            return true;
          }
        }
        blockCtxMenu.value.show = false;
        return false;
      },
    },

    transformPastedHTML(html) {
      return html
        .replace(/color\s*:\s*[^;"]+;?/gi, '')
        .replace(/background-color\s*:\s*[^;"]+;?/gi, '')
        .replace(/color="[^"]*"/gi, '')
        .replace(/bgcolor="[^"]*"/gi, '');
    },
    attributes: {
      class: `prose focus:outline-none dark:prose-invert max-w-none w-full ${props.minHeightClass || 'min-h-[500px]'} break-words whitespace-pre-wrap`,
    },
    handleClick: (_view, _pos, event) => {
      const target = event.target as HTMLElement;
      const link = target.closest('a');
      if (link) {
          const href = link.getAttribute('href');
          if (href?.startsWith('synabit://')) {
              event.preventDefault();
              if (event.metaKey || event.ctrlKey) {
                  // Extract the type and ID from synabit://type/id
                  const match = href.match(/synabit:\/\/([^\/]+)\/(.+)/);
                  if (match) {
                      const type = match[1];
                      const nodeId = decodeURIComponent(match[2]);
                      emit('open-internal-note', { id: nodeId, type });
                  }
              }
              return true;
          }
      }
      return false;
    },
    handleDrop: function(view, event, _slice, moved) {
      if (!moved && event.dataTransfer && event.dataTransfer.files && event.dataTransfer.files.length > 0) {
        event.preventDefault();
        const file = event.dataTransfer.files[0];
        const { clientX, clientY } = event;
        const pos = view.posAtCoords({ left: clientX, top: clientY })?.pos;

        if (props.vaultPath) {
           file.arrayBuffer().then(async (buffer) => {
              try {
                  const relativePath = await invoke<string>('save_asset', {
                      vaultPath: props.vaultPath,
                      filename: file.name,
                      bytes: Array.from(new Uint8Array(buffer))
                  });
                  const sep = props.vaultPath.includes('\\') ? '\\' : '/';
                  const absPath = `${props.vaultPath}${sep}${relativePath}`;
                  const renderUrl = convertFileSrc(absPath);
                  
                  if (pos !== undefined) {
                     editor.value?.commands.insertContentAt(pos, { type: 'image', attrs: { src: renderUrl, alt: file.name } });
                  } else {
                     editor.value?.commands.setImage({ src: renderUrl, alt: file.name });
                  }
              } catch(e) { logger.error("Failed to save dropped asset", e); }
           });
        }
        return true; 
      }
      return false; 
    },
    handlePaste: function(_view, event, _slice) {
      // Handle synabit:// block reference URIs
      const text = event.clipboardData?.getData('text/plain') || '';
      const blockMatch = text.match(/^synabit:\/\/block\/([^#]+)#(.+)$/);
      if (blockMatch) {
        event.preventDefault();
        const [, nodeId, blockId] = blockMatch;
        editor.value?.commands.insertContent({
          type: 'transclusion',
          attrs: { target: `${nodeId}#${blockId}`, nodeId },
        });
        return true;
      }
      // Handle pasted images
      if (event.clipboardData && event.clipboardData.items) {
        let imageHandled = false;
        for (const item of event.clipboardData.items) {
          if (item.type.startsWith('image/')) {
            const file = item.getAsFile();
            if (file && props.vaultPath) {
              imageHandled = true;
              event.preventDefault();
              
              file.arrayBuffer().then(async (buffer) => {
                 try {
                     const relativePath = await invoke<string>('save_asset', {
                         vaultPath: props.vaultPath,
                         filename: file.name || 'pasted-image.png',
                         bytes: Array.from(new Uint8Array(buffer))
                     });
                     const sep = props.vaultPath.includes('\\') ? '\\' : '/';
                     const absPath = `${props.vaultPath}${sep}${relativePath}`;
                     const renderUrl = convertFileSrc(absPath);
                     
                     editor.value?.commands.setImage({ src: renderUrl, alt: file.name || 'Pasted Image' });
                 } catch(e) { logger.error("Paste image failed", e); }
              });
            }
          }
        }
        if (imageHandled) return true;
      }
      return false;
    }
  },
});

const loadContent = (markdown: string) => {
  if (editor.value) {
    editor.value.commands.setContent(injectLocalAssets(markdown));
  }
};

const focus = () => {
  if (editor.value) {
    editor.value.commands.focus('start');
  }
};

defineExpose({
  loadContent,
  focus
});

// Close context menu on click outside
const onDocMouseDown = (e: MouseEvent) => {
  let target = e.target as HTMLElement | Node | null;
  if (target && target.nodeType === Node.TEXT_NODE) {
    target = target.parentElement;
  }
  const el = target as HTMLElement;
  if (el && el.closest && !el.closest('.tc-ctx-menu, .tc-corner-handle, .tc-col-handle, .tc-row-handle')) {
    closeCtxMenu();
  }
};

onMounted(() => {
  document.addEventListener('mousedown', onDocMouseDown, true);

  // Listen for whiteboard embed "Open in Whiteboard" events
  const editorDom = editor.value?.view?.dom;
  if (editorDom) {
    editorDom.addEventListener('open-whiteboard-embed', ((e: CustomEvent) => {
      emit('open-internal-note', { id: e.detail.id, type: 'whiteboard' });
    }) as EventListener);
  }
});

watch(() => props.modelValue, (newVal) => {
  if (editor.value) {
    const currentMd = (editor.value.storage as any).markdown.getMarkdown();
    if (stripLocalAssets(currentMd) !== newVal) {
       editor.value.commands.setContent(injectLocalAssets(newVal));
    }
  }
});

onBeforeUnmount(() => {
  document.removeEventListener('mousedown', onDocMouseDown, true);
  if (editor.value) {
    editor.value.destroy();
  }
});
</script>

<template>
  <div class="tiptap-wrapper w-full relative">
    <!-- Floating Toolbar (teleported to body to escape overflow clipping) -->
    <Teleport to="body">
    <Transition name="bubble">
      <div
        v-if="showBubble && editor"
        ref="bubbleMenuRef"
        class="bubble-menu"
        :style="{ top: bubblePos.top + 'px', left: bubblePos.left + 'px' }"
        @mousedown.prevent
      >
        <button
          @click="editor!.chain().focus().toggleBold().run()"
          :class="{ 'is-active': editor!.isActive('bold') }"
          title="Bold"
        >
          <BoldIcon class="w-4 h-4" />
        </button>
        <button
          @click="editor!.chain().focus().toggleItalic().run()"
          :class="{ 'is-active': editor!.isActive('italic') }"
          title="Italic"
        >
          <ItalicIcon class="w-4 h-4" />
        </button>
        <button
          @click="editor!.chain().focus().toggleUnderline().run()"
          :class="{ 'is-active': editor!.isActive('underline') }"
          title="Underline"
        >
          <UnderlineIcon class="w-4 h-4" />
        </button>
        <button
          @click="editor!.chain().focus().toggleStrike().run()"
          :class="{ 'is-active': editor!.isActive('strike') }"
          title="Strikethrough"
        >
          <StrikeThroughIcon class="w-4 h-4" />
        </button>
        <div class="bubble-divider" />
        <button
          @click="editor!.chain().focus().toggleHighlight().run()"
          :class="{ 'is-active': editor!.isActive('highlight') }"
          title="Highlight"
        >
          <Highlighter class="w-4 h-4" />
        </button>
        <button
          @click="editor!.chain().focus().toggleCode().run()"
          :class="{ 'is-active': editor!.isActive('code') }"
          title="Inline Code"
        >
          <Code class="w-4 h-4" />
        </button>
        <div class="bubble-divider" />
        <button
          @click="setLink"
          :class="{ 'is-active': editor!.isActive('link') }"
          title="Link"
        >
          <LinkIcon class="w-4 h-4" />
        </button>
        <div class="bubble-divider" />
        <button
          @click="editor!.chain().focus().setTextAlign('left').run()"
          :class="{ 'is-active': editor!.isActive({ textAlign: 'left' }) }"
          title="Align Left"
        >
          <AlignLeft class="w-4 h-4" />
        </button>
        <button
          @click="editor!.chain().focus().setTextAlign('center').run()"
          :class="{ 'is-active': editor!.isActive({ textAlign: 'center' }) }"
          title="Align Center"
        >
          <AlignCenter class="w-4 h-4" />
        </button>
        <button
          @click="editor!.chain().focus().setTextAlign('right').run()"
          :class="{ 'is-active': editor!.isActive({ textAlign: 'right' }) }"
          title="Align Right"
        >
          <AlignRight class="w-4 h-4" />
        </button>
        <button
          @click="editor!.chain().focus().setTextAlign('justify').run()"
          :class="{ 'is-active': editor!.isActive({ textAlign: 'justify' }) }"
          title="Align Justify"
        >
          <AlignJustify class="w-4 h-4" />
        </button>
        <div class="bubble-divider" />
        <label
          title="Text Color"
          class="relative flex items-center justify-center p-1.5 rounded-sm hover:bg-slate-200 dark:hover:bg-slate-700 cursor-pointer text-slate-700 dark:text-slate-300 transition-colors tooltip-wrapper"
        >
          <Palette class="w-4 h-4" />
          <input 
            type="color" 
            @input="(e) => editor!.chain().focus().setColor((e.target as HTMLInputElement).value).run()" 
            :value="editor!.getAttributes('textStyle').color || '#000000'"
            class="absolute opacity-0 inset-0 w-full h-full cursor-pointer"
          />
        </label>
      </div>
    </Transition>
    </Teleport>

    <!-- Table Controls: + buttons, row/col handles -->
    <template v-if="isInTable && activeTableEl">
      <!-- Column handles (top of each column) -->
      <button
        v-for="(col, i) in colPositions" :key="'ch-'+i"
        v-show="i === activeColIdx"
        class="tc-col-handle"
        :style="{ position: 'absolute', top: (tableRect.top - 20) + 'px', left: (col.left + col.width / 2 - 10) + 'px' }"
        @mousedown.prevent.stop="(e: MouseEvent) => { selectColumn(i, e); openContextMenu(e); }"
        @click.stop
      >
        <GripVertical class="w-3 h-3 rotate-90" />
      </button>

      <!-- Row handles (left of each row) -->
      <button
        v-for="(row, i) in rowPositions" :key="'rh-'+i"
        v-show="i === activeRowIdx"
        class="tc-row-handle"
        :style="{ position: 'absolute', top: (row.top + row.height / 2 - 10) + 'px', left: (tableRect.left - 22) + 'px' }"
        @mousedown.prevent.stop="(e: MouseEvent) => { selectRow(i, e); openContextMenu(e); }"
        @click.stop
      >
        <GripVertical class="w-3 h-3" />
      </button>

      <!-- Corner handle (select whole table) -->
      <button
        class="tc-corner-handle"
        :style="{ position: 'absolute', top: (tableRect.top - 22) + 'px', left: (tableRect.left - 24) + 'px' }"
        @mousedown.prevent.stop="(e: MouseEvent) => { selectWholeTable(); openContextMenu(e); }"
        @click.stop
      >
        <svg width="10" height="10" viewBox="0 0 10 10"><rect x="0" y="0" width="4" height="4" fill="currentColor" rx="0.5"/><rect x="6" y="0" width="4" height="4" fill="currentColor" rx="0.5"/><rect x="0" y="6" width="4" height="4" fill="currentColor" rx="0.5"/><rect x="6" y="6" width="4" height="4" fill="currentColor" rx="0.5"/></svg>
      </button>

      <!-- Add row button (bottom) -->
      <button
        class="tc-add-btn tc-add-row"
        :style="{ position: 'absolute', top: (tableRect.bottom + 2) + 'px', left: (tableRect.left + tableRect.width / 2 - 14) + 'px' }"
        @mousedown.prevent="addRowAtBottom"
        title="Add row"
      >
        <Plus class="w-3.5 h-3.5" />
      </button>

      <!-- Add column button (right) -->
      <button
        class="tc-add-btn tc-add-col"
        :style="{ position: 'absolute', top: (tableRect.top + tableRect.height / 2 - 14) + 'px', left: (tableRect.right + 2) + 'px' }"
        @mousedown.prevent="addColAtRight"
        title="Add column"
      >
        <Plus class="w-3.5 h-3.5" />
      </button>
    </template>

    <!-- Table Context Menu -->
    <Transition name="bubble">
      <div
        v-if="showCtxMenu && editor"
        class="tc-ctx-menu"
        :style="{ position: 'absolute', top: ctxMenuPos.top + 'px', left: ctxMenuPos.left + 'px' }"
        @mousedown.prevent.stop
      >
        <button @click="ctxAction('addRowAbove')">Add row above</button>
        <button @click="ctxAction('addRowBelow')">Add row below</button>
        <button @click="ctxAction('deleteRow')" class="ctx-danger">Delete row</button>
        <div class="ctx-sep" />
        <button @click="ctxAction('addColLeft')">Add column left</button>
        <button @click="ctxAction('addColRight')">Add column right</button>
        <button @click="ctxAction('deleteCol')" class="ctx-danger">Delete column</button>
        <div class="ctx-sep" />
        <button @click="ctxAction('mergeCells')">Merge cells</button>
        <button @click="ctxAction('splitCell')">Split cell</button>
        <button @click="ctxAction('toggleHeaderRow')">Toggle header row</button>
        <button @click="ctxAction('toggleHeaderCol')">Toggle header column</button>
        <div class="ctx-sep" />
        
        <div class="flex items-center gap-2 px-3 py-1.5 border-b border-gray-100 dark:border-[#333]">
          <span class="text-xs text-gray-500 font-medium w-10 shrink-0">Color:</span>
          <div class="flex items-center gap-2 flex-1">
            <div class="w-5 h-5 shrink-0 rounded-full border border-gray-200 dark:border-gray-700 bg-transparent flex items-center justify-center text-[10px] text-gray-400 hover:text-gray-700 dark:hover:text-gray-200 hover:bg-gray-100 dark:hover:bg-[#444] transition-colors cursor-pointer" @click="setCellColor(null)" title="Clear color">✕</div>
            <div class="w-5 h-5 shrink-0 rounded-full border border-gray-200 dark:border-gray-700 hover:scale-110 transition-transform cursor-pointer" style="background-color: #fee2e2;" @click="setCellColor('rgba(239, 68, 68, 0.15)')"></div>
            <div class="w-5 h-5 shrink-0 rounded-full border border-gray-200 dark:border-gray-700 hover:scale-110 transition-transform cursor-pointer" style="background-color: #dbeafe;" @click="setCellColor('rgba(59, 130, 246, 0.15)')"></div>
            <div class="w-5 h-5 shrink-0 rounded-full border border-gray-200 dark:border-gray-700 hover:scale-110 transition-transform cursor-pointer" style="background-color: #d1fae5;" @click="setCellColor('rgba(16, 185, 129, 0.15)')"></div>
            <div class="w-5 h-5 shrink-0 rounded-full border border-gray-200 dark:border-gray-700 hover:scale-110 transition-transform cursor-pointer" style="background-color: #fef3c7;" @click="setCellColor('rgba(245, 158, 11, 0.15)')"></div>
            <div class="w-5 h-5 shrink-0 rounded-full border border-gray-200 dark:border-gray-700 hover:scale-110 transition-transform cursor-pointer" style="background-color: #f3e8ff;" @click="setCellColor('rgba(168, 85, 247, 0.15)')"></div>
            <label class="w-5 h-5 shrink-0 rounded-full border border-gray-200 dark:border-gray-700 flex items-center justify-center cursor-pointer hover:bg-gray-100 dark:hover:bg-[#444] relative hover:scale-110 transition-transform" title="Custom color">
              <Palette class="w-3 h-3 text-gray-500 dark:text-gray-400" />
              <input 
                type="color" 
                @input="(e) => setCellColor((e.target as HTMLInputElement).value, false)" 
                class="absolute opacity-0 inset-0 w-full h-full cursor-pointer"
              />
            </label>
          </div>
        </div>

        <button @click="ctxAction('deleteTable')" class="ctx-danger">Delete table</button>
      </div>
    </Transition>

    <!-- Block Context Menu (Copy Block Link) -->
    <Transition name="bubble">
      <div
        v-if="blockCtxMenu.show"
        class="tc-ctx-menu"
        :style="{ position: 'absolute', top: blockCtxMenu.top + 'px', left: blockCtxMenu.left + 'px', zIndex: 100 }"
        @mousedown.prevent
      >
        <button @click="copyBlockLink" class="flex items-center gap-2">
          <LinkIcon class="w-3.5 h-3.5" />
          Copy Block Link
        </button>
      </div>
    </Transition>

    <div :class="{
      'list-style-decimal': nestedNumberListStyle === 'decimal',
      'list-style-alpha': nestedNumberListStyle === 'alpha',
      'list-style-nested': nestedNumberListStyle === 'nested'
    }" class="editor-wrapper h-full w-full">
      <editor-content :editor="editor" @contextmenu="(e: MouseEvent) => { if (editor?.isActive('table')) openContextMenu(e); }" @click="blockCtxMenu.show = false" />
    </div>

    <!-- Link URL Modal (replaces window.prompt) -->
    <Teleport to="body">
      <div v-if="linkModal.show" class="fixed inset-0 z-[999] flex items-center justify-center bg-black/40 backdrop-blur-sm" @click.self="linkModal.show = false">
        <div class="bg-white dark:bg-[#2a2a2a] rounded-2xl shadow-2xl p-6 w-96 border border-[#e6e6e6] dark:border-[#3a3a3a]">
          <h3 class="text-base font-semibold text-[#1c1c1e] dark:text-[#f4f4f5] mb-4">Insert Link</h3>
          <input
            v-model="linkModal.url"
            type="url"
            placeholder="https://example.com"
            class="w-full px-3 py-2 rounded-lg border border-[#e0e0e0] dark:border-[#444] bg-white dark:bg-[#1e1e1e] text-[#1c1c1e] dark:text-[#f4f4f5] text-sm focus:outline-none focus:ring-2 focus:ring-black/10 dark:focus:ring-white/20"
            @keydown.enter="confirmLink"
            autofocus
          />
          <div class="flex justify-end gap-2 mt-4">
            <button @click="linkModal.url = ''; confirmLink()" class="px-4 py-1.5 text-sm rounded-lg text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors">Remove Link</button>
            <button @click="linkModal.show = false" class="px-4 py-1.5 text-sm rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-[#333] transition-colors">Cancel</button>
            <button @click="confirmLink" class="px-4 py-1.5 text-sm rounded-lg bg-black dark:bg-white text-white dark:text-black font-medium hover:opacity-80 transition-opacity">Apply</button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- Video Modal -->
    <Teleport to="body">
      <div v-if="videoModal.show" class="fixed inset-0 z-[999] flex items-center justify-center bg-black/40 backdrop-blur-sm" @click.self="videoModal.show = false">
        <div class="bg-white dark:bg-[#2a2a2a] rounded-2xl shadow-2xl p-6 w-96 border border-[#e6e6e6] dark:border-[#3a3a3a]">
          <h3 class="text-base font-semibold text-[#1c1c1e] dark:text-[#f4f4f5] mb-4">Embed Video</h3>
          
          <div class="space-y-4">
            <div>
              <label class="block text-xs font-medium text-gray-500 dark:text-gray-400 mb-1">YouTube or Web URL</label>
              <input
                v-model="videoModal.url"
                type="url"
                placeholder="https://youtube.com/watch?v=..."
                class="w-full px-3 py-2 rounded-lg border border-[#e0e0e0] dark:border-[#444] bg-white dark:bg-[#1e1e1e] text-[#1c1c1e] dark:text-[#f4f4f5] text-sm focus:outline-none focus:ring-2 focus:ring-black/10 dark:focus:ring-white/20"
                @keydown.enter="confirmVideo"
                autofocus
              />
            </div>
            
            <div class="flex items-center justify-center">
              <div class="h-px bg-gray-200 dark:bg-[#444] flex-1"></div>
              <span class="text-xs text-gray-400 px-3 uppercase tracking-wider font-semibold">Or</span>
              <div class="h-px bg-gray-200 dark:bg-[#444] flex-1"></div>
            </div>
            
            <button @click="selectLocalVideo" class="w-full py-2 px-4 rounded-lg bg-[#f4f4f5] dark:bg-[#333] text-sm text-[#1c1c1e] dark:text-[#f4f4f5] font-medium hover:bg-[#e4e4e7] dark:hover:bg-[#444] transition-colors border border-[#e0e0e0] dark:border-[#444] flex items-center justify-center gap-2">
              <VideoIcon class="w-4 h-4" />
              Browse Local File
            </button>
          </div>
          
          <div class="flex justify-end gap-2 mt-6">
            <button @click="videoModal.show = false" class="px-4 py-1.5 text-sm rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-[#333] transition-colors">Cancel</button>
            <button @click="confirmVideo" class="px-4 py-1.5 text-sm rounded-lg bg-black dark:bg-white text-white dark:text-black font-medium hover:opacity-80 transition-opacity">Embed</button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- Audio Modal -->
    <Teleport to="body">
      <div v-if="audioModal.show" class="fixed inset-0 z-[999] flex items-center justify-center bg-black/40 backdrop-blur-sm" @click.self="audioModal.show = false">
        <div class="bg-white dark:bg-[#2a2a2a] rounded-2xl shadow-2xl p-6 w-96 border border-[#e6e6e6] dark:border-[#3a3a3a]">
          <h3 class="text-base font-semibold text-[#1c1c1e] dark:text-[#f4f4f5] mb-4">Embed Audio</h3>
          
          <div class="space-y-4">
            <div>
              <label class="block text-xs font-medium text-gray-500 dark:text-gray-400 mb-1">Spotify, SoundCloud or Web URL</label>
              <input
                v-model="audioModal.url"
                type="url"
                placeholder="https://open.spotify.com/track/..."
                class="w-full px-3 py-2 rounded-lg border border-[#e0e0e0] dark:border-[#444] bg-white dark:bg-[#1e1e1e] text-[#1c1c1e] dark:text-[#f4f4f5] text-sm focus:outline-none focus:ring-2 focus:ring-black/10 dark:focus:ring-white/20"
                @keydown.enter="confirmAudio"
                autofocus
              />
            </div>
            
            <div class="flex items-center justify-center">
              <div class="h-px bg-gray-200 dark:bg-[#444] flex-1"></div>
              <span class="text-xs text-gray-400 px-3 uppercase tracking-wider font-semibold">Or</span>
              <div class="h-px bg-gray-200 dark:bg-[#444] flex-1"></div>
            </div>
            
            <button @click="selectLocalAudio" class="w-full py-2 px-4 rounded-lg bg-[#f4f4f5] dark:bg-[#333] text-sm text-[#1c1c1e] dark:text-[#f4f4f5] font-medium hover:bg-[#e4e4e7] dark:hover:bg-[#444] transition-colors border border-[#e0e0e0] dark:border-[#444] flex items-center justify-center gap-2">
              <MusicIcon class="w-4 h-4" />
              Browse Local File
            </button>
          </div>
          
          <div class="flex justify-end gap-2 mt-6">
            <button @click="audioModal.show = false" class="px-4 py-1.5 text-sm rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-[#333] transition-colors">Cancel</button>
            <button @click="confirmAudio" class="px-4 py-1.5 text-sm rounded-lg bg-black dark:bg-white text-white dark:text-black font-medium hover:opacity-80 transition-opacity">Embed</button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- Location Modal -->
    <Teleport to="body">
      <div v-if="locationModal.show" class="fixed inset-0 z-[999] flex items-center justify-center bg-black/40 backdrop-blur-sm" @click.self="locationModal.show = false">
        <div class="bg-white dark:bg-[#2a2a2a] rounded-2xl shadow-2xl p-6 w-[420px] border border-[#e6e6e6] dark:border-[#3a3a3a]">
          <h3 class="text-base font-semibold text-[#1c1c1e] dark:text-[#f4f4f5] mb-1 flex items-center gap-2">
            <MapPinIcon class="w-4 h-4 text-red-500" />
            Insert Location
          </h3>
          <p class="text-xs text-gray-400 dark:text-gray-500 mb-4">Paste a map URL, enter coordinates, or search an address</p>
          
          <div class="space-y-3">
            <!-- Input -->
            <div class="relative">
              <input
                :value="locationModal.input"
                @input="(e: Event) => onLocationInput((e.target as HTMLInputElement).value)"
                type="text"
                placeholder="Paste URL, lat/lng, or type an address..."
                class="w-full px-3 py-2.5 rounded-lg border border-[#e0e0e0] dark:border-[#444] bg-white dark:bg-[#1e1e1e] text-[#1c1c1e] dark:text-[#f4f4f5] text-sm focus:outline-none focus:ring-2 focus:ring-red-500/20 dark:focus:ring-red-400/20 focus:border-red-400 dark:focus:border-red-500 pr-8"
                @keydown.enter="confirmLocation"
                autofocus
              />
              <div v-if="locationModal.searching" class="absolute right-3 top-1/2 -translate-y-1/2">
                <div class="w-4 h-4 border-2 border-gray-300 dark:border-gray-600 border-t-red-500 rounded-full animate-spin" />
              </div>
            </div>

            <!-- Suggestions -->
            <div v-if="locationModal.suggestions.length > 0" class="rounded-lg border border-[#e0e0e0] dark:border-[#444] bg-[#fafafa] dark:bg-[#1a1a1a] max-h-[160px] overflow-y-auto">
              <button
                v-for="(s, i) in locationModal.suggestions"
                :key="i"
                class="w-full text-left px-3 py-2 text-xs text-[#374151] dark:text-[#d4d4d8] hover:bg-[#f3f4f6] dark:hover:bg-[#252525] transition-colors border-b border-[#f3f4f6] dark:border-[#2a2a2a] last:border-0 flex items-start gap-2"
                @click="selectSuggestion(s)"
              >
                <MapPinIcon class="w-3 h-3 text-red-400 flex-shrink-0 mt-0.5" />
                <span class="line-clamp-2">{{ s.display }}</span>
              </button>
            </div>

            <!-- Error -->
            <p v-if="locationModal.error" class="text-xs text-red-500">{{ locationModal.error }}</p>

            <!-- Resolved coordinates preview -->
            <div v-if="locationModal.lat !== null && locationModal.lng !== null" class="flex items-center gap-2 px-3 py-2 rounded-lg bg-green-50 dark:bg-green-900/20 border border-green-200 dark:border-green-800/30">
              <MapPinIcon class="w-3.5 h-3.5 text-green-600 dark:text-green-400 flex-shrink-0" />
              <div class="flex-1 min-w-0">
                <span class="text-xs font-medium text-green-700 dark:text-green-300">{{ locationModal.lat.toFixed(5) }}, {{ locationModal.lng.toFixed(5) }}</span>
              </div>
            </div>

            <!-- Label -->
            <div v-if="locationModal.lat !== null">
              <label class="block text-xs font-medium text-gray-500 dark:text-gray-400 mb-1">Label (optional)</label>
              <input
                v-model="locationModal.label"
                type="text"
                placeholder="e.g., Hanoi Opera House"
                class="w-full px-3 py-2 rounded-lg border border-[#e0e0e0] dark:border-[#444] bg-white dark:bg-[#1e1e1e] text-[#1c1c1e] dark:text-[#f4f4f5] text-sm focus:outline-none focus:ring-2 focus:ring-black/10 dark:focus:ring-white/20"
                @keydown.enter="confirmLocation"
              />
            </div>

            <!-- Map Provider -->
            <div v-if="locationModal.lat !== null">
              <label class="block text-xs font-medium text-gray-500 dark:text-gray-400 mb-1.5">Map Provider</label>
              <div class="flex gap-2">
                <button
                  @click="locationModal.provider = 'osm'"
                  class="flex-1 py-2 px-3 rounded-lg text-xs font-medium transition-all border"
                  :class="locationModal.provider === 'osm'
                    ? 'bg-emerald-50 dark:bg-emerald-900/20 border-emerald-300 dark:border-emerald-700 text-emerald-700 dark:text-emerald-300'
                    : 'bg-[#fafafa] dark:bg-[#1a1a1a] border-[#e0e0e0] dark:border-[#444] text-gray-500 dark:text-gray-400 hover:bg-[#f3f4f6] dark:hover:bg-[#252525]'"
                >
                  🗺️ OpenStreetMap
                  <span class="block text-[10px] mt-0.5 opacity-70">Free · Privacy-first</span>
                </button>
                <button
                  @click="locationModal.provider = 'google'"
                  class="flex-1 py-2 px-3 rounded-lg text-xs font-medium transition-all border"
                  :class="locationModal.provider === 'google'
                    ? 'bg-blue-50 dark:bg-blue-900/20 border-blue-300 dark:border-blue-700 text-blue-700 dark:text-blue-300'
                    : 'bg-[#fafafa] dark:bg-[#1a1a1a] border-[#e0e0e0] dark:border-[#444] text-gray-500 dark:text-gray-400 hover:bg-[#f3f4f6] dark:hover:bg-[#252525]'"
                >
                  📍 Google Maps
                  <span class="block text-[10px] mt-0.5 opacity-70">Detailed · Satellite</span>
                </button>
              </div>
            </div>
          </div>
          
          <div class="flex justify-end gap-2 mt-5">
            <button @click="locationModal.show = false" class="px-4 py-1.5 text-sm rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-[#333] transition-colors">Cancel</button>
            <button
              @click="confirmLocation"
              :disabled="locationModal.lat === null || locationModal.lng === null"
              class="px-4 py-1.5 text-sm rounded-lg bg-red-500 text-white font-medium hover:bg-red-600 transition-colors disabled:opacity-40 disabled:cursor-not-allowed flex items-center gap-1.5"
            >
              <MapPinIcon class="w-3.5 h-3.5" />
              Insert
            </button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- Route Modal -->
    <Teleport to="body">
      <div v-if="routeModal.show" class="fixed inset-0 z-[999] flex items-center justify-center bg-black/40 backdrop-blur-sm" @click.self="routeModal.show = false">
        <div class="bg-white dark:bg-[#1e1e1e] rounded-2xl shadow-2xl border border-[#e5e7eb] dark:border-[#333] w-[420px] max-w-[95vw] p-5" @keydown.esc="routeModal.show = false">
          <div class="flex items-center gap-2 mb-4">
            <NavigationIcon class="w-4 h-4 text-indigo-500" />
            <h3 class="text-sm font-semibold text-gray-800 dark:text-gray-200">Insert Route</h3>
          </div>

          <!-- URL Input -->
          <div class="mb-3">
            <label class="text-[10px] font-semibold text-gray-400 uppercase tracking-wider mb-1 block">Directions URL</label>
            <input
              v-model="routeModal.urlInput"
              type="text"
              placeholder="Paste Google Maps or OpenStreetMap directions URL..."
              class="w-full px-3 py-2 text-sm rounded-lg border border-[#e0e0e0] dark:border-[#444] bg-[#fafafa] dark:bg-[#252525] text-gray-800 dark:text-gray-200 outline-none focus:border-indigo-400 transition-colors"
              @keydown.enter.stop="confirmRoute"
            />
            <p class="text-[10px] text-gray-400 mt-1">Supports Google Maps and OpenStreetMap directions links.</p>
          </div>

          <!-- Optional Label -->
          <div class="mb-3">
            <label class="text-[10px] font-semibold text-gray-400 uppercase tracking-wider mb-1 block">Label <span class="text-gray-300 dark:text-gray-500">(optional)</span></label>
            <input
              v-model="routeModal.label"
              type="text"
              placeholder="e.g., Home → Office"
              class="w-full px-3 py-2 text-sm rounded-lg border border-[#e0e0e0] dark:border-[#444] bg-[#fafafa] dark:bg-[#252525] text-gray-800 dark:text-gray-200 outline-none focus:border-indigo-400 transition-colors"
              @keydown.enter.stop="confirmRoute"
            />
          </div>

          <div v-if="routeModal.error" class="text-xs text-red-400 mb-2">{{ routeModal.error }}</div>

          <div class="flex justify-end gap-2 mt-4">
            <button @click="routeModal.show = false" class="px-4 py-1.5 text-sm rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-[#333] transition-colors">Cancel</button>
            <button
              @click="confirmRoute"
              :disabled="!isValidRouteUrl"
              class="px-4 py-1.5 text-sm rounded-lg bg-indigo-500 text-white font-medium hover:bg-indigo-600 transition-colors disabled:opacity-40 disabled:cursor-not-allowed flex items-center gap-1.5"
            >
              <NavigationIcon class="w-3.5 h-3.5" />
              Insert Route
            </button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- Whiteboard Picker Modal -->
    <Teleport to="body">
      <div v-if="whiteboardPickerModal.show" class="fixed inset-0 z-[999] flex items-center justify-center bg-black/40 backdrop-blur-sm" @click.self="whiteboardPickerModal.show = false">
        <div class="bg-white dark:bg-[#1e1e1e] rounded-2xl shadow-2xl border border-[#e5e7eb] dark:border-[#333] w-[420px] max-w-[95vw] max-h-[520px] flex flex-col overflow-hidden" @keydown.esc="whiteboardPickerModal.show = false">
          <!-- Header -->
          <div class="flex items-center gap-2 p-4 pb-0">
            <PenToolIcon class="w-4 h-4 text-violet-500" />
            <h3 class="text-sm font-semibold text-gray-800 dark:text-gray-200">Insert Whiteboard</h3>
          </div>

          <!-- Search -->
          <div class="px-4 pt-3 pb-2">
            <input
              v-model="whiteboardPickerModal.search"
              type="text"
              placeholder="Search whiteboards..."
              class="w-full px-3 py-2 text-sm rounded-lg border border-[#e0e0e0] dark:border-[#444] bg-[#fafafa] dark:bg-[#252525] text-gray-800 dark:text-gray-200 outline-none focus:border-violet-400 transition-colors"
              autofocus
              @keydown.esc="whiteboardPickerModal.show = false"
            />
          </div>

          <!-- Board List -->
          <div class="flex-1 overflow-y-auto px-4 pb-4">
            <!-- Loading -->
            <div v-if="whiteboardPickerModal.loading" class="flex flex-col items-center justify-center py-12 gap-2 text-gray-400 text-sm">
              <div class="w-5 h-5 border-2 border-gray-200 dark:border-gray-600 border-t-violet-500 rounded-full animate-spin"></div>
              <span>Loading whiteboards…</span>
            </div>

            <!-- Empty -->
            <div v-else-if="filteredWhiteboards.length === 0" class="flex flex-col items-center justify-center py-12 gap-2 text-gray-400 text-sm">
              <PenToolIcon class="w-6 h-6 opacity-40" />
              <span>{{ whiteboardPickerModal.search ? 'No matching whiteboards' : 'No whiteboards found' }}</span>
            </div>

            <!-- List -->
            <div v-else class="space-y-1 mt-1">
              <button
                v-for="board in filteredWhiteboards"
                :key="board.id || board.path"
                @click="confirmWhiteboard(board)"
                class="w-full flex items-center gap-3 px-3 py-2.5 rounded-xl text-left hover:bg-violet-50 dark:hover:bg-violet-500/10 transition-colors group cursor-pointer"
              >
                <div class="w-9 h-9 rounded-lg bg-violet-100 dark:bg-violet-500/15 flex items-center justify-center flex-shrink-0 group-hover:bg-violet-200 dark:group-hover:bg-violet-500/25 transition-colors">
                  <PenToolIcon class="w-4 h-4 text-violet-600 dark:text-violet-400" />
                </div>
                <div class="flex-1 min-w-0">
                  <div class="text-sm font-medium text-gray-800 dark:text-gray-200 truncate">
                    {{ board.title || 'Untitled Board' }}
                  </div>
                  <div class="flex items-center gap-2 mt-0.5">
                    <span v-if="board.tags && board.tags.length" class="text-[10px] text-gray-400 truncate">
                      {{ board.tags.slice(0, 3).join(', ') }}
                    </span>
                    <span class="text-[10px] text-gray-400">
                      {{ board.updated_at ? new Date(board.updated_at).toLocaleDateString() : '' }}
                    </span>
                  </div>
                </div>
              </button>
            </div>
          </div>

          <!-- Footer -->
          <div class="flex justify-end p-4 pt-2 border-t border-[#f3f4f6] dark:border-[#2a2a2a]">
            <button @click="whiteboardPickerModal.show = false" class="px-4 py-1.5 text-sm rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-[#333] transition-colors">Cancel</button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- Embed Picker Modal (Transclusion 2.0) -->
    <EmbedPickerModal
      :show="embedPickerModal"
      :notes="allNodes"
      :vault-path="vaultPath"
      @close="embedPickerModal = false"
      @embed="confirmEmbed"
    />

    <!-- PDF Embed Modal -->
    <Teleport to="body">
      <div v-if="pdfModal.show" class="fixed inset-0 z-[999] flex items-center justify-center bg-black/40 backdrop-blur-sm" @click.self="pdfModal.show = false">
        <div class="bg-white dark:bg-[#1e1e1e] rounded-2xl shadow-2xl border border-[#e5e7eb] dark:border-[#333] w-[400px] p-6">
          <h3 class="text-base font-semibold text-[#111827] dark:text-[#f4f4f5] mb-1">Embed PDF</h3>
          <p class="text-sm text-gray-500 dark:text-gray-400 mb-5">Select a PDF file to embed in this note</p>
          <button
            @click="selectPdfFile"
            class="w-full flex items-center justify-center gap-2 px-4 py-3 bg-[#f3f4f6] dark:bg-[#2a2a2a] hover:bg-[#e5e7eb] dark:hover:bg-[#333] border border-[#e5e7eb] dark:border-[#444] rounded-xl text-sm font-medium text-[#111827] dark:text-[#f4f4f5] transition-colors cursor-pointer"
          >
            <BookOpenIcon class="w-5 h-5 text-red-500" />
            Choose PDF File
          </button>
          <button
            @click="pdfModal.show = false"
            class="w-full mt-2 px-4 py-2 text-sm text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-300 transition-colors cursor-pointer"
          >
            Cancel
          </button>
        </div>
      </div>
    </Teleport>

    <!-- Emoji Picker Modal -->
    <Teleport to="body">
      <div v-if="emojiPicker.show" class="fixed inset-0 z-[999] flex items-center justify-center bg-black/40 backdrop-blur-sm" @click.self="emojiPicker.show = false">
        <div class="emoji-picker-panel bg-white dark:bg-[#1e1e1e] rounded-2xl shadow-2xl border border-[#e5e7eb] dark:border-[#333] w-[360px] max-h-[420px] flex flex-col overflow-hidden">
          <!-- Search -->
          <div class="p-3 border-b border-[#e5e7eb] dark:border-[#333]">
            <input
              v-model="emojiPicker.search"
              type="text"
              placeholder="Search emoji..."
              class="w-full px-3 py-2 text-sm bg-[#f3f4f6] dark:bg-[#2a2a2a] border border-transparent rounded-lg focus:outline-none focus:ring-1 focus:ring-purple-500/50 text-[#111827] dark:text-[#f4f4f5] placeholder:text-gray-400"
              autofocus
              @keydown.esc="emojiPicker.show = false"
            />
          </div>
          <!-- Category Tabs -->
          <div v-if="!emojiPicker.search" class="flex gap-0.5 px-2 py-1.5 border-b border-[#e5e7eb] dark:border-[#333] overflow-x-auto">
            <button
              v-for="cat in emojiCategories"
              :key="cat.id"
              @click="emojiPicker.activeCategory = cat.id"
              class="px-2 py-1 text-lg rounded-md transition-colors flex-shrink-0"
              :class="emojiPicker.activeCategory === cat.id ? 'bg-[#e5e7eb] dark:bg-[#333]' : 'hover:bg-[#f3f4f6] dark:hover:bg-[#2a2a2a]'"
              :title="cat.title"
            >{{ cat.label }}</button>
          </div>
          <!-- Emoji Grid -->
          <div class="flex-1 overflow-y-auto p-2">
            <div v-if="filteredPickerEmojis.length === 0" class="py-8 text-center text-sm text-gray-400">No emoji found</div>
            <div class="grid grid-cols-8 gap-0.5">
              <button
                v-for="item in filteredPickerEmojis"
                :key="item.shortcode"
                @click="insertEmoji(item.emoji)"
                class="w-9 h-9 flex items-center justify-center text-xl rounded-lg hover:bg-[#f3f4f6] dark:hover:bg-[#2a2a2a] transition-colors cursor-pointer"
                :title="':' + item.shortcode + ':'"
              >{{ item.emoji }}</button>
            </div>
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style>
/* === Placeholder === */
.tiptap p.is-editor-empty:first-child::before {
  content: attr(data-placeholder);
  float: left;
  color: #adb5bd;
  pointer-events: none;
  height: 0;
}
.dark .tiptap p.is-editor-empty:first-child::before {
  color: #71717a;
}

/* === Images === */
.tiptap div:has(> img) {
  line-height: 0;
}
.tiptap img {
  border-radius: 0.5rem;
  max-width: 100%;
  vertical-align: bottom;
  display: block;
  margin: 0 !important;
}

/* === Prose overrides === */
.prose {
  --tw-prose-body: var(--color-text-light);
  --tw-prose-headings: var(--color-text);
}

/* Synabit Internal Links */
.prose a[href^="synabit://"] {
  color: var(--color-blue-600, #2563eb);
  text-decoration: none;
  border-bottom: 1px dashed var(--color-blue-300, #93c5fd);
  font-weight: 500;
  transition: all 0.2s ease;
  cursor: pointer;
  padding: 0 2px;
  border-radius: 4px;
}

.prose a[href^="synabit://"]:hover {
  background: var(--color-blue-50, #eff6ff);
  border-bottom-style: solid;
}

.dark .prose a[href^="synabit://"] {
  color: var(--color-blue-400, #60a5fa);
  border-bottom-color: var(--color-blue-800, #1e40af);
}

.dark .prose a[href^="synabit://"]:hover {
  background: var(--color-blue-900, #1e3a8a);
  border-bottom-color: var(--color-blue-400, #60a5fa);
}

/* === Task List === */
.tiptap ul[data-type="taskList"] {
  list-style: none;
  padding-left: 0;
  margin-left: 0;
}

.tiptap ul[data-type="taskList"] li {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  margin-bottom: 4px;
}

.tiptap ul[data-type="taskList"] li > label {
  flex-shrink: 0;
  margin-top: 3px;
  user-select: none;
}

.tiptap ul[data-type="taskList"] li > label input[type="checkbox"] {
  appearance: none;
  -webkit-appearance: none;
  width: 18px;
  height: 18px;
  border: 2px solid #d1d5db;
  border-radius: 4px;
  cursor: pointer;
  position: relative;
  transition: all 0.15s ease;
  background: transparent;
}

.tiptap ul[data-type="taskList"] li > label input[type="checkbox"]:checked {
  background: #111;
  border-color: #111;
}


 .dark .tiptap ul[data-type="taskList"] li > label input[type="checkbox"] { 
    border-color: #52525b;
  }
 .dark .tiptap ul[data-type="taskList"] li > label input[type="checkbox"]:checked { 
    background: #f4f4f5;
    border-color: #f4f4f5;
  }

.tiptap ul[data-type="taskList"] li > label input[type="checkbox"]:checked::after {
  content: '';
  position: absolute;
  left: 5px;
  top: 1px;
  width: 5px;
  height: 10px;
  border: solid #fff;
  border-width: 0 2px 2px 0;
  transform: rotate(45deg);
}


 .dark .tiptap ul[data-type="taskList"] li > label input[type="checkbox"]:checked::after { 
    border-color: #111;
  }

.tiptap ul[data-type="taskList"] li > div {
  flex: 1;
  min-width: 0;
}

.tiptap ul[data-type="taskList"] li > div > p {
  margin-top: 0;
  margin-bottom: 0;
  line-height: inherit;
}

.tiptap ul[data-type="taskList"] li[data-checked="true"] > div > p {
  text-decoration: line-through;
  opacity: 0.5;
}

/* === Link === */
.tiptap a {
  color: #2563eb;
  text-decoration: underline;
  text-underline-offset: 3px;
  cursor: pointer;
  transition: color 0.15s;
}
.tiptap a:hover {
  color: #1d4ed8;
}


 .dark .tiptap a { 
    color: #60a5fa;
  }
 .dark .tiptap a:hover { 
    color: #93bbfd;
  }

/* === Highlight === */
.tiptap mark {
  background-color: #fef08a;
  border-radius: 2px;
  padding: 1px 2px;
}


 .dark .tiptap mark { 
    background-color: #854d0e;
    color: #fef9c3;
  }

/* === Bubble Menu === */
.bubble-menu {
  position: fixed;
  z-index: 9999;
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 4px 6px;
  background: #fff;
  border: 1px solid #e5e7eb;
  border-radius: 10px;
  box-shadow: 0 4px 12px rgba(0,0,0,0.08), 0 1px 3px rgba(0,0,0,0.04);
  transform: translateX(-50%);
  white-space: nowrap;
}


 .dark .bubble-menu { 
    background: #1e1e1e;
    border-color: #333;
    box-shadow: 0 4px 12px rgba(0,0,0,0.4);
  }

.bubble-menu button {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  border: none;
  background: transparent;
  border-radius: 6px;
  cursor: pointer;
  color: #6b7280;
  transition: all 0.12s;
}

.bubble-menu button:hover {
  background: #f3f4f6;
  color: #111;
}

.bubble-menu button.is-active {
  background: #111;
  color: #fff;
}


 .dark .bubble-menu button { 
    color: #a1a1aa;
  }
 .dark .bubble-menu button:hover { 
    background: #2a2a2a;
    color: #f4f4f5;
  }
 .dark .bubble-menu button.is-active { 
    background: #f4f4f5;
    color: #111;
  }

.bubble-divider {
  width: 1px;
  height: 18px;
  background: #e5e7eb;
  margin: 0 3px;
}


 .dark .bubble-divider { 
    background: #3a3a3a;
  }

/* Bubble transition */
.bubble-enter-active {
  transition: opacity 0.12s ease, transform 0.12s ease;
}
.bubble-leave-active {
  transition: opacity 0.08s ease;
}
.bubble-enter-from {
  opacity: 0;
  transform: translateX(-50%) translateY(4px) !important;
}
.bubble-leave-to {
  opacity: 0;
}

/* === Code Block (Syntax Highlighting) === */
.tiptap pre {
  background: #f8f9fa !important;
  border: 1px solid #e5e7eb;
  border-radius: 8px;
  padding: 16px 20px;
  font-family: 'SF Mono', 'Fira Code', 'JetBrains Mono', 'Menlo', monospace;
  font-size: 13px;
  line-height: 1.6;
  overflow-x: auto;
  position: relative;
  color: #24292e !important;
}
.tiptap pre code { background: none !important; padding: 0; font-size: inherit; color: inherit !important; }

/* Light theme syntax colors */
.tiptap pre .hljs-comment,
.tiptap pre .hljs-quote { color: #6a737d; font-style: italic; }
.tiptap pre .hljs-keyword,
.tiptap pre .hljs-selector-tag,
.tiptap pre .hljs-addition { color: #d73a49; }
.tiptap pre .hljs-number,
.tiptap pre .hljs-literal,
.tiptap pre .hljs-symbol,
.tiptap pre .hljs-bullet { color: #005cc5; }
.tiptap pre .hljs-string,
.tiptap pre .hljs-doctag,
.tiptap pre .hljs-regexp { color: #032f62; }
.tiptap pre .hljs-title,
.tiptap pre .hljs-section,
.tiptap pre .hljs-built_in { color: #6f42c1; }
.tiptap pre .hljs-attr,
.tiptap pre .hljs-attribute { color: #005cc5; }
.tiptap pre .hljs-variable,
.tiptap pre .hljs-template-variable { color: #e36209; }
.tiptap pre .hljs-type,
.tiptap pre .hljs-name { color: #22863a; }
.tiptap pre .hljs-tag { color: #22863a; }
.tiptap pre .hljs-meta { color: #6a737d; }

.dark .tiptap pre {
  background: #1e1e1e !important;
  border-color: #2c2c2e;
  color: #e4e4e7 !important;
}
.dark .tiptap pre code { color: #e4e4e7; }
.dark .tiptap pre .hljs-comment,
.dark .tiptap pre .hljs-quote { color: #636366; }
.dark .tiptap pre .hljs-keyword,
.dark .tiptap pre .hljs-selector-tag,
.dark .tiptap pre .hljs-addition { color: #ff7b72; }
.dark .tiptap pre .hljs-number,
.dark .tiptap pre .hljs-literal,
.dark .tiptap pre .hljs-symbol,
.dark .tiptap pre .hljs-bullet { color: #79c0ff; }
.dark .tiptap pre .hljs-string,
.dark .tiptap pre .hljs-doctag,
.dark .tiptap pre .hljs-regexp { color: #a5d6ff; }
.dark .tiptap pre .hljs-title,
.dark .tiptap pre .hljs-section,
.dark .tiptap pre .hljs-built_in { color: #d2a8ff; }
.dark .tiptap pre .hljs-attr,
.dark .tiptap pre .hljs-attribute { color: #79c0ff; }
.dark .tiptap pre .hljs-variable,
.dark .tiptap pre .hljs-template-variable { color: #ffa657; }
.dark .tiptap pre .hljs-type,
.dark .tiptap pre .hljs-name { color: #7ee787; }
.dark .tiptap pre .hljs-tag { color: #7ee787; }
.dark .tiptap pre .hljs-meta { color: #636366; }

/* === Table === */
.tiptap table {
  border-collapse: collapse !important;
  table-layout: fixed !important;
  width: 100%;
  margin: 1em 0;
  border: 1px solid #e5e7eb !important;
}

.tiptap table td,
.tiptap table th {
  min-width: 80px;
  padding: 8px 12px !important;
  border: 1px solid #e5e7eb !important;
  vertical-align: top;
  position: relative;
  text-align: left;
  font-size: 14px;
}

.tiptap table th {
  background: #f3f4f6;
  font-weight: 600;
  font-size: 13px;
  color: #6b7280;
}

.tiptap table td > p,
.tiptap table th > p {
  margin: 0;
}

/* Selected cell */
.tiptap table .selectedCell {
  background: rgba(59, 130, 246, 0.08);
}

/* Resize handle */
.tiptap .tableWrapper {
  overflow-x: auto;
  margin: 1em 0;
}

.tiptap .column-resize-handle {
  position: absolute;
  right: -2px;
  top: 0;
  bottom: 0;
  width: 4px;
  background: #3b82f6;
  cursor: col-resize;
  z-index: 20;
}

.tiptap.resize-cursor {
  cursor: col-resize;
}

.dark .tiptap table {
  border-color: #333 !important;
}
.dark .tiptap table td,
.dark .tiptap table th {
  border-color: #333 !important;
}
.dark .tiptap table th {
  background: #1e1e1e !important;
  color: #a1a1aa !important;
}
.dark .tiptap table .selectedCell {
  background: rgba(96, 165, 250, 0.1);
}

/* === Blockquote === */
.tiptap blockquote {
  border-left: 3px solid #d1d5db;
  padding-left: 16px;
  color: #6b7280;
  font-style: italic;
}


 .dark .tiptap blockquote { 
    border-left-color: #4a4a4a;
    color: #a1a1aa;
  }

/* === Table Controls (Confluence-style) === */
.tc-col-handle, .tc-row-handle, .tc-corner-handle {
  z-index: 40;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border: 1px solid #e0e0e0;
  border-radius: 4px;
  background: #fff;
  color: #aaa;
  cursor: pointer;
  transition: all 0.1s;
  opacity: 0.6;
  padding: 0;
}
.tc-col-handle:hover, .tc-row-handle:hover, .tc-corner-handle:hover {
  background: #3b82f6;
  border-color: #3b82f6;
  color: #fff;
  opacity: 1;
}

 .dark .tc-col-handle, .dark .tc-row-handle, .dark .tc-corner-handle { 
    background: #252525;
    border-color: #3a3a3a;
    color: #666;
  }
 .dark .tc-col-handle:hover, .dark .tc-row-handle:hover, .dark .tc-corner-handle:hover { 
    background: #3b82f6;
    border-color: #3b82f6;
    color: #fff;
  }

/* Add buttons */
.tc-add-btn {
  z-index: 40;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border: 2px dashed #d1d5db;
  border-radius: 6px;
  background: transparent;
  color: #9ca3af;
  cursor: pointer;
  transition: all 0.15s;
  padding: 0;
}
.tc-add-btn:hover {
  border-color: #3b82f6;
  color: #3b82f6;
  background: rgba(59,130,246,0.05);
}

 .dark .tc-add-btn { 
    border-color: #3a3a3a;
    color: #636366;
  }
 .dark .tc-add-btn:hover { 
    border-color: #60a5fa;
    color: #60a5fa;
    background: rgba(96,165,250,0.08);
  }

/* Context Menu */
.tc-ctx-menu {
  position: absolute;
  z-index: 10000;
  min-width: 200px;
  background: #fff;
  border: 1px solid #e5e7eb;
  border-radius: 10px;
  box-shadow: 0 8px 24px rgba(0,0,0,0.1), 0 2px 6px rgba(0,0,0,0.05);
  padding: 4px;
  overflow: hidden;
}

 .dark .tc-ctx-menu { 
    background: #1e1e1e;
    border-color: #333;
    box-shadow: 0 8px 24px rgba(0,0,0,0.5);
  }
.tc-ctx-menu button {
  display: block;
  width: 100%;
  text-align: left;
  padding: 7px 12px;
  border: none;
  background: transparent;
  border-radius: 6px;
  cursor: pointer;
  font-size: 13px;
  color: #374151;
  transition: background 0.08s;
}
.tc-ctx-menu button:hover {
  background: #f3f4f6;
}
.tc-ctx-menu button.ctx-danger {
  color: #dc2626;
}
.tc-ctx-menu button.ctx-danger:hover {
  background: #fee2e2;
}

 .dark .tc-ctx-menu button { 
    color: #d4d4d8;
  }
 .dark .tc-ctx-menu button:hover { 
    background: #2a2a2a;
  }
 .dark .tc-ctx-menu button.ctx-danger { 
    color: #f87171;
  }
 .dark .tc-ctx-menu button.ctx-danger:hover { 
    background: #450a0a;
  }
.ctx-sep {
  height: 1px;
  background: #e5e7eb;
  margin: 3px 8px;
}

 .dark .ctx-sep { 
    background: #333;
  }

/* Cell selection highlight (prosemirror-tables applies this class) */
.prose td.selectedCell,
.prose th.selectedCell {
  background: rgba(139, 92, 246, 0.12) !important;
  outline: 2px solid rgba(139, 92, 246, 0.4);
  outline-offset: -2px;
}
.dark .prose td.selectedCell,
.dark .prose th.selectedCell {
  background: rgba(139, 92, 246, 0.2) !important;
  outline: 2px solid rgba(139, 92, 246, 0.5);
  outline-offset: -2px;
}

.prose blockquote {
  border-left-color: #9ca3af !important;
}

/* === Global Dark Theme Overrides for Text === */
.dark .prose.dark\:prose-invert {
  --tw-prose-body: #e4e4e7 !important;
  --tw-prose-headings: #f4f4f5 !important;
  --tw-prose-lead: #d4d4d8 !important;
  --tw-prose-bold: #ffffff !important;
  --tw-prose-counters: #a1a1aa !important;
  --tw-prose-bullets: #71717a !important;
  --tw-prose-hr: #3f3f46 !important;
  --tw-prose-quotes: #e4e4e7 !important;
  --tw-prose-quote-borders: #52525b !important;
  --tw-prose-captions: #a1a1aa !important;
  --tw-prose-th-borders: #52525b !important;
  --tw-prose-td-borders: #3f3f46 !important;
  color: #e4e4e7 !important;
}
.dark .prose.dark\:prose-invert p,
.dark .prose.dark\:prose-invert li {
  color: #e4e4e7 !important;
}

/* === Fix for Cursor Bouncing/Jumping at Line Ends (macOS IME & WebKit bug) === */
.ProseMirror {
  word-break: break-word !important;
  overflow-wrap: break-word !important;
  white-space: break-spaces !important; 
}
.ProseMirror * {
  max-width: 100%;
}


.prose a[href^="synabit://note/"] {
  background-color: rgba(168, 85, 247, 0.1);
  color: #a855f7;
  padding: 2px 6px;
  border-radius: 6px;
  text-decoration: none;
  font-weight: 700;
  transition: all 0.2s;
  cursor: pointer;
}

.prose a[href^="synabit://note/"]:hover {
  background-color: rgba(168, 85, 247, 0.2);
}

.dark .prose a[href^="synabit://note/"] {
  background-color: rgba(168, 85, 247, 0.2);
  color: #c084fc;
}

.dark .prose a[href^="synabit://note/"]:hover {
  background-color: rgba(168, 85, 247, 0.3);
}

.prose a[href^="synabit://event/"] {
  background-color: rgba(225, 29, 72, 0.1);
  color: #e11d48;
  padding: 2px 6px;
  border-radius: 6px;
  text-decoration: none;
  font-weight: 700;
  transition: all 0.2s;
  cursor: pointer;
}

.prose a[href^="synabit://event/"]:hover {
  background-color: rgba(225, 29, 72, 0.2);
}

.dark .prose a[href^="synabit://event/"] {
  background-color: rgba(225, 29, 72, 0.2);
  color: #fb7185;
}

.dark .prose a[href^="synabit://event/"]:hover {
  background-color: rgba(225, 29, 72, 0.3);
}

/* === Notion/Obsidian Style Spacing for Prose === */
.prose h1 {
  font-size: 1.875rem !important; /* Tailwind text-3xl */
  margin-top: 1.5em !important;
  margin-bottom: 0.5em !important;
  font-weight: 700 !important;
  line-height: 1.3 !important;
}
.prose h2 {
  font-size: 1.5rem !important; /* Tailwind text-2xl */
  margin-top: 1.25em !important;
  margin-bottom: 0.5em !important;
  font-weight: 600 !important;
  line-height: 1.4 !important;
}
.prose h3 {
  font-size: 1.25rem !important; /* Tailwind text-xl */
  margin-top: 1em !important;
  margin-bottom: 0.25em !important;
  font-weight: 600 !important;
  line-height: 1.5 !important;
}
.prose p {
  margin-top: 0.25em !important;
  margin-bottom: 0.25em !important;
  line-height: 1.5 !important;
}
.prose ul, .prose ol {
  margin-top: 0.25em !important;
  margin-bottom: 0.25em !important;
}
.prose li {
  margin-top: 0.1em !important;
  margin-bottom: 0.1em !important;
}
.prose li p {
  margin-top: 0 !important;
  margin-bottom: 0 !important;
}
.prose hr {
  margin-top: 1.5em !important;
  margin-bottom: 1.5em !important;
  border-top-color: #e5e7eb !important; /* Tailwind gray-200 */
}
.dark .prose hr {
  border-top-color: #3f3f46 !important; /* Tailwind zinc-700 */
}
.prose blockquote p:first-of-type::before,
.prose blockquote p:last-of-type::after {
  content: none !important;
}

/* === Nested List Styles === */
/* Alpha Style */
.list-style-alpha .prose ol ol { list-style-type: lower-alpha !important; }
.list-style-alpha .prose ol ol ol { list-style-type: lower-roman !important; }

/* Nested Numbered Style (1.1, 1.2) */
.list-style-nested .prose ol {
  counter-reset: item;
  list-style-type: none !important;
}
.list-style-nested .prose ol > li {
  counter-increment: item;
  position: relative;
}
.list-style-nested .prose ol > li::before {
  content: counters(item, ".") ". ";
  position: absolute;
  right: 100%;
  padding-right: 0.5rem;
  color: var(--tw-prose-counters);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
  font-weight: 400;
}

/* ─── PDF Embed Card ───────────────────────────── */
.pdf-embed-card {
  display: flex;
  align-items: center;
  padding: 12px 16px;
  margin: 8px 0;
  border-radius: 12px;
  border: 1px solid #e5e7eb;
  background: linear-gradient(135deg, #fff5f5 0%, #fff 100%);
  cursor: pointer;
  transition: all 0.15s ease;
}
.pdf-embed-card:hover {
  border-color: #fca5a5;
  box-shadow: 0 2px 8px rgba(239, 68, 68, 0.08);
}
:is(.dark) .pdf-embed-card {
  border-color: #333;
  background: linear-gradient(135deg, #1a1212 0%, #1e1e1e 100%);
}
:is(.dark) .pdf-embed-card:hover {
  border-color: #555;
}
.pdf-embed-inner {
  display: flex;
  align-items: center;
  gap: 12px;
  width: 100%;
}
.pdf-embed-icon {
  font-size: 28px;
  flex-shrink: 0;
}
.pdf-embed-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.pdf-embed-title {
  font-size: 13px;
  font-weight: 600;
  color: #111827;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
:is(.dark) .pdf-embed-title {
  color: #f4f4f5;
}
.pdf-embed-path {
  font-size: 11px;
  color: #9ca3af;
  font-family: 'JetBrains Mono', monospace;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

</style>

