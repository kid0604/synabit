<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch, computed } from 'vue';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { emit as emitTauri } from '@tauri-apps/api/event';
import { confirm, message, open as openDialog } from '@tauri-apps/plugin-dialog';
import { CheckSquare, Image as ImageIcon, Trash2, Palette, Tag, X, Search, FileText } from 'lucide-vue-next';
import { useEditor, EditorContent } from '@tiptap/vue-3';
import StarterKit from '@tiptap/starter-kit';
import { Markdown } from 'tiptap-markdown';
import TiptapImage from '@tiptap/extension-image';
import Placeholder from '@tiptap/extension-placeholder';
import TaskEditModal from './TaskEditModal.vue';
import NoteEditModal from './NoteEditModal.vue';

const props = defineProps<{
  vaultPath: string;
}>();

export interface QuickCapMetadata {
    id: string;
    date: string;
    content: string;
    path: string;
}

const quickCaps = ref<QuickCapMetadata[]>([]);
const newCapText = ref('');
const isSubmitting = ref(false);
const inputRef = ref<HTMLTextAreaElement | null>(null);
const selectedCap = ref<QuickCapMetadata | null>(null);

const editingContent = ref('');
let saveTimeout: ReturnType<typeof setTimeout> | null = null;
const currentTags = ref<string[]>([]);

const taggingCapId = ref<string | null>(null);
const colorPickerCapId = ref<string | null>(null);
const tagInputText = ref('');
const searchQuery = ref('');

const PALETTE = [
   { name: 'Default', value: '' },
   { name: 'Red', value: 'bg-red-50 dark:bg-red-950/30' },
   { name: 'Orange', value: 'bg-orange-50 dark:bg-orange-950/30' },
   { name: 'Yellow', value: 'bg-yellow-50 dark:bg-yellow-950/30' },
   { name: 'Green', value: 'bg-green-50 dark:bg-green-950/30' },
   { name: 'Blue', value: 'bg-blue-50 dark:bg-blue-950/30' },
   { name: 'Purple', value: 'bg-purple-50 dark:bg-purple-950/30' },
   { name: 'Pink', value: 'bg-pink-50 dark:bg-pink-950/30' },
];

const filteredCaps = computed(() => {
    const q = searchQuery.value.trim().toLowerCase();
    if (!q) return quickCaps.value;
    
    const isTagSearch = q.startsWith('#');
    const tagQuery = isTagSearch ? q.substring(1) : q;
    
    return quickCaps.value.filter(cap => {
        if (isTagSearch) {
            const tags = extractTags(cap.content).map(t => t.toLowerCase());
            return tags.some(t => t.includes(tagQuery));
        } else {
            return cap.content.toLowerCase().includes(q);
        }
    });
});

const activeTags = computed(() => {
    const newlyTyped = extractTags(editingContent.value);
    return Array.from(new Set([...currentTags.value, ...newlyTyped]));
});

const appendTagToInput = () => {
    newCapText.value += (newCapText.value && !newCapText.value.endsWith(' ') && !newCapText.value.endsWith('\n') ? ' #' : '#');
    inputRef.value?.focus();
};

const openTagInput = (cap: QuickCapMetadata) => {
    taggingCapId.value = cap.id;
    tagInputText.value = '';
};

const saveInlineTag = async (cap: QuickCapMetadata) => {
    if (!tagInputText.value.trim()) {
        taggingCapId.value = null;
        return;
    }
    const rawTag = tagInputText.value.trim().replace(/^#/, '').replace(/#$/, '');
    const isMultiWord = rawTag.includes(' ');
    const formattedTag = isMultiWord ? `#${rawTag}#` : `#${rawTag}`;
    const updatedContent = `${cap.content}\n\n${formattedTag}`;
    try {
        await invoke('update_note', { vaultPath: props.vaultPath, path: cap.path, content: updatedContent });
        cap.content = updatedContent;
        taggingCapId.value = null;
        tagInputText.value = '';
    } catch(e) {
        console.error("Failed to update note", e);
    }
};

const getCapColor = (content: string) => {
    const match = content.match(/<!--color:(.*?)-->/);
    if (match) return match[1];
    return '';
};

const toggleColorPicker = (capId: string) => {
    if (colorPickerCapId.value === capId) {
        colorPickerCapId.value = null;
    } else {
        colorPickerCapId.value = capId;
    }
};

const changeCapColor = async (cap: QuickCapMetadata, colorValue: string) => {
    let rawContent = cap.content.replace(/<!--color:.*?-->\n?/g, '').trim();
    let updatedContent = rawContent;
    if (colorValue) {
        updatedContent = `<!--color:${colorValue}-->\n${rawContent}`;
    }
    
    try {
        await invoke('update_note', { vaultPath: props.vaultPath, path: cap.path, content: updatedContent });
        cap.content = updatedContent;
    } catch(e) {
        console.error("Failed to update color", e);
    }
    colorPickerCapId.value = null;
};

const loadCaps = async () => {
    if (!props.vaultPath) return;
    try {
        quickCaps.value = await invoke('scan_quick_caps', { vaultPath: props.vaultPath });
    } catch (e) {
        console.error("Failed to load quick caps", e);
    }
};

const saveSelectedCap = async () => {
    if (!selectedCap.value) return;
    
    let textOnly = editingContent.value;
    // Strip inline tags cleanly before appending them at bottom
    textOnly = textOnly.replace(/(?:^|\s)#([^#\n]+)#(?=\s|$)/g, ' ');
    textOnly = textOnly.replace(/(?:^|\s)#[a-zA-Z0-9_\-\u00C0-\u024F\u1E00-\u1EFF]+(?=\s|$)/g, ' ');
    textOnly = textOnly.replace(/\n{3,}/g, '\n\n').trim();
    
    let finalPayload = textOnly;
    const allTags = activeTags.value;
    
    if (allTags.length > 0) {
        const formattedTags = allTags.map(t => t.includes(' ') ? `#${t}#` : `#${t}`).join(' ');
        finalPayload += (finalPayload ? `\n\n${formattedTags}` : formattedTags);
    }
    
    const colorMatch = selectedCap.value.content.match(/<!--color:(.*?)-->/);
    if (colorMatch) {
       finalPayload = `<!--color:${colorMatch[1]}-->\n${finalPayload}`;
    }
    
    if (selectedCap.value.content === finalPayload) return;
    
    try {
        await invoke('update_note', { vaultPath: props.vaultPath, path: selectedCap.value.path, content: finalPayload });
        selectedCap.value.content = finalPayload;
    } catch(e) {
        console.error("Failed to update note", e);
    }
};

const injectLocalAssets = (md: string) => {
   if (!props.vaultPath) return md;
   
   const cleanVaultPath = props.vaultPath.endsWith('/') || props.vaultPath.endsWith('\\') 
        ? props.vaultPath.slice(0, -1) : props.vaultPath;
   const sep = cleanVaultPath.includes('\\') ? '\\' : '/';
   
   let result = md.replace(/\]\(assets\/([^\)]+)\)/g, (_m: string, filename: string) => {
      const decodedFilename = decodeURIComponent(filename);
      const absPath = `${cleanVaultPath}${sep}assets${sep}${decodedFilename}`;
      const assetUrl = convertFileSrc(absPath); 
      return `](${assetUrl})`;
   });
   
   result = result.replace(/src="assets\/([^"]+)"/g, (_m: string, filename: string) => {
      const decodedFilename = decodeURIComponent(filename);
      const absPath = `${cleanVaultPath}${sep}assets${sep}${decodedFilename}`;
      const assetUrl = convertFileSrc(absPath); 
      return `src="${assetUrl}"`;
   });
   return result;
};

const stripLocalAssets = (md: string) => {
   let result = md.replace(/\]\(asset:\/\/[^\)]+(?:\/|%2F)assets(?:\/|%2F)([^\)]+)\)/g, (_m: string, filename: string) => {
      return `](assets/${decodeURIComponent(filename)})`;
   });
   result = result.replace(/src="asset:\/\/[^"]+(?:\/|%2F)assets(?:\/|%2F)([^"]+)"/g, (_m: string, filename: string) => {
      return `src="assets/${decodeURIComponent(filename)}"`;
   });
   return result;
};

const editor = useEditor({
  content: '',
  extensions: [
    StarterKit.configure({ codeBlock: false }),
    Markdown,
    TiptapImage,
    Placeholder.configure({ placeholder: 'Note content...' }),
  ],
  onUpdate: ({ editor: ed }) => {
    let md = (ed.storage as any).markdown.getMarkdown();
    editingContent.value = stripLocalAssets(md);
    
    if (saveTimeout) clearTimeout(saveTimeout);
    saveTimeout = setTimeout(() => {
        saveSelectedCap();
    }, 1000);
  },
  editorProps: {
    attributes: {
      class: 'prose prose-sm sm:prose dark:prose-invert focus:outline-none max-w-none w-full min-h-[100px]',
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
                     const filename = file.name ? `${Date.now()}-${file.name}` : `pasted-image-${Date.now()}.png`;
                     const relativePath = await invoke<string>('save_asset', {
                         vaultPath: props.vaultPath,
                         filename: filename,
                         bytes: Array.from(new Uint8Array(buffer))
                     });
                     const sep = props.vaultPath.includes('\\') ? '\\' : '/';
                     const absPath = `${props.vaultPath}${sep}${relativePath}`;
                     const renderUrl = convertFileSrc(absPath);
                     
                     editor.value?.commands.insertContent(`\n![Image](${renderUrl})\n`);
                 } catch(e) { console.error("Paste image failed", e); }
              });
            }
          }
        }
        if (imageHandled) return true;
      }
      return false;
    }
  }
});

const openFullView = (cap: QuickCapMetadata) => {
    selectedCap.value = cap;
    let rawStr = cap.content.replace(/<!--color:.*?-->\n?/g, '').trim();
    
    currentTags.value = extractTags(rawStr);
    
    let textOnly = rawStr;
    textOnly = textOnly.replace(/(?:^|\s)#([^#\n]+)#(?=\s|$)/g, ' ');
    textOnly = textOnly.replace(/(?:^|\s)#[a-zA-Z0-9_\-\u00C0-\u024F\u1E00-\u1EFF]+(?=\s|$)/g, ' ');
    textOnly = textOnly.replace(/\n{3,}/g, '\n\n').trim();
    
    editingContent.value = textOnly;
    
    if (editor.value) {
       editor.value.commands.setContent(injectLocalAssets(textOnly));
    }
};

const closeFullView = async () => {
    if (saveTimeout) clearTimeout(saveTimeout);
    await saveSelectedCap();
    selectedCap.value = null;
    if (editor.value) {
       editor.value.commands.clearContent();
    }
};

const handleInput = () => {
    if (inputRef.value) {
        inputRef.value.style.height = 'auto';
        inputRef.value.style.height = inputRef.value.scrollHeight + 'px';
    }
};

const submitCap = async () => {
    if (!newCapText.value.trim() || !props.vaultPath) return;
    isSubmitting.value = true;
    try {
        const newCap: QuickCapMetadata = await invoke('create_quick_cap', {
            vaultPath: props.vaultPath,
            content: newCapText.value
        });
        quickCaps.value.unshift(newCap);
        newCapText.value = '';
        if (inputRef.value) {
            inputRef.value.style.height = 'auto';
        }
    } catch (e) {
        console.error("Failed to create quick cap", e);
    } finally {
        isSubmitting.value = false;
    }
};

const handleGlobalPaste = async (e: ClipboardEvent) => {
   if (document.activeElement !== inputRef.value) return;

   if (e.clipboardData && e.clipboardData.files.length > 0) {
      const file = e.clipboardData.files[0];
      if (file.type.startsWith('image/')) {
          e.preventDefault();
          const arrayBuffer = await file.arrayBuffer();
          const bytes = Array.from(new Uint8Array(arrayBuffer));
          const filename = file.name ? `${Date.now()}-${file.name}` : `pasted-image-${Date.now()}.png`;
          
          const targetRef = inputRef.value;
          const oldPlaceholder = targetRef?.placeholder;
          if (targetRef) targetRef.placeholder = "Uploading image...";
          isSubmitting.value = true;
          try {
             const assetPath = await invoke<string>('save_asset', {
                vaultPath: props.vaultPath,
                filename,
                bytes
             });
             const imgMd = `![Image](${assetPath})`;
             const start = targetRef?.selectionStart || newCapText.value.length;
             const end = targetRef?.selectionEnd || newCapText.value.length;
             
             newCapText.value = newCapText.value.substring(0, start) + "\n" + imgMd + "\n" + newCapText.value.substring(end);
          } catch(err) {
             console.error("Paste image save error:", err);
          } finally {
             isSubmitting.value = false;
             if (targetRef) targetRef.placeholder = oldPlaceholder || "Take a quick note...";
          }
      }
   }
};

const pickImageForNewCap = async () => {
    try {
        const selected = await openDialog({
            multiple: false,
            filters: [{ name: 'Image', extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp'] }]
        });
        if (selected && typeof selected === 'string') {
            const relPath = await invoke<string>('copy_asset_to_vault', { 
                vaultPath: props.vaultPath, 
                sourcePath: selected 
            });
            const imgMd = `![Image](${relPath})`;
            newCapText.value += (newCapText.value && !newCapText.value.endsWith('\n') ? '\n\n' : '') + imgMd;
            inputRef.value?.focus();
        }
    } catch(e) {
        console.error("Failed to pick image", e);
    }
};

const pickImageForExistingCap = async (cap: QuickCapMetadata) => {
    try {
        const selected = await openDialog({
            multiple: false,
            filters: [{ name: 'Image', extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp'] }]
        });
        if (selected && typeof selected === 'string') {
            const relPath = await invoke<string>('copy_asset_to_vault', { 
                vaultPath: props.vaultPath, 
                sourcePath: selected 
            });
            const imgMd = `\n\n![Image](${relPath})`;
            const updatedContent = cap.content + imgMd;
            await invoke('update_note', { vaultPath: props.vaultPath, path: cap.path, content: updatedContent });
            cap.content = updatedContent;
        }
    } catch(e) {
        console.error("Failed to pick image", e);
    }
};

const convertingTaskCap = ref<QuickCapMetadata | null>(null);
const convertingTaskParams = ref({
    title: '',
    content: '',
    status: 'todo',
    start_date: '',
    due_date: '',
    priority: '',
    tags: '',
    checklist: [] as {content: string, completed: boolean}[],
    is_transferred: false,
    transferred_to: '',
    track_progress: false,
    comment: ''
});

const openConvertTaskModal = (cap: QuickCapMetadata) => {
    convertingTaskCap.value = cap;
    const cleanContent = cap.content.replace(/<!--color:.*?-->/g, '').trim();
    const displayLines = cleanContent.split('\n').filter(l => l.trim() !== '');
    convertingTaskParams.value = {
        title: displayLines.length > 0 ? displayLines[0].substring(0, 50) + (displayLines[0].length > 50 ? '...' : '') : 'QuickCap Task',
        content: cleanContent,
        status: 'todo',
        start_date: '',
        due_date: '',
        priority: '',
        tags: extractTags(cap.content).join(', '),
        checklist: [],
        is_transferred: false,
        transferred_to: '',
        track_progress: false,
        comment: ''
    };
};

const closeTaskModal = () => {
    convertingTaskCap.value = null;
};

const convertingNoteCap = ref<QuickCapMetadata | null>(null);
const convertingNoteParams = ref({
    title: '',
    content: '',
    tags: ''
});

const openConvertNoteModal = (cap: QuickCapMetadata) => {
    convertingNoteCap.value = cap;
    const cleanContent = cap.content.replace(/<!--color:.*?-->/g, '').trim();
    const displayLines = cleanContent.split('\n').filter(l => l.trim() !== '');
    const titleLine = displayLines.length > 0 ? displayLines[0] : 'QuickCap Note';
    const defaultTitle = titleLine.substring(0, 50) + (titleLine.length > 50 ? '...' : '');
    
    convertingNoteParams.value = {
        title: defaultTitle,
        content: cleanContent,
        tags: extractTags(cap.content).join(', ')
    };
};

const closeNoteModal = () => {
    convertingNoteCap.value = null;
};

const confirmTurnIntoNote = async (payload: any) => {
    const cap = convertingNoteCap.value;
    if (!cap) return;
    
    try {
        const path = await invoke<string>('create_new_note', { vaultPath: props.vaultPath });
        
        let tagsArray: string[] = [];
        if (payload.tags) {
            tagsArray = payload.tags.split(',').map((t: string) => t.trim()).filter((t: string) => t !== '');
        }
        
        const frontmatter = `---\ntitle: "${payload.title.replace(/"/g, '\\"')}"\ntags: [${tagsArray.map(t => `"${t}"`).join(', ')}]\n---\n\n`;
        await invoke('update_note', { vaultPath: props.vaultPath, path, content: frontmatter + payload.content });
        
        const index = quickCaps.value.findIndex(c => c.id === cap.id);
        if (index !== -1) {
            await invoke('delete_note', { vaultPath: props.vaultPath, path: cap.path });
            quickCaps.value.splice(index, 1);
        }
        
        await emitTauri('vault-changed');
        closeNoteModal();
    } catch(e) {
        console.error("Failed to convert to note", e);
        await message('Lỗi khi chuyển thành Note.', { title: 'Synabit', kind: 'error' });
    }
};

const confirmTurnIntoTask = async (payload: any) => {
    const cap = convertingTaskCap.value;
    if (!cap) return;
    try {
        const tagArray = payload.tags.split(',').map((t: string) => t.trim()).filter((t: string) => t !== '');
        
        await invoke('create_task', {
            vaultPath: props.vaultPath,
            metadata: {
                title: payload.title || 'Untitled',
                status: payload.status,
                is_transferred: payload.is_transferred,
                transferred_to: payload.transferred_to,
                track_progress: payload.track_progress,
                priority: payload.priority,
                start_date: payload.start_date,
                due_date: payload.due_date,
                comment: payload.comment,
                source_link: cap.path,
                tags: tagArray,
                checklist: payload.checklist
            },
            content: payload.content
        });
        
        const index = quickCaps.value.findIndex(c => c.id === cap.id);
        if (index !== -1) {
            await invoke('delete_note', { vaultPath: props.vaultPath, path: cap.path });
            quickCaps.value.splice(index, 1);
        }
        
        closeTaskModal();
    } catch(e) {
        console.error("Failed to create task", e);
        await message('Lỗi khi tạo Task.', { title: 'Synabit', kind: 'error' });
    }
};

onMounted(() => {
    loadCaps();
    window.addEventListener('paste', handleGlobalPaste);
});

onUnmounted(() => {
    window.removeEventListener('paste', handleGlobalPaste);
    if (editor.value) editor.value.destroy();
});

watch(() => props.vaultPath, () => {
    loadCaps();
});

const extractTags = (content: string) => {
    if (!content) return [];
    const tags: string[] = [];
    
    // 1. Extract Bear-style wrapped tags: #tag name#
    const wrappedMatches = [...content.matchAll(/(?:^|\s)#([^#\n]+)#(?=\s|$)/g)];
    wrappedMatches.forEach(m => tags.push(m[1].trim()));
    
    // 2. Remove them so we don't accidentally match parts of them next
    let remaining = content.replace(/(?:^|\s)#([^#\n]+)#(?=\s|$)/g, ' ');
    
    // 3. Extract traditional tags: #tag
    const tradMatches = [...remaining.matchAll(/(?:^|\s)#([a-zA-Z0-9_\-\u00C0-\u024F\u1E00-\u1EFF]+)(?=\s|$)/g)];
    tradMatches.forEach(m => tags.push(m[1].trim()));
    
    return Array.from(new Set(tags));
};

const removeTag = async (cap: QuickCapMetadata, tag: string) => {
    const isConfirmed = await confirm(`Bạn có chắc chắn muốn xoá tag [${tag}]?`, { title: 'Xoá tag', kind: 'warning' });
    if (!isConfirmed) return;
    
    // Escape tag to safely use in regex
    const safeTag = tag.replace(/[-[\]{}()*+?.,\\^$|#\s]/g, '\\$&');
    
    // Attempt to remove wrapped version first
    let regexWrapped = new RegExp(`(?:^|\\s)#${safeTag}#(?=\\s|$)`, 'g');
    let updatedContent = cap.content.replace(regexWrapped, ' ');
    
    // Attempt to remove traditional version
    let regexTrad = new RegExp(`(?:^|\\s)#${safeTag}(?=\\s|$)`, 'g');
    updatedContent = updatedContent.replace(regexTrad, ' ').trim();
    
    // Clean up excessive newlines caused by tag removal
    updatedContent = updatedContent.replace(/\n{3,}/g, '\n\n').trim();
    
    try {
        await invoke('update_note', { vaultPath: props.vaultPath, path: cap.path, content: updatedContent });
        cap.content = updatedContent;
    } catch(e) {
        console.error("Failed to remove tag", e);
    }
};

const removeActiveTag = (tag: string) => {
    currentTags.value = currentTags.value.filter(t => t !== tag);
    
    const safeTag = tag.replace(/[-[\]{}()*+?.,\\^$|#\s]/g, '\\$&');
    let regexWrapped = new RegExp(`(?:^|\\s)#${safeTag}#(?=\\s|$)`, 'g');
    let regexTrad = new RegExp(`(?:^|\\s)#${safeTag}(?=\\s|$)`, 'g');
    
    let updatedContent = editingContent.value.replace(regexWrapped, ' ');
    updatedContent = updatedContent.replace(regexTrad, ' ').trim();
    updatedContent = updatedContent.replace(/\n{3,}/g, '\n\n').trim();
    
    editingContent.value = updatedContent;
    if (editor.value) {
       editor.value.commands.setContent(injectLocalAssets(updatedContent));
    }
    
    if (saveTimeout) clearTimeout(saveTimeout);
    saveTimeout = setTimeout(saveSelectedCap, 1000);
};

const renderPreview = (content: string) => {
    if (!content) return '';
    
    // Remove tags from the main text body so they are only displayed as bottom chips
    let textBody = content.trim();
    textBody = textBody.replace(/<!--color:.*?-->\n?/g, ''); // hide color code
    textBody = textBody.replace(/(?:^|\s)#([^#\n]+)#(?=\s|$)/g, ' ');
    textBody = textBody.replace(/(?:^|\s)#[a-zA-Z0-9_\-\u00C0-\u024F\u1E00-\u1EFF]+(?=\s|$)/g, ' ').trim();

    // Escape HTML to prevent XSS
    let html = textBody
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;");
        
    // Process auto-links: <http...>
    html = html.replace(/&lt;(https?:\/\/[^\s"'<]+)&gt;/g, '<a href="$1" target="_blank" class="text-blue-500 hover:underline break-all" @click.stop>$1</a>');
    
    // Process standard markdown links: [text](http...)
    html = html.replace(/(^|[^!])\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g, '$1<a href="$3" target="_blank" class="text-blue-500 hover:underline break-all" @click.stop>$2</a>');
        
    // Process markdown images: ![alt](url)
    html = html.replace(/!\[(.*?)\]\((.*?)\)/g, (_match, alt, path) => {
        let absPath = path;
        try { path = decodeURIComponent(path); } catch(e) {}
        
        const cleanVaultPath = props.vaultPath.endsWith('/') || props.vaultPath.endsWith('\\') 
             ? props.vaultPath.slice(0, -1) : props.vaultPath;
        const sep = cleanVaultPath.includes('\\') ? '\\' : '/';
        
        if (path.startsWith('assets/')) {
            absPath = `${cleanVaultPath}${sep}${path}`;
        }
        const src = convertFileSrc(absPath);
        return `<img src="${src}" alt="${alt}" class="max-w-full max-h-64 object-contain rounded-lg my-2 border border-gray-200 dark:border-[#2c2c2c]" loading="lazy" />`;
    });
    
    // Process HTML images exported by raw Markdown serializers
    html = html.replace(/&lt;img.*?src=["'](.*?)["'].*?&gt;/g, (_match, path) => {
        let absPath = path;
        try { path = decodeURIComponent(path); } catch(e) {}
        
        const cleanVaultPath = props.vaultPath.endsWith('/') || props.vaultPath.endsWith('\\') 
             ? props.vaultPath.slice(0, -1) : props.vaultPath;
        const sep = cleanVaultPath.includes('\\') ? '\\' : '/';
        
        const assetMatch = path.match(/assets(%2F|\/)([^?&'"]+)/);
        if (assetMatch) {
            absPath = `${cleanVaultPath}${sep}assets${sep}${decodeURIComponent(assetMatch[2])}`;
        } else if (path.startsWith('assets/')) {
            absPath = `${cleanVaultPath}${sep}${path}`;
        }
        const src = convertFileSrc(absPath);
        return `<img src="${src}" class="max-w-full max-h-64 object-contain rounded-lg my-2 border border-gray-200 dark:border-[#2c2c2c]" loading="lazy" />`;
    });
    
    return html;
};

const deleteCap = async (path: string, index: number) => {
    const isConfirmed = await confirm('Bạn có chắc chắn muốn xoá ghi chú này không?', { title: 'Xác nhận xoá', kind: 'warning' });
    if (!isConfirmed) return;
    
    try {
        await invoke('delete_note', { vaultPath: props.vaultPath, path });
        quickCaps.value.splice(index, 1);
    } catch(e) {
        console.error(e);
    }
};
</script>

<template>
  <div class="h-full bg-[#fdfdfc] dark:bg-[#242424] overflow-y-auto w-full pt-12 pb-16 px-4">
    <!-- Input Bar -->
    <div class="mx-auto w-full max-w-2xl bg-white dark:bg-[#1e1e1e] rounded-xl shadow-[0_2px_8px_rgba(0,0,0,0.04)] dark:shadow-[0_2px_8px_rgba(0,0,0,0.2)] border border-[#e6e6e6] dark:border-[#2c2c2c] overflow-hidden focus-within:ring-1 focus-within:ring-black dark:focus-within:ring-white transition-all relative mb-12">
        <textarea
           ref="inputRef"
           v-model="newCapText"
           @input="handleInput"
           @keydown.enter.ctrl="submitCap"
           @keydown.enter.meta="submitCap"
           placeholder="Take a quick note... (Cmd+Enter to save)"
           class="w-full bg-transparent p-5 min-h-[60px] max-h-[400px] resize-none outline-none text-[#1c1c1e] dark:text-[#f4f4f5] pb-14 overflow-y-auto"
        ></textarea>
        <!-- Actions bottom bar -->
        <div class="absolute bottom-0 left-0 w-full flex items-center justify-between p-2 px-3 bg-white dark:bg-[#1e1e1e]">
           <div class="flex items-center gap-1 opacity-70">
              <button title="Lists coming soon" class="p-2 rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-[#2a2a2a] transition-colors cursor-pointer">
                  <CheckSquare class="w-4 h-4"/>
              </button>
              <button @click="pickImageForNewCap" title="Pick an image to upload" class="p-2 rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-[#2a2a2a] transition-colors cursor-pointer">
                  <ImageIcon class="w-4 h-4"/>
              </button>
              <button @click="appendTagToInput" title="Add Tag" class="p-2 rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-[#2a2a2a] transition-colors cursor-pointer">
                  <Tag class="w-4 h-4"/>
              </button>
           </div>
           <button @click="submitCap" :disabled="isSubmitting || !newCapText.trim()" class="px-5 py-1.5 bg-black dark:bg-white text-white dark:text-black rounded-lg text-sm font-semibold hover:scale-95 transition-all disabled:opacity-50 cursor-pointer shadow-sm">
               Save
           </button>
        </div>
    </div>
    
    <!-- Filter Bar -->
    <div class="w-full max-w-7xl px-4 flex items-center justify-between mb-8 mx-auto -mt-4">
        <div class="relative w-full sm:max-w-xs group">
            <div class="absolute inset-y-0 left-0 pl-3.5 flex items-center pointer-events-none">
                <Search class="h-4 w-4 text-gray-400 group-focus-within:text-blue-500 transition-colors" />
            </div>
            <input 
                v-model="searchQuery" 
                type="text" 
                class="block w-full pl-10 pr-3 py-2 border border-gray-200 dark:border-[#2c2c2c] rounded-full leading-5 bg-white dark:bg-[#1e1e1e] text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-black/5 dark:focus:ring-white/10 sm:text-sm transition-all shadow-[0_2px_8px_rgba(0,0,0,0.02)]" 
                placeholder="Search text or #tag..." 
            />
            <button v-if="searchQuery" @click="searchQuery = ''" class="absolute inset-y-0 right-0 pr-3 flex items-center cursor-pointer">
                <X class="h-4 w-4 text-gray-400 hover:text-gray-600 transition-colors" />
            </button>
        </div>
    </div>

    <!-- Masonry Grid -->
    <div class="w-full max-w-7xl px-4 columns-1 sm:columns-2 lg:columns-3 xl:columns-4 gap-6 mx-auto">
        <div v-for="cap in filteredCaps" :key="cap.id" class="break-inside-avoid relative group mb-6 inline-block w-full cursor-pointer" @click="openFullView(cap)">
            <div class="rounded-2xl shadow-sm hover:shadow-md border border-[#e6e6e6] dark:border-[#2c2c2c] transition-all relative flex flex-col" :class="getCapColor(cap.content) || 'bg-white dark:bg-[#1e1e1e]'" style="max-height: 320px;">
               <!-- Text Content Wrapper -->
               <div class="p-5 pb-0 flex-1 overflow-hidden relative" :style="(cap.content.length > 250 || cap.content.split('\n').length > 6) ? '-webkit-mask-image: linear-gradient(to bottom, black 60%, transparent 100%); mask-image: linear-gradient(to bottom, black 60%, transparent 100%);' : ''">
                   <div class="whitespace-pre-wrap text-[15px] font-medium leading-normal text-[#1c1c1e] dark:text-[#f4f4f5] break-words" v-html="renderPreview(cap.content)"></div>
               </div>
               
               <!-- Tags Wrapper (Always visible) -->
               <div class="px-5 pt-3 pb-11 relative z-10 w-full shrink-0">
                   <div v-if="extractTags(cap.content).length > 0" class="flex flex-wrap gap-1.5 w-full">
                       <span v-for="tag in extractTags(cap.content)" :key="tag" class="group/tag inline-flex items-center text-[11px] font-medium text-gray-600 dark:text-gray-300 bg-gray-100 dark:bg-[#2a2a2a] px-2 py-0.5 rounded-md transition-colors border border-transparent hover:border-gray-300 dark:hover:border-gray-500 cursor-default">
                           {{ tag }}
                           <button @click.stop="removeTag(cap, tag)" class="ml-1 opacity-0 w-0 overflow-hidden group-hover/tag:opacity-100 group-hover/tag:w-auto transition-all text-gray-400 hover:text-red-500 cursor-pointer">
                               <X class="w-2.5 h-2.5" />
                           </button>
                       </span>
                   </div>
               </div>

               <!-- Bottom Actions Bar (Fixed at bottom of card) -->
               <div class="absolute bottom-0 left-0 w-full px-4 py-2 border-t border-transparent group-hover:border-black/5 dark:group-hover:border-white/5 flex items-center justify-between z-10 transition-colors">
                   <!-- Date (visible by default, hidden on hover) -->
                  <span class="text-[11px] text-gray-400 font-mono tracking-tight group-hover:opacity-0 transition-opacity absolute px-1 pointer-events-none">{{ cap.date }}</span>
                  
                  <!-- Actions (hidden by default, visible on hover) -->
                  <div class="flex items-center opacity-0 group-hover:opacity-100 transition-opacity w-full justify-between" @click.stop>
                      <div v-if="taggingCapId === cap.id" class="flex items-center w-full bg-gray-50 dark:bg-[#1a1a1a] rounded px-2 py-0.5 mr-2">
                          <span class="text-gray-400 text-xs mr-1">#</span>
                          <input 
                              v-model="tagInputText" 
                              @keydown.enter.prevent="saveInlineTag(cap)"
                              @keydown.esc="taggingCapId = null"
                              class="bg-transparent border-none outline-none text-xs w-full text-[#1c1c1e] dark:text-[#f4f4f5]"
                              placeholder="tag..."
                              autofocus
                          />
                          <button @click="saveInlineTag(cap)" class="ml-1 text-black dark:text-white font-medium text-[11px] hover:underline">Save</button>
                      </div>
                      <template v-else>
                          <button @click.stop="deleteCap(cap.path, quickCaps.findIndex(c => c.id === cap.id))" title="Delete note" class="text-gray-400 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 p-1.5 rounded-full transition-colors cursor-pointer">
                              <Trash2 class="w-3.5 h-3.5"/>
                          </button>
                          <div class="flex items-center gap-0.5 relative">
                              <div class="relative">
                                  <button @click.stop="toggleColorPicker(cap.id)" title="Change Color" class="text-gray-400 hover:text-gray-800 dark:hover:text-gray-200 hover:bg-black/5 dark:hover:bg-white/10 p-1.5 rounded-full transition-colors cursor-pointer">
                                      <Palette class="w-3.5 h-3.5"/>
                                  </button>
                                  
                                  <!-- Color Picker Popup -->
                                  <div v-if="colorPickerCapId === cap.id" class="absolute bottom-[calc(100%+8px)] right-0 p-2 bg-white dark:bg-[#2a2a2a] rounded-xl shadow-xl border border-gray-100 dark:border-gray-700 flex flex-wrap gap-2 z-50 w-[140px]" @click.stop>
                                      <button v-for="color in PALETTE" :key="color.name" 
                                          @click="changeCapColor(cap, color.value)"
                                          class="w-6 h-6 rounded-full border border-gray-200 dark:border-gray-600 transition-transform hover:scale-110 cursor-pointer"
                                          :class="color.value || 'bg-[#fdfdfc] dark:bg-[#1e1e1e]'"
                                          :title="color.name"
                                      ></button>
                                  </div>
                              </div>
                              <button @click.stop="openConvertNoteModal(cap)" title="Convert to Note" class="text-gray-400 hover:text-gray-800 dark:hover:text-gray-200 hover:bg-black/5 dark:hover:bg-white/10 p-1.5 rounded-full transition-colors cursor-pointer">
                                  <FileText class="w-3.5 h-3.5" />
                              </button>
                              <button @click.stop="openConvertTaskModal(cap)" title="Capture to Task" class="text-gray-400 hover:text-gray-800 dark:hover:text-gray-200 hover:bg-black/5 dark:hover:bg-white/10 p-1.5 rounded-full transition-colors cursor-pointer">
                                  <CheckSquare class="w-3.5 h-3.5"/>
                              </button>
                              <button @click.stop="pickImageForExistingCap(cap)" title="Add Image" class="text-gray-400 hover:text-gray-800 dark:hover:text-gray-200 hover:bg-black/5 dark:hover:bg-white/10 p-1.5 rounded-full transition-colors cursor-pointer">
                                  <ImageIcon class="w-3.5 h-3.5"/>
                              </button>
                              <button @click="openTagInput(cap)" title="Add Tag" class="text-gray-400 hover:text-gray-800 dark:hover:text-gray-200 hover:bg-black/5 dark:hover:bg-white/10 p-1.5 rounded-full transition-colors cursor-pointer">
                                  <Tag class="w-3.5 h-3.5"/>
                              </button>
                          </div>
                      </template>
                  </div>
               </div>
            </div>
        </div>
    </div>

    <!-- Empty State -->
    <div v-if="quickCaps.length === 0" class="flex flex-col items-center justify-center opacity-30 mt-12 w-full">
        <CheckSquare class="w-16 h-16 mb-4"/>
        <p class="text-lg">No quick caps yet. Jot down your thoughts!</p>
    </div>

    <!-- Full View Modal -->
    <div v-if="selectedCap" class="fixed inset-0 z-[60] flex items-center justify-center p-4 bg-black/40 dark:bg-black/60 backdrop-blur-sm" @click="closeFullView">
        <div class="w-full max-w-2xl max-h-[85vh] rounded-2xl shadow-xl flex flex-col border border-[#e6e6e6] dark:border-[#2c2c2c] overflow-hidden" :class="getCapColor(selectedCap.content) || 'bg-white dark:bg-[#1e1e1e]'" @click.stop>
            <div class="p-8 overflow-y-auto flex-1 flex flex-col min-h-0 bg-transparent">
                <EditorContent :editor="editor" class="w-full" />
                
                <!-- Render tags as chips in modal -->
                <div v-if="activeTags.length > 0" class="flex flex-wrap gap-2 mt-6 relative z-10 w-full shrink-0 pt-4 border-t border-gray-100 dark:border-[#2c2c2c]">
                   <span v-for="tag in activeTags" :key="tag" class="group/tag inline-flex items-center text-[12px] font-semibold text-gray-600 dark:text-gray-300 bg-gray-100 dark:bg-[#2a2a2a] px-2.5 py-1 rounded-md transition-colors border border-transparent hover:border-gray-300 dark:hover:border-gray-500 cursor-default">
                       {{ tag }}
                       <button @click.stop="removeActiveTag(tag)" class="ml-1 opacity-0 w-0 overflow-hidden group-hover/tag:opacity-100 group-hover/tag:w-auto transition-all text-gray-400 hover:text-red-500 cursor-pointer">
                           <X class="w-3 h-3" />
                       </button>
                   </span>
                </div>
            </div>
            <div class="py-4 px-6 bg-gray-50 dark:bg-[#191919] border-t border-[#e6e6e6] dark:border-[#2c2c2c] flex items-center justify-between mt-auto shrink-0">
                <span class="text-xs text-gray-500 font-mono tracking-tight">{{ selectedCap.date }}</span>
                <button @click="closeFullView" class="px-5 py-2 bg-black dark:bg-white text-white dark:text-black rounded-lg text-sm font-semibold hover:scale-95 transition-all shadow-sm cursor-pointer">
                    Close
                </button>
            </div>
        </div>
    </div>

    <!-- Convert to Task Modal -->
    <TaskEditModal 
        v-if="convertingTaskCap" 
        :task="convertingTaskParams" 
        :showActions="true"
        @save="confirmTurnIntoTask" 
        @close="closeTaskModal" 
    />
    
    <!-- Convert to Note Modal -->
    <NoteEditModal 
        v-if="convertingNoteCap"
        :note="convertingNoteParams"
        @save="confirmTurnIntoNote"
        @close="closeNoteModal"
    />
  </div>
</template>
