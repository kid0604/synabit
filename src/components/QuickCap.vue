<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch, computed } from 'vue';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { confirm, message, open as openDialog } from '@tauri-apps/plugin-dialog';
import { CheckSquare, Image as ImageIcon, Trash2, Palette, Tag, X, Search } from 'lucide-vue-next';

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
const editInputRef = ref<HTMLTextAreaElement | null>(null);
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
        await invoke('update_note', { path: cap.path, content: updatedContent });
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
        await invoke('update_note', { path: cap.path, content: updatedContent });
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
        await invoke('update_note', { path: selectedCap.value.path, content: finalPayload });
        selectedCap.value.content = finalPayload;
    } catch(e) {
        console.error("Failed to update note", e);
    }
};

const resizeEditingTextarea = () => {
    if (editInputRef.value) {
        editInputRef.value.style.height = 'auto';
        editInputRef.value.style.height = editInputRef.value.scrollHeight + 'px';
    }
};

const handleEditInput = () => {
    resizeEditingTextarea();
    if (saveTimeout) clearTimeout(saveTimeout);
    saveTimeout = setTimeout(() => {
        saveSelectedCap();
    }, 1000);
};

const openFullView = (cap: QuickCapMetadata) => {
    selectedCap.value = cap;
    let rawStr = cap.content.replace(/<!--color:.*?-->\n?/g, '').trim();
    
    currentTags.value = extractTags(rawStr);
    
    let textOnly = rawStr;
    textOnly = textOnly.replace(/(?:^|\s)#([^#\n]+)#(?=\s|$)/g, ' ');
    textOnly = textOnly.replace(/(?:^|\s)#[a-zA-Z0-9_\-\u00C0-\u024F\u1E00-\u1EFF]+(?=\s|$)/g, ' ');
    textOnly = textOnly.replace(/\n{3,}/g, '\n\n').trim();
    
    editingContent.value = textOnly;
    
    setTimeout(() => {
        resizeEditingTextarea();
    }, 50);
};

const closeFullView = async () => {
    if (saveTimeout) clearTimeout(saveTimeout);
    await saveSelectedCap();
    selectedCap.value = null;
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
          const filename = file.name || 'pasted-image.png';
          
          const oldPlaceholder = inputRef.value?.placeholder;
          if (inputRef.value) inputRef.value.placeholder = "Uploading image...";
          isSubmitting.value = true;
          try {
             const assetPath = await invoke<string>('save_asset', {
                vaultPath: props.vaultPath,
                filename,
                bytes
             });
             // Replace absolute path marker with safe tauri URL mapping if needed, but for now just text
             const imgMd = `![Image](${assetPath})`;
             const start = inputRef.value?.selectionStart || newCapText.value.length;
             const end = inputRef.value?.selectionEnd || newCapText.value.length;
             newCapText.value = newCapText.value.substring(0, start) + "\n" + imgMd + "\n" + newCapText.value.substring(end);
          } catch(err) {
             console.error(err);
          } finally {
             isSubmitting.value = false;
             if (inputRef.value) inputRef.value.placeholder = oldPlaceholder || "Take a quick note...";
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
            await invoke('update_note', { path: cap.path, content: updatedContent });
            cap.content = updatedContent;
        }
    } catch(e) {
        console.error("Failed to pick image", e);
    }
};

const convertingTaskCap = ref<QuickCapMetadata | null>(null);
const convertingTaskParams = ref({
    title: '',
    content: ''
});

const openConvertTaskModal = (cap: QuickCapMetadata) => {
    convertingTaskCap.value = cap;
    const cleanContent = cap.content.replace(/<!--color:.*?-->/g, '').trim();
    const displayLines = cleanContent.split('\n').filter(l => l.trim() !== '');
    convertingTaskParams.value.title = displayLines.length > 0 ? displayLines[0].substring(0, 50) + (displayLines[0].length > 50 ? '...' : '') : 'QuickCap Task';
    convertingTaskParams.value.content = cleanContent;
};

const closeTaskModal = () => {
    convertingTaskCap.value = null;
};

const confirmTurnIntoTask = async () => {
    const cap = convertingTaskCap.value;
    if (!cap) return;
    try {
        await invoke('create_task', {
            vaultPath: props.vaultPath,
            metadata: {
                title: convertingTaskParams.value.title,
                status: 'todo',
                start_date: '',
                due_date: '',
                comment: '',
                source_link: cap.path,
                tags: extractTags(cap.content)
            },
            content: convertingTaskParams.value.content
        });
        
        const index = quickCaps.value.findIndex(c => c.id === cap.id);
        if (index !== -1) {
            await invoke('delete_note', { path: cap.path });
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
        await invoke('update_note', { path: cap.path, content: updatedContent });
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
    handleEditInput();
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
        
    // Process markdown images: ![alt](url)
    html = html.replace(/!\[.*?\]\((.*?)\)/g, (match, path) => {
        let absPath = path;
        if (path.startsWith('assets/')) {
            absPath = `${props.vaultPath}/${path}`;
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
        await invoke('delete_note', { path });
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
        <div v-for="(cap, i) in filteredCaps" :key="cap.id" class="break-inside-avoid relative group mb-6 inline-block w-full cursor-pointer" @click="openFullView(cap)">
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
    <div v-if="selectedCap" class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/40 dark:bg-black/60 backdrop-blur-sm" @click="closeFullView">
        <div class="w-full max-w-2xl max-h-[85vh] rounded-2xl shadow-xl flex flex-col border border-[#e6e6e6] dark:border-[#2c2c2c] overflow-hidden" :class="getCapColor(selectedCap.content) || 'bg-white dark:bg-[#1e1e1e]'" @click.stop>
            <div class="p-8 overflow-y-auto flex-1 flex flex-col min-h-0 bg-transparent">
                <textarea 
                    ref="editInputRef"
                    v-model="editingContent"
                    @input="handleEditInput"
                    class="w-full shrink-0 bg-transparent resize-none outline-none text-[16px] leading-relaxed text-[#1c1c1e] dark:text-[#f4f4f5] border-none focus:ring-0 appearance-none m-0 p-0 overflow-hidden min-h-[100px]"
                    placeholder="Note content..."
                    autofocus
                ></textarea>
                
                <!-- Render tags as chips in modal -->
                <div v-if="activeTags.length > 0" class="flex flex-wrap gap-2 mt-6 relative z-10 w-full shrink-0">
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
    <div v-if="convertingTaskCap" class="fixed inset-0 z-[60] flex items-center justify-center p-4 bg-black/40 dark:bg-black/60 backdrop-blur-sm" @click="closeTaskModal">
        <div class="w-full max-w-lg rounded-2xl shadow-xl flex flex-col border border-[#e6e6e6] dark:border-[#2c2c2c] bg-white dark:bg-[#1e1e1e] overflow-hidden" @click.stop>
            <div class="p-6 border-b border-[#e6e6e6] dark:border-[#2c2c2c]">
                <h3 class="text-xl font-bold flex items-center gap-2 text-[#1c1c1e] dark:text-[#f4f4f5]">
                    <CheckSquare class="w-6 h-6 text-black dark:text-white" />
                    Conver to Task
                </h3>
                <p class="text-sm text-gray-500 mt-1">Sắp xếp lại suy nghĩ trước khi chốt thành Action.</p>
            </div>
            <div class="p-6 flex flex-col gap-4">
                <div>
                    <label class="block text-xs font-semibold uppercase tracking-wide text-gray-500 mb-1">Task Title <span class="text-red-500">*</span></label>
                    <input 
                        v-model="convertingTaskParams.title" 
                        class="w-full bg-gray-50 dark:bg-[#191919] border border-gray-200 dark:border-gray-700 rounded-lg p-3 outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-all text-[#1c1c1e] dark:text-[#f4f4f5]"
                        placeholder="Task Name"
                    />
                </div>
                <div>
                    <label class="block text-xs font-semibold uppercase tracking-wide text-gray-500 mb-1">Description</label>
                    <textarea 
                        v-model="convertingTaskParams.content" 
                        class="w-full bg-gray-50 dark:bg-[#191919] border border-gray-200 dark:border-gray-700 rounded-lg p-3 min-h-[120px] outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-all text-[#1c1c1e] dark:text-[#f4f4f5]"
                        placeholder="Task Details..."
                    ></textarea>
                </div>
            </div>
            <div class="py-4 px-6 bg-gray-50 dark:bg-[#191919] border-t border-[#e6e6e6] dark:border-[#2c2c2c] flex items-center justify-end gap-3 rounded-b-2xl">
                <button @click="closeTaskModal" class="px-5 py-2 hover:bg-gray-200 dark:hover:bg-[#2c2c2c] text-gray-700 dark:text-gray-300 rounded-lg text-sm font-medium transition-all cursor-pointer">
                    Cancel
                </button>
                <button @click="confirmTurnIntoTask" class="px-5 py-2 bg-black dark:bg-white text-white dark:text-black rounded-lg text-sm font-semibold hover:scale-95 transition-all shadow-sm cursor-pointer flex items-center gap-1.5">
                    <CheckSquare class="w-4 h-4" /> Create Task
                </button>
            </div>
        </div>
    </div>
  </div>
</template>
