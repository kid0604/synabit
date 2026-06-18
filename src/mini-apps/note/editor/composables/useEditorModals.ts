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
  const linkModal = ref<{ show: boolean; url: string }>({ show: false, url: '' });

  const setLink = () => {
    if (!editorRef.value) return;
    const previousUrl = editorRef.value.getAttributes('link').href;
    linkModal.value = { show: true, url: previousUrl || 'https://' };
  };

  const confirmLink = () => {
    linkModal.value.show = false;
    if (!editorRef.value) return;
    const url = linkModal.value.url;
    if (!url || url === '') {
      editorRef.value.chain().focus().extendMarkRange('link').unsetLink().run();
      return;
    }
    editorRef.value.chain().focus().extendMarkRange('link').setLink({ href: url }).run();
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
  };
}
