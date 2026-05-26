<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch, nextTick, inject } from 'vue';
import { FileText, Search, PanelLeft, PanelLeftClose, PanelRight, PanelRightClose, Hash, Plus, MoreVertical, Pin, Trash2, Edit2, X, ArrowLeft, ArrowRight, ExternalLink, Sun, CaseSensitive, Globe, Calendar, CheckSquare, Palette, Monitor, Download } from 'lucide-vue-next';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { emit as tauriEmit, listen } from '@tauri-apps/api/event';
import { ask, save } from '@tauri-apps/plugin-dialog';



import TiptapEditor from './TiptapEditor.vue';
import NoteGraph from './NoteGraph.vue';
import NavButtons from '../../shared/components/NavButtons.vue';
import NoteExportModal, { type ExportOptions } from './NoteExportModal.vue';
import { marked } from 'marked';
import html2pdf from 'html2pdf.js';
import { writeTextFile, writeFile } from '@tauri-apps/plugin-fs';

import { useAppStore } from '../../stores/useAppStore';
import { storeToRefs } from 'pinia';
import type { NodeMetadata } from '../../types/ipc';
import { logger } from '../../utils/logger';
import type { NavEntry } from '../../stores/useNavigationStore';

// ─── Intra-app navigation ──────────────────────────────────
const pushNavigation = inject<(entry?: NavEntry) => void>('pushNavigation');
let skipNavPush = false;

export interface NoteItem {
  id: string;
  title: string;
  summary: string;
  date: string;
  tags: string[];
  path: string;
  pinned: boolean;
  full_width: boolean;
  content: string;
  linked_projects?: string[];
}

const emit = defineEmits(['open-node']);

const props = defineProps<{
  vaultPath: string;
  isFloatingView?: boolean;
  floatingNoteId?: string | null;
}>();

const appStore = useAppStore();
const { enableDailyNotes, dailyNoteFormat, dailyNoteTag } = storeToRefs(appStore);

const formatDate = (dateStr: string) => {
    if (!dateStr) return '';
    if (!dateStr.includes('T')) return dateStr;
    try {
        const d = new Date(dateStr);
        if (isNaN(d.getTime())) return dateStr;
        const pad = (n: number) => String(n).padStart(2, '0');
        return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
    } catch (e) {
        return dateStr;
    }
};

// ─── Note State ────────────────────────────────────────────
const notes = ref<NoteItem[]>([]);
const currentNoteId = ref<string | null>(null);

// ─── Tab / Content Management ──────────────────────────────
const activeTabs = ref<string[]>([]);
const tabContents = ref<Record<string, string>>({});
const focusedTitles = ref<Record<string, string>>({});
const tabAccessTime = new Map<string, number>();

const currentContent = computed({
   get: () => currentNoteId.value ? tabContents.value[currentNoteId.value] || '' : '',
   set: (val) => {
       if (currentNoteId.value) tabContents.value[currentNoteId.value] = val;
   }
});

const saveTimeouts = new Map<string, ReturnType<typeof setTimeout>>();
const renamedTabs = new Map<string, string>(); // Maps old path to new path to catch delayed editor updates
let isCreatingNote = false;
let suppressWatcherUntil = 0;

const loadNoteFile = async (id: string) => {
    if (!id) return;
    tabAccessTime.set(id, Date.now());
    
    if (!activeTabs.value.includes(id)) {
        if (activeTabs.value.length >= 10) {
            let oldestId = activeTabs.value[0];
            let oldestTime = tabAccessTime.get(oldestId) || Infinity;
            for (const t of activeTabs.value) {
                const time = tabAccessTime.get(t) || 0;
                if (time < oldestTime) {
                    oldestTime = time;
                    oldestId = t;
                }
            }
            activeTabs.value = activeTabs.value.filter(t => t !== oldestId);
            delete tabContents.value[oldestId];
            tabAccessTime.delete(oldestId);
        }
        activeTabs.value.push(id);
    }
    
    if (tabContents.value[id] === undefined) {
        let note = notes.value.find(n => n.id === id);
        if (!note) {
            try {
                const fetchedNode = await invoke<any>('get_node', { id });
                if (fetchedNode) {
                    note = {
                        id: fetchedNode.id,
                        title: fetchedNode.title,
                        content: fetchedNode.content,
                        date: fetchedNode.created_at,
                        path: fetchedNode.id,
                        tags: Array.isArray(fetchedNode.properties?.tags) ? fetchedNode.properties.tags : [],
                        pinned: !!fetchedNode.properties?.pinned,
                        full_width: !!fetchedNode.properties?.full_width,
                        linked_projects: Array.isArray(fetchedNode.properties?.linked_projects) ? fetchedNode.properties.linked_projects : [],
                        summary: fetchedNode.content.substring(0, 150).trim()
                    };
                    notes.value.unshift(note);
                }
            } catch (e) {
                console.error("Failed to fetch missing note", e);
            }
        }
        
        if (note) {
            tabContents.value[id] = note.content;
        }
    }
};

// ─── Size & Toggle State ───────────────────────────────────
const wNoteSidebar = ref(300);
const showNoteSidebar = ref(window.innerWidth >= 768);
const wRightSidebar = ref(288);
const showRightSidebar = ref(window.innerWidth >= 768);

const isDraggingNoteSidebar = ref(false);
const startDragNoteSidebar = () => { isDraggingNoteSidebar.value = true; };
const isDraggingRightSidebar = ref(false);
const startDragRightSidebar = () => { isDraggingRightSidebar.value = true; };

const onMouseMove = (e: MouseEvent) => {
  if (isDraggingNoteSidebar.value) {
    wNoteSidebar.value = Math.max(220, Math.min(e.clientX - 64, 600));
  } else if (isDraggingRightSidebar.value) {
    wRightSidebar.value = Math.max(200, Math.min(window.innerWidth - e.clientX, 600));
  }
};
const onMouseUp = () => {
  isDraggingNoteSidebar.value = false;
  isDraggingRightSidebar.value = false;
};

// ─── Note Manager State ────────────────────────────────────
const viewMode = ref<'editor' | 'manager'>('editor');
const managerFilter = ref('');
const managerSearchQuery = ref('');

// ─── Context Menu & Search ─────────────────────────────────
const activeContextMenu = ref<string | null>(null);

const exportModalVisible = ref(false);

const convertAssetsToBase64 = async (html: string): Promise<string> => {
    if (!props.vaultPath) return html;
    const sep = props.vaultPath.includes('\\') ? '\\' : '/';
    const parser = new DOMParser();
    const doc = parser.parseFromString(html, 'text/html');
    const imgs = doc.querySelectorAll('img');
    
    for (let img of imgs) {
        const src = img.getAttribute('src');
        if (src && src.startsWith('assets/')) {
            try {
                const decodedName = decodeURIComponent(src.substring(7));
                const absPath = `${props.vaultPath}${sep}assets${sep}${decodedName}`;
                const assetUrl = convertFileSrc(absPath);
                
                const response = await fetch(assetUrl);
                if (!response.ok) throw new Error(`Network response was not ok: ${response.statusText}`);
                const blob = await response.blob();
                
                const base64 = await new Promise<string>((resolve, reject) => {
                    const reader = new FileReader();
                    reader.onloadend = () => resolve(reader.result as string);
                    reader.onerror = reject;
                    reader.readAsDataURL(blob);
                });
                
                img.setAttribute('src', base64);
            } catch (e) {
                logger.error('Failed to convert image to base64:', src, e);
            }
        }
    }
    return doc.body.innerHTML;
};

const handleExportOption = async (options: ExportOptions) => {
    exportModalVisible.value = false;
    const note = notes.value.find(n => n.id === currentNoteId.value);
    if (!note) return;

    try {
        let defaultFileName = note.title ? note.title.replace(/[/\\?%*:|"<>]/g, '-') : 'Untitled';
        if (options.format === 'md') {
            const filePath = await save({ defaultPath: `${defaultFileName}.md`, filters: [{ name: 'Markdown', extensions: ['md'] }] });
            if (!filePath) return;
            
            let content = '';
            if (options.includeTitle) content += `# ${note.title}\n\n`;
            if (options.includeTags && note.tags.length > 0) content += `Tags: ${note.tags.map(t => '#' + t.split('/').pop()).join(', ')}\n\n`;
            content += currentContent.value;
            
            await writeTextFile(filePath, content);
        } else if (options.format === 'html') {
            const filePath = await save({ defaultPath: `${defaultFileName}.html`, filters: [{ name: 'HTML', extensions: ['html'] }] });
            if (!filePath) return;
            
            let mdContent = '';
            if (options.includeTitle) mdContent += `# ${note.title}\n\n`;
            if (options.includeTags && note.tags.length > 0) mdContent += `**Tags:** ${note.tags.map(t => '#' + t.split('/').pop()).join(', ')}\n\n`;
            mdContent += currentContent.value;
            
            let htmlBody = await marked.parse(mdContent);
            htmlBody = await convertAssetsToBase64(htmlBody);
            
            const htmlContent = `
<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>${note.title}</title>
<style>
  body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif; line-height: 1.6; color: #1c1c1e; padding: 2rem; max-width: 800px; margin: 0 auto; }
  h1, h2, h3, h4, h5, h6 { color: #000; font-weight: 600; margin-top: 1.5em; margin-bottom: 0.5em; }
  h1 { font-size: 2em; border-bottom: 1px solid #eaecef; padding-bottom: 0.3em; }
  a { color: #0366d6; text-decoration: none; }
  a:hover { text-decoration: underline; }
  pre { background-color: #f6f8fa; padding: 16px; overflow: auto; border-radius: 3px; font-family: ui-monospace, SFMono-Regular, SF Mono, Menlo, Consolas, Liberation Mono, monospace; font-size: 85%; }
  pre code { background-color: transparent; padding: 0; border-radius: 0; font-size: 100%; }
  code { background-color: rgba(27,31,35,0.05); padding: 0.2em 0.4em; border-radius: 3px; font-family: ui-monospace, SFMono-Regular, SF Mono, Menlo, Consolas, Liberation Mono, monospace; font-size: 85%; }
  blockquote { padding: 0 1em; color: #6a737d; border-left: 0.25em solid #dfe2e5; margin: 0; }
  img { max-width: 100%; height: auto; display: block; margin: 1em 0; border-radius: 4px; }
  table { border-collapse: collapse; width: 100%; margin-top: 0; margin-bottom: 16px; }
  table th, table td { padding: 6px 13px; border: 1px solid #dfe2e5; }
  table tr:nth-child(2n) { background-color: #f6f8fa; }
  ul, ol { padding-left: 2em; }
  hr { border: 0; border-bottom: 1px solid #eaecef; margin: 2em 0; }
</style>
</head>
<body>
${htmlBody}
</body>
</html>`;
            await writeTextFile(filePath, htmlContent);
        } else if (options.format === 'pdf') {
            const filePath = await save({ defaultPath: `${defaultFileName}.pdf`, filters: [{ name: 'PDF', extensions: ['pdf'] }] });
            if (!filePath) return;
            
            let mdContent = '';
            if (options.includeTitle) mdContent += `# ${note.title}\n\n`;
            if (options.includeTags && note.tags.length > 0) mdContent += `**Tags:** ${note.tags.map(t => '#' + t.split('/').pop()).join(', ')}\n\n`;
            mdContent += currentContent.value;
            
            let htmlBody = await marked.parse(mdContent);
            htmlBody = await convertAssetsToBase64(htmlBody);
            
            const container = document.createElement('div');
            container.innerHTML = htmlBody;
            container.style.padding = '20px';
            container.style.fontFamily = '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif';
            container.style.color = '#1c1c1e';
            container.style.lineHeight = '1.6';
            
            const headings = container.querySelectorAll('h1, h2, h3, h4, h5, h6');
            headings.forEach((el: any) => { el.style.color = '#000'; el.style.fontWeight = '600'; el.style.marginTop = '1em'; el.style.marginBottom = '0.5em'; });
            const h1s = container.querySelectorAll('h1');
            h1s.forEach((el: any) => { el.style.fontSize = '2em'; el.style.borderBottom = '1px solid #eaecef'; el.style.paddingBottom = '0.3em'; });
            const pres = container.querySelectorAll('pre');
            pres.forEach((el: any) => { el.style.backgroundColor = '#f6f8fa'; el.style.padding = '16px'; el.style.overflow = 'auto'; el.style.borderRadius = '3px'; el.style.whiteSpace = 'pre-wrap'; el.style.fontFamily = 'ui-monospace, SFMono-Regular, SF Mono, Menlo, Consolas, Liberation Mono, monospace'; el.style.fontSize = '85%'; });
            const codes = container.querySelectorAll('code');
            codes.forEach((el: any) => { el.style.backgroundColor = 'rgba(27,31,35,0.05)'; el.style.padding = '0.2em 0.4em'; el.style.borderRadius = '3px'; el.style.fontFamily = 'ui-monospace, SFMono-Regular, SF Mono, Menlo, Consolas, Liberation Mono, monospace'; el.style.fontSize = '85%'; });
            const preCodes = container.querySelectorAll('pre code');
            preCodes.forEach((el: any) => { el.style.backgroundColor = 'transparent'; el.style.padding = '0'; el.style.borderRadius = '0'; el.style.fontSize = '100%'; });
            const blockquotes = container.querySelectorAll('blockquote');
            blockquotes.forEach((el: any) => { el.style.padding = '0 1em'; el.style.color = '#6a737d'; el.style.borderLeft = '0.25em solid #dfe2e5'; el.style.margin = '0'; });
            const imgs = container.querySelectorAll('img');
            imgs.forEach((el: any) => { el.style.maxWidth = '100%'; el.style.height = 'auto'; el.style.display = 'block'; el.style.margin = '1em 0'; el.style.borderRadius = '4px'; });
            const tables = container.querySelectorAll('table');
            tables.forEach((el: any) => { el.style.borderCollapse = 'collapse'; el.style.width = '100%'; el.style.marginBottom = '16px'; });
            const thsTds = container.querySelectorAll('th, td');
            thsTds.forEach((el: any) => { el.style.padding = '6px 13px'; el.style.border = '1px solid #dfe2e5'; });
            const hrs = container.querySelectorAll('hr');
            hrs.forEach((el: any) => { el.style.border = '0'; el.style.borderBottom = '1px solid #eaecef'; el.style.margin = '2em 0'; });
            
            document.body.appendChild(container);
            
            const opt: any = {
              margin:       10,
              filename:     defaultFileName + '.pdf',
              image:        { type: 'jpeg', quality: 0.98 },
              html2canvas:  { scale: 2, useCORS: true },
              jsPDF:        { unit: 'mm', format: options.pdfFormat, orientation: options.pdfOrientation },
              pagebreak:    { mode: ['css', 'legacy', 'avoid-all'] }
            };
            
            const pdfBlob = await html2pdf().set(opt).from(container).output('blob');
            document.body.removeChild(container);
            
            const buffer = await pdfBlob.arrayBuffer();
            const uint8Array = new Uint8Array(buffer);
            
            await writeFile(filePath, uint8Array);
        }
    } catch (e) {
        logger.error('Export failed:', e);
        alert('Export failed. Check the logs for details.');
    }
};

const zenMode = ref(false);
watch(zenMode, (val) => {
    if (val) {
        document.body.classList.add('zen-mode');
        showNoteSidebar.value = false;
        showRightSidebar.value = false;
    } else {
        document.body.classList.remove('zen-mode');
        showNoteSidebar.value = true;
    }
});

const searchQuery = ref('');
const newTagInput = ref('');
const isCaseSensitiveSearch = ref(false);
const backendSearchIds = ref<string[] | null>(null); // null = no active backend search
let searchTimeout: ReturnType<typeof setTimeout>;

// ─── Rename Modal (replaces window.prompt for mobile compat) ──
const renameModal = ref<{ show: boolean; noteId: string; value: string }>({ show: false, noteId: '', value: '' });

const toggleContext = (id: string, e: Event) => {
  e.stopPropagation();
  activeContextMenu.value = activeContextMenu.value === id ? null : id;
};

// ─── Daily Note Format Validation ──────────────────────────
const isValidDailyFormat = computed(() => {
  const fmt = dailyNoteFormat.value;
  return fmt && (fmt.includes('YYYY') || fmt.includes('YY')) && (fmt.includes('MM') || fmt.includes('M')) && (fmt.includes('DD') || fmt.includes('D'));
});

// ─── Frontmatter Utils ─────────────────────────────────────
// Frontmatter handled by Node core

// ─── Note CRUD Operations ──────────────────────────────────
const handleNoteSelect = (id: string) => {
    if (id !== currentNoteId.value && currentNoteId.value && !skipNavPush) {
        pushNavigation?.({ app: 'note', itemId: currentNoteId.value });
    }
    currentNoteId.value = id;
    viewMode.value = 'editor';
    if (window.innerWidth < 768) {
        showNoteSidebar.value = false;
    }
};

const editorFullWidth = computed({
    get: () => {
        if (!currentNoteId.value) return false;
        const note = notes.value.find(n => n.id === currentNoteId.value);
        return note ? note.full_width : false;
    },
    set: async (val: boolean) => {
        if (!currentNoteId.value) return;
        const note = notes.value.find(n => n.id === currentNoteId.value);
        if (note) {
            note.full_width = val;
            await invoke('write_node_file', { 
                vaultPath: props.vaultPath, 
                relPath: note.id, 
                title: note.title,
                nodeType: 'note',
                properties: { pinned: note.pinned, full_width: note.full_width, tags: note.tags, linked_projects: note.linked_projects },
                content: currentContent.value 
            });
        }
    }
});

const togglePin = async (id: string) => {
    const note = notes.value.find(n => n.id === id);
    if (!note) return;
    note.pinned = !note.pinned;
    try {
        const body = tabContents.value[id] !== undefined ? tabContents.value[id] : note.content;
        await invoke('write_node_file', { 
            vaultPath: props.vaultPath, 
            relPath: note.id, 
            title: note.title,
            nodeType: 'note',
            properties: { pinned: note.pinned, full_width: note.full_width, tags: note.tags, linked_projects: note.linked_projects },
            content: body 
        });
        scanVault();
    } catch(e) { logger.error('Pin fail:', e); }
};

const deleteNote = async (id: string) => {
    const isConfirmed = await ask('This note will be permanently deleted. This action cannot be undone.', { 
        title: 'Delete this note?', 
        kind: 'warning',
        okLabel: 'Delete',
        cancelLabel: 'Cancel'
    });
    if (!isConfirmed) return;
    try {
        if (saveTimeouts.has(id)) {
           clearTimeout(saveTimeouts.get(id)!);
           saveTimeouts.delete(id);
        }
        await invoke('delete_node_file', { vaultPath: props.vaultPath, relPath: id });
        delete tabContents.value[id];
        activeTabs.value = activeTabs.value.filter(t => t !== id);
        tabAccessTime.delete(id);
        if (currentNoteId.value === id) {
           currentNoteId.value = null;
        }
        scanVault();
    } catch(e) { logger.error('Delete fail:', e); }
};

const openInNewWindow = async (id: string) => {
    try { await invoke('spawn_node_window', { nodeId: id }); } catch(e) { logger.error("Failed to open node in new window", e); }
    activeContextMenu.value = null;
};

const handleRenamePrompt = (id: string) => {
    const note = notes.value.find(n => n.id === id);
    if (!note) return;
    renameModal.value = { show: true, noteId: id, value: note.title };
    activeContextMenu.value = null;
};

const confirmRename = async () => {
    const { noteId, value: newName } = renameModal.value;
    renameModal.value.show = false;
    const note = notes.value.find(n => n.id === noteId);
    if (!note || !newName || newName === note.title) return;
    try {
        const oldId = note.id;
        // Cancel any pending auto-save for the old path to prevent it from recreating the file after rename
        if (saveTimeouts.has(oldId)) {
            clearTimeout(saveTimeouts.get(oldId)!);
            saveTimeouts.delete(oldId);
        }
        const savedContent = tabContents.value[oldId];
        const newPath = await invoke<string>('rename_node_file', { vaultPath: props.vaultPath, oldRelPath: oldId, newName });
        
        // Secondary cancellation: if the user typed during the await rename_node_file, a new timeout for the old path might have been created.
        let needsSave = false;
        if (saveTimeouts.has(oldId)) {
            clearTimeout(saveTimeouts.get(oldId)!);
            saveTimeouts.delete(oldId);
            needsSave = true;
        }

        note.title = newName;
        if (oldId !== newPath) {
            renamedTabs.set(oldId, newPath);
            
            if (savedContent !== undefined) {
                tabContents.value[newPath] = tabContents.value[oldId] || savedContent;
                delete tabContents.value[oldId];
            }
            if (activeTabs.value.includes(oldId)) {
                activeTabs.value = activeTabs.value.map(id => id === oldId ? newPath : id);
            }
            if (tabAccessTime.has(oldId)) {
                tabAccessTime.set(newPath, tabAccessTime.get(oldId)!);
                tabAccessTime.delete(oldId);
            }
        }

        if (currentNoteId.value === oldId) {
            currentNoteId.value = newPath;
        }
        
        const contentBody = tabContents.value[newPath] || savedContent || note.content;
        await invoke('write_node_file', { 
            vaultPath: props.vaultPath, 
            relPath: newPath, 
            title: newName,
            nodeType: 'note',
            properties: { pinned: note.pinned, full_width: note.full_width, tags: note.tags, linked_projects: note.linked_projects },
            content: contentBody 
        });
        
        if (needsSave) {
            saveNoteForTab(newPath);
        }
        if (oldId !== newPath) {
            delete focusedTitles.value[oldId];
        }
        delete focusedTitles.value[newPath];
        scanVault();
    } catch(err) { alert(err); }
};

const renameTopTitle = async (e: Event) => {
    const isEnter = e.type === 'keydown' && (e as KeyboardEvent).key === 'Enter';
    const newTitle = (e.target as HTMLInputElement).value.trim();
    const note = notes.value.find(n => n.id === currentNoteId.value);
    
    const focusEditor = () => {
        if (editorRefs.value && editorRefs.value.length > 0) {
            editorRefs.value.forEach(ref => {
                if (ref && typeof ref.focus === 'function') ref.focus();
            });
        }
    };

    if (!note || note.title === newTitle || !newTitle) {
        if (isEnter) focusEditor();
        if (note) delete focusedTitles.value[note.id];
        return;
    }
    
    try {
        const oldId = note.id;
        // Cancel any pending auto-save for the old path to prevent it from recreating the file after rename
        if (saveTimeouts.has(oldId)) {
            clearTimeout(saveTimeouts.get(oldId)!);
            saveTimeouts.delete(oldId);
        }
        const savedContent = tabContents.value[oldId] || '';
        const newPath = await invoke<string>('rename_node_file', { vaultPath: props.vaultPath, oldRelPath: oldId, newName: newTitle });
        
        // Secondary cancellation: if the user typed during the await rename_node_file, a new timeout for the old path might have been created.
        let needsSave = false;
        if (saveTimeouts.has(oldId)) {
            clearTimeout(saveTimeouts.get(oldId)!);
            saveTimeouts.delete(oldId);
            needsSave = true;
        }

        note.title = newTitle;
        if (oldId !== newPath) {
            if (tabContents.value[oldId] !== undefined) {
                tabContents.value[newPath] = tabContents.value[oldId];
                delete tabContents.value[oldId];
            }
            if (activeTabs.value.includes(oldId)) {
                activeTabs.value = activeTabs.value.map(id => id === oldId ? newPath : id);
            }
            if (tabAccessTime.has(oldId)) {
                tabAccessTime.set(newPath, tabAccessTime.get(oldId)!);
                tabAccessTime.delete(oldId);
            }
        }

        currentNoteId.value = newPath;
        const contentBody = tabContents.value[newPath] || savedContent || note.content;
        await invoke('write_node_file', { 
            vaultPath: props.vaultPath, 
            relPath: newPath, 
            title: newTitle,
            nodeType: 'note',
            properties: { pinned: note.pinned, full_width: note.full_width, tags: note.tags, linked_projects: note.linked_projects },
            content: contentBody 
        });
        scanVault();
        
        if (needsSave) {
            saveNoteForTab(newPath);
        }
        
        if (isEnter) {
            setTimeout(focusEditor, 50);
        }
    } catch(err) { alert(err); }
};

const addTag = async (e: KeyboardEvent) => {
   if (e.key === 'Enter' && newTagInput.value.trim()) {
       const note = notes.value.find(n => n.id === currentNoteId.value);
       if (note && !note.tags.includes(newTagInput.value.trim())) {
           note.tags.push(newTagInput.value.trim());
           newTagInput.value = '';
           await invoke('write_node_file', { 
               vaultPath: props.vaultPath, 
               relPath: note.id, 
               title: note.title,
               nodeType: 'note',
               properties: { pinned: note.pinned, full_width: note.full_width, tags: note.tags, linked_projects: note.linked_projects },
               content: currentContent.value 
           });
           scanVault();
       }
   }
};

const removeTag = async (tagToRemove: string) => {
   const note = notes.value.find(n => n.id === currentNoteId.value);
   if (note) {
       note.tags = note.tags.filter(t => t !== tagToRemove);
       await invoke('write_node_file', { 
           vaultPath: props.vaultPath, 
           relPath: note.id, 
           title: note.title,
           nodeType: 'note',
           properties: { pinned: note.pinned, full_width: note.full_width, tags: note.tags, linked_projects: note.linked_projects },
           content: currentContent.value 
       });
       scanVault();
   }
};

// ─── Tags Data Logic ───────────────────────────────────────
interface TagNode {
  name: string;
  basename: string;
  count: number;
  expanded: boolean;
  children: TagNode[];
}

const tagTree = ref<TagNode[]>([]);
const selectedTags = ref<Set<string>>(new Set());

const buildTagTree = (allNotes: NoteItem[]) => {
  const map = new Map<string, { count: number, children: Set<string> }>();
  allNotes.forEach(n => {
    n.tags.forEach(tagPath => {
       const parts = tagPath.split('/');
       const parent = parts[0];
       if (!map.has(parent)) map.set(parent, { count: 0, children: new Set() });
       map.get(parent)!.count++;
       if (parts.length > 1) {
          const childName = `${parent}/${parts[1]}`;
          map.get(parent)!.children.add(childName);
          if (!map.has(childName)) map.set(childName, { count: 0, children: new Set() });
          map.get(childName)!.count++;
       }
    })
  });
  const tree: TagNode[] = [];
  map.forEach((data, name) => {
    if (!name.includes('/')) {
      const children: TagNode[] = Array.from(data.children).map(childName => ({
        name: childName, basename: childName.split('/')[1], count: map.get(childName)?.count || 0, expanded: false, children: []
      }));
      tree.push({ name, basename: name, count: data.count, expanded: true, children });
    }
  });
  tagTree.value = tree.sort((a,b) => a.name.localeCompare(b.name));
};

const toggleTagSelection = (tagName: string) => {
  const newSet = new Set(selectedTags.value);
  if (newSet.has(tagName)) newSet.delete(tagName);
  else newSet.add(tagName);
  selectedTags.value = newSet;
};

// ─── API Calls ─────────────────────────────────────────────
async function scanVault() {
   if (!props.vaultPath) return;
   try {
       const scannedNodes = await invoke<NodeMetadata[]>('get_nodes', { nodeType: 'note' });
       const scannedNotes = scannedNodes.map(n => {
           let tags: string[] = [];
           if (Array.isArray(n.properties?.tags)) tags = n.properties.tags as string[];
           return {
               id: n.id,
               title: n.title,
               content: n.content,
               date: n.created_at,
               path: n.id,
               tags: tags,
               pinned: !!n.properties?.pinned,
               full_width: !!n.properties?.full_width,
               linked_projects: Array.isArray(n.properties?.linked_projects) ? n.properties.linked_projects : [],
               summary: n.content.substring(0, 150).trim()
           };
       });
       notes.value = scannedNotes;
       buildTagTree(scannedNotes);
       if (scannedNotes.length > 0 && !currentNoteId.value) {
           currentNoteId.value = scannedNotes[0].id;
       } else if (scannedNotes.length === 0) {
           currentNoteId.value = null;
       }
   } catch(e) { logger.error("Failed to scan vault:", e); }
}

const createNewNote = async () => {
    if (!props.vaultPath || isCreatingNote) return;
    isCreatingNote = true;
    suppressWatcherUntil = Date.now() + 3000;
    try {
        const newPath = await invoke<string>('create_node_file', { vaultPath: props.vaultPath, directory: 'Notes', nodeType: 'note' });
        await scanVault();
        if (newPath) {
            currentNoteId.value = newPath;
            viewMode.value = 'editor';
            await nextTick();
            const titleInput = document.querySelector('.note-title-input') as HTMLInputElement;
            if (titleInput) {
                titleInput.focus();
                titleInput.select();
            }
        }
    } catch(e) { logger.error("Failed to create note:", e); }
    finally { isCreatingNote = false; }
}

async function openDailyNote() {
    if (!props.vaultPath) return;
    try {
        const finalFormat = isValidDailyFormat.value ? dailyNoteFormat.value : 'YYYY-MM-DD';
        const tag = dailyNoteTag.value.trim();
        const dailyPath = await invoke<string>('open_daily_note', { vaultPath: props.vaultPath, formatStr: finalFormat, tag });
        await scanVault();
        if (dailyPath) { currentNoteId.value = dailyPath; viewMode.value = 'editor'; }
    } catch(e) { logger.error("Failed to open daily note:", e); }
}

const handleOpenDailyNote = async () => {
    await openDailyNote();
    if (window.innerWidth < 768) showNoteSidebar.value = false;
};

const handleCreateNewNote = async () => {
    await createNewNote();
    if (window.innerWidth < 768) showNoteSidebar.value = false;
};

// ─── Save & Editor ─────────────────────────────────────────
const saveNoteForTab = (rawTabId: string) => {
    let tabId = rawTabId;
    while (renamedTabs.has(tabId)) {
        tabId = renamedTabs.get(tabId)!;
    }
    const note = notes.value.find(n => n.id === tabId);
    if (!note) { logger.warn('[NoteApp] saveNoteForTab: note not found for', tabId); return; }
    const existing = saveTimeouts.get(tabId);
    if (existing) clearTimeout(existing);
    saveTimeouts.set(tabId, setTimeout(async () => {
        saveTimeouts.delete(tabId);
        suppressWatcherUntil = Date.now() + 3000;
        const content = tabContents.value[tabId] || '';
        let fullRaw = content;
        try {
            await invoke('write_node_file', { 
                vaultPath: props.vaultPath, 
                relPath: note.id, 
                title: note.title,
                nodeType: 'note',
                properties: { pinned: note.pinned, full_width: note.full_width, tags: note.tags, linked_projects: note.linked_projects },
                content: fullRaw 
            });
            note.summary = content.substring(0, 150).trim();
            tauriEmit('note-updated', { id: note.id, content });
            // Notify transclusion nodes that this note's blocks may have changed
            window.dispatchEvent(new CustomEvent('synabit-block-refresh', {
              detail: { nodeId: note.id }
            }));
        } catch(e) { logger.error("Failed to save note:", String(e)); }
    }, 600));
}

const currentBacklinks = ref<NodeMetadata[]>([]);
const activeNote = computed(() => notes.value.find(n => n.id === currentNoteId.value) || null);

const currentOutgoingLinks = computed(() => {
    if (!currentContent.value) return [];
    const regex = /synabit:\/\/note\/([^\s\)"']+)/g;
    const links = new Set<string>();
    let m;
    while ((m = regex.exec(currentContent.value)) !== null) {
        const targetFilename = decodeURIComponent(m[1]);
        const targetNote = notes.value.find(n => n.path.endsWith(targetFilename));
        if (targetNote) links.add(targetNote.id);
        else links.add(targetFilename);
    }
    return Array.from(links);
});

const editorRefs = ref<any[]>([]);

const onEditorUpdate = (val: string, rawTabId: string) => {
    let tabId = rawTabId;
    while (renamedTabs.has(tabId)) {
        tabId = renamedTabs.get(tabId)!;
    }
    tabContents.value[tabId] = val;
    if (currentNoteId.value === tabId) {
        tauriEmit('note-updated', { id: tabId, content: val });
    }
    saveNoteForTab(tabId);
};

watch(currentNoteId, async (newId) => {
    if (newId) {
        await loadNoteFile(newId);
        try {
            const note = notes.value.find(n => n.id === newId);
            const backlinks = await invoke<NodeMetadata[]>('get_linked_nodes', { targetTitle: note?.title || '', targetId: newId });
            
            let outgoingProjects: NodeMetadata[] = [];
            const linkedProjects: string[] = (note as any)?.linked_projects || [];
            for (const link of linkedProjects) {
               const m = /synabit:\/\/project\/([^\s\)"']+)/.exec(link);
               if (m && m[1]) {
                   try {
                       const proj = await invoke<any>('get_node', { id: decodeURIComponent(m[1]) });
                       if (proj) {
                           proj.node_type = 'project';
                           proj._is_outgoing_project = true;
                           outgoingProjects.push(proj);
                       }
                   } catch(e) {}
               }
            }
            
            currentBacklinks.value = [...backlinks, ...outgoingProjects];
        } catch (e) { logger.error(String(e)); currentBacklinks.value = []; }
    } else { currentBacklinks.value = []; }
});

const handleOpenInternalNote = (data: any) => {
    // If we receive a string, it's legacy behavior (assume note)
    const noteId = typeof data === 'string' ? data : data.id;
    const type = typeof data === 'string' ? 'note' : data.type;

    if (type === 'note' || type === 'node') {
        const exists = notes.value.find(n => n.id === noteId);
        const resolved = exists || notes.value.find(n => n.id.endsWith(noteId));
        if (resolved) {
            if (resolved.id !== currentNoteId.value && currentNoteId.value && !skipNavPush) {
                pushNavigation?.({ app: 'note', itemId: currentNoteId.value });
            }
            currentNoteId.value = resolved.id;
        }
    } else {
        // Emit up to App.vue to switch tools and open the node
        emit('open-node', noteId, type);
    }
};

const unlinkProject = async (projectId: string, projectTitle?: string) => {
    const isConfirmed = await ask(
        `This note will no longer be linked to "${projectTitle || 'this project'}".`, 
        { 
            title: 'Unlink project?', 
            kind: 'warning',
            okLabel: 'Unlink',
            cancelLabel: 'Cancel'
        }
    );
    if (!isConfirmed) return;

    const note = notes.value.find(n => n.id === currentNoteId.value);
    if (!note || !note.linked_projects) return;
    
    const linkToRemove = note.linked_projects.find((link: string) => {
        const m = /synabit:\/\/project\/([^\s\)"']+)/.exec(link);
        return m && decodeURIComponent(m[1]) === projectId;
    });

    if (linkToRemove) {
        note.linked_projects = note.linked_projects.filter((l: string) => l !== linkToRemove);
        currentBacklinks.value = currentBacklinks.value.filter(bl => bl.id !== projectId);
        
        await invoke('write_node_file', { 
            vaultPath: props.vaultPath, 
            relPath: note.id, 
            title: note.title,
            nodeType: 'note',
            properties: { pinned: note.pinned, full_width: note.full_width, tags: note.tags, linked_projects: note.linked_projects },
            content: currentContent.value 
        });
        scanVault();
    }
};

// ─── Derived State ─────────────────────────────────────────
const allTags = computed(() => {
    const counts = new Map<string, number>();
    notes.value.forEach(note => { note.tags.forEach(tag => { counts.set(tag, (counts.get(tag) || 0) + 1); }); });
    return Array.from(counts.entries()).map(([name, count]) => ({ name, count })).sort((a,b) => b.count - a.count);
});

const topTags = computed(() => allTags.value.slice(0, 10));
const allPinnedNotes = computed(() => filteredNotes.value.filter(n => n.pinned));
const topPinnedNotes = computed(() => allPinnedNotes.value.slice(0, 5));
const recentNotes = computed(() => filteredNotes.value.filter(n => !n.pinned).slice(0, 10));

const openNoteManager = (filterType: string) => {
    managerFilter.value = filterType;
    viewMode.value = 'manager';
    if (window.innerWidth < 768) {
        showNoteSidebar.value = false;
    }
};

const managerBackendSearchIds = ref<string[] | null>(null);
let managerSearchTimeout: ReturnType<typeof setTimeout>;

const managerFilteredNotes = computed(() => {
   let result = notes.value;
   if (managerSearchQuery.value.trim()) {
      // Backend FTS5 results available
      if (managerBackendSearchIds.value !== null) {
          const idSet = new Set(managerBackendSearchIds.value);
          result = result.filter(n => idSet.has(n.id));
          const orderMap = new Map(managerBackendSearchIds.value.map((id, i) => [id, i]));
          result = result.sort((a, b) => (orderMap.get(a.id) ?? 999) - (orderMap.get(b.id) ?? 999));
      } else {
          // Fallback: local search while backend is loading
          const q = managerSearchQuery.value.trim();
          const isTagSearch = q.startsWith('#');
          const searchTerm = isTagSearch ? q.slice(1) : q;
          const searchStr = isCaseSensitiveSearch.value ? searchTerm : searchTerm.toLowerCase();
          const match = (text: string) => {
             if (!text) return false;
             return isCaseSensitiveSearch.value ? text.includes(searchStr) : text.toLowerCase().includes(searchStr);
          };
          result = result.filter(n => {
             if (isTagSearch) return n.tags.some(t => match(t));
             return match(n.title) || n.tags.some(t => match(t)) || match(n.content);
          });
      }
   }
   if (managerFilter.value === 'notes' || !managerFilter.value || managerFilter.value === 'tags') return result;
   else if (managerFilter.value === 'pinned') return result.filter(n => n.pinned);
   else return result.filter(n => n.tags.includes(managerFilter.value));
});

const managerCurrentPage = ref(1);
const managerItemsPerPage = 50;

watch([managerSearchQuery, managerFilter], () => {
    managerCurrentPage.value = 1;
});

// Debounced backend search for Note Manager
watch(managerSearchQuery, (q) => {
    clearTimeout(managerSearchTimeout);
    if (!q.trim()) {
        managerBackendSearchIds.value = null;
        return;
    }
    managerSearchTimeout = setTimeout(async () => {
        try {
            const resp = await invoke<{ results: { id: string }[], total_count: number, query_time_ms: number }>('search_notes', {
                vaultPath: props.vaultPath,
                query: q
            });
            if (managerSearchQuery.value === q) {
                managerBackendSearchIds.value = resp.results.map(r => r.id);
            }
        } catch (e) {
            logger.error('Manager backend search error', e);
        }
    }, 200);
});

const managerTotalPages = computed(() => Math.ceil(managerFilteredNotes.value.length / managerItemsPerPage));

const managerPaginatedNotes = computed(() => {
    const start = (managerCurrentPage.value - 1) * managerItemsPerPage;
    return managerFilteredNotes.value.slice(start, start + managerItemsPerPage);
});

const managerNextPage = () => {
    if (managerCurrentPage.value < managerTotalPages.value) managerCurrentPage.value++;
};

const managerPrevPage = () => {
    if (managerCurrentPage.value > 1) managerCurrentPage.value--;
};

const filteredNotes = computed(() => {
  let result = notes.value;
  // Use backend FTS5 search results when available
  if (searchQuery.value.trim() && backendSearchIds.value !== null) {
      const idSet = new Set(backendSearchIds.value);
      result = result.filter(n => idSet.has(n.id));
      // Preserve the order from backend (BM25 ranked)
      const orderMap = new Map(backendSearchIds.value.map((id, i) => [id, i]));
      result = result.sort((a, b) => (orderMap.get(a.id) ?? 999) - (orderMap.get(b.id) ?? 999));
  } else if (searchQuery.value.trim()) {
      // Fallback: local search while backend is loading
      const q = searchQuery.value.trim();
      const isTagSearch = q.startsWith('#');
      const searchTerm = isTagSearch ? q.slice(1) : q;
      const searchStr = isCaseSensitiveSearch.value ? searchTerm : searchTerm.toLowerCase();
      const match = (text: string) => {
         if (!text) return false;
         return isCaseSensitiveSearch.value ? text.includes(searchStr) : text.toLowerCase().includes(searchStr);
      };
      result = result.filter(n => {
          if (isTagSearch) return n.tags.some(t => match(t));
          return match(n.title) || n.tags.some(t => match(t)) || match(n.content);
      });
  }
  if (selectedTags.value.size > 0) {
      result = result.filter(n => n.tags.some(t => selectedTags.value.has(t)));
  }
  return result.sort((a,b) => {
      if (a.pinned && !b.pinned) return -1;
      if (!a.pinned && b.pinned) return 1;
      return b.date.localeCompare(a.date);
  });
});

// Debounced backend search
watch(searchQuery, (q) => {
    clearTimeout(searchTimeout);
    if (!q.trim()) {
        backendSearchIds.value = null;
        return;
    }
    searchTimeout = setTimeout(async () => {
        try {
            const resp = await invoke<{ results: { id: string }[], total_count: number, query_time_ms: number }>('search_notes', {
                vaultPath: props.vaultPath,
                query: q
            });
            // Only apply if query hasn't changed
            if (searchQuery.value === q) {
                backendSearchIds.value = resp.results.map(r => r.id);
            }
        } catch (e) {
            logger.error('Backend search error', e);
        }
    }, 200);
});

// ─── Public API for parent (Nexus cross-navigation) ────────
const openNoteById = async (id: string, _skipNavPush = false) => {
    // Push current note onto nav stack if switching to a different note
    if (!_skipNavPush && currentNoteId.value && currentNoteId.value !== id && !skipNavPush) {
        pushNavigation?.({ app: 'note', itemId: currentNoteId.value });
    }
    // Set synchronously to prevent concurrent scanVault from overwriting it
    currentNoteId.value = id;
    viewMode.value = 'editor';
    
    // Ensure notes array is loaded to properly resolve suffixes
    if (notes.value.length === 0) {
        await scanVault();
    }
    
    let finalId = id;
    const exists = notes.value.find(n => n.id === id) || notes.value.find(n => n.id.endsWith(id));
    if (exists) {
        finalId = exists.id;
        currentNoteId.value = finalId; // Update with resolved path
    }
    
    await loadNoteFile(finalId);
};
defineExpose({ openNoteById, scanVault, notes, tabContents, loadNoteFile, currentNoteId });

// ─── Lifecycle ─────────────────────────────────────────────
const onClickOutside = () => { activeContextMenu.value = null; };
let unlistenFns: (() => void)[] = [];

// --- Handle transclusion "Open source note" navigation ---
const onSynabitNavigate = (e: Event) => {
  const detail = (e as CustomEvent).detail;
  if (detail?.type === 'note' && detail?.id) {
    handleOpenInternalNote({ id: detail.id, type: 'note' });
  }
};

onUnmounted(() => {
    document.body.classList.remove('zen-mode');
    window.removeEventListener('mousemove', onMouseMove);
    window.removeEventListener('mouseup', onMouseUp);
    document.removeEventListener('click', onClickOutside);
    window.removeEventListener('synabit-navigate', onSynabitNavigate as EventListener);
    unlistenFns.forEach(fn => fn());
    unlistenFns = [];
});

onMounted(async () => {
  window.addEventListener('mousemove', onMouseMove);
  window.addEventListener('mouseup', onMouseUp);
  document.addEventListener('click', onClickOutside);
  window.addEventListener('synabit-navigate', onSynabitNavigate as EventListener);

  if (props.isFloatingView && props.floatingNoteId) {
      currentNoteId.value = props.floatingNoteId;
      viewMode.value = 'editor';
      showNoteSidebar.value = false;
      showRightSidebar.value = false;
  }

  if (props.vaultPath) {
     await scanVault();
  }

  listen('note-updated', (event: any) => {
      const data = event.payload as { id: string, content: string };
      if (currentNoteId.value === data.id) return;
      if (tabContents.value[data.id] !== undefined) {
         tabContents.value[data.id] = data.content;
      }
  }).then(fn => unlistenFns.push(fn));

  listen('vault-changed', () => { scanVault(); }).then(fn => unlistenFns.push(fn));
  
  listen('vault-file-modified', () => {
      if (Date.now() < suppressWatcherUntil) return;
      scanVault();
  }).then(fn => unlistenFns.push(fn));
  
  listen('vault-file-created-deleted', () => {
      if (Date.now() < suppressWatcherUntil) return;
      scanVault();
  }).then(fn => unlistenFns.push(fn));
});
</script>

<template>
  <div class="flex flex-1 h-full overflow-hidden"
       :class="{'cursor-col-resize': isDraggingNoteSidebar || isDraggingRightSidebar}">
    <!-- Note Sidebar -->
    <aside 
      v-show="showNoteSidebar && !isFloatingView" 
      class="border-r border-[#e6e6e6] dark:border-[#2c2c2c] bg-[#fbfbfc] dark:bg-[#191919] flex flex-col relative shrink-0 max-md:!w-full max-md:absolute max-md:inset-0 max-md:z-50"
      :style="{ width: wNoteSidebar + 'px' }"
    >
      <div class="hidden md:block absolute top-0 right-0 w-1.5 h-full cursor-col-resize hover:bg-black/10 dark:hover:bg-white/10 z-10 opacity-0 hover:opacity-100 transition-opacity" @mousedown.stop="startDragNoteSidebar"></div>

      <div class="h-10 flex-shrink-0 flex items-center justify-between px-4 border-b border-[#e6e6e6] dark:border-[#2c2c2c]" data-tauri-drag-region>
         <!-- Close button for mobile -->
         <button @click="showNoteSidebar = false" class="md:hidden p-1.5 -ml-1.5 rounded-md hover:bg-gray-200 dark:hover:bg-[#333] text-[#8b8b8b] transition-colors" title="Close Sidebar">
            <X class="w-4 h-4" />
         </button>

         <div class="flex gap-1 ml-auto" @mousedown.stop>
           <button v-if="enableDailyNotes" @click="handleOpenDailyNote" class="px-2 py-1.5 flex items-center gap-1.5 rounded-md hover:bg-[#e6e6e6] dark:hover:bg-[#333] text-[#52525b] dark:text-[#a1a1aa] hover:text-[#1c1c1e] dark:hover:text-white transition-colors" title="Today's Daily Note">
             <Sun class="w-3.5 h-3.5" />
             <span class="text-xs font-medium">Today</span>
           </button>
           <button @click="handleCreateNewNote" class="px-2 py-1.5 flex items-center gap-1.5 rounded-md bg-[#e6e6e6] text-[#1c1c1e] dark:bg-[#333] dark:text-white hover:opacity-80 transition-opacity" title="New Note">
             <Plus class="w-3.5 h-3.5" />
             <span class="text-xs font-medium">New</span>
           </button>
         </div>
      </div>
      
      <div class="px-3 pt-3 pb-2 sticky top-0 bg-[#fbfbfc] dark:bg-[#191919] z-10" @mousedown.stop>
          <div class="relative w-full">
            <Search class="absolute left-2.5 top-1/2 -translate-y-1/2 w-4 h-4 text-[#8b8b8b] dark:text-[#71717a]" />
            <input v-model="searchQuery" type="text" placeholder="Search notes..." class="w-full pl-8 pr-14 py-1.5 bg-white dark:bg-[#2c2c2c] border border-[#e6e6e6] dark:border-transparent mx-auto block rounded-md text-sm focus:outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-shadow text-[#1c1c1e] dark:text-[#f4f4f5] placeholder:text-gray-400 dark:placeholder:text-gray-500">
            <button v-if="searchQuery" @click="searchQuery = ''" class="absolute right-7 top-1/2 -translate-y-1/2 p-0.5 rounded-full hover:bg-gray-100 dark:hover:bg-[#3f3f46] text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors">
              <X class="w-3.5 h-3.5" />
            </button>
            <button @click="isCaseSensitiveSearch = !isCaseSensitiveSearch" :class="['absolute right-2 top-1/2 -translate-y-1/2 p-0.5 rounded-sm transition-colors', isCaseSensitiveSearch ? 'bg-purple-100 text-purple-600 dark:bg-purple-500/20 dark:text-purple-400' : 'text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 hover:bg-gray-100 dark:hover:bg-[#3f3f46]']" title="Match Case">
              <CaseSensitive class="w-3.5 h-3.5" />
            </button>
          </div>
      </div>

      <div class="flex-1 overflow-y-auto" @mousedown.stop>
         <!-- Pinned Section -->
         <div class="mb-4" v-if="allPinnedNotes.length > 0">
             <div class="flex justify-between items-center px-4 mb-2 mt-3">
                 <span class="text-[11px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider">Pinned Notes</span>
                 <button @click="openNoteManager('pinned')" class="text-[10px] text-purple-500 hover:text-purple-600 font-medium p-2 -m-2">Show all</button>
             </div>
             <div class="px-2 space-y-0.5">
                 <div v-for="note in topPinnedNotes" :key="note.id"
                    @click="handleNoteSelect(note.id)"
                    class="px-3 py-2 border border-transparent rounded-lg cursor-pointer transition-colors relative group"
                    :class="currentNoteId === note.id ? 'bg-white dark:bg-[#2a2a2a] shadow-sm border-[#e6e6e6] dark:border-[#3a3a3a]' : 'hover:bg-white/50 dark:hover:bg-[#252525] hover:border-[#e6e6e6] dark:hover:border-[#2f2f2f]'">
                    <div class="absolute right-2 top-2 md:opacity-0 opacity-100 group-hover:opacity-100 transition-opacity z-10" :class="{'md:opacity-100': activeContextMenu === note.id}">
                       <button @click.stop="(e) => toggleContext(note.id, e)" class="p-1 rounded bg-white dark:bg-[#2a2a2a] shadow-sm hover:bg-gray-100 border border-gray-200 dark:border-gray-600">
                          <MoreVertical class="w-3.5 h-3.5 text-gray-500"/>
                       </button>
                       <div v-if="activeContextMenu === note.id" class="absolute right-0 top-6 w-32 bg-white dark:bg-[#2c2c2c] shadow-lg rounded border border-gray-200 dark:border-gray-700 z-50 py-1 overflow-hidden">
                          <button @click.stop="togglePin(note.id)" class="w-full text-left px-3 py-2 text-xs hover:bg-gray-100 dark:hover:bg-gray-600 flex items-center gap-2"><Pin class="w-3 h-3" /> {{ note.pinned ? 'Unpin' : 'Pin' }}</button>
                          <button @click.stop="openInNewWindow(note.id)" class="w-full text-left px-3 py-2 text-xs hover:bg-gray-100 dark:hover:bg-gray-600 flex items-center gap-2"><ExternalLink class="w-3 h-3" /> Open in New Window</button>
                          <button @click.stop="handleRenamePrompt(note.id)" class="w-full text-left px-3 py-2 text-xs hover:bg-gray-100 dark:hover:bg-gray-600 flex items-center gap-2"><Edit2 class="w-3 h-3" /> Rename</button>
                          <button @click.stop="deleteNote(note.id)" class="w-full text-left px-3 py-2 text-xs hover:bg-red-50 dark:hover:bg-red-900/30 text-red-600 dark:text-red-400 flex items-center gap-2"><Trash2 class="w-3 h-3" /> Delete</button>
                       </div>
                    </div>
                    <div class="flex items-center gap-2 mb-1.5 pr-6">
                        <Pin class="w-3 h-3 text-orange-500 shrink-0 fill-orange-500/20" />
                        <span class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] truncate">{{ note.title || 'Untitled Note' }}</span>
                    </div>
                    <div class="flex flex-wrap gap-1" v-if="note.tags.length">
                        <span v-for="tag in note.tags" :key="tag" class="text-[9px] px-1.5 py-0.5 rounded bg-gray-200/60 dark:bg-[#333] text-gray-600 dark:text-gray-300">{{ tag.split('/').pop() }}</span>
                    </div>
                 </div>
                 
                 <button v-if="allPinnedNotes.length > 5" @click="openNoteManager('pinned')" class="w-full text-center py-2.5 mt-2 text-xs font-medium text-blue-500 hover:text-blue-600 hover:bg-blue-50 dark:hover:bg-blue-900/20 rounded-lg transition-colors">
                     Show {{ allPinnedNotes.length - 5 }} more...
                 </button>
             </div>
         </div>

         <!-- Tags Section -->
         <div class="mb-4">
             <div class="flex justify-between items-center px-4 mb-2 mt-2">
                 <span class="text-[11px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider">Top Tags</span>
                 <button @click="openNoteManager('tags')" class="text-[10px] text-purple-500 hover:text-purple-600 font-medium p-2 -m-2">Show all</button>
             </div>
             <div class="px-2 space-y-0.5" v-if="topTags.length > 0">
                 <div v-for="tag in topTags" :key="tag.name"
                      @click="toggleTagSelection(tag.name)"
                      class="w-full flex items-center justify-between px-3 py-1.5 rounded-lg text-sm transition-colors cursor-pointer group"
                      :class="selectedTags.has(tag.name) ? 'bg-black/5 dark:bg-white/10' : 'hover:bg-gray-100 dark:hover:bg-[#2a2a2a] text-[#52525b] dark:text-[#a1a1aa]'">
                      <div class="flex items-center gap-2 truncate">
                          <Hash class="w-3.5 h-3.5 opacity-70 group-hover:text-black dark:group-hover:text-white transition-colors" />
                          <span class="truncate select-none group-hover:text-black dark:group-hover:text-white transition-colors">{{ tag.name.split('/').pop() }}</span>
                      </div>
                      <span class="text-[10px] opacity-50 bg-black/5 dark:bg-white/10 px-1.5 py-0.5 rounded-full min-w-[20px] text-center">{{ tag.count }}</span>
                 </div>
             </div>
             <div v-else class="text-center p-4 text-xs text-gray-400">No tags found</div>
         </div>

         <!-- Recent Notes -->
         <div class="mb-4">
             <div class="flex justify-between items-center px-4 mb-2 mt-2">
                 <span class="text-[11px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider">Recent Notes</span>
                 <button @click="openNoteManager('notes')" class="text-[10px] text-purple-500 hover:text-purple-600 font-medium p-2 -m-2">Show all</button>
             </div>
             <div class="px-2 space-y-0.5">
                 <div v-for="note in recentNotes" :key="note.id"
                    @click="handleNoteSelect(note.id)"
                    class="px-3 py-2 border border-transparent rounded-lg cursor-pointer transition-colors relative group"
                    :class="currentNoteId === note.id ? 'bg-white dark:bg-[#2a2a2a] shadow-sm border-[#e6e6e6] dark:border-[#3a3a3a]' : 'hover:bg-white/50 dark:hover:bg-[#252525] hover:border-[#e6e6e6] dark:hover:border-[#2f2f2f]'">
                    <div class="absolute right-2 top-2 md:opacity-0 opacity-100 group-hover:opacity-100 transition-opacity z-10" :class="{'md:opacity-100': activeContextMenu === note.id}">
                       <button @click.stop="(e) => toggleContext(note.id, e)" class="p-1 rounded bg-white dark:bg-[#2a2a2a] shadow-sm hover:bg-gray-100 border border-gray-200 dark:border-gray-600">
                          <MoreVertical class="w-3.5 h-3.5 text-gray-500"/>
                       </button>
                       <div v-if="activeContextMenu === note.id" class="absolute right-0 top-6 w-32 bg-white dark:bg-[#2c2c2c] shadow-lg rounded border border-gray-200 dark:border-gray-700 z-50 py-1 overflow-hidden">
                          <button @click.stop="togglePin(note.id)" class="w-full text-left px-3 py-2 text-xs hover:bg-gray-100 dark:hover:bg-gray-600 flex items-center gap-2"><Pin class="w-3 h-3" /> {{ note.pinned ? 'Unpin' : 'Pin' }}</button>
                          <button @click.stop="openInNewWindow(note.id)" class="w-full text-left px-3 py-2 text-xs hover:bg-gray-100 dark:hover:bg-gray-600 flex items-center gap-2"><ExternalLink class="w-3 h-3" /> Open in New Window</button>
                          <button @click.stop="handleRenamePrompt(note.id)" class="w-full text-left px-3 py-2 text-xs hover:bg-gray-100 dark:hover:bg-gray-600 flex items-center gap-2"><Edit2 class="w-3 h-3" /> Rename</button>
                          <button @click.stop="deleteNote(note.id)" class="w-full text-left px-3 py-2 text-xs hover:bg-red-50 dark:hover:bg-red-900/30 text-red-600 dark:text-red-400 flex items-center gap-2"><Trash2 class="w-3 h-3" /> Delete</button>
                       </div>
                    </div>
                    <div class="flex items-center gap-2 mb-1.5 pr-6">
                        <FileText class="w-3.5 h-3.5 text-gray-400 shrink-0 opacity-80" />
                        <span class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] truncate">{{ note.title || 'Untitled Note' }}</span>
                    </div>
                    <div class="flex flex-wrap gap-1" v-if="note.tags.length">
                        <span v-for="tag in note.tags" :key="tag" class="text-[9px] px-1.5 py-0.5 rounded bg-gray-200/60 dark:bg-[#333] text-gray-600 dark:text-gray-300">{{ tag.split('/').pop() }}</span>
                    </div>
                 </div>
             </div>
             <div v-if="recentNotes.length === 0" class="p-8 text-center text-sm text-[#52525b] dark:text-[#a1a1aa]">
               No notes match.
             </div>
         </div>
      </div>
    </aside>

    <!-- Main Area: Editor / Manager -->
    <main class="flex-1 flex flex-col bg-[#fdfdfc] dark:bg-[#242424] min-w-[300px] max-md:min-w-0" @mousedown.stop>
      <template v-if="viewMode === 'editor'">
          <div v-if="!isFloatingView" class="h-10 flex-shrink-0 w-full flex items-center justify-between px-4" data-tauri-drag-region>
            <div class="flex gap-2">
              <NavButtons />
              <button @click="showNoteSidebar = !showNoteSidebar" class="p-1 rounded-md hover:bg-gray-200 dark:hover:bg-gray-800 text-gray-500 transition-colors" title="Toggle Sidebar">
                <PanelLeftClose v-if="showNoteSidebar" class="w-4 h-4" />
                <PanelLeft v-else class="w-4 h-4" />
              </button>
            </div>
            <div class="flex gap-2">
              <button v-if="currentNoteId && viewMode === 'editor'" @click="zenMode = !zenMode" class="p-1 rounded-md hover:bg-gray-200 dark:hover:bg-gray-800 text-gray-500 transition-colors hidden md:flex items-center justify-center w-8 h-7" :title="zenMode ? 'Exit Zen Mode' : 'Zen Mode'">
                <Monitor class="w-4 h-4" />
              </button>
              <button v-if="currentNoteId && viewMode === 'editor'" @click="editorFullWidth = !editorFullWidth" class="p-1 rounded-md hover:bg-gray-200 dark:hover:bg-gray-800 text-gray-500 transition-colors hidden md:flex items-center justify-center w-8 h-7" :title="editorFullWidth ? 'Standard Width' : 'Full Width'">
                <!-- Shrink Icon -->
                <div v-if="editorFullWidth" class="flex items-center space-x-[1px]">
                  <ArrowRight class="w-3 h-3" />
                  <ArrowLeft class="w-3 h-3" />
                </div>
                <!-- Expand Icon -->
                <div v-else class="flex items-center space-x-[1px]">
                  <ArrowLeft class="w-3 h-3" />
                  <ArrowRight class="w-3 h-3" />
                </div>
              </button>
              <button v-if="currentNoteId && viewMode === 'editor'" @click="exportModalVisible = true" class="p-1 rounded-md hover:bg-gray-200 dark:hover:bg-gray-800 text-gray-500 transition-colors hidden md:flex items-center justify-center w-8 h-7" title="Export Note">
                <Download class="w-4 h-4" />
              </button>
              <div class="relative flex items-center h-full">

              </div>
              <button v-if="currentNoteId && viewMode === 'editor'" @click="showRightSidebar = !showRightSidebar" class="p-1 relative ml-2 rounded-md hover:bg-gray-200 dark:hover:bg-gray-800 text-gray-500 transition-colors" title="Toggle Right Sidebar">
                <PanelRightClose v-if="showRightSidebar" class="w-4 h-4" />
                <PanelRight v-else class="w-4 h-4" />
              </button>
            </div>
          </div>
          
          <div v-if="zenMode" class="absolute top-4 right-4 z-50">
             <button @click="zenMode = false" class="p-2 bg-black/10 dark:bg-white/10 hover:bg-black/20 dark:hover:bg-white/20 rounded-full text-gray-500 hover:text-black dark:hover:text-white transition-all shadow-sm backdrop-blur-md opacity-0 hover:opacity-100 group-hover:opacity-100" title="Exit Zen Mode">
                <Monitor class="w-4 h-4" />
             </button>
          </div>

          <div v-else-if="!isFloatingView && viewMode !== 'editor'" class="h-8 flex-shrink-0 w-full z-50 bg-[#fdfdfc] dark:bg-[#242424]" data-tauri-drag-region></div>

          <template v-if="activeTabs.length > 0">
            <template v-for="tabId in activeTabs" :key="tabId">
              <div v-show="currentNoteId === tabId" class="flex-1 overflow-y-auto w-full relative">
                <div v-if="tabContents[tabId] === undefined" class="absolute inset-0 flex items-center justify-center bg-[#fdfdfc] dark:bg-[#242424]">
                    <div class="w-8 h-8 rounded-full border-2 border-gray-200 border-t-gray-400 animate-spin"></div>
                </div>
                <div v-else class="px-4 md:px-12 pb-12 mx-auto w-full cursor-text transition-all duration-300" :class="editorFullWidth ? 'max-w-none' : 'max-w-4xl'">
                <div class="mb-4 pt-4">
                   <div class="flex gap-2 mb-4 flex-wrap items-center">
                      <span v-for="tag in notes.find(n => n.id === tabId)?.tags" :key="tag" class="text-xs px-2 py-1 rounded-md bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-300 flex items-center gap-1 group/tag">
                          <Hash class="w-3 h-3 opacity-50"/>
                          {{ tag }}
                          <button @click="removeTag(tag)" class="opacity-0 group-hover/tag:opacity-100 hover:text-red-500 transition-opacity ml-1 p-0.5"><X class="w-3 h-3"/></button>
                       </span>
                       <div class="relative flex items-center">
                          <Plus class="w-3 h-3 absolute left-1.5 text-gray-400" />
                          <input v-model="newTagInput" @keydown="addTag" placeholder="Add tag..." class="text-xs bg-transparent border border-dashed border-gray-300 dark:border-gray-600 rounded-md py-1 pl-5 pr-2 w-24 focus:w-32 focus:outline-none focus:border-gray-400 transition-all text-[#1c1c1e] dark:text-[#f4f4f5]" />
                       </div>
                   </div>
                   <div class="w-full grid grow-wrap" :data-replicated-value="(focusedTitles[tabId] !== undefined ? focusedTitles[tabId] : notes.find(n => n.id === tabId)?.title) || ''">
                     <textarea class="note-title-input w-full text-4xl font-bold bg-transparent border-none outline-none text-[#1c1c1e] dark:text-[#f4f4f5] placeholder:text-gray-300 dark:placeholder:text-gray-700 resize-none overflow-hidden col-start-1 row-start-1 h-full" 
                       rows="1"
                       :value="focusedTitles[tabId] !== undefined ? focusedTitles[tabId] : notes.find(n => n.id === tabId)?.title" 
                       @focus="focusedTitles[tabId] = ($event.target as HTMLTextAreaElement).value"
                       @input="focusedTitles[tabId] = ($event.target as HTMLTextAreaElement).value"
                       @blur="renameTopTitle" 
                       @keydown.enter.prevent="renameTopTitle" 
                       placeholder="Note Title"></textarea>
                   </div>
                </div>
                <div class="mt-4 pb-20 w-full text-text dark:text-text-dark" :class="{'zen-editor-container': zenMode && !editorFullWidth}">
                   <TiptapEditor ref="editorRefs" :model-value="tabContents[tabId]" :vault-path="vaultPath" :notes="notes" :zen-mode="zenMode" :current-note-id="tabId" @update:model-value="(val: string) => onEditorUpdate(val, tabId)" @open-internal-note="handleOpenInternalNote" />
                </div>
                </div>
              </div>
            </template>
          </template>
          <div v-else class="flex-1 flex items-center justify-center text-[#52525b] dark:text-[#a1a1aa]">
            <div class="text-center">
              <FileText class="w-12 h-12 mx-auto mb-4 opacity-20" />
              <p>Select a note to start editing</p>
            </div>
          </div>
      </template>
      <template v-else-if="viewMode === 'manager'">
          <div class="flex-1 flex flex-col bg-[#fdfdfc] dark:bg-[#242424] h-full relative z-0 overflow-y-auto">
             <div class="flex items-center justify-between px-6 h-10 border-b border-[#e6e6e6] dark:border-[#2c2c2c] shrink-0 sticky top-0 bg-[#fdfdfc] dark:bg-[#242424] z-10" data-tauri-drag-region>
                <div class="flex items-center gap-3">
                   <button @click="viewMode = 'editor'" class="p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] transition-colors text-gray-500">
                      <ArrowLeft class="w-5 h-5" />
                   </button>
                   <h1 class="text-xl font-bold text-[#1c1c1e] dark:text-[#f4f4f5] flex items-center gap-2">
                      {{ managerFilter === 'tags' && !managerSearchQuery ? 'All Tags' : managerSearchQuery ? 'Search Results' : managerFilter === 'notes' || !managerFilter ? 'All Notes' : managerFilter === 'pinned' ? 'Pinned Notes' : 'Tag: ' + managerFilter.split('/').pop() }}
                      <span class="text-[12px] font-medium px-2 py-0.5 mt-0.5 rounded-full bg-gray-100 dark:bg-[#333] text-gray-500">
                        {{ managerFilter === 'tags' && !managerSearchQuery ? allTags.length : managerFilteredNotes.length }}
                      </span>
                   </h1>
                </div>
             </div>
             
             <div class="flex-1 flex flex-col p-8 md:p-12 lg:p-16 w-full max-w-6xl mx-auto">
                 <div class="relative w-full mb-8">
                   <Search class="absolute left-4 top-1/2 -translate-y-1/2 w-5 h-5 text-[#8b8b8b] dark:text-[#71717a]" />
                   <input v-model="managerSearchQuery" type="text" placeholder="Search notes or tags..." class="w-full pl-12 pr-20 py-3 bg-white dark:bg-[#1a1a1a] border border-[#e6e6e6] dark:border-[#2c2c2c] rounded-xl text-base shadow-sm focus:outline-none focus:ring-2 focus:ring-purple-500/50 transition-shadow placeholder:text-gray-400 manager-search-input">
                   <button v-if="managerSearchQuery" @click="managerSearchQuery = ''" class="absolute right-12 top-1/2 -translate-y-1/2 p-1.5 rounded-full hover:bg-gray-100 dark:hover:bg-[#2c2c2c] text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors">
                     <X class="w-4 h-4" />
                   </button>
                   <button @click="isCaseSensitiveSearch = !isCaseSensitiveSearch" :class="['absolute right-3 top-1/2 -translate-y-1/2 p-1.5 rounded-md transition-colors', isCaseSensitiveSearch ? 'bg-purple-100 text-purple-600 dark:bg-purple-500/20 dark:text-purple-400' : 'text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 hover:bg-gray-100 dark:hover:bg-[#2c2c2c]']" title="Match Case">
                     <CaseSensitive class="w-4 h-4" />
                   </button>
                 </div>
                 
                 <!-- Tags View -->
                 <div v-if="managerFilter === 'tags' && !managerSearchQuery" class="w-full">
                    <div class="flex flex-wrap gap-3">
                       <div v-for="tag in allTags" :key="tag.name" @click="managerFilter = tag.name" class="px-4 py-2 bg-white dark:bg-[#1f1f1f] border border-[#e6e6e6] dark:border-[#2c2c2c] rounded-lg cursor-pointer hover:border-[#d4d4d8] dark:hover:border-[#444] transition-all flex items-center gap-2 group">
                          <Hash class="w-4 h-4 text-gray-400 group-hover:text-[#1c1c1e] dark:group-hover:text-white transition-colors" />
                          <span class="font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">{{ tag.name.split('/').pop() }}</span>
                          <span class="text-xs bg-gray-100 dark:bg-[#2c2c2c] px-2 py-0.5 rounded text-gray-500">{{ tag.count }}</span>
                       </div>
                    </div>
                 </div>
                 
                 <!-- Notes Table View -->
                 <div v-else class="w-full">
                   <div class="bg-white dark:bg-[#252525] border border-[#e6e6e6] dark:border-[#333] rounded-xl overflow-hidden shadow-sm">
                      <table class="w-full text-left border-collapse">
                         <thead>
                            <tr class="bg-gray-50 dark:bg-[#1a1a1a] border-b border-[#e6e6e6] dark:border-[#333]">
                               <th class="py-2.5 px-4 text-xs font-semibold text-gray-500 uppercase w-8"></th>
                               <th class="py-2.5 px-4 text-xs font-semibold text-gray-500 uppercase w-5/12">Title</th>
                               <th class="py-2.5 px-4 text-xs font-semibold text-gray-500 uppercase">Tags</th>
                               <th class="py-2.5 px-4 text-xs font-semibold text-gray-500 uppercase whitespace-nowrap text-right">Modified</th>
                               <th class="py-2.5 px-4 text-xs font-semibold text-gray-500 uppercase w-12 text-center">Action</th>
                            </tr>
                         </thead>
                         <tbody class="divide-y divide-[#e6e6e6] dark:divide-[#333] text-sm">
                            <tr v-for="note in managerPaginatedNotes" :key="note.id" @click="handleNoteSelect(note.id)" class="hover:bg-gray-50 dark:hover:bg-[#2a2a2a] cursor-pointer transition-colors group">
                               <td class="py-3 px-4 w-8">
                                  <Pin v-if="note.pinned" class="w-3.5 h-3.5 text-orange-500 fill-orange-500/20" />
                                  <FileText v-else class="w-3.5 h-3.5 text-gray-400 opacity-50" />
                               </td>
                               <td class="py-3 px-4 font-medium text-[#1c1c1e] dark:text-[#f4f4f5] max-w-[250px] truncate">{{ note.title || 'Untitled Note' }}</td>
                               <td class="py-3 px-4">
                                  <div class="flex flex-wrap gap-1" v-if="note.tags.length">
                                     <span v-for="tag in note.tags.slice(0, 3)" :key="tag" class="text-[10px] px-1.5 py-0.5 rounded bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-300">{{ tag.split('/').pop() }}</span>
                                     <span v-if="note.tags.length > 3" class="text-[10px] px-1.5 py-0.5 rounded bg-gray-100 dark:bg-gray-800 text-gray-500">+{{ note.tags.length - 3 }}</span>
                                  </div>
                                  <span v-else class="text-xs text-gray-400 italic">No tags</span>
                               </td>
                               <td class="py-3 px-4 text-xs text-gray-500 dark:text-gray-400 whitespace-nowrap text-right">{{ formatDate(note.date) }}</td>
                               <td class="py-3 px-4 w-12 text-center" @click.stop>
                                  <div class="relative flex justify-center">
                                     <button @click="(e) => toggleContext('manager_'+note.id, e)" class="p-1 rounded md:opacity-0 opacity-100 group-hover:opacity-100 hover:bg-gray-200 dark:hover:bg-[#444] transition">
                                        <MoreVertical class="w-4 h-4 text-gray-500" />
                                     </button>
                                     <div v-if="activeContextMenu === 'manager_'+note.id" class="absolute right-6 top-0 w-32 bg-white dark:bg-[#2c2c2c] shadow-lg rounded border border-gray-200 dark:border-gray-700 z-50 py-1 overflow-hidden">
                                        <button @click.stop="togglePin(note.id); activeContextMenu = null;" class="w-full text-left px-3 py-2 text-xs hover:bg-gray-100 dark:hover:bg-gray-600 flex items-center gap-2"><Pin class="w-3 h-3" /> {{ note.pinned ? 'Unpin' : 'Pin' }}</button>
                                        <button @click.stop="deleteNote(note.id); activeContextMenu = null;" class="w-full text-left px-3 py-2 text-xs hover:bg-red-50 dark:hover:bg-red-900/30 text-red-600 dark:text-red-400 flex items-center gap-2"><Trash2 class="w-3 h-3" /> Delete</button>
                                     </div>
                                  </div>
                               </td>
                            </tr>
                            <tr v-if="managerFilteredNotes.length === 0">
                               <td colspan="5" class="py-12 text-center text-gray-500">No notes found matching current filters.</td>
                            </tr>
                         </tbody>
                      </table>
                   </div>
                   
                   <!-- Pagination Controls -->
                   <div v-if="managerTotalPages > 1" class="mt-4 flex items-center justify-between text-[13px] text-gray-500">
                      <div>Showing {{ (managerCurrentPage - 1) * managerItemsPerPage + 1 }} to {{ Math.min(managerCurrentPage * managerItemsPerPage, managerFilteredNotes.length) }} of {{ managerFilteredNotes.length }} notes</div>
                      <div class="flex items-center gap-2">
                         <button @click="managerPrevPage" :disabled="managerCurrentPage === 1" class="px-3 py-1.5 rounded-lg border border-[#e6e6e6] dark:border-[#333] hover:bg-gray-50 dark:hover:bg-[#2c2c2c] disabled:opacity-50 disabled:cursor-not-allowed transition-colors text-[#1c1c1e] dark:text-[#f4f4f5]">Previous</button>
                         <span class="font-medium px-2 text-[#1c1c1e] dark:text-[#f4f4f5]">Page {{ managerCurrentPage }} of {{ managerTotalPages }}</span>
                         <button @click="managerNextPage" :disabled="managerCurrentPage === managerTotalPages" class="px-3 py-1.5 rounded-lg border border-[#e6e6e6] dark:border-[#333] hover:bg-gray-50 dark:hover:bg-[#2c2c2c] disabled:opacity-50 disabled:cursor-not-allowed transition-colors text-[#1c1c1e] dark:text-[#f4f4f5]">Next</button>
                      </div>
                   </div>
                 </div>
             </div>
          </div>
      </template>
    </main>

    <!-- Right Sidebar: Graph & Backlinks -->
    <aside v-if="currentNoteId && !isFloatingView && viewMode === 'editor'" v-show="showRightSidebar" class="shrink-0 relative border-l border-[#e6e6e6] dark:border-[#2c2c2c] bg-[#fbfbfc] dark:bg-[#191919] flex flex-col overflow-hidden max-md:!w-full max-md:absolute max-md:inset-0 max-md:z-[60]" :style="{ width: wRightSidebar + 'px' }">
      <div class="hidden md:block absolute top-0 left-0 w-1.5 h-full cursor-col-resize hover:bg-black/10 dark:hover:bg-white/10 z-10 opacity-0 hover:opacity-100 transition-opacity" @mousedown.stop="startDragRightSidebar"></div>
      <div class="h-10 flex-shrink-0 flex items-center px-4 border-b border-[#e6e6e6] dark:border-[#2c2c2c]" data-tauri-drag-region>
          <Globe class="w-4 h-4 text-gray-500 mr-2" />
          <span class="font-bold text-[11px] tracking-wider text-gray-500 uppercase mt-0.5">Graph View</span>
          <button @click="showRightSidebar = false" class="p-1 ml-auto rounded-md hover:bg-gray-200 dark:hover:bg-gray-800 text-gray-400 transition-colors">
             <X class="w-3.5 h-3.5" />
          </button>
      </div>
      <div class="h-1/2 border-b border-[#e6e6e6] dark:border-[#2c2c2c] overflow-hidden">
          <NoteGraph v-if="activeNote" :current-note-id="currentNoteId || ''" :current-note-title="activeNote.title || 'Untitled Node'" :tags="activeNote.tags || []" :outgoing-links="currentOutgoingLinks" :backlinks="currentBacklinks" :all-notes="notes" @open-note="handleOpenInternalNote" />
      </div>
      <div class="h-10 flex-shrink-0 flex items-center px-4 border-b border-[#e6e6e6] dark:border-[#2c2c2c]">
          <span class="font-bold text-[11px] tracking-wider text-[#8b8b8b] dark:text-[#71717a] uppercase mt-0.5">Linked Mentions ({{ currentBacklinks.length }})</span>
      </div>
      <div class="flex-1 overflow-y-auto p-2 space-y-1">
          <div v-if="currentBacklinks.length === 0" class="text-[13px] text-gray-400 text-center py-4">No linked mentions.</div>
          <div v-for="bl in currentBacklinks" :key="bl.id" @click="handleOpenInternalNote({ id: bl.id, type: bl.node_type })" class="p-3 border border-transparent rounded-lg cursor-pointer hover:bg-white/50 dark:hover:bg-[#252525] hover:border-[#e6e6e6] dark:hover:border-[#2f2f2f] transition-all group">
            <h5 class="flex items-center gap-2 pr-2">
                <Calendar v-if="bl.node_type === 'event'" class="w-3.5 h-3.5 text-rose-500 shrink-0 opacity-80 group-hover:opacity-100 transition-colors"/>
                <CheckSquare v-else-if="bl.node_type === 'task'" class="w-3.5 h-3.5 text-emerald-500 shrink-0 opacity-80 group-hover:opacity-100 transition-colors"/>
                <FileText v-else class="w-3.5 h-3.5 text-gray-400 shrink-0 opacity-80 group-hover:text-purple-500 group-hover:opacity-100 transition-colors"/>
                <span class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] truncate">{{ bl.title }}</span>
                <span v-if="bl.node_type === 'event' && bl.properties && bl.properties.start_at" class="ml-auto text-[9px] text-gray-400 font-medium tracking-wider whitespace-nowrap">{{ (bl.properties.start_at as string).split('T')[0] }}</span>
                <button v-if="bl._is_outgoing_project" @click.stop="unlinkProject(bl.id, bl.title)" class="ml-auto p-1.5 -mr-1.5 text-gray-400 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-900/30 rounded-md opacity-0 group-hover:opacity-100 transition-all" title="Unlink Project">
                   <X class="w-3.5 h-3.5" />
                </button>
            </h5>
          </div>
      </div>
    </aside>

    <!-- Rename Modal (replaces window.prompt for mobile compat) -->
    <Teleport to="body">
      <div v-if="renameModal.show" class="fixed inset-0 z-[999] flex items-center justify-center bg-black/40 backdrop-blur-sm" @click.self="renameModal.show = false">
        <div class="bg-white dark:bg-[#2a2a2a] rounded-2xl shadow-2xl p-6 w-80 border border-[#e6e6e6] dark:border-[#3a3a3a]">
          <h3 class="text-base font-semibold text-[#1c1c1e] dark:text-[#f4f4f5] mb-4">Rename Note</h3>
          <input
            v-model="renameModal.value"
            type="text"
            class="w-full px-3 py-2 rounded-lg border border-[#e0e0e0] dark:border-[#444] bg-white dark:bg-[#1e1e1e] text-[#1c1c1e] dark:text-[#f4f4f5] text-sm focus:outline-none focus:ring-2 focus:ring-black/10 dark:focus:ring-white/20"
            @keydown.enter="confirmRename"
            autofocus
          />
          <div class="flex justify-end gap-2 mt-4">
            <button @click="renameModal.show = false" class="px-4 py-1.5 text-sm rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-[#333] transition-colors">Cancel</button>
            <button @click="confirmRename" class="px-4 py-1.5 text-sm rounded-lg bg-black dark:bg-white text-white dark:text-black font-medium hover:opacity-80 transition-opacity">Rename</button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- Export Modal -->
    <NoteExportModal 
      v-if="exportModalVisible" 
      @close="exportModalVisible = false" 
      @export="handleExportOption" 
    />

  </div>
</template>

<style scoped>
[data-tauri-drag-region] {
  -webkit-app-region: drag;
}

.manager-search-input {
  color: #1c1c1e !important;
}
html.dark .manager-search-input {
  color: #f4f4f5 !important;
}

/* Auto-resizing textarea for note title */
.grow-wrap {
  display: grid;
}
.grow-wrap::after {
  content: attr(data-replicated-value) " ";
  white-space: pre-wrap;
  visibility: hidden;
  grid-area: 1 / 1 / 2 / 2;
  font-size: 2.25rem; /* Matches text-4xl */
  line-height: 2.5rem; /* Matches text-4xl */
  font-weight: 700; /* Matches font-bold */
  word-break: break-word;
}
.grow-wrap > textarea {
  grid-area: 1 / 1 / 2 / 2;
}
</style>
