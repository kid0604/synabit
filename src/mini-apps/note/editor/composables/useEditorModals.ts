import { ref, computed } from 'vue';
import type { Editor } from '@tiptap/vue-3';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { readFile } from '@tauri-apps/plugin-fs';
import { emojiData } from '../../emojiData';
import { logger } from '../../../../utils/logger';

export function useEditorModals(vaultPath: string, currentNoteId?: string) {
  const editorRef = ref<Editor | null>(null);

  const setEditor = (editor: Editor | null | undefined) => {
    editorRef.value = editor ?? null;
  };

  // --- Link Modal ---
  const linkModal = ref<{ show: boolean; url: string; text: string; originalText: string }>({
    show: false,
    url: '',
    text: '',
    originalText: '',
  });

  const setLink = () => {
    if (!editorRef.value) return;
    const editor = editorRef.value;
    const previousUrl = editor.getAttributes('link').href;

    // Widen the selection to the whole link before reading its text, so that a
    // cursor resting inside one edits that link rather than nothing. Harmless
    // when there is no link: the selection is left as it was.
    if (editor.isActive('link')) {
      editor.chain().focus().extendMarkRange('link').run();
    }
    const { from, to } = editor.state.selection;
    const text = editor.state.doc.textBetween(from, to, ' ');

    linkModal.value = {
      show: true,
      url: previousUrl || 'https://',
      text,
      originalText: text,
    };
  };

  const confirmLink = () => {
    linkModal.value.show = false;
    if (!editorRef.value) return;
    const { url, text, originalText } = linkModal.value;
    if (!url || url === '') {
      editorRef.value.chain().focus().extendMarkRange('link').unsetLink().run();
      return;
    }

    // Nothing to rewrite: leave the text alone rather than replacing it with an
    // identical copy, which would cost the user an undo step for no change.
    if (text.trim() === '' || text === originalText) {
      editorRef.value.chain().focus().extendMarkRange('link').setLink({ href: url }).run();
      return;
    }

    // The text and the destination are separate things — a note titled "Công ty
    // cổ phần ABC" is worth calling "công ty cũ" mid-sentence — so changing one
    // must not disturb the other. The replacement carries the link mark
    // explicitly because typing over a link would otherwise drop it: Tiptap's
    // `Link` is not inclusive, by design.
    editorRef.value
      .chain()
      .focus()
      .extendMarkRange('link')
      .insertContent({
        type: 'text',
        marks: [{ type: 'link', attrs: { href: url } }],
        text,
      })
      .run();
  };

  // --- Video prompt ---
  const videoModal = ref<{ show: boolean; url: string }>({ show: false, url: '' });

  const confirmVideo = () => {
    videoModal.value.show = false;
    if (!editorRef.value) return;
    const url = videoModal.value.url;
    if (!url || url === '') return;

    let finalUrl = url;
    if (url.startsWith('assets/')) {
      const sep = vaultPath.includes('\\') ? '\\' : '/';
      const filename = url.substring(7);
      const absPath = `${vaultPath}${sep}assets${sep}${decodeURIComponent(filename)}`;
      finalUrl = convertFileSrc(absPath);
    }

    editorRef.value.commands.setVideo({ src: finalUrl });
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

      if (selectedPath && !Array.isArray(selectedPath) && vaultPath) {
        const pathStr = selectedPath as string;
        const match = pathStr.match(/[\\/]([^\\/]+)$/);
        const filename = match ? match[1] : `video-${Date.now()}.mp4`;
        const buffer = await readFile(pathStr);

        const relativePath = await invoke<string>('save_asset', {
          vaultPath: vaultPath,
          filename: filename,
          bytes: Array.from(buffer)
        });
        const sep = vaultPath.includes('\\') ? '\\' : '/';
        const absPath = `${vaultPath}${sep}${relativePath}`;
        const renderUrl = convertFileSrc(absPath);

        videoModal.value.show = false;
        editorRef.value?.commands.setVideo({ src: renderUrl });
      }
    } catch (e) {
      logger.error("Failed to insert local video", e);
    }
  };

  // --- Audio prompt ---
  const audioModal = ref<{ show: boolean; url: string }>({ show: false, url: '' });

  const confirmAudio = () => {
    audioModal.value.show = false;
    if (!editorRef.value) return;
    const url = audioModal.value.url;
    if (!url || url === '') return;

    let finalUrl = url;
    if (url.startsWith('assets/')) {
      const sep = vaultPath.includes('\\') ? '\\' : '/';
      const filename = url.substring(7);
      const absPath = `${vaultPath}${sep}assets${sep}${decodeURIComponent(filename)}`;
      finalUrl = convertFileSrc(absPath);
    }

    editorRef.value.commands.setAudio({ src: finalUrl });
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

      if (selectedPath && !Array.isArray(selectedPath) && vaultPath) {
        const pathStr = selectedPath as string;
        const match = pathStr.match(/[\\/]([^\\/]+)$/);
        const filename = match ? match[1] : `audio-${Date.now()}.mp3`;
        const buffer = await readFile(pathStr);

        const relativePath = await invoke<string>('save_asset', {
          vaultPath: vaultPath,
          filename: filename,
          bytes: Array.from(buffer)
        });
        const sep = vaultPath.includes('\\') ? '\\' : '/';
        const absPath = `${vaultPath}${sep}${relativePath}`;
        const renderUrl = convertFileSrc(absPath);

        audioModal.value.show = false;
        editorRef.value?.commands.setAudio({ src: renderUrl });
      }
    } catch (e) {
      logger.error("Failed to insert local audio", e);
    }
  };

  // --- Emoji Picker (full panel from /emoji) ---
  const emojiPicker = ref({ show: false, search: '', activeCategory: 'smileys' });

  const insertEmoji = (emoji: string) => {
    if (!editorRef.value) return;
    editorRef.value.chain().focus().insertContent(emoji).run();
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
    if (!editorRef.value) return;
    editorRef.value.commands.setWhiteboard({
      boardId: board.id || board.path,
      boardPath: board.path,
      title: board.title || 'Untitled Board',
    });
    whiteboardPickerModal.value = { show: false, boards: [], loading: false, search: '' };
  };

  // --- Embed Picker Modal (Transclusion 2.0) ---
  const embedPickerModal = ref(false);

  const confirmEmbed = (payload: { nodeId: string; blockId?: string; noteTitle: string }) => {
    if (!editorRef.value) return;
    const target = payload.blockId
      ? `${payload.nodeId}#${payload.blockId}`
      : payload.noteTitle;
    editorRef.value.commands.insertContent({
      type: 'transclusion',
      attrs: { target, nodeId: payload.nodeId },
    });
    embedPickerModal.value = false;
  };

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

      if (selectedPath && !Array.isArray(selectedPath) && vaultPath) {
        const pathStr = selectedPath as string;
        const match = pathStr.match(/[\\/]([^\\/]+)$/);
        const filename = match ? match[1] : `document-${Date.now()}.pdf`;

        // Copy to vault assets
        const relativePath = await invoke<string>('copy_asset_to_vault', {
          vaultPath: vaultPath,
          sourcePath: pathStr,
        });

        pdfModal.value.show = false;

        if (editorRef.value) {
          editorRef.value.commands.setPdf({
            src: relativePath,
            title: filename.replace(/\.pdf$/i, ''),
          });
        }
      }
    } catch (e) {
      logger.error('Failed to embed PDF', e);
    }
  };

  // --- Block Context Menu (right-click → Copy Block Link) ---
  /**
   * The menu that opens on right-clicking a link.
   *
   * Right-clicking a link used to land on the block-reference menu below —
   * that handler matches any paragraph or heading, and a link lives inside
   * one. So the one gesture people reach for on a link offered them "Copy
   * Block Link" and nothing about the link at all, while editing its text
   * meant selecting it by hand first to make the bubble toolbar appear.
   */
  const linkCtxMenu = ref<{ show: boolean; top: number; left: number; href: string }>({
    show: false, top: 0, left: 0, href: ''
  });

  const menuPosition = (event: MouseEvent) => {
    const wrapper = (event.target as HTMLElement).closest('.tiptap-wrapper');
    const wrapperRect = wrapper ? wrapper.getBoundingClientRect() : { top: 0, left: 0 };
    return {
      top: event.clientY - wrapperRect.top,
      left: event.clientX - wrapperRect.left,
    };
  };

  /**
   * Open the link menu, and put the editor's selection on the link first.
   *
   * Selecting it here is what lets every action below be an ordinary command
   * on the current selection. `posAtCoords` uses the pointer rather than the
   * element, so a link broken across two lines resolves to the half that was
   * actually clicked.
   */
  const openLinkContextMenu = (event: MouseEvent, href: string) => {
    const editor = editorRef.value;
    if (!editor) return;

    const at = editor.view.posAtCoords({ left: event.clientX, top: event.clientY });
    if (at) {
      editor.chain().focus().setTextSelection(at.pos).extendMarkRange('link').run();
    }

    linkCtxMenu.value = { show: true, ...menuPosition(event), href };
  };

  const closeLinkContextMenu = () => { linkCtxMenu.value.show = false; };

  const editLinkFromMenu = () => {
    linkCtxMenu.value.show = false;
    setLink();
  };

  const removeLinkFromMenu = () => {
    linkCtxMenu.value.show = false;
    editorRef.value?.chain().focus().extendMarkRange('link').unsetLink().run();
  };

  const blockCtxMenu = ref<{ show: boolean; top: number; left: number; text: string }>({
    show: false, top: 0, left: 0, text: ''
  });

  const openBlockContextMenu = (event: MouseEvent, text: string) => {
    blockCtxMenu.value = { show: true, ...menuPosition(event), text };
  };

  const copyBlockLink = async () => {
    if (!currentNoteId || !blockCtxMenu.value.text) return;
    try {
      const blockId = await invoke<string>('create_block_reference', {
        vaultPath: vaultPath,
        nodeId: currentNoteId,
        contentSnippet: blockCtxMenu.value.text.trim(),
      });
      const uri = `synabit://block/${currentNoteId}#${blockId}`;
      await navigator.clipboard.writeText(uri);
      blockCtxMenu.value.show = false;
    } catch (err) {
      console.error('Failed to copy block link:', err);
      blockCtxMenu.value.show = false;
    }
  };

  return {
    setEditor,
    linkModal, setLink, confirmLink,
    videoModal, confirmVideo, selectLocalVideo,
    audioModal, confirmAudio, selectLocalAudio,
    emojiPicker, filteredPickerEmojis, insertEmoji,
    whiteboardPickerModal, filteredWhiteboards, confirmWhiteboard,
    embedPickerModal, confirmEmbed,
    pdfModal, selectPdfFile,
    blockCtxMenu, openBlockContextMenu, copyBlockLink,
    linkCtxMenu, openLinkContextMenu, closeLinkContextMenu, editLinkFromMenu, removeLinkFromMenu,
  };
}
