<script setup lang="ts">
import { watch, onBeforeUnmount, onMounted, ref } from 'vue';
import { useEditor, EditorContent, VueRenderer, VueNodeViewRenderer } from '@tiptap/vue-3';
import StarterKit from '@tiptap/starter-kit';
import Placeholder from '@tiptap/extension-placeholder';
import { CustomImage } from './extensions/CustomImage';
import { ImageCopyFix } from './extensions/ImageCopyFix';
import { ImageGallery } from './extensions/ImageGallery';
import TaskList from '@tiptap/extension-task-list';
import TaskItem from '@tiptap/extension-task-item';
import Link from '@tiptap/extension-link';
import Underline from '@tiptap/extension-underline';
import Highlight from '@tiptap/extension-highlight';
import CodeBlockLowlight from '@tiptap/extension-code-block-lowlight';
import { Table, TableRow } from '@tiptap/extension-table';
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
import 'katex/dist/katex.min.css';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { Extension } from '@tiptap/core';
import { PluginKey } from '@tiptap/pm/state';
import Suggestion from '@tiptap/suggestion';
import tippy, { type Instance as TippyInstance } from 'tippy.js';
import type { SlashCommandItem } from './SlashCommandMenu.vue';
import SlashCommandMenu from './SlashCommandMenu.vue';
import NoteMentionMenu from './NoteMentionMenu.vue';
import EmojiSuggestionMenu from './EmojiSuggestionMenu.vue';
import { emojiData } from './emojiData';
import CodeBlockComponent from './CodeBlockComponent.vue';
import { useSettings } from '../../composables/useSettings';
import { logger } from '../../utils/logger';

// --- Extracted CSS ---
import './editor/styles/editor-base.css';
import './editor/styles/editor-toolbar.css';
import './editor/styles/editor-table.css';
import './editor/styles/editor-code.css';

// --- Extracted Extensions ---
import { CustomTableCell, CustomTableHeader } from './editor/extensions/customTable';
import { SlashCommands } from './editor/extensions/slashCommands';
import { EmojiSuggestion } from './editor/extensions/emojiSuggestion';
import { TabIndentExtension } from './editor/extensions/tabIndent';
import { ArrowExtension, CustomBlockquote } from './editor/extensions/arrowTypography';

// --- Extracted Composables ---
import { useAssetPaths } from './editor/composables/useAssetPaths';
import { useEditorModals } from './editor/composables/useEditorModals';
import { useLocationPicker } from './editor/composables/useLocationPicker';
import { createSlashCommandItems } from './editor/config/slashCommandItems';

// --- Extracted Components ---
import EditorBubbleMenu from './editor/components/EditorBubbleMenu.vue';
import EditorTableControls from './editor/components/EditorTableControls.vue';
import EditorBlockMenu from './editor/components/EditorBlockMenu.vue';
import LinkModal from './editor/components/modals/LinkModal.vue';
import MediaModal from './editor/components/modals/MediaModal.vue';
import LocationModal from './editor/components/modals/LocationModal.vue';
import RouteModal from './editor/components/modals/RouteModal.vue';
import WhiteboardPickerModal from './editor/components/modals/WhiteboardPickerModal.vue';
import EmbedPickerModal from './EmbedPickerModal.vue';
import PdfModal from './editor/components/modals/PdfModal.vue';
import EmojiPickerModal from './editor/components/modals/EmojiPickerModal.vue';

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
  (e: 'open-internal-note', payload: { id: string; type: string }): void;
}>();

// --- Settings ---
const { nestedNumberListStyle } = useSettings();

// --- Composables ---
const { injectLocalAssets, stripLocalAssets } = useAssetPaths(props.vaultPath);
const modals = useEditorModals(props.vaultPath, props.currentNoteId);
const location = useLocationPicker();

// --- Fetch all nodes for @mention ---
const allNodes = ref<any[]>([]);
onMounted(async () => {
  try {
    allNodes.value = await invoke<any[]>('get_all_nodes');
  } catch (e) {
    logger.error('Failed to fetch all nodes for mention', e);
  }
});

// --- Bubble Menu ---
const showBubble = ref(false);
const bubblePos = ref({ top: 0, left: 0 });

const updateBubbleMenu = () => {
  if (!editor.value) return;
  const { from, to, empty } = editor.value.state.selection;
  if (empty || editor.value.isActive('codeBlock')) {
    showBubble.value = false;
    return;
  }
  const start = editor.value.view.coordsAtPos(from);
  const end = editor.value.view.coordsAtPos(to);
  bubblePos.value = {
    top: start.top - 50,
    left: (start.left + end.left) / 2,
  };
  showBubble.value = true;
};

// --- Table Controls ref ---
const tableControlsRef = ref<InstanceType<typeof EditorTableControls> | null>(null);

// --- Slash command items factory ---
const slashCommandItems = (): SlashCommandItem[] => createSlashCommandItems({
  vaultPath: props.vaultPath,
  videoModal: modals.videoModal,
  audioModal: modals.audioModal,
  locationModal: location.locationModal,
  routeModal: location.routeModal,
  emojiPicker: modals.emojiPicker,
  whiteboardPickerModal: modals.whiteboardPickerModal,
  embedPickerModal: modals.embedPickerModal,
  pdfModal: modals.pdfModal,
});

// --- Editor ---
const editor = useEditor({
  content: injectLocalAssets(props.modelValue),
  extensions: [
    StarterKit.configure({
      codeBlock: false,
      blockquote: false,
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
                popup?.setProps({ getReferenceClientRect: props.clientRect });
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
                    popup?.setProps({ getReferenceClientRect: suggestionProps.clientRect });
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
            onStart: (props: any) => { createPopup(props); },
            onUpdate: (props: any) => {
              if (!component) { createPopup(props); return; }
              component.updateProps(props);
              if (!props.items.length) { popup?.hide(); return; }
              popup?.show();
              if (props.clientRect) {
                popup?.setProps({ getReferenceClientRect: props.clientRect });
              }
            },
            onKeyDown: (props: any) => {
              if (props.event.key === 'Escape') { popup?.hide(); return true; }
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
    md = md.replace(/<span[^>]*data-transclusion="([^"]+)"[^>]*>.*?<\/span>/g, (_m: string, target: string) => `![[${target}]]`);
    emit('update:modelValue', stripLocalAssets(md));
    setTimeout(updateBubbleMenu, 10);
  },
  onSelectionUpdate: ({ editor: ed }) => {
    setTimeout(updateBubbleMenu, 10);
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
      contextmenu: (_view, event) => {
        const target = event.target as HTMLElement;
        if (target.closest('td, th') && target.closest('table')) {
          event.preventDefault();
          tableControlsRef.value?.updateTableControls();
          tableControlsRef.value?.openContextMenu(event);
          return true;
        }
        const blockEl = target.closest('p, h1, h2, h3, h4, h5, h6');
        if (blockEl && props.currentNoteId && !target.closest('table')) {
          const text = blockEl.textContent?.trim();
          if (text) {
            event.preventDefault();
            modals.openBlockContextMenu(event, text);
            return true;
          }
        }
        modals.blockCtxMenu.value.show = false;
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

// --- Set editor ref in composables ---
onMounted(() => {
  if (editor.value) {
    modals.setEditor(editor.value);
    location.setEditor(editor.value);
  }

  // Listen for whiteboard embed "Open in Whiteboard" events
  const editorDom = editor.value?.view?.dom;
  if (editorDom) {
    editorDom.addEventListener('open-whiteboard-embed', ((e: CustomEvent) => {
      emit('open-internal-note', { id: e.detail.id, type: 'whiteboard' });
    }) as EventListener);
  }
});

// --- Public API ---
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

defineExpose({ loadContent, focus });

// --- Watch for external model changes ---
watch(() => props.modelValue, (newVal) => {
  if (editor.value) {
    const currentMd = (editor.value.storage as any).markdown.getMarkdown();
    if (stripLocalAssets(currentMd) !== newVal) {
       editor.value.commands.setContent(injectLocalAssets(newVal));
    }
  }
});

// --- Cleanup ---
onBeforeUnmount(() => {
  if (editor.value) {
    editor.value.destroy();
  }
});
</script>

<template>
  <div class="tiptap-wrapper w-full relative">
    <!-- Floating Toolbar -->
    <EditorBubbleMenu
      :editor="editor"
      :show="showBubble"
      :position="bubblePos"
      @set-link="modals.setLink"
    />

    <!-- Table Controls -->
    <EditorTableControls
      v-if="editor"
      ref="tableControlsRef"
      :editor="editor"
    />

    <!-- Block Context Menu -->
    <EditorBlockMenu
      :show="modals.blockCtxMenu.value.show"
      :top="modals.blockCtxMenu.value.top"
      :left="modals.blockCtxMenu.value.left"
      @copy-block-link="modals.copyBlockLink"
    />

    <!-- Editor Content -->
    <div :class="{
      'list-style-decimal': nestedNumberListStyle === 'decimal',
      'list-style-alpha': nestedNumberListStyle === 'alpha',
      'list-style-nested': nestedNumberListStyle === 'nested'
    }" class="editor-wrapper h-full w-full">
      <editor-content :editor="editor" @click="modals.blockCtxMenu.value.show = false" />
    </div>

    <!-- Modals -->
    <LinkModal
      :show="modals.linkModal.value.show"
      :url="modals.linkModal.value.url"
      @update:show="v => modals.linkModal.value.show = v"
      @update:url="v => modals.linkModal.value.url = v"
      @confirm="modals.confirmLink"
      @remove="() => { modals.linkModal.value.url = ''; modals.confirmLink(); }"
    />

    <MediaModal
      type="video"
      :show="modals.videoModal.value.show"
      :url="modals.videoModal.value.url"
      @update:show="v => modals.videoModal.value.show = v"
      @update:url="v => modals.videoModal.value.url = v"
      @confirm="modals.confirmVideo"
      @browse-local="modals.selectLocalVideo"
    />

    <MediaModal
      type="audio"
      :show="modals.audioModal.value.show"
      :url="modals.audioModal.value.url"
      @update:show="v => modals.audioModal.value.show = v"
      @update:url="v => modals.audioModal.value.url = v"
      @confirm="modals.confirmAudio"
      @browse-local="modals.selectLocalAudio"
    />

    <LocationModal
      :model-value="location.locationModal.value"
      @input="location.onLocationInput"
      @select-suggestion="location.selectSuggestion"
      @confirm="location.confirmLocation"
      @close="location.locationModal.value.show = false"
      @update:model-value="v => location.locationModal.value = v"
    />

    <RouteModal
      :show="location.routeModal.value.show"
      :url-input="location.routeModal.value.urlInput"
      :label="location.routeModal.value.label"
      :error="location.routeModal.value.error"
      :is-valid="location.isValidRouteUrl.value"
      @update:url-input="v => location.routeModal.value.urlInput = v"
      @update:label="v => location.routeModal.value.label = v"
      @confirm="location.confirmRoute"
      @close="location.routeModal.value.show = false"
    />

    <WhiteboardPickerModal
      :show="modals.whiteboardPickerModal.value.show"
      :boards="modals.filteredWhiteboards.value"
      :loading="modals.whiteboardPickerModal.value.loading"
      :search="modals.whiteboardPickerModal.value.search"
      @update:search="v => modals.whiteboardPickerModal.value.search = v"
      @select="modals.confirmWhiteboard"
      @close="modals.whiteboardPickerModal.value.show = false"
    />

    <EmbedPickerModal
      :show="modals.embedPickerModal.value"
      :notes="allNodes"
      :vault-path="vaultPath"
      @close="modals.embedPickerModal.value = false"
      @embed="modals.confirmEmbed"
    />

    <PdfModal
      :show="modals.pdfModal.value.show"
      @select-file="modals.selectPdfFile"
      @close="modals.pdfModal.value.show = false"
    />

    <EmojiPickerModal
      :show="modals.emojiPicker.value.show"
      :search="modals.emojiPicker.value.search"
      :active-category="modals.emojiPicker.value.activeCategory"
      :filtered-emojis="modals.filteredPickerEmojis.value"
      @select="modals.insertEmoji"
      @update:search="v => modals.emojiPicker.value.search = v"
      @update:active-category="v => modals.emojiPicker.value.activeCategory = v"
      @close="modals.emojiPicker.value.show = false"
    />
  </div>
</template>
