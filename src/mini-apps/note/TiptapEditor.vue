<script setup lang="ts">
import { watch, onBeforeUnmount, onMounted, ref } from 'vue';
import { useEditor, EditorContent } from '@tiptap/vue-3';
import { VueRenderer, VueNodeViewRenderer } from '@tiptap/vue-3';
import StarterKit from '@tiptap/starter-kit';
import Placeholder from '@tiptap/extension-placeholder';
import ImageResize from 'tiptap-extension-resize-image';
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
import 'katex/dist/katex.min.css';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { Extension } from '@tiptap/core';
import { PluginKey } from '@tiptap/pm/state';
import Suggestion from '@tiptap/suggestion';
import tippy, { type Instance as TippyInstance } from 'tippy.js';
import SlashCommandMenu from './SlashCommandMenu.vue';
import type { SlashCommandItem } from './SlashCommandMenu.vue';
import NoteMentionMenu from './NoteMentionMenu.vue';
import CodeBlockComponent from './CodeBlockComponent.vue';
import {
  Heading1, Heading2, Heading3,
  List, ListOrdered, ListChecks,
  Quote, Code2, Minus, Type, Table2,
  Image as ImageIcon, Sigma, Video as VideoIcon,
  Music as MusicIcon
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

const { nestedNumberListStyle } = useSettings();

const lowlight = createLowlight(common);

const props = defineProps<{
  modelValue: string;
  vaultPath: string;
  notes?: any[];
}>();

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
  (e: 'open-internal-note', noteId: string): void;
}>();

// --- Asset path helpers ---
const injectLocalAssets = (md: string) => {
   if (!props.vaultPath) return md;
   let processed = md;
   
   // Tiptap-markdown sometimes serializes images with custom attributes as raw HTML <img> tags.
   // This causes markdown-it to crash or swallow subsequent text on reload.
   // We force convert them back to standard Markdown before processing.
   processed = processed.replace(/<img\s+[^>]*src="([^"]+)"[^>]*>/g, (m, src) => {
      const altMatch = m.match(/alt="([^"]*)"/);
      const alt = altMatch ? altMatch[1] : 'Image';
      return `![${alt}](${src})`;
   });
   
   processed = processed.replace(/<video\s+([^>]*)src="assets\/([^"]+)"([^>]*)>/g, (m, before, filename, after) => {
      const sep = props.vaultPath.includes('\\') ? '\\' : '/';
      const decodedName = decodeURIComponent(filename);
      const absPath = `${props.vaultPath}${sep}assets${sep}${decodedName}`;
      const assetUrl = convertFileSrc(absPath); 
      return `<video ${before}src="${assetUrl}"${after}>`;
   });
   
   processed = processed.replace(/<audio\s+([^>]*)src="assets\/([^"]+)"([^>]*)>/g, (m, before, filename, after) => {
      const sep = props.vaultPath.includes('\\') ? '\\' : '/';
      const decodedName = decodeURIComponent(filename);
      const absPath = `${props.vaultPath}${sep}assets${sep}${decodedName}`;
      const assetUrl = convertFileSrc(absPath); 
      return `<audio ${before}src="${assetUrl}"${after}>`;
   });
   
   processed = processed.replace(/\[([^\]]*)\]\(synabit:\/\/note\/([^)]+)\)/g, (match, label, uri) => {
      const decoded = decodeURIComponent(uri);
      return `[${label}](synabit://note/${encodeURIComponent(decoded)})`;
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
   
   // Also handle HTML tags when saving, just in case tiptap-markdown output an <img> tag during edit
   processed = processed.replace(/<img\s+[^>]*src="([^"]+)"[^>]*>/g, (m, src) => {
      const altMatch = m.match(/alt="([^"]*)"/);
      const alt = altMatch ? altMatch[1] : 'Image';
      return `![${alt}](${src})`;
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
  
  editor.value.commands.setVideo({ src: url });
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
  
  editor.value.commands.setAudio({ src: url });
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
  
  if (empty || from === to) {
    showBubble.value = false;
    return;
  }
  
  const view = editor.value.view;
  const start = view.coordsAtPos(from);
  const end = view.coordsAtPos(to);
  
  // Calculate center in viewport coordinates (fixed positioning)
  let centerX = (start.left + end.right) / 2;
  let topY = Math.min(start.top, end.top) - BUBBLE_MENU_HEIGHT - 8;
  
  // Clamp horizontal: keep fully visible within viewport
  const halfW = BUBBLE_MENU_WIDTH / 2;
  const vw = window.innerWidth;
  centerX = Math.max(halfW + BUBBLE_PADDING, Math.min(centerX, vw - halfW - BUBBLE_PADDING));
  
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

const updateTableControls = () => {
  if (!editor.value) { isInTable.value = false; return; }
  const inTable = editor.value.isActive('table');
  isInTable.value = inTable;
  if (!inTable) { activeTableEl.value = null; return; }
  
  canMerge.value = editor.value.can().mergeCells();
  canSplit.value = editor.value.can().splitCell();

  // Find the actual table DOM element
  const { from } = editor.value.state.selection;
  const domAtPos = editor.value.view.domAtPos(from);
  let el = domAtPos.node as HTMLElement;
  while (el && el.tagName !== 'TABLE') {
    el = el.parentElement as HTMLElement;
  }
  if (!el) return;
  activeTableEl.value = el;

  const rect = el.getBoundingClientRect();
  tableRect.value = { top: rect.top, left: rect.left, width: rect.width, height: rect.height, bottom: rect.bottom, right: rect.right };

  // Read column positions from first row
  const firstRow = el.querySelector('tr');
  if (firstRow) {
    const cells = firstRow.querySelectorAll('td, th');
    colPositions.value = Array.from(cells).map(c => {
      const cr = c.getBoundingClientRect();
      return { left: cr.left, width: cr.width };
    });
  }

  // Read row positions
  const rows = el.querySelectorAll('tr');
  rowPositions.value = Array.from(rows).map(r => {
    const rr = r.getBoundingClientRect();
    return { top: rr.top, height: rr.height };
  });

  // Determine active row and col for showing specific handles
  let cell = el;
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
  ctxMenuPos.value = { top: e.clientY, left: e.clientX };
  showCtxMenu.value = true;
};

const closeCtxMenu = () => { showCtxMenu.value = false; };

const ctxAction = (action: string) => {
  if (!editor.value) return;
  const chain = editor.value.chain().focus();
  switch (action) {
    case 'addRowAbove': chain.addRowBefore().run(); break;
    case 'addRowBelow': chain.addRowAfter().run(); break;
    case 'deleteRow': chain.deleteRow().run(); break;
    case 'addColLeft': chain.addColumnBefore().run(); break;
    case 'addColRight': chain.addColumnAfter().run(); break;
    case 'deleteCol': chain.deleteColumn().run(); break;
    case 'mergeCells': chain.mergeCells().run(); break;
    case 'splitCell': chain.splitCell().run(); break;
    case 'toggleHeaderRow': chain.toggleHeaderRow().run(); break;
    case 'toggleHeaderCol': chain.toggleHeaderColumn().run(); break;
    case 'deleteTable': chain.deleteTable().run(); break;
  }
  closeCtxMenu();
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

const selectColumn = (colIdx: number) => {
  if (!editor.value || !activeTableEl.value) return;
  // Select all cells in this column
  const rows = activeTableEl.value.querySelectorAll('tr');
  if (rows.length > 0) {
    const firstCell = rows[0].querySelectorAll('td, th')[colIdx];
    if (firstCell) {
      const pos = editor.value.view.posAtDOM(firstCell, 0);
      editor.value.chain().setTextSelection(pos).focus().run();
    }
  }
};

const selectRow = (rowIdx: number) => {
  if (!editor.value || !activeTableEl.value) return;
  const row = activeTableEl.value.querySelectorAll('tr')[rowIdx];
  if (row) {
    const firstCell = row.querySelector('td, th');
    if (firstCell) {
      const pos = editor.value.view.posAtDOM(firstCell, 0);
      editor.value.chain().setTextSelection(pos).focus().run();
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

// --- Editor ---
const editor = useEditor({
  content: injectLocalAssets(props.modelValue),
  extensions: [
    StarterKit.configure({
      codeBlock: false, // replaced by CodeBlockLowlight
    }),
    TabIndentExtension,
    Markdown,
    ImageResize,
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
    VideoExtension,
    AudioExtension,
    Table.configure({
      resizable: true,
    }),
    TableRow,
    TableCell,
    TableHeader,
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
              const basename = props.id.split('/').pop().split('\\').pop();
              editor
                .chain()
                .focus()
                .deleteRange(range)
                .insertContent({
                  type: 'text',
                  marks: [
                    {
                      type: 'link',
                      attrs: { href: `synabit://note/${encodeURIComponent(basename)}` }
                    }
                  ],
                  text: props.title
                })
                .insertContent(' ')
                .run();
            },
            items: ({ query }) => {
              if (!props.notes || props.notes.length === 0) return [];
              const lowerQuery = query.toLowerCase();
              return props.notes
                .filter(n => n.title.toLowerCase().includes(lowerQuery) || n.summary.toLowerCase().includes(lowerQuery))
                .slice(0, 5)
                .map(n => ({
                  id: n.id,
                  title: n.title,
                  summary: n.summary
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
  ],
  onUpdate: ({ editor: ed }) => {
    const md = (ed.storage as any).markdown.getMarkdown();
    emit('update:modelValue', stripLocalAssets(md));
    // Update bubble menu position on content change
    setTimeout(updateBubbleMenu, 10);
  },
  onSelectionUpdate: () => {
    setTimeout(updateBubbleMenu, 10);
    setTimeout(updateTableControls, 10);
  },
  onBlur: () => {
    setTimeout(() => { showBubble.value = false; }, 200);
  },
  editorProps: {
    transformPastedHTML(html) {
      return html
        .replace(/color\s*:\s*[^;"]+;?/gi, '')
        .replace(/background-color\s*:\s*[^;"]+;?/gi, '')
        .replace(/color="[^"]*"/gi, '')
        .replace(/bgcolor="[^"]*"/gi, '');
    },
    attributes: {
      class: 'prose focus:outline-none dark:prose-invert max-w-none w-full min-h-[500px] break-words whitespace-pre-wrap',
    },
    handleClick: (_view, _pos, event) => {
      if (event.target instanceof HTMLElement) {
          const anchor = event.target.closest('a');
          if (anchor) {
              const href = anchor.getAttribute('href');
              if (href?.startsWith('synabit://note/')) {
                  if (event.metaKey || event.ctrlKey) {
                      const noteId = decodeURIComponent(href.replace('synabit://note/', ''));
                      emit('open-internal-note', noteId);
                      event.preventDefault();
                      return true;
                  }
              }
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
const onDocClick = (e: MouseEvent) => {
  const target = e.target as HTMLElement;
  if (!target.closest('.tc-ctx-menu')) {
    closeCtxMenu();
  }
};

// Update table controls on scroll (since they use fixed positioning)
const onEditorScroll = () => {
  if (isInTable.value) {
    updateTableControls();
  }
};

onMounted(() => {
  document.addEventListener('click', onDocClick);
  // Find the scrollable editor container and listen for scroll
  const wrapper = document.querySelector('.tiptap-wrapper')?.closest('.overflow-y-auto');
  wrapper?.addEventListener('scroll', onEditorScroll);
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
  document.removeEventListener('click', onDocClick);
  const wrapper = document.querySelector('.tiptap-wrapper')?.closest('.overflow-y-auto');
  wrapper?.removeEventListener('scroll', onEditorScroll);
  if (editor.value) {
    editor.value.destroy();
  }
});
</script>

<template>
  <div class="tiptap-wrapper w-full relative overflow-x-hidden">
    <!-- Floating Toolbar -->
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
          title="Justify"
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

    <!-- Table Controls: + buttons, row/col handles -->
    <template v-if="isInTable && activeTableEl">
      <!-- Column handles (top of each column) -->
      <button
        v-for="(col, i) in colPositions" :key="'ch-'+i"
        v-show="i === activeColIdx"
        class="tc-col-handle"
        :style="{ position: 'fixed', top: (tableRect.top - 20) + 'px', left: (col.left + col.width / 2 - 10) + 'px' }"
        @click.prevent="(e: MouseEvent) => { selectColumn(i); openContextMenu(e); }"
      >
        <GripVertical class="w-3 h-3 rotate-90" />
      </button>

      <!-- Row handles (left of each row) -->
      <button
        v-for="(row, i) in rowPositions" :key="'rh-'+i"
        v-show="i === activeRowIdx"
        class="tc-row-handle"
        :style="{ position: 'fixed', top: (row.top + row.height / 2 - 10) + 'px', left: (tableRect.left - 22) + 'px' }"
        @click.prevent="(e: MouseEvent) => { selectRow(i); openContextMenu(e); }"
      >
        <GripVertical class="w-3 h-3" />
      </button>

      <!-- Corner handle (select whole table) -->
      <button
        class="tc-corner-handle"
        :style="{ position: 'fixed', top: (tableRect.top - 22) + 'px', left: (tableRect.left - 24) + 'px' }"
        @click.prevent="(e: MouseEvent) => { editor?.chain().focus().run(); openContextMenu(e); }"
      >
        <svg width="10" height="10" viewBox="0 0 10 10"><rect x="0" y="0" width="4" height="4" fill="currentColor" rx="0.5"/><rect x="6" y="0" width="4" height="4" fill="currentColor" rx="0.5"/><rect x="0" y="6" width="4" height="4" fill="currentColor" rx="0.5"/><rect x="6" y="6" width="4" height="4" fill="currentColor" rx="0.5"/></svg>
      </button>

      <!-- Add row button (bottom) -->
      <button
        class="tc-add-btn tc-add-row"
        :style="{ position: 'fixed', top: (tableRect.bottom + 2) + 'px', left: (tableRect.left + tableRect.width / 2 - 14) + 'px' }"
        @mousedown.prevent="addRowAtBottom"
        title="Add row"
      >
        <Plus class="w-3.5 h-3.5" />
      </button>

      <!-- Add column button (right) -->
      <button
        class="tc-add-btn tc-add-col"
        :style="{ position: 'fixed', top: (tableRect.top + tableRect.height / 2 - 14) + 'px', left: (tableRect.right + 2) + 'px' }"
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
        :style="{ top: ctxMenuPos.top + 'px', left: ctxMenuPos.left + 'px' }"
        @mousedown.prevent
      >
        <button @click="ctxAction('addRowAbove')">Add row above</button>
        <button @click="ctxAction('addRowBelow')">Add row below</button>
        <button @click="ctxAction('deleteRow')" class="ctx-danger">Delete row</button>
        <div class="ctx-sep" />
        <button @click="ctxAction('addColLeft')">Add column left</button>
        <button @click="ctxAction('addColRight')">Add column right</button>
        <button @click="ctxAction('deleteCol')" class="ctx-danger">Delete column</button>
        <div class="ctx-sep" />
        <button v-if="canMerge" @click="ctxAction('mergeCells')">Merge cells</button>
        <button v-if="canSplit" @click="ctxAction('splitCell')">Split cell</button>
        <button @click="ctxAction('toggleHeaderRow')">Toggle header row</button>
        <button @click="ctxAction('toggleHeaderCol')">Toggle header column</button>
        <div class="ctx-sep" />
        <button @click="ctxAction('deleteTable')" class="ctx-danger">Delete table</button>
      </div>
    </Transition>

    <div :class="{
      'list-style-decimal': nestedNumberListStyle === 'decimal',
      'list-style-alpha': nestedNumberListStyle === 'alpha',
      'list-style-nested': nestedNumberListStyle === 'nested'
    }" class="editor-wrapper h-full w-full">
      <editor-content :editor="editor" @contextmenu="(e: MouseEvent) => { if (editor?.isActive('table')) openContextMenu(e); }" />
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
.tiptap img {
  border-radius: 0.5rem;
  max-width: 100%;
}

/* === Prose overrides === */
.prose {
  --tw-prose-body: var(--color-text-light);
  --tw-prose-headings: var(--color-text);
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
  text-transform: uppercase;
  letter-spacing: 0.02em;
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
  position: fixed;
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

</style>
