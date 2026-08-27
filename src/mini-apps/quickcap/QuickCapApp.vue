<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch, computed, nextTick } from 'vue';
import { useI18n } from 'vue-i18n';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import DOMPurify from 'dompurify';
import { useIntersectionObserver, useWindowSize } from '@vueuse/core';
import { useEventBus } from '../../composables/useEventBus';
import { usePlatform } from '../../composables/usePlatform';
import { useNodeService } from '../../composables/useNodeService';
import { ask, message, open as openDialog } from '@tauri-apps/plugin-dialog';
import { CheckSquare, Image as ImageIcon, Trash2, Palette, Tag, X, Search, FileText, LayoutGrid, List, Plus, Mic, Square, Pin, Archive } from 'lucide-vue-next';
import { useEditor, EditorContent } from '@tiptap/vue-3';
import StarterKit from '@tiptap/starter-kit';
import { Markdown } from 'tiptap-markdown';
import TiptapImage from '@tiptap/extension-image';
import Placeholder from '@tiptap/extension-placeholder';
import TaskEditModal from '../task/TaskEditModal.vue';
import NoteEditModal from '../note/NoteEditModal.vue';
import NavButtons from '../../shared/components/NavButtons.vue';
import PromoteModal from './PromoteModal.vue';
import TransactionModal from '../finance/TransactionModal.vue';
import { appendTransaction, loadFinanceSetup, repairFinanceStorage, type FinanceSetup } from '../finance/ledger';
import { currentCurrency } from '../finance/currency';
import type { Transaction } from '../finance/types';
import type { PromoteTarget } from './PromoteModal.vue';
import { logger } from '../../utils/logger';
import { CAP_COLOURS, colourClass, deriveTitle, extractTags, removeTagFromContent, stripColorComment } from './parsing';
import { makeThumbnail, thumbnailNameFor } from '../../shared/thumbnails';
import { useQuickCapWriter } from './useQuickCapWriter';
import { useAudioCapture, formatDuration } from './useAudioCapture';

const bus = useEventBus();
const ns = useNodeService();
const { t, locale } = useI18n();
const { isMobileOS } = usePlatform();
const { writeCap, createCap } = useQuickCapWriter();
const audio = useAudioCapture();

const props = defineProps<{
  vaultPath: string;
}>();

export interface NodeMetadata {
    /** Path relative to the vault root — what the backend takes as `relPath`. */
    id: string;
    node_type: string;
    title: string;
    content: string;
    created_at: string;
    updated_at: string;
    properties: any;
    color?: string;
    tags?: string[];
}

const quickCaps = ref<NodeMetadata[]>([]);
const newCapText = ref('');
const isSubmitting = ref(false);
const inputRef = ref<HTMLTextAreaElement | null>(null);
const selectedCap = ref<NodeMetadata | null>(null);

const editingContent = ref('');
let saveTimeout: ReturnType<typeof setTimeout> | null = null;


const taggingCapId = ref<string | null>(null);
const colorPickerCapId = ref<string | null>(null);
const tagInputText = ref('');
const searchQuery = ref('');
const mobileViewMode = ref<'list' | 'grid'>('list');
const backendSearchIds = ref<string[] | null>(null);
let qcSearchTimeout: ReturnType<typeof setTimeout>;

const isMobileModalOpen = ref(false);
const mobileInputRef = ref<HTMLTextAreaElement | null>(null);

watch(isMobileModalOpen, (isOpen) => {
    if (isOpen) {
        nextTick(() => {
            mobileInputRef.value?.focus();
        });
    }
});

const submitCapMobile = async () => {
    await submitCap();
    if (!isSubmitting.value && !hasDraft.value) {
        isMobileModalOpen.value = false;
    }
};

/** The picker offers "no colour" plus whatever the contract defines. The
 *  value stored is the name; classes are resolved at render time. */
const PALETTE = [
   { name: 'default', value: '' },
   ...CAP_COLOURS.map((c) => ({ name: c.name, value: c.name })),
];

/**
 * How long a cap may sit before it counts as cold.
 *
 * Ahrens says process a fleeting note within a day or two. Fourteen days is
 * generous against that, and deliberately so: this is not a deadline, it is
 * the point at which a thought has demonstrably stopped being acted on.
 *
 * The number matters less than there being one. A capture inbox with no sense
 * of age is a junk drawer, which is exactly how Google Keep fails.
 */
const COLD_AFTER_DAYS = 14;

const isCold = (cap: NodeMetadata) => {
    const created = new Date(cap.created_at);
    if (Number.isNaN(created.getTime())) return false;
    return Date.now() - created.getTime() > COLD_AFTER_DAYS * 86_400_000;
};

/** Which slice of the inbox is on screen. */
const ageFilter = ref<'all' | 'fresh' | 'cold' | 'archived'>('all');

const coldCount = computed(() => quickCaps.value.filter((c) => isCold(c) && !isArchived(c)).length);
const archivedCount = computed(() => quickCaps.value.filter(isArchived).length);

watch(ageFilter, () => {
    renderLimit.value = RENDER_BATCH;
});

const filteredCaps = computed(() => {
    // Put-away caps are out of every view except their own: that is the
    // difference between filing something and merely ignoring it.
    const inbox = quickCaps.value.filter((cap) => !isArchived(cap));

    const byAge =
        ageFilter.value === 'archived'
            ? quickCaps.value.filter(isArchived)
            : ageFilter.value === 'all'
              ? inbox
              : inbox.filter((cap) => (ageFilter.value === 'cold' ? isCold(cap) : !isCold(cap)));

    // Pinned first, and only then by recency. A pin is the user overruling
    // the sort, so nothing else may overrule the pin.
    byAge.sort((a, b) => Number(isPinned(b)) - Number(isPinned(a)));

    const q = searchQuery.value.trim().toLowerCase();
    if (!q) return byAge;
    
    // Backend FTS5 results available
    if (backendSearchIds.value !== null) {
        const idSet = new Set(backendSearchIds.value);
        const filtered = byAge.filter(cap => idSet.has(cap.id));
        const orderMap = new Map(backendSearchIds.value.map((id, i) => [id, i]));
        return filtered.sort((a, b) => (orderMap.get(a.id) ?? 999) - (orderMap.get(b.id) ?? 999));
    }
    
    // Fallback: local search while backend is loading
    const isTagSearch = q.startsWith('#');
    const tagQuery = isTagSearch ? q.substring(1) : q;
    
    return byAge.filter(cap => {
        if (isTagSearch) {
            const tags = extractTags(cap.content).map(t => t.toLowerCase());
            return tags.some(t => t.includes(tagQuery));
        } else {
            return cap.content.toLowerCase().includes(q);
        }
    });
});

// Debounced backend search for QuickCap
watch(searchQuery, (q) => {
    clearTimeout(qcSearchTimeout);
    if (!q.trim()) {
        backendSearchIds.value = null;
        return;
    }
    qcSearchTimeout = setTimeout(async () => {
        try {
            const resp = await invoke<{ results: { id: string }[], total_count: number, query_time_ms: number }>('search_quickcaps', {
                vaultPath: props.vaultPath,
                query: q
            });
            if (searchQuery.value === q) {
                backendSearchIds.value = resp.results.map(r => r.id);
            }
        } catch (e) {
            logger.error('QuickCap backend search error', e);
        }
    }, 200);
});

/** The cap's tags are whatever its body says they are — there is no second
 *  list to keep in step, because nothing moves them out of the text. */
const activeTags = computed(() => extractTags(editingContent.value));

const appendTagToInput = () => {
    newCapText.value += (newCapText.value && !newCapText.value.endsWith(' ') && !newCapText.value.endsWith('\n') ? ' #' : '#');
    inputRef.value?.focus();
};

const openTagInput = (cap: NodeMetadata) => {
    taggingCapId.value = cap.id;
    tagInputText.value = '';
};

const saveInlineTag = async (cap: NodeMetadata) => {
    if (!tagInputText.value.trim()) {
        taggingCapId.value = null;
        return;
    }
    const rawTag = tagInputText.value.trim().replace(/^#/, '').replace(/#$/, '');
    const formattedTag = rawTag.includes(' ') ? `#${rawTag}#` : `#${rawTag}`;

    // While a cap is open the editor holds the newer text; appending to the
    // stored copy would throw away whatever has not autosaved yet.
    if (selectedCap.value && selectedCap.value.id === cap.id) {
        const body = editingContent.value.trim();
        editingContent.value = body ? `${body}\n\n${formattedTag}` : formattedTag;
        editor.value?.commands.setContent(injectLocalAssets(editingContent.value));
        taggingCapId.value = null;
        tagInputText.value = '';
        if (saveTimeout) clearTimeout(saveTimeout);
        saveTimeout = setTimeout(saveSelectedCap, 1000);
        return;
    }

    const updatedContent = `${cap.content}\n\n${formattedTag}`;
    try {
        await writeCap({ relPath: cap.id, nodeType: cap.node_type, properties: cap.properties, content: updatedContent });
        cap.content = updatedContent;
        taggingCapId.value = null;
        tagInputText.value = '';
    } catch(e) {
        logger.error("Failed to update note", e);
    }
};



const toggleColorPicker = (capId: string) => {
    if (colorPickerCapId.value === capId) {
        colorPickerCapId.value = null;
    } else {
        colorPickerCapId.value = capId;
    }
};

const changeCapColor = async (cap: NodeMetadata, colorName: string) => {
    // The colour is a property, and only a property. The previous version
    // wrote the file without the `<!--color:-->` comment but cached the
    // content *with* it, so the two disagreed from that moment on and the
    // comment reappeared in the file on the next edit.
    const body = stripColorComment(cap.content).trim();
    const properties = { ...cap.properties, color: colorName };

    try {
        await writeCap({ relPath: cap.id, nodeType: cap.node_type, properties, content: body });
        cap.content = body;
        cap.properties = properties;
        cap.color = colorName;
    } catch(e) {
        logger.error("Failed to update color", e);
    }
    colorPickerCapId.value = null;
};

const isPinned = (cap: NodeMetadata) => cap.properties?.pinned === true;
const isArchived = (cap: NodeMetadata) => cap.properties?.archived === true;

/**
 * Flip a flag on some caps and write it down.
 *
 * Pinning and putting away are the two answers that are neither "turn this
 * into something" nor "throw it away" — keep it in front of me, and keep it
 * but stop asking. Without the second one an inbox can only ever be emptied
 * by promoting or deleting, and a thought that deserves neither has nowhere
 * to go but to sit there going stale.
 */
const setCapFlag = async (caps: NodeMetadata[], flag: 'pinned' | 'archived', value: boolean) => {
    for (const cap of caps) {
        const properties = { ...cap.properties, [flag]: value };
        try {
            await writeCap({
                relPath: cap.id,
                nodeType: cap.node_type,
                properties,
                content: cap.content,
            });
            cap.properties = properties;
        } catch (e) {
            logger.error(`Could not ${flag} ${cap.id}`, e);
        }
    }
};

const mapNodeToQuickCap = (node: any): NodeMetadata => {
    const rawTags = node.properties?.tags;
    const tagsArray = Array.isArray(rawTags) ? rawTags : (typeof rawTags === 'string' && rawTags.trim() !== '' ? [rawTags] : []);

    return {
        id: node.id,
        node_type: node.node_type,
        title: node.title,
        content: node.content,
        created_at: node.created_at,
        updated_at: node.updated_at,
        properties: node.properties || {},
        color: node.properties?.color || '',
        tags: tagsArray
    };
};

const loadCaps = async () => {
    if (!props.vaultPath) return;
    try {
        const nodes: any[] = await ns.getNodes('quickcap');

        quickCaps.value = nodes.map(mapNodeToQuickCap);

        // Drop cached previews for caps that are no longer here, so the map
        // tracks the vault rather than growing for the life of the session.
        const live = new Set(quickCaps.value.map((cap) => cap.id));
        for (const id of previewCache.keys()) {
            if (!live.has(id)) previewCache.delete(id);
        }
    } catch (e) {
        logger.error("Failed to load quick caps", e);
    }
};

const saveSelectedCap = async () => {
    if (!selectedCap.value) return;

    // What the user typed is what gets written. The previous version tore
    // every tag out of the body and re-appended the set at the bottom,
    // which moved words the user had put in the middle of a sentence.
    let finalPayload = editingContent.value.trim();

    // Older caps carry their colour as an HTML comment in the body. Keep
    // whatever the file already had until the migration retires the format;
    // dropping it here would blank the card's colour on the next keystroke.
    const colorMatch = selectedCap.value.content.match(/<!--color:(.*?)-->/);
    if (colorMatch) {
       finalPayload = `<!--color:${colorMatch[1]}-->\n${finalPayload}`;
    }
    
    if (selectedCap.value.content === finalPayload) return;
    
    try {
        await writeCap({ relPath: selectedCap.value.id, nodeType: selectedCap.value.node_type, properties: selectedCap.value.properties, content: finalPayload });
        selectedCap.value.content = finalPayload;
    } catch(e) {
        logger.error("Failed to update note", e);
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
    StarterKit,
    Markdown,
    TiptapImage,
    Placeholder.configure({ placeholder: 'Note content...' }),
  ],
  onUpdate: ({ editor: ed }) => {
    const md = (ed.storage as any).markdown.getMarkdown();
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
                     void ensureThumbnail(relativePath);
                 } catch(e) { logger.error("Paste image failed", e); }
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

const openFullView = (cap: NodeMetadata) => {
    selectedCap.value = cap;

    // The editor gets the cap exactly as written, tags included. Lifting
    // them out here is what made saving put them back somewhere else.
    const body = stripColorComment(cap.content).trim();
    editingContent.value = body;

    if (editor.value) {
       editor.value.commands.setContent(injectLocalAssets(body));
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

const openEditById = async (id: string) => {
    if (quickCaps.value.length === 0) {
        await loadCaps();
    }
    const normalizedId = id.replace(/\\/g, '/');
    const cap = quickCaps.value.find(c => c.id.replace(/\\/g, '/') === normalizedId) 
             || quickCaps.value.find(c => c.id.replace(/\\/g, '/').endsWith(normalizedId));
    if (cap) {
        openFullView(cap);
    }
};

/**
 * Put the cursor in the compose box.
 *
 * Called when the app is opened by the launcher shortcut, which means "let me
 * write something" — arriving at QuickCap with the field unfocused would put
 * the tap back that the shortcut exists to remove. On a phone the compose box
 * is a modal, so opening that *is* focusing it.
 */
const focusCompose = async () => {
    if (isMobileOS.value) {
        isMobileModalOpen.value = true;
        return;
    }
    await nextTick();
    inputRef.value?.focus();
};

defineExpose({ openEditById, focusCompose });

/**
 * The pictures a draft refers to.
 *
 * The quick-capture bar is a plain textarea on purpose — it has to be ready
 * to type into before anything else loads, which a rich editor is not. The
 * cost is that attaching an image writes a line of Markdown the user then
 * has to read as confirmation that it worked, and a content-addressed name
 * is thirty-two characters of hex.
 *
 * So the text stays exactly as it is, because it is what gets saved, and the
 * confirmation comes from showing the picture underneath instead.
 */
const draftAttachments = ref<string[]>([]);

/**
 * Record a voice note and attach it to the draft.
 *
 * The recording is stored as an ordinary asset, so it is content-addressed
 * and deduplicated like every other attachment, and it lands in the cap as a
 * plain Markdown link. A link rather than an embed on purpose: opened in any
 * other editor the vault stays readable, which an `![]()` pointing at audio
 * would not.
 */
const toggleRecording = async () => {
    if (audio.state.value === 'recording') {
        const recording = await audio.stop();
        if (!recording) return;

        try {
            const assetPath = await invoke<string>('save_asset', {
                vaultPath: props.vaultPath,
                filename: `voice.${recording.extension}`,
                bytes: Array.from(recording.bytes),
            });
            attachToDraft(assetPath);
        } catch (e) {
            logger.error('Could not save the voice note', e);
        }
        return;
    }

    await audio.start();
};

const attachToDraft = (assetPath: string) => {
    if (!draftAttachments.value.includes(assetPath)) {
        draftAttachments.value.push(assetPath);
    }
};

const removeDraftImage = (assetPath: string) => {
    draftAttachments.value = draftAttachments.value.filter((path) => path !== assetPath);
};

/** The draft as it will be saved: what was typed, then what was attached. */
const composeDraft = () =>
    [
        newCapText.value.trim(),
        ...draftAttachments.value.map((path) => `![Image](${path})`),
    ]
        .filter(Boolean)
        .join('\n\n');

const hasDraft = computed(() => Boolean(newCapText.value.trim() || draftAttachments.value.length));

/**
 * Attachments are kept beside the text rather than written into it.
 *
 * Appending `![Image](assets/…)` to the textarea meant the confirmation that
 * a picture attached was a line of Markdown the user had to read — thirty-two
 * characters of hex, sitting in the middle of what they were writing, easy to
 * break by typing into. Holding the paths separately and joining them at save
 * time is what every compose box with attachments does, and it leaves the
 * textarea holding only prose.
 */
const AUDIO_EXTENSIONS = ['webm', 'm4a', 'ogg', 'mp3', 'wav'];

const isAudioPath = (path: string) =>
    AUDIO_EXTENSIONS.includes(path.split('.').pop()?.toLowerCase() ?? '');

const assetUrl = (path: string) =>
    convertFileSrc(`${trimmedVaultPath()}${vaultSeparator()}${path}`);

const draftImageSrc = (path: string) => {
    if (!path.startsWith('assets/')) return path;
    return convertFileSrc(`${trimmedVaultPath()}${vaultSeparator()}${displayPathForAsset(path)}`);
};

const handleInput = () => {
    if (inputRef.value) {
        inputRef.value.style.height = 'auto';
        inputRef.value.style.height = inputRef.value.scrollHeight + 'px';
    }
};

const submitCap = async () => {
    const content = composeDraft();
    if (!content || !props.vaultPath) return;
    isSubmitting.value = true;
    try {
        await createCap(content);

        await loadCaps();
        newCapText.value = '';
        draftAttachments.value = [];
        if (inputRef.value) {
            inputRef.value.style.height = 'auto';
        }
    } catch (e) {
        logger.error("Failed to create quick cap", e);
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
             attachToDraft(assetPath);
             void ensureThumbnail(assetPath);
          } catch(err) {
             logger.error("Paste image save error:", err);
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
            attachToDraft(relPath);
            inputRef.value?.focus();
            void ensureThumbnail(relPath);
        }
    } catch(e) {
        logger.error("Failed to pick image", e);
    }
};

const pickImageForExistingCap = async (cap: NodeMetadata) => {
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
            void ensureThumbnail(relPath);
            await writeCap({ relPath: cap.id, nodeType: cap.node_type, properties: cap.properties, content: updatedContent });
            cap.content = updatedContent;
        }
    } catch(e) {
        logger.error("Failed to pick image", e);
    }
};

/**
 * Prove a promoted node exists before retiring the cap that became it.
 *
 * Promotion is the moment a fleeting note turns into something worth
 * keeping, so it is the one moment where losing the original actually
 * costs the user their idea.
 */
const confirmWritten = async (relPath: string) => {
    const written = await invoke<unknown>('get_node', { id: relPath });
    if (!written) {
        throw new Error(`${relPath} was written but cannot be read back`);
    }
};

/**
 * The caps whose destination the palette is asking about.
 *
 * An array rather than one cap, because merging several into a single note is
 * the same act as promoting one — the only difference is how much text goes
 * in. Keeping them apart would have meant a second palette, a second set of
 * destinations, and two places to fix anything wrong with either.
 */
const promotingCaps = ref<NodeMetadata[]>([]);

/** What the selected caps say, in the order they appear. */
const promotedContent = () =>
    promotingCaps.value
        .map((cap) => stripColorComment(cap.content).trim())
        .filter(Boolean)
        .join('\n\n');

/**
 * Accounts and categories, loaded only when the palette opens.
 *
 * `null` means this vault has no Finance configuration, which is why the
 * destination is not offered rather than offered and refused.
 */
const financeSetup = ref<FinanceSetup | null>(null);

const openPromote = async (caps: NodeMetadata[]) => {
    if (caps.length === 0) return;
    promotingCaps.value = caps;

    if (caps.length === 1 && financeSetup.value === null) {
        try {
            // QuickCap can be the first thing to write to the ledger, and a
            // vault still storing whole units refuses a row that is not. It
            // repairs before it offers.
            await repairFinanceStorage(props.vaultPath);
            financeSetup.value = await loadFinanceSetup();
            // Finance's transaction form reads and writes amounts in the
            // vault's currency. Without this it would use the default scale
            // and record a hundredth of what was typed.
            if (financeSetup.value) currentCurrency.value = financeSetup.value.currency;
        } catch (e) {
            logger.error('Could not read the Finance setup', e);
        }
    }
};

/** The cap waiting on a transaction form, and the draft it prefilled. */
const bookingCap = ref<NodeMetadata | null>(null);
const draftTransaction = ref<Transaction | null>(null);

/**
 * Open the Finance app's own transaction form, prefilled with what the cap
 * can honestly supply.
 *
 * Only the note and the date: amount, category and account are not in the
 * text, and guessing an amount from "cà phê 45k" would put a wrong number in
 * somebody's accounts — worse than putting none. The form is Finance's, not
 * a copy, so it already knows the accounts, the categories and the currency.
 */
const openTransactionForm = (cap: NodeMetadata) => {
    const setup = financeSetup.value;
    if (!setup) return;

    bookingCap.value = cap;
    draftTransaction.value = {
        id: `tx-${crypto.randomUUID()}`,
        type: 'expense',
        amount: 0,
        category: '',
        accountId: setup.accounts[0]?.id ?? '',
        date: new Date(cap.created_at || Date.now()).toISOString(),
        note: stripColorComment(cap.content).trim(),
    };
};

const closeTransactionForm = () => {
    bookingCap.value = null;
    draftTransaction.value = null;
};

const confirmTransaction = async (tx: Transaction) => {
    const cap = bookingCap.value;
    closeTransactionForm();
    if (!cap) return;

    try {
        const relPath = await appendTransaction(tx);
        await retireCaps([cap], relPath);
        bus.emit('node:updated', { nodeType: 'finance_month', id: relPath, title: '' });
    } catch (e) {
        logger.error('Could not record the transaction', e);
        await message(t('quickcap.transaction_failed'), { title: 'Synabit', kind: 'error' });
    }
};

/**
 * Add a cap to the end of a note that already exists.
 *
 * The destination this app was missing. Processing a fleeting note is mostly
 * "this belongs in the note I already wrote about X", and until now the only
 * offers were a brand-new note or a task.
 *
 * The cap's text is appended rather than merged: deciding where inside
 * somebody's note a stray thought belongs is not a decision software should
 * make, and the bottom is where they will look for what just arrived.
 */
const appendToNote = async (caps: NodeMetadata[], relPath: string, title: string) => {
    try {
        const note = await invoke<any>('get_node', { id: relPath });
        if (!note) throw new Error(`${relPath} could not be read`);

        const addition = promotedContent();
        if (!addition) return;

        const merged = `${(note.content ?? '').trimEnd()}\n\n${addition}`;

        await ns.writeNode({
            relPath,
            nodeType: note.node_type ?? 'note',
            title: note.title,
            properties: note.properties ?? {},
            content: merged,
        });

        await retireCaps(caps, relPath);
        bus.emit('node:updated', { nodeType: 'note', id: relPath, title });
        logger.info(`Cap appended to ${relPath}`);
    } catch (e) {
        logger.error('Could not append the cap to that note', e);
        await message(t('quickcap.append_failed'), { title: 'Synabit', kind: 'error' });
    }
};

/**
 * Retire a cap once whatever it became is provably on disk.
 *
 * Shared by every promotion that writes somewhere else first. The order is
 * the point: verify, then trash. Doing it the other way loses the thought
 * outright whenever the write fails.
 */
const retireCaps = async (caps: NodeMetadata[], writtenTo: string) => {
    await confirmWritten(writtenTo);

    for (const cap of caps) {
        await invoke('trash_node_file', { vaultPath: props.vaultPath, relPath: cap.id });
        const index = quickCaps.value.findIndex((c) => c.id === cap.id);
        if (index !== -1) quickCaps.value.splice(index, 1);
        if (selectedCap.value?.id === cap.id) selectedCap.value = null;
        selectedIds.value.delete(cap.id);
    }
};

/**
 * Turn a cap into a calendar entry.
 *
 * Created as an all-day event today rather than asking for a date first. A
 * capture is written on the day it matters, and a promotion that opens a form
 * before it will accept anything is the friction this whole phase removes —
 * so it lands in the calendar and the user is taken there to adjust it.
 */
const promoteToEvent = async (caps: NodeMetadata[]) => {
    const body = promotedContent();
    if (!body) return;

    const title = deriveTitle(body);
    const day = new Date().toISOString().slice(0, 10);
    const safeName = title.replace(/[^a-z0-9]/gi, '_').toLowerCase().slice(0, 40);
    const relPath = `Events/${safeName}_${Date.now()}.md`;

    try {
        await ns.writeNode({
            relPath,
            nodeType: 'event',
            title,
            properties: {
                is_all_day: true,
                start_at: day,
                end_at: day,
                location: '',
                tags: extractTags(body),
                relations: [],
                source_link: caps[0]?.id,
            },
            content: body,
            eventType: 'created',
        });

        await retireCaps(caps, relPath);
        bus.emit('navigate:to-item', { app: 'calendar', itemId: relPath });
    } catch (e) {
        logger.error('Could not turn the cap into an event', e);
        await message(t('quickcap.convert_task_error'), { title: 'Synabit', kind: 'error' });
    }
};

/**
 * Append a cap to somebody's page.
 *
 * The same move as adding to a note — "this is a thing they said" — and it
 * uses the person's own body, which is what the People app already shows and
 * edits.
 */
const appendToPerson = async (caps: NodeMetadata[], relPath: string, title: string) => {
    try {
        const person = await invoke<any>('get_node', { id: relPath });
        if (!person) throw new Error(`${relPath} could not be read`);

        const addition = promotedContent();
        if (!addition) return;

        await ns.writeNode({
            relPath,
            nodeType: person.node_type ?? 'person',
            title: person.title,
            properties: person.properties ?? {},
            content: `${(person.content ?? '').trimEnd()}\n\n${addition}`,
        });

        await retireCaps(caps, relPath);
        bus.emit('node:updated', { nodeType: 'person', id: relPath, title });
    } catch (e) {
        logger.error('Could not append the cap to that person', e);
        await message(t('quickcap.append_person_failed'), { title: 'Synabit', kind: 'error' });
    }
};

const onPromoteChosen = async (target: PromoteTarget) => {
    const caps = promotingCaps.value;
    promotingCaps.value = [];
    if (caps.length === 0) return;

    switch (target.kind) {
        case 'new-note':
            openConvertNoteModal(caps);
            return;
        case 'new-task':
            openConvertTaskModal(caps);
            return;
        case 'new-event':
            await promoteToEvent(caps);
            return;
        case 'new-transaction':
            openTransactionForm(caps[0]);
            return;
        case 'append-note':
            await appendToNote(caps, target.relPath, target.title);
            return;
        case 'append-person':
            await appendToPerson(caps, target.relPath, target.title);
    }
};

const convertingTaskCaps = ref<NodeMetadata[]>([]);
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

const openConvertTaskModal = (caps: NodeMetadata[]) => {
    convertingTaskCaps.value = caps;
    const cleanContent = promotedContent();
    const displayLines = cleanContent.split('\n').filter(l => l.trim() !== '');
    convertingTaskParams.value = {
        title: displayLines.length > 0 ? displayLines[0].substring(0, 50) + (displayLines[0].length > 50 ? '...' : '') : 'QuickCap Task',
        content: cleanContent,
        status: 'todo',
        start_date: '',
        due_date: '',
        priority: '',
        tags: extractTags(cleanContent).join(', '),
        checklist: [],
        is_transferred: false,
        transferred_to: '',
        track_progress: false,
        comment: ''
    };
};

const closeTaskModal = () => {
    convertingTaskCaps.value = [];
};

const convertingNoteCaps = ref<NodeMetadata[]>([]);
const convertingNoteParams = ref({
    title: '',
    content: '',
    tags: ''
});

const openConvertNoteModal = (caps: NodeMetadata[]) => {
    convertingNoteCaps.value = caps;
    const cleanContent = promotedContent();
    const displayLines = cleanContent.split('\n').filter(l => l.trim() !== '');
    const titleLine = displayLines.length > 0 ? displayLines[0] : 'QuickCap Note';
    const defaultTitle = titleLine.substring(0, 50) + (titleLine.length > 50 ? '...' : '');
    
    convertingNoteParams.value = {
        title: defaultTitle,
        content: cleanContent,
        tags: extractTags(cleanContent).join(', ')
    };
};

const closeNoteModal = () => {
    convertingNoteCaps.value = [];
};

const confirmTurnIntoNote = async (payload: any) => {
    const caps = convertingNoteCaps.value;
    if (caps.length === 0) return;
    
    try {
        let tagsArray: string[] = [];
        if (payload.tags) {
            tagsArray = payload.tags.split(',').map((t: string) => t.trim()).filter((t: string) => t !== '');
        }
        
        const safeName = (payload.title || 'Untitled').replace(/[^a-z0-9]/gi, '_').toLowerCase();
        const relPath = `Notes/${safeName}_${Date.now()}.md`;

        await ns.writeNode({
            relPath: relPath,
            nodeType: 'note',
            title: payload.title || 'Untitled',
            properties: {
                tags: tagsArray,
                // The task path has always recorded where it came from; the
                // note path did not, so a promoted note lost all trace of it.
                source_link: caps[0]?.id,
            },
            content: payload.content,
            eventType: 'created',
        });

        // Retire the cap only once the note can actually be read back. The
        // previous order — create, then delete regardless — lost the cap
        // outright if anything failed in between.
        await retireCaps(caps, relPath);

        bus.emit('vault:changed');
        bus.emit('navigate:to-item', { app: 'note', itemId: relPath });
        closeNoteModal();
    } catch(e) {
        logger.error("Failed to convert to note", e);
        await message(t('quickcap.convert_note_error'), { title: 'Synabit', kind: 'error' });
    }
};

const confirmTurnIntoTask = async (payload: any) => {
    const caps = convertingTaskCaps.value;
    if (caps.length === 0) return;
    try {
        const tagArray = payload.tags.split(',').map((t: string) => t.trim()).filter((t: string) => t !== '');
        
        const safeName = (payload.title || 'Untitled').replace(/[^a-z0-9]/gi, '_').toLowerCase();
        const relPath = `Tasks/${safeName}_${Date.now()}.md`;
        
        await ns.writeNode({
            relPath: relPath,
            nodeType: 'task',
            title: payload.title || 'Untitled',
            properties: {
                status: payload.status,
                is_transferred: payload.is_transferred,
                transferred_to: payload.transferred_to,
                track_progress: payload.track_progress,
                priority: payload.priority,
                start_date: payload.start_date,
                due_date: payload.due_date,
                comment: payload.comment,
                source_link: caps[0]?.id,
                tags: tagArray
            },
            content: payload.content,
            eventType: 'created',
        });
        
        await retireCaps(caps, relPath);

        bus.emit('navigate:to-item', { app: 'task', itemId: relPath });
        closeTaskModal();
    } catch(e) {
        logger.error("Failed to create task", e);
        await message(t('quickcap.convert_task_error'), { title: 'Synabit', kind: 'error' });
    }
};

/** A capture's timestamp, at the resolution the reader actually needs.
 *  Today's caps get a clock — that is what distinguishes them from each
 *  other. Older ones get a date, and the year only once it stops being
 *  this one. */
const formatDate = (iso: string) => {
    if (!iso) return '';
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return iso;
    const now = new Date();
    if (d.toDateString() === now.toDateString()) {
        return new Intl.DateTimeFormat(locale.value, { hour: '2-digit', minute: '2-digit' }).format(d);
    }
    return new Intl.DateTimeFormat(locale.value, {
        day: 'numeric',
        month: 'short',
        ...(d.getFullYear() === now.getFullYear() ? {} : { year: 'numeric' }),
    }).format(d);
};

/** Escape closes one layer at a time, innermost first, so a stray press
 *  never discards more than the user was looking at. */
const handleGlobalKeydown = (e: KeyboardEvent) => {
    if (e.key !== 'Escape') {
        handleTriageKey(e);
        return;
    }
    if (colorPickerCapId.value) { colorPickerCapId.value = null; return; }
    if (taggingCapId.value) { taggingCapId.value = null; return; }
    if (selectedCap.value) { closeFullView(); return; }
    if (isMobileModalOpen.value) { isMobileModalOpen.value = false; return; }
    // Nothing layered over the grid. A selection is the more surprising state
    // to be left in, so it goes first; a second Escape puts the cursor away.
    if (selectedIds.value.size > 0) { clearSelection(); return; }
    cursor.value = -1;
};

/**
 * The one-time repair that moves tags into frontmatter and colour into a
 * name. See `src-tauri/src/commands/migration.rs` for why it does not go
 * through the ordinary write path.
 *
 * The flag is scoped to the vault, not the device: opening a second vault
 * has to repair that vault too. Losing the flag costs a pass over the file
 * list and no writes at all, because the transform returns nothing for a
 * cap already in the target shape — so this is an optimisation, not a
 * correctness gate.
 */
const repairStorageOnce = async () => {
    if (!props.vaultPath) return;
    const key = `quickcap-tags-colours-v1:${props.vaultPath}`;
    try {
        if (await invoke<string | null>('get_migration_flag', { key })) return;

        const report = await invoke<{ changed: number; unchanged: number; failed: number }>(
            'migrate_quickcap_storage',
            { vaultPath: props.vaultPath },
        );
        logger.info(
            `QuickCap storage repair: ${report.changed} repaired, ${report.unchanged} already current, ${report.failed} failed`,
        );

        // Only a clean pass is recorded. A partial one is retried on the next
        // launch, which costs nothing for the caps already done.
        if (report.failed === 0) {
            await invoke('set_migration_flag', { key, value: new Date().toISOString() });
        }
    } catch (e) {
        logger.error('QuickCap storage repair failed', e);
    }
};

onMounted(() => {
    window.addEventListener('paste', handleGlobalPaste);
    window.addEventListener('keydown', handleGlobalKeydown);

    // Repair before the first read, so the list is never drawn from the old
    // shape and then redrawn from the new one.
    repairStorageOnce().finally(loadCaps);

    void loadThumbnails();

    bus.on('vault:file-modified', () => {
        loadCaps();
    });

    bus.on('vault:file-created-deleted', () => {
        loadCaps();
    });

    bus.on('vault:sync-completed', () => {
        loadCaps();
    });
});

onUnmounted(() => {
    // A stream left open keeps the system's recording indicator lit, which
    // reads as the app still listening after the user navigated away.
    audio.cancel();
    window.removeEventListener('paste', handleGlobalPaste);
    window.removeEventListener('keydown', handleGlobalKeydown);
    if (editor.value) editor.value.destroy();
});

watch(() => props.vaultPath, () => {
    void loadThumbnails();
    repairStorageOnce().finally(loadCaps);
});

// Deleting a whole cap no longer asks, so asking before removing one tag
// off it would gate the smaller action harder than the larger one. Retyping
// a tag costs a second; the dialog cost one on every removal.
const removeTag = async (cap: NodeMetadata, tag: string) => {
    const updatedContent = removeTagFromContent(cap.content, tag);
    
    try {
        await writeCap({ relPath: cap.id, nodeType: cap.node_type, properties: cap.properties, content: updatedContent });
        cap.content = updatedContent;
    } catch(e) {
        logger.error("Failed to remove tag", e);
    }
};

const removeActiveTag = (tag: string) => {
    const updatedContent = removeTagFromContent(editingContent.value, tag);
    editingContent.value = updatedContent;
    if (editor.value) {
       editor.value.commands.setContent(injectLocalAssets(updatedContent));
    }
    
    if (saveTimeout) clearTimeout(saveTimeout);
    saveTimeout = setTimeout(saveSelectedCap, 1000);
};

const renderPreview = (content: string) => {
    if (!content) return '';
    
    // Tags stay where they were written. Stripping them made sense while
    // saving herded them all to the bottom; now that a tag can sit in the
    // middle of a sentence, removing it would tear a hole in the sentence.
    const textBody = stripColorComment(content.trim()).trim();

    // Escape HTML to prevent XSS
    let html = textBody
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;");
        
    // Markdown task items become boxes that can be ticked from the card.
    //
    // Numbered by their order among task lines rather than by line number:
    // the colour comment is stripped before this runs, and on an unmigrated
    // cap that shifts every line after it. An ordinal survives that.
    let taskOrdinal = -1;
    html = html.split('\n').map((line) => {
        const match = line.match(/^(\s*)[-*]\s+\[([ xX])\]\s+(.*)$/);
        if (!match) return line;
        taskOrdinal += 1;
        const done = match[2] !== ' ';
        const box = done
            ? '<span class="inline-flex items-center justify-center w-[15px] h-[15px] rounded border border-transparent bg-black dark:bg-white text-white dark:text-black text-[10px] leading-none">✓</span>'
            : '<span class="inline-block w-[15px] h-[15px] rounded border border-gray-400 dark:border-gray-500"></span>';
        return `<span data-task="${taskOrdinal}" class="flex items-start gap-2 my-0.5 cursor-pointer group/task"><span class="mt-[3px] shrink-0">${box}</span><span class="${done ? 'line-through opacity-50' : ''}">${match[3]}</span></span>`;
    }).join('\n');

    // A local audio link becomes a player. `preload="none"` matters: a grid
    // of caps would otherwise fetch every recording in it just to draw.
    html = html.replace(
        /\[([^\]]*)\]\((assets\/[^\s)]+\.(?:webm|m4a|ogg|mp3|wav))\)/gi,
        (_match, _label, path) => {
            const src = assetUrl(decodeURIComponent(path));
            return `<audio controls preload="none" src="${src}" class="w-full my-2"></audio>`;
        },
    );

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
            absPath = `${cleanVaultPath}${sep}${displayPathForAsset(path)}`;
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
            const rel = `assets/${decodeURIComponent(assetMatch[2])}`;
            absPath = `${cleanVaultPath}${sep}${displayPathForAsset(rel)}`;
        } else if (path.startsWith('assets/')) {
            absPath = `${cleanVaultPath}${sep}${displayPathForAsset(path)}`;
        }
        const src = convertFileSrc(absPath);
        return `<img src="${src}" class="max-w-full max-h-64 object-contain rounded-lg my-2 border border-gray-200 dark:border-[#2c2c2c]" loading="lazy" />`;
    });
    
    return DOMPurify.sanitize(html, {
        ADD_ATTR: ['target', 'controls', 'preload'],
        // `preload` has to be declared as *not* a URI. Setting
        // ALLOWED_URI_REGEXP makes DOMPurify measure attribute values against
        // it, and `preload="none"` is not a URI, so it was being dropped —
        // silently, which meant a grid of caps fetched every recording in it
        // to draw the page. Exactly what the attribute was added to prevent.
        ADD_URI_SAFE_ATTR: ['preload'],
        ALLOWED_URI_REGEXP: /^(?:(?:https?|asset):)|(?:data:image\/)/i,
    });
};

/**
 * Which images already have a small copy under `assets/.thumbs/`.
 *
 * Read once per load rather than asked per image: the alternative is a
 * filesystem call for every picture on every paint.
 */
const thumbnails = ref<Set<string>>(new Set());

const vaultSeparator = () => (props.vaultPath.includes('\\') ? '\\' : '/');

const trimmedVaultPath = () =>
    props.vaultPath.endsWith('/') || props.vaultPath.endsWith('\\')
        ? props.vaultPath.slice(0, -1)
        : props.vaultPath;

/**
 * The file a card should actually load for an asset: the thumbnail when one
 * exists, the original otherwise.
 */
const displayPathForAsset = (assetRelPath: string) => {
    const filename = assetRelPath.split('/').pop() ?? '';
    const thumb = thumbnailNameFor(filename);
    return thumbnails.value.has(thumb) ? `assets/.thumbs/${thumb}` : assetRelPath;
};

const loadThumbnails = async () => {
    if (!props.vaultPath) return;
    try {
        const names = await invoke<string[]>('list_thumbnails', { vaultPath: props.vaultPath });
        thumbnails.value = new Set(names);
    } catch (e) {
        logger.error('Failed to list thumbnails', e);
    }
};

/**
 * Make a small copy of a freshly saved image, if it is big enough to be
 * worth one. Fire-and-forget: a card without a thumbnail simply loads the
 * original, so nothing here is worth blocking a paste on.
 */
const ensureThumbnail = async (assetRelPath: string) => {
    const filename = assetRelPath.split('/').pop();
    if (!filename || thumbnails.value.has(thumbnailNameFor(filename))) return;

    try {
        // The absolute path, not an asset:// URL — the thumbnail is built from
        // the file's bytes so the canvas stays untainted. See `thumbnails.ts`.
        const bytes = await makeThumbnail(`${trimmedVaultPath()}${vaultSeparator()}${assetRelPath}`);
        if (!bytes) return;

        const stored = await invoke<string>('save_thumbnail', {
            vaultPath: props.vaultPath,
            assetName: filename,
            bytes: Array.from(bytes),
        });
        thumbnails.value = new Set([...thumbnails.value, stored]);
    } catch (e) {
        logger.error('Failed to build thumbnail', e);
    }
};

/**
 * How many caps are built into DOM at a time.
 *
 * The grid used to render every cap in the vault at once. A card is not one
 * element — it is a masked body, a tag row and six buttons — so a few
 * thousand caps is tens of thousands of nodes, all laid out before the first
 * one appears.
 *
 * Note that the *summaries* half of this plan item was dropped after
 * checking what `NodeSummary` actually carries: a hard 150-character cut of
 * the body. QuickCap's cards render Markdown images, and a content-addressed
 * asset name is 32 hex characters, so that cut lands inside
 * `![Image](assets/…png)` often enough to show broken syntax instead of a
 * picture. The payload argument behind it was written for notes, which are
 * long; caps are short by design. Rendering was the cost worth removing.
 */
const RENDER_BATCH = 60;

const renderLimit = ref(RENDER_BATCH);

/** A new search is a new list, so it starts from the top again. */
watch(searchQuery, () => {
    renderLimit.value = RENDER_BATCH;
});

/**
 * Everything a card needs, worked out once per cap rather than once per
 * render.
 *
 * `renderPreview` escapes, runs four regex passes and calls
 * `DOMPurify.sanitize`; `extractTags` runs a scanner. Both were called
 * straight from the template — and `extractTags` twice per card, once for
 * the `v-if` and again for the `v-for`. Vue re-evaluates a template
 * expression on every render, so typing one character into the search box
 * re-sanitised the entire vault.
 *
 * The cache is keyed on the cap's own content, so any edit recomputes that
 * cap and only that cap. Filtering and searching, which change what is
 * displayed but not what is in it, now cost nothing.
 */
const previewCache = new Map<string, { content: string; html: string; tags: string[] }>();

// A new thumbnail changes what every card should load, and the preview cache
// is keyed only on a cap's text — so it has to be dropped when the set moves.
watch(thumbnails, () => previewCache.clear());

const capViews = computed(() =>
    // The index travels with the view because the grid deals caps across
    // columns: a card's position on screen says nothing about where it sits
    // in reading order, and keyboard navigation follows reading order.
    filteredCaps.value.slice(0, renderLimit.value).map((cap, index) => {
        const cached = previewCache.get(cap.id);
        if (cached && cached.content === cap.content) {
            return { cap, index, html: cached.html, tags: cached.tags };
        }
        const html = renderPreview(cap.content);
        const tags = extractTags(cap.content);
        previewCache.set(cap.id, { content: cap.content, html, tags });
        return { cap, index, html, tags };
    }),
);

const { width: windowWidth } = useWindowSize();

/** Mirrors the Tailwind breakpoints the grid used to switch at. */
const columnCount = computed(() => {
    const w = windowWidth.value;
    if (w >= 1280) return 4;
    if (w >= 1024) return 3;
    if (w >= 640) return 2;
    return mobileViewMode.value === 'grid' ? 2 : 1;
});

/**
 * The caps dealt across columns, newest first, left to right.
 *
 * CSS `columns` fills column one to the bottom before starting column two,
 * so the second-newest cap sat *below* the newest and the fourth sat off in
 * another column entirely. For a feed in time order that is the wrong
 * reading order — and it is the kind of wrongness a person feels without
 * being able to name it.
 *
 * Dealing round-robin by index restores left-to-right order. Columns can
 * end up slightly uneven, which is the trade every masonry makes; putting
 * each cap in the shortest column instead would balance them and scramble
 * the order again.
 */
const capColumns = computed(() => {
    const count = columnCount.value;
    const columns: (typeof capViews.value)[] = Array.from({ length: count }, () => []);
    capViews.value.forEach((view, index) => columns[index % count].push(view));
    return columns;
});

/** True while there are caps matched but not yet built. */
const hasMoreToRender = computed(() => renderLimit.value < filteredCaps.value.length);

const loadMoreAnchor = ref<HTMLElement | null>(null);

// Well before the anchor is on screen, so the next batch is laid out by the
// time the user scrolls to where it goes.
useIntersectionObserver(
    loadMoreAnchor,
    ([entry]) => {
        if (entry?.isIntersecting && hasMoreToRender.value) {
            renderLimit.value += RENDER_BATCH;
        }
    },
    { rootMargin: '800px' },
);

/**
 * Delete a cap: confirm, then move it to the vault's trash straight away.
 *
 * An earlier version held the deletion in memory for six seconds so an undo
 * toast could cancel it, and touched no files until that window closed. That
 * made undo instant and free of any race with sync — but it also meant a
 * reload inside those six seconds silently undid the delete, because the
 * only record of it was a variable in this component.
 *
 * Once a confirmation dialog is in the way, that trade stops paying. The
 * user has already said yes; an action they confirmed must survive a reload.
 * Recovery still exists and is now the thing the dialog actually promises:
 * the file sits in `.trash/` for thirty days.
 */
/**
 * Working through the inbox without touching the mouse.
 *
 * Promotion is where a cap stops being fleeting and starts being worth
 * something, so the speed of *that* is the speed of the whole method. Forty
 * caps is a normal week; forty caps at one mouse trip each is why people stop
 * processing and start hoarding.
 *
 * `j`/`k` because this is a list to move through, and arrows because not
 * everyone reads Vim. Nothing here fires while a field has focus — a capture
 * app whose letters are shortcuts would be unable to take a capture.
 */
const cursor = ref(-1);

/**
 * Caps picked out for one action.
 *
 * Held by id rather than index because the list moves underneath: a sync
 * arrives, the age filter changes, a cap is retired. An index-based selection
 * would quietly start pointing at different caps than the ones highlighted.
 */
const selectedIds = ref<Set<string>>(new Set());

const selectedCaps = computed(() =>
    // In list order, not click order: several caps merged into one note should
    // read the way they read on screen.
    quickCaps.value.filter((cap) => selectedIds.value.has(cap.id)),
);

const toggleSelected = (id: string) => {
    const next = new Set(selectedIds.value);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    selectedIds.value = next;
};

const clearSelection = () => {
    if (selectedIds.value.size > 0) selectedIds.value = new Set();
};

/** What `p` and `x` act on: the selection if there is one, else the cursor. */
const capsInPlay = () => {
    if (selectedCaps.value.length > 0) return selectedCaps.value;
    const cap = capViews.value[cursor.value]?.cap;
    return cap ? [cap] : [];
};

const isTyping = () => {
    const el = document.activeElement as HTMLElement | null;
    if (!el) return false;
    return (
        el.tagName === 'INPUT' ||
        el.tagName === 'TEXTAREA' ||
        el.isContentEditable
    );
};

/** True while anything is layered over the grid. */
const hasOverlay = () =>
    Boolean(selectedCap.value || promotingCaps.value.length > 0 || convertingNoteCaps.value.length > 0 || convertingTaskCaps.value.length > 0 || bookingCap.value || isMobileModalOpen.value);

const moveCursor = async (delta: number) => {
    const total = capViews.value.length;
    if (total === 0) return;

    cursor.value = cursor.value < 0
        ? (delta > 0 ? 0 : total - 1)
        : (cursor.value + delta + total) % total;

    await nextTick();
    document
        .querySelector(`[data-cap-index="${cursor.value}"]`)
        ?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
};

const capAtCursor = () => capViews.value[cursor.value]?.cap ?? null;

// A cap leaving the list must not leave the cursor pointing past the end.
watch(capViews, (views) => {
    if (cursor.value >= views.length) cursor.value = views.length - 1;
});

const handleTriageKey = (e: KeyboardEvent) => {
    if (isTyping() || e.metaKey || e.ctrlKey || e.altKey) return;
    if (hasOverlay()) return;

    switch (e.key) {
        case 'j':
        case 'ArrowDown':
            e.preventDefault();
            void moveCursor(1);
            return;
        case 'k':
        case 'ArrowUp':
            e.preventDefault();
            void moveCursor(-1);
            return;
        case 'Enter': {
            const cap = capAtCursor();
            if (!cap) return;
            e.preventDefault();
            openFullView(cap);
            return;
        }
        case ' ': {
            const cap = capAtCursor();
            if (!cap) return;
            e.preventDefault();
            toggleSelected(cap.id);
            void moveCursor(1);
            return;
        }
        case 's': {
            const caps = capsInPlay();
            if (caps.length === 0) return;
            e.preventDefault();
            void setCapFlag(caps, 'pinned', !caps.every(isPinned));
            return;
        }
        case 'a': {
            const caps = capsInPlay();
            if (caps.length === 0) return;
            e.preventDefault();
            void setCapFlag(caps, 'archived', !caps.every(isArchived)).then(clearSelection);
            return;
        }
        case 'p': {
            const caps = capsInPlay();
            if (caps.length === 0) return;
            e.preventDefault();
            void openPromote(caps);
            return;
        }
        case 'x':
        case 'Backspace': {
            const caps = capsInPlay();
            if (caps.length === 0) return;
            e.preventDefault();
            void deleteCaps(caps);
            return;
        }
    }
};

/**
 * Delete several caps behind one confirmation.
 *
 * One dialog for the batch, not one per cap: asking forty times is not forty
 * times the safety, it is a prompt people learn to dismiss without reading.
 */
/**
 * Tick or untick one item of a cap's checklist.
 *
 * Written straight back into the Markdown, because that is where the list
 * lives: `- [ ]` is what every other editor understands, so a cap ticked here
 * is ticked in Obsidian too. Storing the state anywhere else would make the
 * card and the file disagree.
 */
const toggleTaskItem = async (cap: NodeMetadata, ordinal: number) => {
    const lines = cap.content.split('\n');
    let seen = -1;

    for (let i = 0; i < lines.length; i += 1) {
        const match = lines[i].match(/^(\s*[-*]\s+\[)([ xX])(\]\s+.*)$/);
        if (!match) continue;
        seen += 1;
        if (seen !== ordinal) continue;
        lines[i] = `${match[1]}${match[2] === ' ' ? 'x' : ' '}${match[3]}`;
        break;
    }
    if (seen < ordinal) return;

    const updated = lines.join('\n');
    try {
        await writeCap({
            relPath: cap.id,
            nodeType: cap.node_type,
            properties: cap.properties,
            content: updated,
        });
        cap.content = updated;
    } catch (e) {
        logger.error('Could not tick that item', e);
    }
};

/**
 * A click inside a card's preview.
 *
 * Ticking an item must not also open the cap. Vue directives do not survive
 * `v-html` — DOMPurify strips them and Vue never compiles them — so the
 * checkbox cannot carry its own handler, and this reads the target instead.
 */
const onPreviewClick = (event: MouseEvent, cap: NodeMetadata) => {
    const item = (event.target as HTMLElement | null)?.closest('[data-task]');
    if (!item) return;
    event.stopPropagation();
    const ordinal = Number(item.getAttribute('data-task'));
    if (Number.isNaN(ordinal)) return;
    void toggleTaskItem(cap, ordinal);
};

/** Start a checklist item in the compose box, wherever the cursor is. */
const insertChecklistItem = async () => {
    const field = inputRef.value;
    const text = newCapText.value;
    const at = field?.selectionStart ?? text.length;

    const atLineStart = at === 0 || text[at - 1] === '\n';
    const prefix = atLineStart ? '- [ ] ' : '\n- [ ] ';

    newCapText.value = text.slice(0, at) + prefix + text.slice(at);
    await nextTick();
    handleInput();
    field?.focus();
    field?.setSelectionRange(at + prefix.length, at + prefix.length);
};

const deleteCaps = async (caps: NodeMetadata[]) => {
    if (caps.length === 0) return;
    if (caps.length === 1) {
        await deleteCap(caps[0].id);
        return;
    }

    const confirmed = await ask(t('quickcap.delete_selected_body'), {
        title: t('quickcap.delete_selected_title', { count: caps.length }),
        kind: 'warning',
        okLabel: t('quickcap.delete_confirm'),
        cancelLabel: t('quickcap.cancel'),
    });
    if (!confirmed) return;

    for (const cap of caps) {
        try {
            await invoke('trash_node_file', { vaultPath: props.vaultPath, relPath: cap.id });
            const index = quickCaps.value.findIndex((c) => c.id === cap.id);
            if (index !== -1) quickCaps.value.splice(index, 1);
            selectedIds.value.delete(cap.id);
            bus.emit('node:deleted', { nodeType: 'quickcap', id: cap.id });
        } catch (e) {
            logger.error(`Could not move ${cap.id} to the trash`, e);
        }
    }
    clearSelection();
};

const deleteCap = async (id: string) => {
    if (!quickCaps.value.some((cap) => cap.id === id)) return;

    // The dialog names the action and says what actually happens. Every other
    // delete in the app warns that it "cannot be undone" — here that would be
    // untrue, and a warning the user can discover is false is worse than no
    // warning at all.
    const confirmed = await ask(t('quickcap.delete_body'), {
        title: t('quickcap.delete_title'),
        kind: 'warning',
        okLabel: t('quickcap.delete_confirm'),
        cancelLabel: t('quickcap.cancel'),
    });
    if (!confirmed) return;

    try {
        await invoke('trash_node_file', { vaultPath: props.vaultPath, relPath: id });

        const index = quickCaps.value.findIndex((cap) => cap.id === id);
        if (index !== -1) quickCaps.value.splice(index, 1);
        if (selectedCap.value?.id === id) {
            selectedCap.value = null;
        }
        bus.emit('node:deleted', { nodeType: 'quickcap', id });
    } catch (e) {
        logger.error('Failed to move quick cap to trash', e);
        await message(t('quickcap.delete_failed'), { title: 'Synabit', kind: 'error' });
    }
};
</script>

<template>
  <div class="h-full bg-[#fdfdfc] dark:bg-[#242424] overflow-y-auto w-full pt-12 pb-16 px-4">
    <!-- Input Bar (Desktop Only) -->
    <div class="hidden md:flex flex-col mx-auto w-full max-w-2xl bg-white dark:bg-[#1e1e1e] rounded-xl shadow-[0_2px_8px_rgba(0,0,0,0.04)] dark:shadow-[0_2px_8px_rgba(0,0,0,0.2)] border border-[#e6e6e6] dark:border-[#2c2c2c] overflow-hidden focus-within:ring-1 focus-within:ring-black dark:focus-within:ring-white transition-all mb-12">
        <textarea
           ref="inputRef"
           v-model="newCapText"
           @input="handleInput"
           @keydown.enter.ctrl="submitCap"
           @keydown.enter.meta="submitCap"
           :placeholder="$t('quickcap.placeholder')"
           class="w-full bg-transparent p-5 min-h-[60px] max-h-[400px] resize-none outline-none text-[#1c1c1e] dark:text-[#f4f4f5] overflow-y-auto"
        ></textarea>

        <!-- What is attached, shown rather than described -->
        <div v-if="draftAttachments.length" class="flex flex-wrap gap-2 px-5 pb-2">
            <div v-for="path in draftAttachments" :key="path" class="relative">
                <div v-if="isAudioPath(path)" class="w-16 h-16 rounded-lg border border-[#e6e6e6] dark:border-[#2c2c2c] flex items-center justify-center bg-gray-50 dark:bg-[#2a2a2a]" :title="$t('quickcap.audio_note')">
                    <Mic class="w-6 h-6 text-gray-500" />
                </div>
                <img v-else :src="draftImageSrc(path)" alt="" class="w-16 h-16 object-cover rounded-lg border border-[#e6e6e6] dark:border-[#2c2c2c]" />
                <button
                    @click="removeDraftImage(path)"
                    :aria-label="$t('quickcap.remove_image')"
                    class="absolute -top-1.5 -right-1.5 w-5 h-5 rounded-full bg-[#1c1c1e] dark:bg-[#f4f4f5] text-white dark:text-black flex items-center justify-center shadow-sm hover:scale-110 transition-transform cursor-pointer"
                >
                    <X class="w-3 h-3" />
                </button>
            </div>
        </div>

        <!-- Actions bottom bar -->
        <div class="flex items-center justify-between p-2 px-3">
           <div class="flex items-center gap-1 opacity-70">
              <button @click="insertChecklistItem" :title="$t('quickcap.checklist')" class="p-2 rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-[#2a2a2a] transition-colors cursor-pointer">
                  <CheckSquare class="w-4 h-4"/>
              </button>
              <button @click="pickImageForNewCap" :title="$t('quickcap.pick_image')" class="p-2 rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-[#2a2a2a] transition-colors cursor-pointer">
                  <ImageIcon class="w-4 h-4"/>
              </button>
              <button
                  v-if="audio.isSupported()"
                  @click="toggleRecording"
                  :title="audio.state.value === 'recording' ? $t('quickcap.stop_recording') : $t('quickcap.record_audio')"
                  class="p-2 rounded-lg transition-colors cursor-pointer flex items-center gap-1.5"
                  :class="audio.state.value === 'recording' ? 'text-red-500 bg-red-50 dark:bg-red-900/20' : 'text-gray-500 hover:bg-gray-100 dark:hover:bg-[#2a2a2a]'"
              >
                  <Square v-if="audio.state.value === 'recording'" class="w-4 h-4 fill-current"/>
                  <Mic v-else class="w-4 h-4"/>
                  <span v-if="audio.state.value === 'recording'" class="text-[11px] font-mono tabular-nums">{{ formatDuration(audio.durationMs.value) }}</span>
              </button>
              <button @click="appendTagToInput" :title="$t('quickcap.add_tag')" class="p-2 rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-[#2a2a2a] transition-colors cursor-pointer">
                  <Tag class="w-4 h-4"/>
              </button>
           </div>
           <button @click="submitCap" :disabled="isSubmitting || !hasDraft" class="px-5 py-1.5 bg-black dark:bg-white text-white dark:text-black rounded-lg text-sm font-semibold hover:scale-95 transition-all disabled:opacity-50 cursor-pointer shadow-sm">
               {{ $t('quickcap.save') }}
           </button>
        </div>
    </div>
    
    <!-- Filter Bar -->
    <div class="w-full max-w-7xl px-4 flex items-center justify-between mb-8 mx-auto -mt-4">
        <div class="flex items-center gap-3 flex-1">
        <NavButtons />
        <div class="relative w-full sm:max-w-xs group">
            <div class="absolute inset-y-0 left-0 pl-3.5 flex items-center pointer-events-none">
                <Search class="h-4 w-4 text-gray-400 group-focus-within:text-blue-500 transition-colors" />
            </div>
            <input 
                v-model="searchQuery" 
                type="text" 
                class="block w-full pl-10 pr-3 py-2 border border-gray-200 dark:border-[#2c2c2c] rounded-full leading-5 bg-white dark:bg-[#1e1e1e] text-[#1c1c1e] dark:text-[#f4f4f5] placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-black/5 dark:focus:ring-white/10 sm:text-sm transition-all shadow-[0_2px_8px_rgba(0,0,0,0.02)]" 
                :placeholder="$t('quickcap.search_placeholder')" 
            />
            <button v-if="searchQuery" @click="searchQuery = ''" class="absolute inset-y-0 right-0 pr-3 flex items-center cursor-pointer" :aria-label="$t('quickcap.clear_search')">
                <X class="h-4 w-4 text-gray-400 hover:text-gray-600 transition-colors" />
            </button>
        </div>
        </div>
        <!--
          Age, not just search. A number on the tab creates pressure only if
          the list can be narrowed to what is actually stale; otherwise a big
          inbox is one undifferentiated wall.
        -->
        <div v-if="coldCount > 0 || archivedCount > 0" class="ml-3 hidden sm:flex shrink-0 bg-white dark:bg-[#1e1e1e] rounded-lg border border-gray-200 dark:border-[#2c2c2c] p-1 shadow-sm">
            <button
                v-for="option in (['all', 'fresh', 'cold', 'archived'] as const)"
                :key="option"
                v-show="option !== 'cold' || coldCount > 0"
                @click="ageFilter = option"
                class="px-2.5 py-1 rounded-md text-[12px] transition-colors cursor-pointer whitespace-nowrap"
                :class="ageFilter === option ? 'bg-black dark:bg-white text-white dark:text-black shadow-sm' : 'text-gray-500 hover:bg-gray-100 dark:hover:bg-[#2a2a2a]'"
            >
                {{ $t(`quickcap.filter_${option}`) }}<span v-if="option === 'cold' && coldCount > 0" class="ml-1 opacity-60">{{ coldCount }}</span><span v-else-if="option === 'archived' && archivedCount > 0" class="ml-1 opacity-60">{{ archivedCount }}</span>
            </button>
        </div>

        <div class="ml-4 flex shrink-0 bg-white dark:bg-[#1e1e1e] rounded-lg border border-gray-200 dark:border-[#2c2c2c] p-1 shadow-sm md:hidden">
            <button 
                @click="mobileViewMode = 'list'" 
                class="p-1.5 rounded-md transition-colors" 
                :class="mobileViewMode === 'list' ? 'bg-black dark:bg-white text-white dark:text-black shadow-sm' : 'text-gray-500 hover:bg-gray-100 dark:hover:bg-[#2a2a2a]'"
                :title="$t('quickcap.list_view')"
            >
                <List class="w-4 h-4" />
            </button>
            <button 
                @click="mobileViewMode = 'grid'" 
                class="p-1.5 rounded-md transition-colors" 
                :class="mobileViewMode === 'grid' ? 'bg-black dark:bg-white text-white dark:text-black shadow-sm' : 'text-gray-500 hover:bg-gray-100 dark:hover:bg-[#2a2a2a]'"
                :title="$t('quickcap.grid_view')"
            >
                <LayoutGrid class="w-4 h-4" />
            </button>
        </div>
    </div>

    <!-- Masonry Grid -->
    <div class="w-full max-w-7xl px-4 mx-auto flex items-start gap-4 sm:gap-6">
      <div v-for="(column, columnIndex) in capColumns" :key="columnIndex" class="flex-1 min-w-0 flex flex-col gap-4 sm:gap-6">
        <div
            v-for="{ cap, index, html, tags } in column"
            :key="cap.id"
            :data-cap-index="index"
            class="relative group w-full cursor-pointer rounded-2xl transition-shadow"
            :class="[
                cursor === index ? 'ring-2 ring-black dark:ring-white ring-offset-2 ring-offset-[#fdfdfc] dark:ring-offset-[#242424]' : '',
                selectedIds.has(cap.id) ? 'ring-2 ring-blue-500 ring-offset-2 ring-offset-[#fdfdfc] dark:ring-offset-[#242424]' : '',
            ]"
            @click="openFullView(cap)"
        >
            <div
                class="rounded-2xl shadow-sm hover:shadow-md border transition-all relative flex flex-col"
                :class="[
                    colourClass(cap.color) || 'bg-white dark:bg-[#1e1e1e]',
                    isCold(cap)
                        ? 'border-dashed border-gray-300 dark:border-[#3a3a3a] opacity-60 hover:opacity-100'
                        : 'border-[#e6e6e6] dark:border-[#2c2c2c]',
                ]"
                :title="isCold(cap) ? $t('quickcap.cold_hint', { days: 14 }) : undefined"
                style="max-height: 320px;"
            >
               <Pin v-if="isPinned(cap)" class="absolute top-3 right-3 w-3.5 h-3.5 text-amber-500 fill-current pointer-events-none z-10" />

               <!-- Text Content Wrapper -->
               <div class="p-5 pb-0 flex-1 overflow-hidden relative" :style="(cap.content.length > 250 || cap.content.split('\n').length > 6) ? '-webkit-mask-image: linear-gradient(to bottom, black 60%, transparent 100%); mask-image: linear-gradient(to bottom, black 60%, transparent 100%);' : ''">
                   <div class="whitespace-pre-wrap text-[15px] font-medium leading-normal text-[#1c1c1e] dark:text-[#f4f4f5] break-words" v-html="html" @click="onPreviewClick($event, cap)"></div>
               </div>
               
               <!-- Tags Wrapper (Always visible) -->
               <div class="px-5 pt-3 pb-11 relative z-10 w-full shrink-0">
                   <div v-if="tags.length > 0" class="flex flex-wrap gap-1.5 w-full">
                       <span v-for="tag in tags" :key="tag" class="group/tag inline-flex items-center text-[11px] font-medium text-gray-600 dark:text-gray-300 bg-gray-100 dark:bg-[#2a2a2a] px-2 py-0.5 rounded-md transition-colors border border-transparent hover:border-gray-300 dark:hover:border-gray-500 cursor-default">
                           {{ tag }}
                           <button @click.stop="removeTag(cap, tag)" class="ml-1 opacity-0 w-0 overflow-hidden group-hover/tag:opacity-100 group-hover/tag:w-auto transition-all text-gray-400 hover:text-red-500 cursor-pointer" :aria-label="$t('quickcap.remove_tag')">
                               <X class="w-2.5 h-2.5" />
                           </button>
                       </span>
                   </div>
               </div>

               <!-- Bottom Actions Bar (Fixed at bottom of card) -->
               <div class="absolute bottom-0 left-0 w-full px-4 py-2 border-t border-transparent group-hover:border-black/5 dark:group-hover:border-white/5 flex items-center justify-between z-10 transition-colors">
                   <!-- Date (visible by default, hidden on hover) -->
                  <span class="text-[11px] text-gray-400 font-mono tracking-tight group-hover:opacity-0 transition-opacity absolute px-1 pointer-events-none" :class="mobileViewMode === 'grid' ? 'opacity-100' : 'opacity-0 md:opacity-100'">{{ formatDate(cap.created_at) }}</span>
                  
                  <!-- Actions (hidden by default, visible on hover) -->
                  <div class="flex items-center transition-opacity w-full justify-between" :class="mobileViewMode === 'grid' ? 'opacity-0 group-hover:opacity-100' : 'opacity-100 md:opacity-0 group-hover:opacity-100'" @click.stop>
                      <div v-if="taggingCapId === cap.id" class="flex items-center w-full bg-gray-50 dark:bg-[#1a1a1a] rounded px-2 py-0.5 mr-2">
                          <span class="text-gray-400 text-xs mr-1">#</span>
                          <input 
                              v-model="tagInputText" 
                              @keydown.enter.prevent="saveInlineTag(cap)"
                              @keydown.esc.stop="taggingCapId = null"
                              class="bg-transparent border-none outline-none text-xs w-full text-[#1c1c1e] dark:text-[#f4f4f5]"
                              placeholder="tag..."
                              autofocus
                          />
                          <button @click="saveInlineTag(cap)" class="ml-1 text-black dark:text-white font-medium text-[11px] hover:underline">{{ $t('quickcap.save') }}</button>
                      </div>
                      <template v-else>
                          <button @click.stop="deleteCap(cap.id)" :title="$t('quickcap.delete_note')" class="text-gray-400 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 p-1.5 rounded-full transition-colors cursor-pointer">
                              <Trash2 class="w-3.5 h-3.5"/>
                          </button>
                          <div class="flex items-center gap-0.5 relative">
                              <div class="relative">
                                  <button @click.stop="toggleColorPicker(cap.id)" :title="$t('quickcap.change_color')" class="text-gray-400 hover:text-gray-800 dark:hover:text-gray-200 hover:bg-black/5 dark:hover:bg-white/10 p-1.5 rounded-full transition-colors cursor-pointer">
                                      <Palette class="w-3.5 h-3.5"/>
                                  </button>
                                  
                                  <!-- Color Picker Popup -->
                                  <div v-if="colorPickerCapId === cap.id" class="absolute bottom-[calc(100%+8px)] right-0 p-2 bg-white dark:bg-[#2a2a2a] rounded-xl shadow-xl border border-gray-100 dark:border-gray-700 flex flex-wrap gap-2 z-50 w-[140px]" @click.stop>
                                      <button v-for="color in PALETTE" :key="color.name" 
                                          @click="changeCapColor(cap, color.value)"
                                          class="w-6 h-6 rounded-full border border-gray-200 dark:border-gray-600 transition-transform hover:scale-110 cursor-pointer"
                                          :class="colourClass(color.value) || 'bg-[#fdfdfc] dark:bg-[#1e1e1e]'"
                                          :title="color.name"
                                      ></button>
                                  </div>
                              </div>
                              <button @click.stop="setCapFlag([cap], 'pinned', !isPinned(cap))" :title="$t(isPinned(cap) ? 'quickcap.unpin' : 'quickcap.pin')" class="p-1.5 rounded-full transition-colors cursor-pointer" :class="isPinned(cap) ? 'text-amber-500' : 'text-gray-400 hover:text-gray-800 dark:hover:text-gray-200 hover:bg-black/5 dark:hover:bg-white/10'">
                                  <Pin class="w-3.5 h-3.5" :class="isPinned(cap) ? 'fill-current' : ''" />
                              </button>
                              <button @click.stop="setCapFlag([cap], 'archived', !isArchived(cap))" :title="$t(isArchived(cap) ? 'quickcap.unarchive' : 'quickcap.archive')" class="text-gray-400 hover:text-gray-800 dark:hover:text-gray-200 hover:bg-black/5 dark:hover:bg-white/10 p-1.5 rounded-full transition-colors cursor-pointer">
                                  <Archive class="w-3.5 h-3.5" />
                              </button>
                              <button @click.stop="openPromote([cap])" :title="$t('quickcap.promote')" class="text-gray-400 hover:text-gray-800 dark:hover:text-gray-200 hover:bg-black/5 dark:hover:bg-white/10 p-1.5 rounded-full transition-colors cursor-pointer">
                                  <FileText class="w-3.5 h-3.5" />
                              </button>
                              <button @click.stop="pickImageForExistingCap(cap)" :title="$t('quickcap.add_image')" class="text-gray-400 hover:text-gray-800 dark:hover:text-gray-200 hover:bg-black/5 dark:hover:bg-white/10 p-1.5 rounded-full transition-colors cursor-pointer">
                                  <ImageIcon class="w-3.5 h-3.5"/>
                              </button>
                              <button @click="openTagInput(cap)" :title="$t('quickcap.add_tag')" class="text-gray-400 hover:text-gray-800 dark:hover:text-gray-200 hover:bg-black/5 dark:hover:bg-white/10 p-1.5 rounded-full transition-colors cursor-pointer">
                                  <Tag class="w-3.5 h-3.5"/>
                              </button>
                          </div>
                      </template>
                  </div>
               </div>
            </div>
        </div>
      </div>
    </div>

    <!-- What is selected, and what can be done with it -->
    <div
        v-if="selectedCaps.length > 0"
        class="fixed left-1/2 -translate-x-1/2 z-[120] flex items-center gap-4 px-4 py-2.5 rounded-xl bg-[#1c1c1e] dark:bg-[#f4f4f5] text-white dark:text-black shadow-[0_8px_24px_rgba(0,0,0,0.24)]"
        style="bottom: calc(env(safe-area-inset-bottom, 16px) + 5.5rem);"
        role="status"
    >
        <span class="text-sm whitespace-nowrap">{{ $t('quickcap.selected_count', { count: selectedCaps.length }) }}</span>
        <button @click="openPromote(selectedCaps)" class="text-sm font-semibold underline underline-offset-2 cursor-pointer hover:opacity-80 transition-opacity whitespace-nowrap">
            {{ $t('quickcap.promote') }}
        </button>
        <button @click="deleteCaps(selectedCaps)" class="text-sm cursor-pointer hover:opacity-80 transition-opacity whitespace-nowrap">
            {{ $t('quickcap.delete_confirm') }}
        </button>
        <button @click="clearSelection" class="text-sm opacity-70 cursor-pointer hover:opacity-100 transition-opacity whitespace-nowrap">
            {{ $t('quickcap.clear_selection') }}
        </button>
    </div>

    <!-- What the keyboard can do here, said once rather than hidden in a manual -->
    <p v-if="!isMobileOS && quickCaps.length > 0" class="hidden md:block w-full max-w-7xl px-4 mx-auto mt-6 text-[11px] text-gray-400 dark:text-gray-500 select-none">
        {{ $t('quickcap.triage_hint') }} · {{ $t('quickcap.triage_hint_select') }} · {{ $t('quickcap.triage_hint_flags') }}
    </p>

    <!-- Scroll anchor: builds the next batch of cards before it comes into view -->
    <div v-if="hasMoreToRender" ref="loadMoreAnchor" class="h-px w-full" aria-hidden="true"></div>

    <!-- Empty States -->
    <div v-if="quickCaps.length === 0" class="flex flex-col items-center justify-center opacity-30 mt-12 w-full">
        <CheckSquare class="w-16 h-16 mb-4"/>
        <p class="text-lg">{{ $t('quickcap.empty_state') }}</p>
    </div>
    <div v-else-if="filteredCaps.length === 0" class="flex flex-col items-center justify-center mt-12 w-full text-gray-400 dark:text-gray-500">
        <Search class="w-12 h-12 mb-4 opacity-40"/>
        <p class="text-base">{{ $t('quickcap.no_results') }}</p>
        <button @click="searchQuery = ''" class="mt-3 text-sm font-medium text-blue-600 dark:text-blue-400 hover:underline cursor-pointer">
            {{ $t('quickcap.clear_search') }}
        </button>
    </div>

    <!-- Mobile FAB -->
    <button @click="isMobileModalOpen = true" class="md:hidden fixed right-5 w-14 h-14 bg-blue-600 hover:bg-blue-700 text-white rounded-full shadow-[0_8px_16px_rgba(37,99,235,0.24)] flex items-center justify-center active:scale-95 transition-transform z-50" style="bottom: calc(env(safe-area-inset-bottom, 20px) + 5rem);" :aria-label="$t('quickcap.new_quickcap')">
        <Plus class="w-6 h-6" />
    </button>

    <!-- Mobile QuickCap Compose Modal -->
    <div v-if="isMobileModalOpen" class="md:hidden fixed inset-0 z-[110] bg-white dark:bg-[#1e1e1e] flex flex-col" style="padding-top: max(env(safe-area-inset-top), 36px);">
        <!-- Header -->
        <div class="flex justify-between items-center px-4 py-3 border-b border-gray-100 dark:border-[#2c2c2c] shrink-0">
            <button @click="isMobileModalOpen = false" class="text-gray-500 hover:text-gray-800 dark:hover:text-gray-200">
                {{ $t('quickcap.cancel') }}
            </button>
            <button @click="submitCapMobile" :disabled="isSubmitting || !hasDraft" class="font-semibold text-blue-500 disabled:opacity-50">
                {{ $t('quickcap.save') }}
            </button>
        </div>
        
        <!-- Textarea -->
        <textarea
           ref="mobileInputRef"
           v-model="newCapText"
           :placeholder="$t('quickcap.placeholder_mobile')"
           class="flex-1 w-full bg-transparent p-5 resize-none outline-none text-[1.1rem] text-[#1c1c1e] dark:text-[#f4f4f5]"
        ></textarea>
        
        <!-- What is attached, shown rather than described -->
        <div v-if="draftAttachments.length" class="flex flex-wrap gap-2 px-5 pb-3 shrink-0">
            <div v-for="path in draftAttachments" :key="path" class="relative">
                <div v-if="isAudioPath(path)" class="w-16 h-16 rounded-lg border border-[#e6e6e6] dark:border-[#2c2c2c] flex items-center justify-center bg-gray-50 dark:bg-[#2a2a2a]" :title="$t('quickcap.audio_note')">
                    <Mic class="w-6 h-6 text-gray-500" />
                </div>
                <img v-else :src="draftImageSrc(path)" alt="" class="w-16 h-16 object-cover rounded-lg border border-[#e6e6e6] dark:border-[#2c2c2c]" />
                <button
                    @click="removeDraftImage(path)"
                    :aria-label="$t('quickcap.remove_image')"
                    class="absolute -top-1.5 -right-1.5 w-5 h-5 rounded-full bg-[#1c1c1e] dark:bg-[#f4f4f5] text-white dark:text-black flex items-center justify-center shadow-sm cursor-pointer"
                >
                    <X class="w-3 h-3" />
                </button>
            </div>
        </div>

        <!-- Bottom Actions (above keyboard) -->
        <div class="p-3 border-t border-gray-100 dark:border-[#2c2c2c] flex items-center gap-2 bg-gray-50 dark:bg-[#191919]" style="padding-bottom: max(env(safe-area-inset-bottom), 16px);">
            <button @click="insertChecklistItem" class="p-2 rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-[#2a2a2a] transition-colors cursor-pointer" :aria-label="$t('quickcap.checklist')">
                <CheckSquare class="w-5 h-5"/>
            </button>
            <button @click="pickImageForNewCap" class="p-2 rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-[#2a2a2a] transition-colors cursor-pointer" :aria-label="$t('quickcap.pick_image')">
                <ImageIcon class="w-5 h-5"/>
            </button>
            <button
                v-if="audio.isSupported()"
                @click="toggleRecording"
                :aria-label="audio.state.value === 'recording' ? $t('quickcap.stop_recording') : $t('quickcap.record_audio')"
                class="p-2 rounded-lg transition-colors cursor-pointer flex items-center gap-1.5"
                :class="audio.state.value === 'recording' ? 'text-red-500 bg-red-50 dark:bg-red-900/20' : 'text-gray-500 hover:bg-gray-100 dark:hover:bg-[#2a2a2a]'"
            >
                <Square v-if="audio.state.value === 'recording'" class="w-5 h-5 fill-current"/>
                <Mic v-else class="w-5 h-5"/>
                <span v-if="audio.state.value === 'recording'" class="text-[12px] font-mono tabular-nums">{{ formatDuration(audio.durationMs.value) }}</span>
            </button>
            <button @click="appendTagToInput" class="p-2 rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-[#2a2a2a] transition-colors cursor-pointer" :aria-label="$t('quickcap.add_tag')">
                <Tag class="w-5 h-5"/>
            </button>
        </div>
    </div>

    <!-- Full View Modal -->
    <div v-if="selectedCap" class="fixed inset-0 z-[60] flex items-center justify-center p-4 bg-black/40 dark:bg-black/60 backdrop-blur-sm" @click="closeFullView">
        <div class="w-full max-w-2xl max-h-[85vh] rounded-2xl shadow-xl flex flex-col border border-[#e6e6e6] dark:border-[#2c2c2c] overflow-hidden" :class="colourClass(selectedCap.color) || 'bg-white dark:bg-[#1e1e1e]'" @click.stop>
            <div class="p-8 overflow-y-auto flex-1 flex flex-col min-h-0 bg-transparent">
                <EditorContent :editor="editor" class="w-full" />
                
                <!-- Render tags as chips in modal -->
                <div v-if="activeTags.length > 0" class="flex flex-wrap gap-2 mt-6 relative z-10 w-full shrink-0 pt-4 border-t border-gray-100 dark:border-[#2c2c2c]">
                   <span v-for="tag in activeTags" :key="tag" class="group/tag inline-flex items-center text-[12px] font-semibold text-gray-600 dark:text-gray-300 bg-gray-100 dark:bg-[#2a2a2a] px-2.5 py-1 rounded-md transition-colors border border-transparent hover:border-gray-300 dark:hover:border-gray-500 cursor-default">
                       {{ tag }}
                       <button @click.stop="removeActiveTag(tag)" class="ml-1 opacity-0 w-0 overflow-hidden group-hover/tag:opacity-100 group-hover/tag:w-auto transition-all text-gray-400 hover:text-red-500 cursor-pointer" :aria-label="$t('quickcap.remove_tag')">
                           <X class="w-3 h-3" />
                       </button>
                   </span>
                </div>
            </div>
            <div class="py-3 px-4 sm:px-6 bg-gray-50 dark:bg-[#191919] border-t border-[#e6e6e6] dark:border-[#2c2c2c] flex flex-wrap items-center justify-between mt-auto shrink-0 gap-3">
                <div class="flex items-center w-full sm:w-auto justify-between sm:justify-start order-2 sm:order-1" @click.stop>
                    <div v-if="taggingCapId === selectedCap.id" class="flex items-center w-full sm:w-auto bg-gray-100 dark:bg-[#2a2a2a] rounded px-2 py-0.5 mr-2 border border-gray-200 dark:border-gray-700">
                        <span class="text-gray-400 text-xs mr-1">#</span>
                        <input 
                            v-model="tagInputText" 
                            @keydown.enter.prevent="saveInlineTag(selectedCap)"
                            @keydown.esc.stop="taggingCapId = null"
                            class="bg-transparent border-none outline-none text-xs w-full text-[#1c1c1e] dark:text-[#f4f4f5]"
                            placeholder="tag..."
                            autofocus
                        />
                        <button @click="saveInlineTag(selectedCap)" class="ml-1 text-black dark:text-white font-medium text-[11px] hover:underline">{{ $t('quickcap.save') }}</button>
                    </div>
                    <template v-else>
                        <button @click.stop="deleteCap(selectedCap.id)" :title="$t('quickcap.delete_note')" class="text-gray-400 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 p-2 rounded-full transition-colors cursor-pointer">
                            <Trash2 class="w-4 h-4"/>
                        </button>
                        <div class="flex items-center gap-1 sm:gap-2 relative ml-auto sm:ml-4">
                            <div class="relative">
                                <button @click.stop="toggleColorPicker(selectedCap.id)" :title="$t('quickcap.change_color')" class="text-gray-400 hover:text-gray-800 dark:hover:text-gray-200 hover:bg-black/5 dark:hover:bg-white/10 p-2 rounded-full transition-colors cursor-pointer">
                                    <Palette class="w-4 h-4"/>
                                </button>
                                <!-- Color Picker Popup -->
                                <div v-if="colorPickerCapId === selectedCap.id" class="absolute bottom-[calc(100%+12px)] left-0 sm:left-auto sm:right-0 p-2 bg-white dark:bg-[#2a2a2a] rounded-xl shadow-xl border border-gray-100 dark:border-gray-700 flex flex-wrap gap-2 z-[70] w-[140px]" @click.stop>
                                    <button v-for="color in PALETTE" :key="color.name" 
                                        @click="changeCapColor(selectedCap, color.value)"
                                        class="w-6 h-6 rounded-full border border-gray-200 dark:border-gray-600 transition-transform hover:scale-110 cursor-pointer"
                                        :class="colourClass(color.value) || 'bg-[#fdfdfc] dark:bg-[#1e1e1e]'"
                                        :title="color.name"
                                    ></button>
                                </div>
                            </div>
                            <button @click.stop="openPromote([selectedCap])" :title="$t('quickcap.promote')" class="text-gray-400 hover:text-gray-800 dark:hover:text-gray-200 hover:bg-black/5 dark:hover:bg-white/10 p-2 rounded-full transition-colors cursor-pointer">
                                <FileText class="w-4 h-4" />
                            </button>
                            <button @click.stop="pickImageForExistingCap(selectedCap)" :title="$t('quickcap.add_image')" class="text-gray-400 hover:text-gray-800 dark:hover:text-gray-200 hover:bg-black/5 dark:hover:bg-white/10 p-2 rounded-full transition-colors cursor-pointer">
                                <ImageIcon class="w-4 h-4"/>
                            </button>
                            <button @click="openTagInput(selectedCap)" :title="$t('quickcap.add_tag')" class="text-gray-400 hover:text-gray-800 dark:hover:text-gray-200 hover:bg-black/5 dark:hover:bg-white/10 p-2 rounded-full transition-colors cursor-pointer">
                                <Tag class="w-4 h-4"/>
                            </button>
                        </div>
                    </template>
                </div>
                
                <div class="flex items-center justify-between w-full sm:w-auto order-1 sm:order-2">
                    <span class="text-xs text-gray-500 font-mono tracking-tight sm:hidden">{{ formatDate(selectedCap.created_at) }}</span>
                    <button @click="closeFullView" class="px-5 py-2 bg-black dark:bg-white text-white dark:text-black rounded-lg text-sm font-semibold hover:scale-95 transition-all shadow-sm cursor-pointer ml-auto">
                        {{ $t('quickcap.close') }}
                    </button>
                </div>
            </div>
        </div>
    </div>

    <!-- Where a cap goes when it stops being fleeting -->
    <PromoteModal
        v-if="promotingCaps.length > 0"
        :vaultPath="vaultPath"
        :capCount="promotingCaps.length"
        :financeReady="financeSetup !== null"
        @close="promotingCaps = []"
        @choose="onPromoteChosen"
    />

    <!-- Finance's own form, not a copy of it -->
    <TransactionModal
        v-if="bookingCap && draftTransaction && financeSetup"
        :show="true"
        :transaction="draftTransaction"
        :incomeCategories="financeSetup.incomeCategories"
        :expenseCategories="financeSetup.expenseCategories"
        :accounts="financeSetup.accounts"
        @close="closeTransactionForm"
        @save="confirmTransaction"
    />

    <!-- Convert to Task Modal -->
    <TaskEditModal 
        v-if="convertingTaskCaps.length > 0" 
        :task="convertingTaskParams" 
        :showActions="true"
        @save="confirmTurnIntoTask" 
        @close="closeTaskModal" 
    />
    
    <!-- Convert to Note Modal -->
    <NoteEditModal 
        v-if="convertingNoteCaps.length > 0"
        :note="convertingNoteParams"
        @save="confirmTurnIntoNote"
        @close="closeNoteModal"
    />
  </div>
</template>
