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
import { useLicenseStore } from '../../stores/useLicenseStore';
import { openUrl } from '@tauri-apps/plugin-opener';
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
import { splitMentionQuery } from './editor/mentionQuery';
import { createDeferredSerializer } from './editor/deferredSerializer';
import { contextTargetFor } from './editor/contextTarget';

// --- Extracted Components ---
import EditorBubbleMenu from './editor/components/EditorBubbleMenu.vue';
import EditorTableControls from './editor/components/EditorTableControls.vue';
import EditorBlockMenu from './editor/components/EditorBlockMenu.vue';
import EditorLinkMenu from './editor/components/EditorLinkMenu.vue';
import LinkModal from './editor/components/modals/LinkModal.vue';
import MediaModal from './editor/components/modals/MediaModal.vue';
import LocationModal from './editor/components/modals/LocationModal.vue';
import RouteModal from './editor/components/modals/RouteModal.vue';
import WhiteboardPickerModal from './editor/components/modals/WhiteboardPickerModal.vue';
import EmbedPickerModal from './EmbedPickerModal.vue';
import PdfModal from './editor/components/modals/PdfModal.vue';
import EmojiPickerModal from './editor/components/modals/EmojiPickerModal.vue';

const lowlight = createLowlight(common);

/**
 * `mermaid`, `markmap` and `query` are ours, not highlight.js's — a diagram or
 * a saved query, rendered below the block rather than coloured inside it.
 *
 * Registering them as plain text is what keeps typing in them fast. The
 * lowlight plugin falls back to `highlightAuto` for any language it does not
 * know, which runs the block through every grammar it has; and it re-runs that
 * for *every* code block in the note on every keystroke made inside one. On a
 * note of five mermaid diagrams that measured 150ms per character, against
 * 5ms once the language is known — a note you could watch yourself type.
 *
 * Naming them here also puts them in the block's language dropdown, which
 * until now could not display the language the block was actually set to.
 */
for (const name of ['mermaid', 'markmap', 'query']) {
  // Written out rather than reusing highlight.js's own `plaintext`, which
  // carries the `text` and `txt` aliases with it and would hand them to
  // whichever of these three registered last.
  lowlight.register(name, () => ({ name, contains: [], disableAutodetect: true }));
}

const props = defineProps<{
  modelValue: string;
  vaultPath: string;
  zenMode?: boolean;
  currentNoteId?: string;
  minHeightClass?: string;
  /**
   * What an empty editor says.
   *
   * Things passed one of these from the day it adopted this editor, and there
   * was no prop to receive it: the attribute fell through onto the wrapper div
   * and every node in the vault offered `Type / for commands...` in English,
   * under a translated heading. The default keeps Notes reading exactly as it
   * did.
   */
  placeholder?: string;
}>();

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void;
  (e: 'open-internal-note', payload: { id: string; type: string }): void;
  /**
   * The caret left the editor.
   *
   * Declared because a parent writing `@blur` on this component was getting
   * nothing: with no such emit the listener falls through onto the wrapper
   * `<div>`, and `blur` does not bubble, so it never fired. Things had exactly
   * that listener as its only way to persist a body — type a paragraph, click
   * another node, and the paragraph was gone.
   *
   * Notes does not listen for it and is unchanged; it commits through the
   * `onBlur` below, which is also what raises this.
   */
  (e: 'blur'): void;
}>();

// --- Settings ---
const { nestedNumberListStyle, codeBlockTabSize } = useSettings();

// --- Composables ---
const { injectLocalAssets, stripLocalAssets } = useAssetPaths(props.vaultPath);
const modals = useEditorModals(props.vaultPath, props.currentNoteId);
const location = useLocationPicker();

/**
 * Find the handful of nodes worth offering after an `@`.
 *
 * Asked of the database per keystroke rather than held in memory. The editor
 * used to load every node in the vault on mount — with each one's full body —
 * so that it could filter five of them with `String.includes`. Every open tab
 * paid for it, and the payload is the same twenty megabytes the note list was
 * rewritten to stop moving.
 *
 * Going through the search index also means an `@` matches the way search
 * does: ranked by BM25, with the title weighted above the body, instead of
 * whichever node happened to come first.
 */
const searchMentions = async (rawQuery: string): Promise<MentionItem[]> => {
  const { search, alias } = splitMentionQuery(rawQuery);

  // With spaces allowed in the query (see `allowSpaces` below) the text after
  // an `@` runs to the end of the line, so a stray `@` earlier in a paragraph
  // would otherwise send the whole rest of it to the search index.
  if (search.length > MENTION_QUERY_MAX) return [];

  try {
    const response = await invoke<{ results: MentionResult[] }>('search_nexus', {
      vaultPath: props.vaultPath,
      query: search,
      page: 1,
      perPage: MENTION_LIMIT,
    });
    return response.results.slice(0, MENTION_LIMIT).map((r) => ({
      id: r.id,
      title: r.title,
      alias,
      // The snippet arrives wrapped in <mark> around the matched words, which
      // is markup for a search result list and literal angle brackets here.
      summary: r.snippet.replace(/<\/?mark>/g, '').trim().substring(0, 50),
      node_type: r.item_type || 'note',
    }));
  } catch (e) {
    logger.error('Could not look up nodes for the mention menu', e);
    return [];
  }
};

interface MentionResult {
  id: string;
  item_type: string;
  title: string;
  snippet: string;
}

interface MentionItem {
  id: string;
  title: string;
  /** What this link should read as, when it should not read as the title. */
  alias: string;
  summary: string;
  node_type: string;
}

/** How many suggestions the menu shows. */
const MENTION_LIMIT = 5;

/** Beyond this the text after an `@` is prose, not a search for a note. */
const MENTION_QUERY_MAX = 80;

/**
 * Follow the link the context menu is open on.
 *
 * A note goes to the same place a Cmd/Ctrl-click goes, so there is one route
 * into a note rather than two that can drift. Anything else is the web, and
 * the web is the operating system's business, not a webview's — opening it
 * in here would replace the user's editor with a page they cannot get back
 * from.
 */
const openLinkTarget = () => {
  const href = modals.linkCtxMenu.value.href;
  modals.closeLinkContextMenu();
  if (!href) return;

  const internal = href.match(/^synabit:\/\/([^/]+)\/(.+)/);
  if (internal) {
    emit('open-internal-note', { id: decodeURIComponent(internal[2]), type: internal[1] });
    return;
  }
  openUrl(href).catch((e) => logger.error('Could not open that link', e));
};


// ── Serialising the document ────────────────────────────────
/** How long after a keystroke the document is turned into markdown. */
const SERIALIZE_DEBOUNCE_MS = 200;

const serializer = createDeferredSerializer({
  delayMs: SERIALIZE_DEBOUNCE_MS,
  produce: () => {
    const ed = editor.value;
    if (!ed) return null;
    let md = (ed.storage as any).markdown.getMarkdown();
    md = md.replace(/<span[^>]*data-transclusion="([^"]+)"[^>]*>.*?<\/span>/g, (_m: string, target: string) => `![[${target}]]`);
    return stripLocalAssets(md);
  },
  emit: (value) => emit('update:modelValue', value),
});

/**
 * Serialise right now, if anything is waiting.
 *
 * Called by whatever is about to read the note's text — a save arriving from
 * somewhere other than typing, an export, a tab closing — so the last fraction
 * of a second of writing is not missing from it.
 */
const flushSerialize = () => serializer.flush();

// --- Toolbar Toggles ---
const showBubble = ref(false);
const bubblePos = ref({ top: 0, left: 0 });

const updateBubbleMenu = () => {
  if (!editor.value) return;
  const { from, to, empty } = editor.value.state.selection;
  // The link context menu selects the link it opened on, which would otherwise
  // bring the floating toolbar up over the same link at the same moment. One
  // gesture, one menu.
  if (empty || editor.value.isActive('codeBlock') || modals.linkCtxMenu.value.show) {
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
const licenseStore = useLicenseStore();

const editor = useEditor({
  content: injectLocalAssets(props.modelValue),
  editable: !licenseStore.isReadOnly,
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
      addKeyboardShortcuts() {
        return {
          ...this.parent?.(),
          'Mod-a': () => {
            const { state } = this.editor;
            const { $from } = state.selection;
            
            if ($from.parent.type.name === 'codeBlock') {
              const start = $from.start();
              const end = start + $from.parent.content.size;
              
              // If already fully selected, allow default behavior (select entire note)
              if (state.selection.from === start && state.selection.to === end) {
                return false;
              }
              
              this.editor.commands.setTextSelection({ from: start, to: end });
              return true;
            }
            return false;
          },
          'Tab': () => {
            if (!this.editor.isActive('codeBlock')) return false;
            const { state, dispatch } = this.editor.view;
            const { selection, tr } = state;
            const tabStr = ' '.repeat(codeBlockTabSize.value);
            
            if (selection.empty) {
               if (dispatch) {
                   tr.insertText(tabStr);
                   dispatch(tr);
               }
               return true;
            }
            // Multi-line indent
            const text = state.doc.textBetween(selection.from, selection.to, '\n');
            const indented = text.split('\n').map(line => tabStr + line).join('\n');
            if (dispatch) {
               tr.insertText(indented, selection.from, selection.to);
               dispatch(tr);
            }
            return true;
          },
          'Shift-Tab': () => {
            if (!this.editor.isActive('codeBlock')) return false;
            const { state, dispatch } = this.editor.view;
            const { selection, tr } = state;
            const tabSize = codeBlockTabSize.value;
            const tabStr = ' '.repeat(tabSize);
            
            // Get the block's text
            const { $from, $to } = selection;
            const blockStart = $from.start();
            const blockEnd = blockStart + $from.parent.content.size;
            
            const text = state.doc.textBetween(blockStart, blockEnd, '\n');
            const lines = text.split('\n');
            
            let currentPos = blockStart;
            let newText = '';
            let firstChange = false;
            
            for (let i = 0; i < lines.length; i++) {
                const line = lines[i];
                const lineStart = currentPos;
                const lineEnd = currentPos + line.length;
                
                if (lineEnd >= selection.from && lineStart <= selection.to) {
                    if (line.startsWith(tabStr)) {
                        newText += line.substring(tabSize);
                        firstChange = true;
                    } else if (line.startsWith('\t') || line.startsWith(' ')) {
                        newText += line.substring(1);
                        firstChange = true;
                    } else {
                        newText += line;
                    }
                } else {
                    newText += line;
                }
                
                if (i < lines.length - 1) newText += '\n';
                currentPos = lineEnd + 1;
            }
            
            if (firstChange && dispatch) {
                tr.insertText(newText, blockStart, blockEnd);
                tr.setSelection((state.selection.constructor as any).create(tr.doc, Math.max(blockStart, selection.from - tabSize), Math.max(blockStart, selection.to - tabSize)));
                dispatch(tr);
            }
            return true;
          },
        };
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
      placeholder: props.placeholder ?? 'Type / for commands...',
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
            // Note titles have spaces in them, and so do the aliases people
            // want to give them. Without this the query stopped at the first
            // space, so "@công ty cổ phần" could never match anything past
            // "công". The default `allowedPrefixes` still requires the `@` to
            // follow a space or start a line, so an address in the middle of a
            // word does not open the menu.
            allowSpaces: true,
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
                  // The alias when one was typed after a `|`, the title
                  // otherwise. Only the text differs — the link still points at
                  // the same node, so backlinks and the graph are unaffected.
                  text: props.alias || props.title
                })
                .insertContent(' ')
                .run();
            },
            items: ({ query }) => searchMentions(query),
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
  onUpdate: () => {
    serializer.schedule();
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
    // Leaving the editor commits what is in it. Every action that rewrites a
    // note without going through typing — pinning it, tagging it, unlinking a
    // project — needs the user to click away from the editor first, so this
    // one line closes the window on all of them at once. They flush explicitly
    // as well: this is the net, not the guarantee.
    flushSerialize();
    // Strictly after the flush. A listener on this saves the node, and the
    // value it reads is whatever the last `update:modelValue` left behind —
    // so raising it first would write the document as it stood a keystroke
    // ago, every time, while looking like it worked.
    emit('blur');
    setTimeout(() => { showBubble.value = false; }, 200);
  },
  editorProps: {
    handleDOMEvents: {
      contextmenu: (_view, event) => {
        const target = event.target as HTMLElement;
        const aimedAt = contextTargetFor(target);

        // Whichever menu opens, the other two close. Two context menus at once
        // is a thing the app used to be one selector away from.
        const closeOthers = () => {
          modals.blockCtxMenu.value.show = false;
          modals.closeLinkContextMenu();
        };

        if (aimedAt.kind === 'table') {
          event.preventDefault();
          closeOthers();
          tableControlsRef.value?.updateTableControls();
          tableControlsRef.value?.openContextMenu(event);
          return true;
        }

        if (aimedAt.kind === 'link') {
          event.preventDefault();
          closeOthers();
          modals.openLinkContextMenu(event, aimedAt.href);
          return true;
        }

        if (aimedAt.kind === 'block' && props.currentNoteId) {
          const text = target.closest('p, h1, h2, h3, h4, h5, h6')?.textContent?.trim();
          if (text) {
            event.preventDefault();
            closeOthers();
            modals.openBlockContextMenu(event, text);
            return true;
          }
        }

        closeOthers();
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
      // A file dragged out of the Files app.
      //
      // Checked before the branch below, because a drag from inside the window
      // carries no `files` — the OS has nothing to hand over. What it carries
      // is a description of something already in the vault, so nothing is
      // copied and nothing is written: the note simply points at it.
      const fromLibrary = event.dataTransfer?.getData('application/x-synabit-file');
      if (!moved && fromLibrary) {
        event.preventDefault();
        try {
          const file = JSON.parse(fromLibrary) as {
            filename: string; extension: string; assetPath: string | null; absPath: string;
          };
          const pos = view.posAtCoords({ left: event.clientX, top: event.clientY })?.pos;
          const src = convertFileSrc(file.absPath);
          const ext = (file.extension || '').toLowerCase();

          // Only a file that lives in the vault's own assets folder can be
          // embedded, because that is the only shape a note can carry to
          // another device. Anything else gets a link to where it is here.
          const embeddable = !!file.assetPath;
          let content: Record<string, unknown>;
          if (embeddable && ['jpg','jpeg','png','gif','webp','svg','bmp','avif'].includes(ext)) {
            content = { type: 'image', attrs: { src, alt: file.filename } };
          } else if (embeddable && ['mp4','mov','webm','mkv'].includes(ext)) {
            content = { type: 'video', attrs: { src } };
          } else if (embeddable && ['mp3','wav','ogg','m4a','flac'].includes(ext)) {
            content = { type: 'audio', attrs: { src } };
          } else {
            content = {
              type: 'text',
              text: file.filename,
              marks: [{ type: 'link', attrs: { href: src } }],
            };
          }

          if (pos !== undefined) {
            editor.value?.commands.insertContentAt(pos, content);
          } else {
            editor.value?.commands.insertContent(content);
          }
        } catch (e) {
          logger.error("Failed to insert a file dragged from the library", e);
        }
        return true;
      }

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

defineExpose({ loadContent, focus, flushSerialize });

// --- Watch for external model changes ---
watch(() => props.modelValue, (newVal) => {
  if (!editor.value) return;

  // Our own value coming back around. Nothing to do, and — more to the point —
  // no reason to serialise the document again to work that out.
  if (serializer.isEcho(newVal)) return;

  const currentMd = (editor.value.storage as any).markdown.getMarkdown();
  if (stripLocalAssets(currentMd) !== newVal) {
     editor.value.commands.setContent(injectLocalAssets(newVal));
     serializer.adopt(newVal);
  }
});

// --- Cleanup ---
onBeforeUnmount(() => {
  // Before the editor goes: a tab being closed or swapped out must not take
  // the last fraction of a second of typing with it.
  flushSerialize();
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

    <!-- Link Context Menu -->
    <EditorLinkMenu
      :show="modals.linkCtxMenu.value.show"
      :top="modals.linkCtxMenu.value.top"
      :left="modals.linkCtxMenu.value.left"
      :href="modals.linkCtxMenu.value.href"
      @open="openLinkTarget"
      @edit="modals.editLinkFromMenu"
      @remove="modals.removeLinkFromMenu"
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
      <editor-content
        :editor="editor"
        @click="modals.blockCtxMenu.value.show = false; modals.closeLinkContextMenu();"
      />
    </div>

    <!-- Modals -->
    <LinkModal
      :show="modals.linkModal.value.show"
      :url="modals.linkModal.value.url"
      :text="modals.linkModal.value.text"
      @update:show="v => modals.linkModal.value.show = v"
      @update:url="v => modals.linkModal.value.url = v"
      @update:text="v => modals.linkModal.value.text = v"
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
