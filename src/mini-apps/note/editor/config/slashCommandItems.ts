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
  Table as SearchIcon,
  ChevronRight as ChevronRightIcon,
  LayoutTemplate as TemplateIcon
} from 'lucide-vue-next';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { readFile } from '@tauri-apps/plugin-fs';
import type { GalleryImage } from '../../extensions/ImageGallery';
import { logger } from '../../../../utils/logger';
import { i18n } from '../../../../i18n';

export interface SlashCommandItem {
  /**
   * English name. Kept alongside the key so `/heading` still finds the item
   * when the interface is in another language — muscle memory and every
   * tutorial are in English — and as a stable `v-for` key.
   */
  title: string;
  /** i18n keys, translated where the menu renders. */
  titleKey: string;
  descriptionKey: string;
  icon: Component;
  command: (props: { editor: any; range: any }) => void;
  /**
   * A power tool: left out of the menu in simple mode. Only the menu — a note
   * that already holds a query or an equation still renders it, because
   * simple mode hides and never converts. See `shared/simpleMode.ts`.
   */
  advanced?: boolean;
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
  /**
   * Offer "Template", and call this when it is chosen. Only Notes passes it:
   * the picker lists notes, and the other apps that mount this editor (a
   * task's description, a Things node) have no business growing one.
   */
  onTemplate?: () => void;
}

export function createSlashCommandItems(deps: SlashCommandDeps): SlashCommandItem[] {
  const { vaultPath, videoModal, audioModal, locationModal, routeModal, emojiPicker, whiteboardPickerModal, embedPickerModal, pdfModal, onTemplate } = deps;

  const items: SlashCommandItem[] = [
    {
      title: 'Text',
      titleKey: 'note.slash.text.title',
      descriptionKey: 'note.slash.text.desc',
      icon: Type,
      command: ({ editor, range }: any) => {
        editor.chain().focus().deleteRange(range).setParagraph().run();
      },
    },
    {
      title: 'Heading 1',
      titleKey: 'note.slash.heading1.title',
      descriptionKey: 'note.slash.heading1.desc',
      icon: Heading1,
      command: ({ editor, range }: any) => {
        editor.chain().focus().deleteRange(range).setHeading({ level: 1 }).run();
      },
    },
    {
      title: 'Heading 2',
      titleKey: 'note.slash.heading2.title',
      descriptionKey: 'note.slash.heading2.desc',
      icon: Heading2,
      command: ({ editor, range }: any) => {
        editor.chain().focus().deleteRange(range).setHeading({ level: 2 }).run();
      },
    },
    {
      title: 'Heading 3',
      titleKey: 'note.slash.heading3.title',
      descriptionKey: 'note.slash.heading3.desc',
      icon: Heading3,
      command: ({ editor, range }: any) => {
        editor.chain().focus().deleteRange(range).setHeading({ level: 3 }).run();
      },
    },
    {
      title: 'Bullet List',
      titleKey: 'note.slash.bullet_list.title',
      descriptionKey: 'note.slash.bullet_list.desc',
      icon: List,
      command: ({ editor, range }: any) => {
        editor.chain().focus().deleteRange(range).toggleBulletList().run();
      },
    },
    {
      title: 'Numbered List',
      titleKey: 'note.slash.numbered_list.title',
      descriptionKey: 'note.slash.numbered_list.desc',
      icon: ListOrdered,
      command: ({ editor, range }: any) => {
        editor.chain().focus().deleteRange(range).toggleOrderedList().run();
      },
    },
    {
      title: 'Task List',
      titleKey: 'note.slash.task_list.title',
      descriptionKey: 'note.slash.task_list.desc',
      icon: ListChecks,
      command: ({ editor, range }: any) => {
        editor.chain().focus().deleteRange(range).toggleTaskList().run();
      },
    },
    {
      title: 'Blockquote',
      titleKey: 'note.slash.blockquote.title',
      descriptionKey: 'note.slash.blockquote.desc',
      icon: Quote,
      command: ({ editor, range }: any) => {
        editor.chain().focus().deleteRange(range).setBlockquote().run();
      },
    },
    {
      title: 'Code Block',
      advanced: true,
      titleKey: 'note.slash.code_block.title',
      descriptionKey: 'note.slash.code_block.desc',
      icon: Code2,
      command: ({ editor, range }: any) => {
        editor.chain().focus().deleteRange(range).setCodeBlock().run();
      },
    },
    {
      title: 'Query',
      advanced: true,
      titleKey: 'note.slash.query.title',
      descriptionKey: 'note.slash.query.desc',
      icon: SearchIcon,
      command: ({ editor, range }: any) => {
        editor
          .chain()
          .focus()
          .deleteRange(range)
          .setCodeBlock({ language: 'query' })
          // A starting point that returns something on any vault, so the block
          // is never simply blank while somebody works out the syntax.
          .insertContent('is:note sort:-updated_at columns:title,updated_at limit:10')
          .run();
      },
    },
    {
      title: 'Markmap',
      advanced: true,
      titleKey: 'note.slash.markmap.title',
      descriptionKey: 'note.slash.markmap.desc',
      icon: MarkmapIcon,
      command: ({ editor, range }: any) => {
        editor.chain().focus().deleteRange(range).setCodeBlock({ language: 'markmap' }).run();
      },
    },
    {
      title: 'Divider',
      titleKey: 'note.slash.divider.title',
      descriptionKey: 'note.slash.divider.desc',
      icon: Minus,
      command: ({ editor, range }: any) => {
        editor.chain().focus().deleteRange(range).setHorizontalRule().run();
      },
    },
    {
      title: 'Image',
      titleKey: 'note.slash.image.title',
      descriptionKey: 'note.slash.image.desc',
      icon: ImageIcon,
      command: async ({ editor, range }: any) => {
        editor.chain().focus().deleteRange(range).run();
        try {
          const selectedPath = await open({
            multiple: false,
            filters: [{
              name: i18n.global.t('note.editor.filter_images'),
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
      titleKey: 'note.slash.image_collection.title',
      descriptionKey: 'note.slash.image_collection.desc',
      icon: Images,
      command: async ({ editor, range }: any) => {
        editor.chain().focus().deleteRange(range).run();
        try {
          const selectedPaths = await open({
            multiple: true,
            filters: [{
              name: i18n.global.t('note.editor.filter_images'),
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
      titleKey: 'note.slash.video.title',
      descriptionKey: 'note.slash.video.desc',
      icon: VideoIcon,
      command: ({ editor, range }: any) => {
        editor.chain().focus().deleteRange(range).run();
        videoModal.value = { show: true, url: '' };
      },
    },
    {
      title: 'Audio',
      titleKey: 'note.slash.audio.title',
      descriptionKey: 'note.slash.audio.desc',
      icon: MusicIcon,
      command: ({ editor, range }: any) => {
        editor.chain().focus().deleteRange(range).run();
        audioModal.value = { show: true, url: '' };
      },
    },
    {
      title: 'Table',
      titleKey: 'note.slash.table.title',
      descriptionKey: 'note.slash.table.desc',
      icon: Table2,
      command: ({ editor, range }: any) => {
        editor.chain().focus().deleteRange(range)
          .insertTable({ rows: 3, cols: 3, withHeaderRow: true })
          .run();
      },
    },
    {
      title: 'Equation',
      advanced: true,
      titleKey: 'note.slash.equation.title',
      descriptionKey: 'note.slash.equation.desc',
      icon: Sigma,
      command: ({ editor, range }: any) => {
        editor.chain().focus().deleteRange(range).insertContent({ type: 'equation', attrs: { latex: '' } }).run();
      },
    },
    {
      title: 'Location',
      titleKey: 'note.slash.location.title',
      descriptionKey: 'note.slash.location.desc',
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
      titleKey: 'note.slash.route.title',
      descriptionKey: 'note.slash.route.desc',
      icon: NavigationIcon,
      command: ({ editor, range }: any) => {
        editor.chain().focus().deleteRange(range).run();
        routeModal.value = { show: true, urlInput: '', error: '', label: '' };
      },
    },
    {
      title: 'Emoji',
      titleKey: 'note.slash.emoji.title',
      descriptionKey: 'note.slash.emoji.desc',
      icon: SmileIcon,
      command: ({ editor, range }: any) => {
        editor.chain().focus().deleteRange(range).run();
        emojiPicker.value = { show: true, search: '', activeCategory: 'smileys' };
      },
    },
    {
      title: 'Whiteboard',
      advanced: true,
      titleKey: 'note.slash.whiteboard.title',
      descriptionKey: 'note.slash.whiteboard.desc',
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
      advanced: true,
      titleKey: 'note.slash.embed.title',
      descriptionKey: 'note.slash.embed.desc',
      icon: EmbedIcon,
      command: ({ editor, range }: any) => {
        editor.chain().focus().deleteRange(range).run();
        embedPickerModal.value = true;
      },
    },
    {
      title: 'PDF',
      titleKey: 'note.slash.pdf.title',
      descriptionKey: 'note.slash.pdf.desc',
      icon: BookOpenIcon,
      command: async ({ editor, range }: any) => {
        editor.chain().focus().deleteRange(range).run();
        pdfModal.value = { show: true };
      },
    },
    {
      title: 'Toggle list',
      titleKey: 'note.slash.toggle_list.title',
      descriptionKey: 'note.slash.toggle_list.desc',
      icon: ChevronRightIcon,
      command: ({ editor, range }: any) => {
        // Empty, so the placeholder in `DetailsNodeView.vue` does its job.
        //
        // It used to insert the words "Toggle heading" as real text. A
        // placeholder that is actually content follows the note everywhere:
        // into the file, into the search index, and out again — a search for
        // `cam đi học` came back with the snippet «Toggle heading Đưa Cam đi
        // phỏng vấn nhập học», which is the editor's own scaffolding showing
        // up in an answer about somebody's life.
        editor.chain().focus().deleteRange(range).setDetails({ summary: '' }).run();
      },
    },
  ];

  if (onTemplate) {
    items.push({
      // `/template` in English, `/mẫu` in Vietnamese — the translated title
      // is what the search matches, alongside this one.
      title: 'Template',
      titleKey: 'note.slash.template.title',
      descriptionKey: 'note.slash.template.desc',
      icon: TemplateIcon,
      command: ({ editor, range }: any) => {
        editor.chain().focus().deleteRange(range).run();
        onTemplate();
      },
    });
  }

  return items;
}

/** The items a menu should offer, with power tools left out in simple mode. */
export function visibleSlashItems<T extends { advanced?: boolean }>(items: T[], simpleMode: boolean): T[] {
  return simpleMode ? items.filter((item) => !item.advanced) : items;
}
