import type { Component } from 'vue';
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
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { readFile } from '@tauri-apps/plugin-fs';
import type { GalleryImage } from '../../extensions/ImageGallery';
import { logger } from '../../../../utils/logger';

export interface SlashCommandItem {
  title: string;
  description: string;
  icon: Component;
  command: (props: { editor: any; range: any }) => void;
}

export interface SlashCommandDeps {
  vaultPath: string;
  videoModal: { value: { show: boolean; url: string } };
  audioModal: { value: { show: boolean; url: string } };
  locationModal: { value: any };
  routeModal: { value: any };
  emojiPicker: { value: any };
  whiteboardPickerModal: { value: any };
  embedPickerModal: { value: boolean };
  pdfModal: { value: { show: boolean } };
}

export function createSlashCommandItems(deps: SlashCommandDeps): SlashCommandItem[] {
  const { vaultPath, videoModal, audioModal, locationModal, routeModal, emojiPicker, whiteboardPickerModal, embedPickerModal, pdfModal } = deps;

  return [
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

          if (selectedPath && !Array.isArray(selectedPath) && vaultPath) {
            const pathStr = selectedPath as string;
            const match = pathStr.match(/[\\/]([^\\/]+)$/);
            const filename = match ? match[1] : `image-${Date.now()}.png`;
            const buffer = await readFile(pathStr);

            const relativePath = await invoke<string>('save_asset', {
              vaultPath: vaultPath,
              filename: filename,
              bytes: Array.from(buffer)
            });
            const sep = vaultPath.includes('\\') ? '\\' : '/';
            const absPath = `${vaultPath}${sep}${relativePath}`;
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

          if (selectedPaths && Array.isArray(selectedPaths) && vaultPath) {
            const newImages: GalleryImage[] = [];
            for (const pathStr of selectedPaths) {
              const match = pathStr.match(/[\\/]([^\\/]+)$/);
              const filename = match ? match[1] : `image-${Date.now()}.png`;
              const buffer = await readFile(pathStr);

              const relativePath = await invoke<string>('save_asset', {
                vaultPath: vaultPath,
                filename: filename,
                bytes: Array.from(buffer)
              });
              const sep = vaultPath.includes('\\') ? '\\' : '/';
              const absPath = `${vaultPath}${sep}${relativePath}`;
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
          const boards = await invoke<any[]>('scan_whiteboards', { vaultPath: vaultPath });
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
}
