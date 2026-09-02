<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch, nextTick, inject, defineAsyncComponent, toRef } from 'vue';
import { FileText, Search, PanelLeft, PanelLeftClose, PanelRight, PanelRightClose, Hash, Plus, MoreVertical, Pin, X, ArrowLeft, ArrowRight, Sun, CaseSensitive, Globe, Calendar, CheckSquare, Monitor, Download, History, Copy, Trash2 } from 'lucide-vue-next';
import { invoke } from '@tauri-apps/api/core';
import { useI18n } from 'vue-i18n';
import { useEventBus } from '../../composables/useEventBus';
import { useNodeService } from '../../composables/useNodeService';
import { ask, message } from '@tauri-apps/plugin-dialog';

import TiptapEditor from './TiptapEditor.vue';
import NoteGraph from './NoteGraph.vue';
import NavButtons from '../../shared/components/NavButtons.vue';
import NoteExportModal from './NoteExportModal.vue';
import NoteHistoryModal from './NoteHistoryModal.vue';
import NoteListItem from './components/NoteListItem.vue';
import NoteContextMenu from './components/NoteContextMenu.vue';
import NoteUndoToast from './components/NoteUndoToast.vue';
import NoteDuplicatesModal from './NoteDuplicatesModal.vue';

import { useAppStore } from '../../stores/useAppStore';
import { storeToRefs } from 'pinia';
import { logger } from '../../utils/logger';
import { routeForNode } from '../../shared/nodeRoutes';
import type { NavEntry } from '../../stores/useNavigationStore';
import { useAppLockStore } from '../../stores/useAppLockStore';
import { useLicenseStore } from '../../stores/useLicenseStore';

import type { NoteItem } from './helpers';
import { formatDate, buildNotePayload, rememberRecentNotes, RECENT_NOTES_KEY } from './helpers';
import { resolveNoteId } from './resolveNoteId';

// ── Composables ─────────────────────────────────────────────
import { useSidebarResize } from '../../composables/useSidebarResize';
import { useNoteTabs } from './composables/useNoteTabs';
import { useNoteExport } from './composables/useNoteExport';
import { useNoteLock } from './composables/useNoteLock';
import { useNoteSave } from './composables/useNoteSave';
import { useNoteTags } from './composables/useNoteTags';
import { useNoteSearch } from './composables/useNoteSearch';
import { useNoteManager } from './composables/useNoteManager';
import { useNoteBacklinks } from './composables/useNoteBacklinks';
import { useNoteRename } from './composables/useNoteRename';
import { useNoteDelete } from './composables/useNoteDelete';
import { useNoteSelection } from './composables/useNoteSelection';

const LockScreenComponent = defineAsyncComponent(() => import('../../shared/components/LockScreen.vue'));

// ── Props & Services ────────────────────────────────────────
const emit = defineEmits(['open-node']);
const { t } = useI18n();
const bus = useEventBus();
const ns = useNodeService();

const props = defineProps<{
  vaultPath: string;
  isFloatingView?: boolean;
  floatingNoteId?: string | null;
}>();

const appStore = useAppStore();
const appLockStore = useAppLockStore();
const licenseStore = useLicenseStore();
const { enableDailyNotes, dailyNoteFormat, dailyNoteTag } = storeToRefs(appStore);
const vaultPathRef = toRef(props, 'vaultPath');

// ── Navigation ──────────────────────────────────────────────
const pushNavigation = inject<(entry?: NavEntry) => void>('pushNavigation');
const skipNavPush = false;

// ── Note State ──────────────────────────────────────────────
const notes = ref<NoteItem[]>([]);
const currentNoteId = ref<string | null>(null);

const recentNoteIds = ref<string[]>([]);
try {
    const stored = localStorage.getItem(RECENT_NOTES_KEY);
    if (stored) recentNoteIds.value = JSON.parse(stored);
} catch (e) {}

const updateRecentNote = (id: string) => {
    let arr = recentNoteIds.value.filter(x => x !== id);
    arr.unshift(id);
    if (arr.length > 50) arr = arr.slice(0, 50);
    recentNoteIds.value = arr;
    rememberRecentNotes(arr);
};

watch(currentNoteId, (newId) => {
    if (newId) updateRecentNote(newId);
});

// ── Context Menu ────────────────────────────────────────────
const activeContextMenu = ref<string | null>(null);
const toggleContext = (id: string, e: Event) => {
    e.stopPropagation();
    activeContextMenu.value = activeContextMenu.value === id ? null : id;
};
const closeContextMenu = () => { activeContextMenu.value = null; };

// ── Composable Wiring ───────────────────────────────────────
// The widths Notes has always opened at, now said out loud rather than
// living as the composable's defaults — Things opens at different ones.
const sidebar = useSidebarResize({
  left: { initial: 300, min: 220, max: 600 },
  right: { initial: 288, min: 200, max: 600 },
});

const tabs = useNoteTabs(notes, currentNoteId, ns, appLockStore);

const save = useNoteSave(notes, currentNoteId, tabs.tabContents, tabs.renamedTabs, ns, bus);

const lock = useNoteLock(appLockStore, handleNoteSelect);

const tags = useNoteTags(notes, currentNoteId, tabs.currentContent, ns, scanVault, () => flushActiveEditor());

const search = useNoteSearch(notes, recentNoteIds, tags.selectedTags, vaultPathRef);

const manager = useNoteManager(notes, search.isCaseSensitiveSearch, vaultPathRef);

/**
 * Make the open editor finish turning the document into markdown.
 *
 * The editor defers that by a fifth of a second so typing stays smooth, which
 * means anything reading the note's text right after a keystroke — exporting
 * it, above all — has to ask first or it reads a slightly older note.
 */
const flushActiveEditor = () => {
    const id = currentNoteId.value;
    if (!id) return;
    const refs = save.editorRefs.value || save.editorRefs;
    (refs as Record<string, { flushSerialize?: () => void }>)[id]?.flushSerialize?.();
};

const backlinks = useNoteBacklinks(notes, currentNoteId, tabs.currentContent, ns, scanVault, () => flushActiveEditor());

/**
 * Deleting a note, held back long enough to take it back.
 *
 * The confirmation dialog that used to guard this is gone on purpose. A dialog
 * asks people to be careful beforehand, which mostly teaches them to click
 * through it; an undo lets them be careless and still be fine. Only one of the
 * two has ever saved a note.
 */
const del = useNoteDelete({
    notes, currentNoteId, recentNoteIds,
    tabContents: tabs.tabContents,
    activeTabs: tabs.activeTabs,
    tabAccessTime: tabs.tabAccessTime,
    saveTimeouts: save.saveTimeouts,
    ns, scanVault,
    onFailed: (note) => {
        void message(t('note.delete_failed'), { title: note.title, kind: 'error' });
    },
});

const deleteNote = async (id: string) => {
    activeContextMenu.value = null;

    // Asked for, and then held back anyway. The two are not the same guard: a
    // dialog catches the click somebody did not mean to make, and the undo
    // window catches the one they meant at the time and regretted a second
    // later. Neither has to be traded for the other.
    const confirmed = await ask(t('note.delete_body'), {
        title: t('note.delete_title'),
        kind: 'warning',
        okLabel: t('note.delete_confirm'),
        cancelLabel: t('note.cancel'),
    });
    if (!confirmed) return;

    return del.deleteNote(id);
};

// ─── Selecting several notes in the manager ───────────────
const selection = useNoteSelection();

/** The rows a click can currently reach — one page, under one filter. */
const visibleManagerIds = computed(() => manager.managerPaginatedNotes.value.map((n) => n.id));

/**
 * Anything that changes which rows are on screen drops the selection.
 *
 * A selection the reader cannot see is one they cannot check before pressing
 * delete, and this is the one button in the app where being wrong costs
 * something. Turning a page is cheap; deleting nine notes you had forgotten
 * were ticked two pages back is not.
 */
watch(
    [manager.viewMode, manager.managerFilter, manager.managerSearchQuery, manager.managerCurrentPage],
    () => selection.clear(),
);

/**
 * Open the note, or tick it when a selection is already under way.
 *
 * Once one row is ticked the list is being used to pick things out rather than
 * to read them, and a click that opened a note there would throw the whole
 * selection away to show something nobody asked for.
 */
const handleManagerRowClick = (id: string, event: MouseEvent) => {
    if (!selection.active.value) {
        handleNoteSelect(id);
        return;
    }
    selection.toggle(id, visibleManagerIds.value, event.shiftKey);
};

const deleteSelected = async () => {
    const ids = selection.ids.value;
    if (ids.length === 0) return;

    const confirmed = await ask(
        ids.length > 1 ? t('note.delete_many_body') : t('note.delete_body'),
        {
            title: ids.length > 1 ? t('note.delete_many_title', { count: ids.length }) : t('note.delete_title'),
            kind: 'warning',
            okLabel: t('note.delete_confirm'),
            cancelLabel: t('note.cancel'),
        },
    );
    if (!confirmed) return;

    // Cleared before the delete, not after: the rows are gone from the list
    // either way, and a selection still holding their ids would put the
    // action bar back on screen offering to delete nothing.
    selection.clear();
    await del.deleteNotes(ids);
};

const noteExport = useNoteExport({
    notes, currentNoteId,
    currentContent: tabs.currentContent,
    vaultPath: vaultPathRef,
});

const rename = useNoteRename(
    notes, currentNoteId, ns, tabs.tabContents, tabs.activeTabs,
    tabs.tabAccessTime, tabs.renamedTabs, tabs.focusedTitles, recentNoteIds,
    save.saveTimeouts, save.saveNoteForTab, scanVault, save.editorRefs,
);

// ── Duplicate notes ─────────────────────────────────────────
const duplicatesModalVisible = ref(false);

// ── Version History ─────────────────────────────────────────
/**
 * Which note the history panel is showing, or null when it is closed.
 *
 * A note of its own rather than just `currentNoteId`, because the panel is
 * reachable from each row's context menu as well as from the toolbar — and on
 * a phone the context menu is the *only* way in, since the toolbar button is
 * desktop-only.
 */
const historyNoteId = ref<string | null>(null);

const openHistory = (id: string) => {
    historyNoteId.value = id;
    activeContextMenu.value = null;
};

const historyNote = computed(() => notes.value.find(n => n.id === historyNoteId.value) || null);

/**
 * Take the restored text back into the open tab.
 *
 * The restore has already written the file and brought the database in line,
 * so this is only about the copy the editor is holding. Without it the editor
 * would still show the old text and the next autosave — 600ms after the next
 * keystroke — would write it straight back over the restore.
 */
const onVersionRestored = (content: string) => {
    const id = historyNoteId.value;
    // Only the tab actually holding this note needs telling. Restoring from a
    // context menu can target a note that is not open, and the file plus the
    // database are already in line by the time this runs.
    if (!id || tabs.tabContents.value[id] === undefined) { scanVault(); return; }
    tabs.tabContents.value[id] = content;
    bus.emit('note:updated-external', { id, content });
    scanVault();
};

// ── Zen Mode ────────────────────────────────────────────────
const zenMode = ref(false);
watch(zenMode, (val) => {
    if (val) {
        document.body.classList.add('zen-mode');
        sidebar.showLeft.value = false;
        sidebar.showRight.value = false;
    } else {
        document.body.classList.remove('zen-mode');
        sidebar.showLeft.value = true;
    }
});

// ── Daily Note ──────────────────────────────────────────────
const isValidDailyFormat = computed(() => {
    const fmt = dailyNoteFormat.value;
    return fmt && (fmt.includes('YYYY') || fmt.includes('YY')) && (fmt.includes('MM') || fmt.includes('M')) && (fmt.includes('DD') || fmt.includes('D'));
});

let isCreatingNote = false;

async function openDailyNote() {
    if (!props.vaultPath) return;
    try {
        const finalFormat = isValidDailyFormat.value ? dailyNoteFormat.value : 'YYYY-MM-DD';
        const tag = dailyNoteTag.value.trim();
        const dailyPath = await invoke<string>('open_daily_note', { vaultPath: props.vaultPath, formatStr: finalFormat, tag });
        await scanVault();
        if (dailyPath) { currentNoteId.value = dailyPath; manager.viewMode.value = 'editor'; }
    } catch(e) { logger.error("Failed to open daily note:", e); }
}

const handleOpenDailyNote = async () => {
    await openDailyNote();
    if (window.innerWidth < 768) sidebar.showLeft.value = false;
};

const handleCreateNewNote = async () => {
    await createNewNote();
    if (window.innerWidth < 768) sidebar.showLeft.value = false;
};

// ── Note CRUD ───────────────────────────────────────────────
function handleNoteSelect(id: string) {
    if (appLockStore.isEnabled && appLockStore.isNoteProtected(id) && !appLockStore.isNoteAccessible(id)) {
        lock.pendingNoteId.value = id;
        lock.pendingNoteAction.value = 'view';
        lock.noteLockTitle.value = 'Enter PIN to view this note';
        lock.showNoteLockScreen.value = true;
        return;
    }
    if (id !== currentNoteId.value && currentNoteId.value && !skipNavPush) {
        pushNavigation?.({ app: 'note', itemId: currentNoteId.value });
    }
    currentNoteId.value = id;
    manager.viewMode.value = 'editor';
    if (window.innerWidth < 768) {
        sidebar.showLeft.value = false;
    }
}

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
            flushActiveEditor();
            await ns.writeNode(buildNotePayload(note, tabs.currentContent.value));
        }
    }
});

const togglePin = async (id: string) => {
    const note = notes.value.find(n => n.id === id);
    if (!note) return;
    note.pinned = !note.pinned;
    try {
        // Pinning rewrites the note file, so it needs the body — but only at
        // the moment it is clicked, and only for this one note. Fetching it
        // here rather than holding every note's body in the list is both
        // cheaper and fresher than the copy the last scan left behind.
        if (id === currentNoteId.value) flushActiveEditor();
        let body = tabs.tabContents.value[id];
        if (body === undefined) {
            const full = await ns.getNode(id);
            if (!full) { logger.error('Pin fail: note not found', id); return; }
            body = full.content;
        }
        await ns.writeNode(buildNotePayload(note, body));
        scanVault();
    } catch(e) { logger.error('Pin fail:', e); }
};

const openInNewWindow = async (id: string) => {
    try { await invoke('spawn_node_window', { nodeId: id }); } catch(e) { logger.error("Failed to open node in new window", e); }
    activeContextMenu.value = null;
};

async function createNewNote() {
    if (!props.vaultPath || isCreatingNote) return;
    isCreatingNote = true;
    save.setSuppressWatcherUntil(Date.now() + 3000);
    try {
        const newPath = await ns.createNode({ directory: 'Notes', nodeType: 'note' });
        await scanVault();
        if (newPath) {
            currentNoteId.value = newPath;
            manager.viewMode.value = 'editor';
            await nextTick();
            const titleInput = document.querySelector('.note-title-input') as HTMLInputElement;
            if (titleInput) { titleInput.focus(); titleInput.select(); }
        }
    } catch(e) { logger.error("Failed to create note:", e); }
    finally { isCreatingNote = false; }
}

// ── Scan Vault ──────────────────────────────────────────────
async function scanVault() {
    if (!props.vaultPath) return;
    try {
        // Summaries, not whole notes. The list renders a title, a date, some
        // tags and a one-line summary; asking for the bodies as well was the
        // bulk of what this call transferred and none of what it showed.
        const scannedNodes = await ns.getNodeSummaries('note');
        const scannedNotes = scannedNodes.map((n: any) => {
            let noteTags: string[] = [];
            if (Array.isArray(n.properties?.tags)) noteTags = n.properties.tags as string[];
            return {
                id: n.id, title: n.title,
                date: n.updated_at || n.created_at, path: n.id, tags: noteTags,
                node_id: typeof n.properties?.node_id === 'string' ? n.properties.node_id : undefined,
                created_at: n.created_at,
                pinned: !!n.properties?.pinned, full_width: !!n.properties?.full_width,
                linked_projects: Array.isArray(n.properties?.linked_projects) ? n.properties.linked_projects : [],
                summary: (n.preview || '').trim()
            };
        });
        // A note waiting out its undo window is still on disk, and the file
        // watcher triggers plenty of rescans. Without this it would reappear in
        // the sidebar underneath the toast offering to undo its deletion.
        const visible = scannedNotes.filter(n => !del.isHidden(n.id));
        notes.value = visible;
        tags.buildTagTree(visible);
        if (visible.length > 0 && !currentNoteId.value) {
            currentNoteId.value = visible[0].id;
        } else if (visible.length === 0) {
            currentNoteId.value = null;
        }
    } catch(e) { logger.error("Failed to scan vault:", e); }
}

// ── Active note ─────────────────────────────────────────────
const activeNote = computed(() => notes.value.find(n => n.id === currentNoteId.value) || null);

// ── Watch currentNoteId → load file ─────────────────────────
watch(currentNoteId, async (newId) => {
    if (newId) await tabs.loadNoteFile(newId);
});

// ── Navigation ──────────────────────────────────────────────
const handleOpenInternalNote = (data: any) => {
    const noteId = typeof data === 'string' ? data : data.id;
    const type = typeof data === 'string' ? 'note' : data.type;
    if (type === 'note' || type === 'node') {
        const resolved = resolveNoteId(notes.value, noteId);
        if (resolved) {
            if (resolved.id !== currentNoteId.value && currentNoteId.value && !skipNavPush) {
                pushNavigation?.({ app: 'note', itemId: currentNoteId.value });
            }
            currentNoteId.value = resolved.id;
        }
    } else {
        emit('open-node', noteId, type);
    }
};

// ── Public API ──────────────────────────────────────────────
const openNoteById = async (id: string, _skipNavPush = false) => {
    if (notes.value.length === 0) { await scanVault(); }

    const exists = resolveNoteId(notes.value, id);

    // Something that is not a note goes to the app that owns it, and this app
    // is left exactly as it was. Checked before `currentNoteId` and the editor
    // view are set, so a mis-routed task does not flash up as an open note on
    // the way past — and never reaches the editor that would save it as one.
    if (!exists) {
        const node = await ns.getNode(id).catch(() => null);
        const route = node && node.node_type !== 'note' ? routeForNode(node.node_type, id) : null;
        if (route) {
            emit('open-node', id, route);
            return;
        }
    }

    if (!_skipNavPush && currentNoteId.value && currentNoteId.value !== id && !skipNavPush) {
        pushNavigation?.({ app: 'note', itemId: currentNoteId.value });
    }
    const finalId = exists ? exists.id : id;
    currentNoteId.value = finalId;
    manager.viewMode.value = 'editor';
    await tabs.loadNoteFile(finalId);
};
defineExpose({ openNoteById, scanVault, notes, tabContents: tabs.tabContents, loadNoteFile: tabs.loadNoteFile, currentNoteId, deleteNote });

// ── Lifecycle ───────────────────────────────────────────────
const onClickOutside = () => { activeContextMenu.value = null; };

const onSynabitNavigate = (e: Event) => {
    const detail = (e as CustomEvent).detail;
    if (!detail?.id) return;
    // Any node type, not just notes: a query table lists tasks and events too,
    // and `handleOpenInternalNote` already knows to hand those to whichever
    // mini-app owns them.
    handleOpenInternalNote({ id: detail.id, type: detail.type || 'note' });
};

onUnmounted(() => {
    document.body.classList.remove('zen-mode');
    window.removeEventListener('mousemove', sidebar.onMouseMove);
    window.removeEventListener('mouseup', sidebar.onMouseUp);
    document.removeEventListener('click', onClickOutside);
    window.removeEventListener('synabit-navigate', onSynabitNavigate as EventListener);
});

onMounted(async () => {
    window.addEventListener('mousemove', sidebar.onMouseMove);
    window.addEventListener('mouseup', sidebar.onMouseUp);
    document.addEventListener('click', onClickOutside);
    window.addEventListener('synabit-navigate', onSynabitNavigate as EventListener);

    if (props.isFloatingView && props.floatingNoteId) {
        currentNoteId.value = props.floatingNoteId;
        manager.viewMode.value = 'editor';
        sidebar.showLeft.value = false;
        sidebar.showRight.value = false;
    }

    if (props.vaultPath) { await scanVault(); }

    bus.on('note:updated-external', (data) => {
        if (currentNoteId.value === data.id) return;
        if (tabs.tabContents.value[data.id] !== undefined) {
            tabs.tabContents.value[data.id] = data.content;
        }
    });

    bus.on('vault:changed', () => { scanVault(); });

    bus.on('vault:file-modified', () => {
        if (Date.now() < save.getSuppressWatcherUntil()) return;
        scanVault();
    });

    bus.on('vault:file-created-deleted', () => {
        if (Date.now() < save.getSuppressWatcherUntil()) return;
        scanVault();
    });

    bus.on('vault:sync-completed', (payload: any) => {
        const pulled_files = payload?.pulled_files as string[] | undefined;
        if (pulled_files && pulled_files.length > 0) {
            pulled_files.forEach((p: string) => {
                delete tabs.tabContents.value[p];
            });
        }
        scanVault().then(async () => {
            if (currentNoteId.value && pulled_files && pulled_files.includes(currentNoteId.value)) {
                await tabs.loadNoteFile(currentNoteId.value);
            }
        });
    });

    bus.on('node:created', ({ nodeType }) => {
        if (nodeType === 'note') scanVault();
    });

    bus.on('node:deleted', ({ nodeType }) => {
        if (nodeType === 'note') scanVault();
    });
});
</script>

<template>
  <div class="flex flex-1 h-full overflow-hidden"
       :class="{'cursor-col-resize': sidebar.isDraggingLeft.value || sidebar.isDraggingRight.value}">
    <!-- Note Sidebar -->
    <aside
      v-show="sidebar.showLeft.value && !isFloatingView"
      class="border-r border-[#e6e6e6] dark:border-[#2c2c2c] bg-[#fbfbfc] dark:bg-[#191919] flex flex-col relative shrink-0 max-md:!w-full max-md:absolute max-md:inset-0 max-md:z-50"
      :style="{ width: sidebar.leftWidth.value + 'px' }"
    >
      <div class="hidden md:block absolute top-0 right-0 w-1.5 h-full cursor-col-resize hover:bg-black/10 dark:hover:bg-white/10 z-10 opacity-0 hover:opacity-100 transition-opacity" @mousedown.stop="sidebar.startDragLeft($event)"></div>

      <div class="h-10 flex-shrink-0 flex items-center justify-between px-4 border-b border-[#e6e6e6] dark:border-[#2c2c2c]" data-tauri-drag-region>
         <button @click="sidebar.showLeft.value = false" class="md:hidden p-1.5 -ml-1.5 rounded-md hover:bg-gray-200 dark:hover:bg-[#333] text-[#8b8b8b] transition-colors" :title="$t('note.close_sidebar')">
            <X class="w-4 h-4" />
         </button>
         <div class="flex gap-1 ml-auto" @mousedown.stop>
           <button v-if="enableDailyNotes" @click="handleOpenDailyNote" class="px-2 py-1.5 flex items-center gap-1.5 rounded-md hover:bg-[#e6e6e6] dark:hover:bg-[#333] text-[#52525b] dark:text-[#a1a1aa] hover:text-[#1c1c1e] dark:hover:text-white transition-colors" :title="$t('note.todays_daily_note')">
             <Sun class="w-3.5 h-3.5" />
             <span class="text-xs font-medium">{{ $t('note.today') }}</span>
           </button>
           <button @click="handleCreateNewNote" class="px-2 py-1.5 flex items-center gap-1.5 rounded-md bg-[#e6e6e6] text-[#1c1c1e] dark:bg-[#333] dark:text-white hover:opacity-80 transition-opacity" :title="$t('note.new_note')">
             <Plus class="w-3.5 h-3.5" />
             <span class="text-xs font-medium">{{ $t('note.new_note') }}</span>
           </button>
         </div>
      </div>

      <div class="px-3 pt-3 pb-2 sticky top-0 bg-[#fbfbfc] dark:bg-[#191919] z-10" @mousedown.stop>
          <div class="relative w-full">
            <Search class="absolute left-2.5 top-1/2 -translate-y-1/2 w-4 h-4 text-[#8b8b8b] dark:text-[#71717a]" />
            <input v-model="search.searchQuery.value" type="text" :placeholder="$t('note.search_placeholder')" class="w-full pl-8 pr-14 py-1.5 bg-white dark:bg-[#2c2c2c] border border-[#e6e6e6] dark:border-transparent mx-auto block rounded-md text-sm focus:outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-shadow text-[#1c1c1e] dark:text-[#f4f4f5] placeholder:text-gray-400 dark:placeholder:text-gray-500">
            <button v-if="search.searchQuery.value" @click="search.searchQuery.value = ''" class="absolute right-7 top-1/2 -translate-y-1/2 p-0.5 rounded-full hover:bg-gray-100 dark:hover:bg-[#3f3f46] text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors" aria-label="Search.search Query.value =">
              <X class="w-3.5 h-3.5" />
            </button>
            <button @click="search.isCaseSensitiveSearch.value = !search.isCaseSensitiveSearch.value" :class="['absolute right-2 top-1/2 -translate-y-1/2 p-0.5 rounded-sm transition-colors', search.isCaseSensitiveSearch.value ? 'bg-purple-100 text-purple-600 dark:bg-purple-500/20 dark:text-purple-400' : 'text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 hover:bg-gray-100 dark:hover:bg-[#3f3f46]']" :title="$t('note.match_case')">
              <CaseSensitive class="w-3.5 h-3.5" />
            </button>
          </div>
      </div>

      <div class="flex-1 overflow-y-auto" @mousedown.stop>
         <!-- Pinned Section -->
         <div class="mb-4" v-if="search.allPinnedNotes.value.length > 0">
             <div class="flex justify-between items-center px-4 mb-2 mt-3">
                 <span class="text-[11px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider">{{ $t('note.pinned_notes') }}</span>
                 <button @click="manager.openNoteManager('pinned', () => { sidebar.showLeft.value = false; })" class="text-[10px] text-purple-500 hover:text-purple-600 font-medium p-2 -m-2">{{ $t('note.show_all') }}</button>
             </div>
             <div class="px-2 space-y-0.5">
                 <NoteListItem v-for="note in search.topPinnedNotes.value" :key="note.id"
                    :note="note" :is-active="currentNoteId === note.id" :show-context-menu="activeContextMenu === note.id" :is-pinned-section="true"
                    @select="handleNoteSelect" @toggle-context="toggleContext"
                    @pin="togglePin" @open-window="openInNewWindow" @rename="rename.handleRenamePrompt($event, closeContextMenu)" @toggle-lock="lock.toggleNoteLock($event, closeContextMenu)" @history="openHistory" @delete="deleteNote"
                 />
                 <button v-if="search.allPinnedNotes.value.length > 5" @click="manager.openNoteManager('pinned', () => { sidebar.showLeft.value = false; })" class="w-full text-center py-2.5 mt-2 text-xs font-medium text-blue-500 hover:text-blue-600 hover:bg-blue-50 dark:hover:bg-blue-900/20 rounded-lg transition-colors">
                     {{ $t('note.show_more', { count: search.allPinnedNotes.value.length - 5 }) }}
                 </button>
             </div>
         </div>

         <!-- Tags Section -->
         <div class="mb-4">
             <div class="flex justify-between items-center px-4 mb-2 mt-2">
                 <span class="text-[11px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider">{{ $t('note.top_tags') }}</span>
                 <button @click="manager.openNoteManager('tags', () => { sidebar.showLeft.value = false; })" class="text-[10px] text-purple-500 hover:text-purple-600 font-medium p-2 -m-2">{{ $t('note.show_all') }}</button>
             </div>
             <div class="px-2 space-y-0.5" v-if="tags.topTags.value.length > 0">
                 <div v-for="tag in tags.topTags.value" :key="tag.name"
                      @click="tags.toggleTagSelection(tag.name)"
                      class="w-full flex items-center justify-between px-3 py-1.5 rounded-lg text-sm transition-colors cursor-pointer group"
                      :class="tags.selectedTags.value.has(tag.name) ? 'bg-black/5 dark:bg-white/10' : 'hover:bg-gray-100 dark:hover:bg-[#2a2a2a] text-[#52525b] dark:text-[#a1a1aa]'">
                      <div class="flex items-center gap-2 truncate">
                          <Hash class="w-3.5 h-3.5 opacity-70 group-hover:text-black dark:group-hover:text-white transition-colors" />
                          <span class="truncate select-none group-hover:text-black dark:group-hover:text-white transition-colors">{{ tag.name.split('/').pop() }}</span>
                      </div>
                      <span class="text-[10px] opacity-50 bg-black/5 dark:bg-white/10 px-1.5 py-0.5 rounded-full min-w-[20px] text-center">{{ tag.count }}</span>
                 </div>
             </div>
             <div v-else class="text-center p-4 text-xs text-gray-400">{{ $t('note.no_tags_found') }}</div>
         </div>

         <!-- Recent Notes -->
         <div class="mb-4">
             <div class="flex justify-between items-center px-4 mb-2 mt-2">
                 <span class="text-[11px] font-semibold text-[#8b8b8b] dark:text-[#71717a] uppercase tracking-wider">{{ $t('note.recent_notes') }}</span>
                 <button @click="manager.openNoteManager('notes', () => { sidebar.showLeft.value = false; })" class="text-[10px] text-purple-500 hover:text-purple-600 font-medium p-2 -m-2">{{ $t('note.show_all') }}</button>
             </div>
             <div class="px-2 space-y-0.5">
                 <NoteListItem v-for="note in search.recentNotes.value" :key="note.id"
                    :note="note" :is-active="currentNoteId === note.id" :show-context-menu="activeContextMenu === note.id" :is-pinned-section="false"
                    @select="handleNoteSelect" @toggle-context="toggleContext"
                    @pin="togglePin" @open-window="openInNewWindow" @rename="rename.handleRenamePrompt($event, closeContextMenu)" @toggle-lock="lock.toggleNoteLock($event, closeContextMenu)" @history="openHistory" @delete="deleteNote"
                 />
             </div>
             <div v-if="search.recentNotes.value.length === 0" class="p-8 text-center text-sm text-[#52525b] dark:text-[#a1a1aa]">
               {{ $t('note.no_notes_match') }}
             </div>
         </div>
      </div>
    </aside>

    <!-- Main Area: Editor / Manager -->
    <main class="flex-1 flex flex-col bg-[#fdfdfc] dark:bg-[#242424] min-w-[300px] max-md:min-w-0" @mousedown.stop>
      <template v-if="manager.viewMode.value === 'editor'">
          <div v-if="!isFloatingView" class="h-10 flex-shrink-0 w-full flex items-center justify-between px-4" data-tauri-drag-region>
            <div class="flex gap-2">
              <NavButtons />
              <button @click="sidebar.showLeft.value = !sidebar.showLeft.value" class="p-1 rounded-md hover:bg-gray-200 dark:hover:bg-gray-800 text-gray-500 transition-colors" :title="$t('note.toggle_sidebar')">
                <PanelLeftClose v-if="sidebar.showLeft.value" class="w-4 h-4" />
                <PanelLeft v-else class="w-4 h-4" />
              </button>
            </div>
            <div class="flex gap-2">
              <button v-if="currentNoteId && manager.viewMode.value === 'editor'" @click="zenMode = !zenMode" class="p-1 rounded-md hover:bg-gray-200 dark:hover:bg-gray-800 text-gray-500 transition-colors hidden md:flex items-center justify-center w-8 h-7" :title="zenMode ? 'Exit Zen Mode' : 'Zen Mode'">
                <Monitor class="w-4 h-4" />
              </button>
              <button v-if="currentNoteId && manager.viewMode.value === 'editor'" @click="editorFullWidth = !editorFullWidth" class="p-1 rounded-md hover:bg-gray-200 dark:hover:bg-gray-800 text-gray-500 transition-colors hidden md:flex items-center justify-center w-8 h-7" :title="editorFullWidth ? 'Standard Width' : 'Full Width'">
                <div v-if="editorFullWidth" class="flex items-center space-x-[1px]">
                  <ArrowRight class="w-3 h-3" />
                  <ArrowLeft class="w-3 h-3" />
                </div>
                <div v-else class="flex items-center space-x-[1px]">
                  <ArrowLeft class="w-3 h-3" />
                  <ArrowRight class="w-3 h-3" />
                </div>
              </button>
              <button v-if="currentNoteId && manager.viewMode.value === 'editor'" @click="openHistory(currentNoteId)" class="p-1 rounded-md hover:bg-gray-200 dark:hover:bg-gray-800 text-gray-500 transition-colors hidden md:flex items-center justify-center w-8 h-7" :title="$t('note.history_title')">
                <History class="w-4 h-4" />
              </button>
              <button v-if="currentNoteId && manager.viewMode.value === 'editor'" @click="noteExport.exportModalVisible.value = true" class="p-1 rounded-md hover:bg-gray-200 dark:hover:bg-gray-800 text-gray-500 transition-colors hidden md:flex items-center justify-center w-8 h-7" :title="$t('note.export_note')">
                <Download class="w-4 h-4" />
              </button>
              <div class="relative flex items-center h-full"></div>
              <button v-if="currentNoteId && manager.viewMode.value === 'editor'" @click="sidebar.showRight.value = !sidebar.showRight.value" class="p-1 relative ml-2 rounded-md hover:bg-gray-200 dark:hover:bg-gray-800 text-gray-500 transition-colors" :title="$t('note.toggle_right_sidebar')">
                <PanelRightClose v-if="sidebar.showRight.value" class="w-4 h-4" />
                <PanelRight v-else class="w-4 h-4" />
              </button>
            </div>
          </div>

          <div v-if="zenMode" class="absolute top-4 right-4 z-50">
             <button @click="zenMode = false" class="p-2 bg-black/10 dark:bg-white/10 hover:bg-black/20 dark:hover:bg-white/20 rounded-full text-gray-500 hover:text-black dark:hover:text-white transition-all shadow-sm backdrop-blur-md opacity-0 hover:opacity-100 group-hover:opacity-100" :title="$t('note.exit_zen_mode')">
                <Monitor class="w-4 h-4" />
             </button>
          </div>

          <div v-else-if="!isFloatingView && manager.viewMode.value !== 'editor'" class="h-8 flex-shrink-0 w-full z-50 bg-[#fdfdfc] dark:bg-[#242424]" data-tauri-drag-region></div>

          <template v-if="tabs.activeTabs.value.length > 0">
            <template v-for="tabId in tabs.activeTabs.value" :key="tabId">
              <div v-show="currentNoteId === tabId" class="flex-1 overflow-y-auto w-full relative">
                <div v-if="tabs.tabContents.value[tabId] === undefined" class="absolute inset-0 flex items-center justify-center bg-[#fdfdfc] dark:bg-[#242424]">
                    <div class="w-8 h-8 rounded-full border-2 border-gray-200 border-t-gray-400 animate-spin"></div>
                </div>
                <div v-else class="px-4 md:px-12 pb-12 mx-auto w-full cursor-text transition-all duration-300" :class="editorFullWidth ? 'max-w-none' : 'max-w-4xl'">
                <div class="mb-4 pt-4">
                   <div class="flex gap-2 mb-4 flex-wrap items-center">
                      <span v-for="tag in notes.find(n => n.id === tabId)?.tags" :key="tag" class="text-xs px-2 py-1 rounded-md bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-300 flex items-center gap-1 group/tag">
                          <Hash class="w-3 h-3 opacity-50"/>
                          {{ tag }}
                          <button @click="tags.removeTag(tag)" class="opacity-0 group-hover/tag:opacity-100 hover:text-red-500 transition-opacity ml-1 p-0.5" aria-label="Tags.remove Tag"><X class="w-3 h-3"/></button>
                       </span>
                       <div class="relative flex items-center">
                          <Plus class="w-3 h-3 absolute left-1.5 text-gray-400" />
                          <input v-model="tags.newTagInput.value" @keydown="tags.addTag" :placeholder="$t('note.add_tag')" class="text-xs bg-transparent border border-dashed border-gray-300 dark:border-gray-600 rounded-md py-1 pl-5 pr-2 w-24 focus:w-32 focus:outline-none focus:border-gray-400 transition-all text-[#1c1c1e] dark:text-[#f4f4f5]" />
                       </div>
                   </div>
                   <div class="w-full grid grow-wrap" :data-replicated-value="(tabs.focusedTitles.value[tabId] !== undefined ? tabs.focusedTitles.value[tabId] : notes.find(n => n.id === tabId)?.title) || ''">
                     <textarea class="note-title-input w-full text-4xl font-bold bg-transparent border-none outline-none text-[#1c1c1e] dark:text-[#f4f4f5] placeholder:text-gray-300 dark:placeholder:text-gray-700 resize-none overflow-hidden col-start-1 row-start-1 h-full"
                       rows="1"
                       :value="tabs.focusedTitles.value[tabId] !== undefined ? tabs.focusedTitles.value[tabId] : notes.find(n => n.id === tabId)?.title"
                       @focus="tabs.focusedTitles.value[tabId] = ($event.target as HTMLTextAreaElement).value"
                       @input="tabs.focusedTitles.value[tabId] = ($event.target as HTMLTextAreaElement).value"
                       @blur="rename.renameTopTitle"
                       @keydown.enter.prevent="rename.renameTopTitle"
                       :placeholder="$t('note.note_title')"
                       :readonly="licenseStore.isReadOnly"></textarea>
                   </div>
                </div>
                <div class="mt-4 pb-20 w-full text-text dark:text-text-dark" :class="{'zen-editor-container': zenMode && !editorFullWidth}">
                   <TiptapEditor :ref="(el) => { const refs = save.editorRefs.value || save.editorRefs; if (el) refs[tabId] = el; else delete refs[tabId]; }" :model-value="tabs.tabContents.value[tabId]" :vault-path="vaultPath" :zen-mode="zenMode" :current-note-id="tabId" @update:model-value="(val: string) => save.onEditorUpdate(val, tabId)" @open-internal-note="handleOpenInternalNote" />
                </div>
                </div>
              </div>
            </template>
          </template>
          <div v-else class="flex-1 flex items-center justify-center text-[#52525b] dark:text-[#a1a1aa]">
            <div class="text-center">
              <FileText class="w-12 h-12 mx-auto mb-4 opacity-20" />
              <p>{{ $t('note.select_to_start') }}</p>
            </div>
          </div>
      </template>
      <template v-else-if="manager.viewMode.value === 'manager'">
          <div class="flex-1 flex flex-col bg-[#fdfdfc] dark:bg-[#242424] h-full relative z-0 overflow-y-auto">
             <div class="flex items-center justify-between px-6 h-10 border-b border-[#e6e6e6] dark:border-[#2c2c2c] shrink-0 sticky top-0 bg-[#fdfdfc] dark:bg-[#242424] z-10" data-tauri-drag-region>
                <div class="flex items-center gap-3">
                   <button @click="manager.viewMode.value = 'editor'" class="p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] transition-colors text-gray-500" aria-label="Manager.view Mode.value =">
                      <ArrowLeft class="w-5 h-5" />
                   </button>
                   <h1 class="text-xl font-bold text-[#1c1c1e] dark:text-[#f4f4f5] flex items-center gap-2">
                      {{ manager.managerFilter.value === 'tags' && !manager.managerSearchQuery.value ? $t('note.all_tags') : manager.managerSearchQuery.value ? $t('note.search_results') : manager.managerFilter.value === 'notes' || !manager.managerFilter.value ? $t('note.all_notes') : manager.managerFilter.value === 'pinned' ? $t('note.pinned_notes') : $t('note.tag_prefix') + manager.managerFilter.value.split('/').pop() }}
                      <span class="text-[12px] font-medium px-2 py-0.5 mt-0.5 rounded-full bg-gray-100 dark:bg-[#333] text-gray-500">
                        {{ manager.managerFilter.value === 'tags' && !manager.managerSearchQuery.value ? tags.allTags.value.length : manager.managerFilteredNotes.value.length }}
                      </span>
                   </h1>
                   <!--
                     Kept in the manager rather than on the editor toolbar: this
                     is a question about the vault as a whole, and this is the
                     screen someone is already on when they are looking at it
                     that way.
                   -->
                   <button
                     @click="duplicatesModalVisible = true"
                     class="ml-auto flex items-center gap-2 px-3 py-1.5 text-[13px] rounded-lg text-gray-500 hover:text-[#1c1c1e] dark:hover:text-[#f4f4f5] hover:bg-gray-100 dark:hover:bg-[#2c2c2c] transition-colors"
                     :title="$t('note.duplicates_hint')"
                   >
                     <Copy class="w-4 h-4" />
                     <span class="hidden sm:inline">{{ $t('note.duplicates_title') }}</span>
                   </button>
                </div>
             </div>

             <div class="flex-1 flex flex-col p-8 md:p-12 lg:p-16 w-full max-w-6xl mx-auto">
                 <div class="relative w-full mb-8">
                   <Search class="absolute left-4 top-1/2 -translate-y-1/2 w-5 h-5 text-[#8b8b8b] dark:text-[#71717a]" />
                   <input v-model="manager.managerSearchQuery.value" type="text" :placeholder="$t('note.search_manager_placeholder')" class="w-full pl-12 pr-20 py-3 bg-white dark:bg-[#1a1a1a] border border-[#e6e6e6] dark:border-[#2c2c2c] rounded-xl text-base shadow-sm focus:outline-none focus:ring-2 focus:ring-purple-500/50 transition-shadow placeholder:text-gray-400 manager-search-input">
                   <button v-if="manager.managerSearchQuery.value" @click="manager.managerSearchQuery.value = ''" class="absolute right-12 top-1/2 -translate-y-1/2 p-1.5 rounded-full hover:bg-gray-100 dark:hover:bg-[#2c2c2c] text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors" aria-label="Manager.manager Search Query.value =">
                     <X class="w-4 h-4" />
                   </button>
                   <button @click="search.isCaseSensitiveSearch.value = !search.isCaseSensitiveSearch.value" :class="['absolute right-3 top-1/2 -translate-y-1/2 p-1.5 rounded-md transition-colors', search.isCaseSensitiveSearch.value ? 'bg-purple-100 text-purple-600 dark:bg-purple-500/20 dark:text-purple-400' : 'text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 hover:bg-gray-100 dark:hover:bg-[#2c2c2c]']" :title="$t('note.match_case')">
                     <CaseSensitive class="w-4 h-4" />
                   </button>
                 </div>

                 <!-- Tags View -->
                 <div v-if="manager.managerFilter.value === 'tags' && !manager.managerSearchQuery.value" class="w-full">
                    <div class="flex flex-wrap gap-3">
                       <div v-for="tag in tags.allTags.value" :key="tag.name" @click="manager.managerFilter.value = tag.name" class="px-4 py-2 bg-white dark:bg-[#1f1f1f] border border-[#e6e6e6] dark:border-[#2c2c2c] rounded-lg cursor-pointer hover:border-[#d4d4d8] dark:hover:border-[#444] transition-all flex items-center gap-2 group">
                          <Hash class="w-4 h-4 text-gray-400 group-hover:text-[#1c1c1e] dark:group-hover:text-white transition-colors" />
                          <span class="font-medium text-[#1c1c1e] dark:text-[#f4f4f5]">{{ tag.name.split('/').pop() }}</span>
                          <span class="text-xs bg-gray-100 dark:bg-[#2c2c2c] px-2 py-0.5 rounded text-gray-500">{{ tag.count }}</span>
                       </div>
                    </div>
                 </div>

                 <!-- Notes Table View -->
                 <div v-else class="w-full">
                   <!--
                     Only on screen while something is ticked. A bar that is
                     always there, greyed out, teaches people to stop reading it.
                   -->
                   <div v-if="selection.active.value" class="mb-3 flex items-center gap-3 px-4 py-2.5 rounded-xl bg-blue-50 dark:bg-blue-900/20 border border-blue-200/70 dark:border-blue-800/50">
                      <span class="text-[13px] font-medium text-blue-900 dark:text-blue-100">{{ $t('note.selected_count', { count: selection.count.value }) }}</span>
                      <button @click="deleteSelected" class="ml-auto flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-[13px] font-medium text-white bg-red-500 hover:bg-red-600 transition-colors">
                         <Trash2 class="w-3.5 h-3.5" />
                         {{ $t('note.delete_selected') }}
                      </button>
                      <button @click="selection.clear()" class="px-3 py-1.5 rounded-lg text-[13px] font-medium text-blue-900 dark:text-blue-100 hover:bg-blue-100 dark:hover:bg-blue-900/40 transition-colors">
                         {{ $t('note.clear_selection') }}
                      </button>
                   </div>
                   <div class="bg-white dark:bg-[#252525] border border-[#e6e6e6] dark:border-[#333] rounded-xl overflow-hidden shadow-sm">
                      <table class="w-full text-left border-collapse">
                         <thead>
                            <tr class="bg-gray-50 dark:bg-[#1a1a1a] border-b border-[#e6e6e6] dark:border-[#333]">
                               <th class="py-2.5 px-4 w-8">
                                  <input
                                     type="checkbox"
                                     class="w-3.5 h-3.5 align-middle accent-blue-500 cursor-pointer"
                                     :checked="selection.allVisibleSelected(visibleManagerIds)"
                                     :indeterminate="selection.someVisibleSelected(visibleManagerIds)"
                                     :aria-label="$t('note.select_all_on_page')"
                                     :title="$t('note.select_all_on_page')"
                                     @click="selection.toggleAll(visibleManagerIds)"
                                  />
                               </th>
                               <th class="py-2.5 px-4 text-xs font-semibold text-gray-500 uppercase w-5/12">{{ $t('note.title_col') }}</th>
                               <th class="py-2.5 px-4 text-xs font-semibold text-gray-500 uppercase">{{ $t('note.tags_col') }}</th>
                               <th class="py-2.5 px-4 text-xs font-semibold text-gray-500 uppercase whitespace-nowrap text-right">{{ $t('note.modified_col') }}</th>
                               <th class="py-2.5 px-4 text-xs font-semibold text-gray-500 uppercase w-12 text-center">{{ $t('note.action_col') }}</th>
                            </tr>
                         </thead>
                         <tbody class="divide-y divide-[#e6e6e6] dark:divide-[#333] text-sm">
                            <tr v-for="note in manager.managerPaginatedNotes.value" :key="note.id" @click="handleManagerRowClick(note.id, $event)" class="cursor-pointer transition-colors group"
                                :class="selection.isSelected(note.id) ? 'bg-blue-50/70 dark:bg-blue-900/20' : 'hover:bg-gray-50 dark:hover:bg-[#2a2a2a]'">
                               <!--
                                 One cell, two things. The tick takes over from
                                 the icon on hover, or as soon as anything is
                                 selected — so the column costs nothing while
                                 the reader is only reading, and is already
                                 under the pointer when they are not.
                               -->
                               <td class="py-3 px-4 w-8" @click.stop>
                                  <div class="relative w-3.5 h-3.5">
                                     <Pin v-if="note.pinned" class="w-3.5 h-3.5 text-orange-500 fill-orange-500/20 transition-opacity group-hover:opacity-0" :class="{ '!opacity-0': selection.active.value }" />
                                     <FileText v-else class="w-3.5 h-3.5 text-gray-400 opacity-50 transition-opacity group-hover:opacity-0" :class="{ '!opacity-0': selection.active.value }" />
                                     <input
                                        type="checkbox"
                                        class="absolute inset-0 w-3.5 h-3.5 accent-blue-500 cursor-pointer opacity-0 transition-opacity group-hover:opacity-100"
                                        :class="{ '!opacity-100': selection.active.value }"
                                        :checked="selection.isSelected(note.id)"
                                        :aria-label="$t('note.select_note')"
                                        @click.stop="selection.toggle(note.id, visibleManagerIds, $event.shiftKey)"
                                     />
                                  </div>
                               </td>
                               <td class="py-3 px-4 font-medium text-[#1c1c1e] dark:text-[#f4f4f5] max-w-[250px] truncate">{{ note.title || $t('note.untitled_note') }}</td>
                               <td class="py-3 px-4">
                                  <div class="flex flex-wrap gap-1" v-if="note.tags.length">
                                     <span v-for="tag in note.tags.slice(0, 3)" :key="tag" class="text-[10px] px-1.5 py-0.5 rounded bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-300">{{ tag.split('/').pop() }}</span>
                                     <span v-if="note.tags.length > 3" class="text-[10px] px-1.5 py-0.5 rounded bg-gray-100 dark:bg-gray-800 text-gray-500">+{{ note.tags.length - 3 }}</span>
                                  </div>
                                  <span v-else class="text-xs text-gray-400 italic">{{ $t('note.no_tags') }}</span>
                               </td>
                               <td class="py-3 px-4 text-xs text-gray-500 dark:text-gray-400 whitespace-nowrap text-right">{{ formatDate(note.date) }}</td>
                               <td class="py-3 px-4 w-12 text-center" @click.stop>
                                  <div class="relative flex justify-center">
                                     <button @click="(e) => toggleContext('manager_'+note.id, e)" class="p-1 rounded md:opacity-0 opacity-100 group-hover:opacity-100 hover:bg-gray-200 dark:hover:bg-[#444] transition">
                                        <MoreVertical class="w-4 h-4 text-gray-500" />
                                     </button>
                                     <NoteContextMenu v-if="activeContextMenu === 'manager_'+note.id" :note-id="note.id" :is-pinned="note.pinned" variant="manager"
                                        @pin="togglePin($event); activeContextMenu = null;"
                                        @history="openHistory"
                                        @delete="deleteNote($event); activeContextMenu = null;"
                                     />
                                  </div>
                               </td>
                            </tr>
                            <tr v-if="manager.managerFilteredNotes.value.length === 0">
                               <td colspan="5" class="py-12 text-center text-gray-500">{{ $t('note.no_notes_found') }}</td>
                            </tr>
                         </tbody>
                      </table>
                   </div>

                   <!-- Pagination Controls -->
                   <div v-if="manager.managerTotalPages.value > 1" class="mt-4 flex items-center justify-between text-[13px] text-gray-500">
                      <div>{{ $t('note.showing') }} {{ (manager.managerCurrentPage.value - 1) * manager.managerItemsPerPage + 1 }} {{ $t('note.to') }} {{ Math.min(manager.managerCurrentPage.value * manager.managerItemsPerPage, manager.managerFilteredNotes.value.length) }} {{ $t('note.of') }} {{ manager.managerFilteredNotes.value.length }} {{ $t('note.notes_lowercase') }}</div>
                      <div class="flex items-center gap-2">
                         <button @click="manager.managerPrevPage()" :disabled="manager.managerCurrentPage.value === 1" class="px-3 py-1.5 rounded-lg border border-[#e6e6e6] dark:border-[#333] hover:bg-gray-50 dark:hover:bg-[#2c2c2c] disabled:opacity-50 disabled:cursor-not-allowed transition-colors text-[#1c1c1e] dark:text-[#f4f4f5]">{{ $t('note.previous') }}</button>
                         <span class="font-medium px-2 text-[#1c1c1e] dark:text-[#f4f4f5]">{{ $t('note.page') }} {{ manager.managerCurrentPage.value }} {{ $t('note.of') }} {{ manager.managerTotalPages.value }}</span>
                         <button @click="manager.managerNextPage()" :disabled="manager.managerCurrentPage.value === manager.managerTotalPages.value" class="px-3 py-1.5 rounded-lg border border-[#e6e6e6] dark:border-[#333] hover:bg-gray-50 dark:hover:bg-[#2c2c2c] disabled:opacity-50 disabled:cursor-not-allowed transition-colors text-[#1c1c1e] dark:text-[#f4f4f5]">{{ $t('note.next') }}</button>
                      </div>
                   </div>
                 </div>
             </div>
          </div>
      </template>
    </main>

    <!-- Right Sidebar: Graph & Backlinks -->
    <aside v-if="currentNoteId && !isFloatingView && manager.viewMode.value === 'editor'" v-show="sidebar.showRight.value" class="shrink-0 relative border-l border-[#e6e6e6] dark:border-[#2c2c2c] bg-[#fbfbfc] dark:bg-[#191919] flex flex-col overflow-hidden max-md:!w-full max-md:absolute max-md:inset-0 max-md:z-[60]" :style="{ width: sidebar.rightWidth.value + 'px' }">
      <div class="hidden md:block absolute top-0 left-0 w-1.5 h-full cursor-col-resize hover:bg-black/10 dark:hover:bg-white/10 z-10 opacity-0 hover:opacity-100 transition-opacity" @mousedown.stop="sidebar.startDragRight"></div>
      <div class="h-10 flex-shrink-0 flex items-center px-4 border-b border-[#e6e6e6] dark:border-[#2c2c2c]" data-tauri-drag-region>
          <Globe class="w-4 h-4 text-gray-500 mr-2" />
          <span class="font-bold text-[11px] tracking-wider text-gray-500 uppercase mt-0.5">{{ $t('note.graph_view') }}</span>
          <button @click="sidebar.showRight.value = false" class="p-1 ml-auto rounded-md hover:bg-gray-200 dark:hover:bg-gray-800 text-gray-400 transition-colors" aria-label="Sidebar.show Right Sidebar.value = false">
             <X class="w-3.5 h-3.5" />
          </button>
      </div>
      <div class="h-1/2 border-b border-[#e6e6e6] dark:border-[#2c2c2c] overflow-hidden">
          <NoteGraph v-if="activeNote" :current-note-id="currentNoteId || ''" :current-note-title="activeNote.title || 'Untitled Node'" :tags="activeNote.tags || []" :outgoing-links="backlinks.currentOutgoingLinks.value" :backlinks="backlinks.currentBacklinks.value" :all-notes="notes" @open-note="handleOpenInternalNote" />
      </div>
      <div class="h-10 flex-shrink-0 flex items-center px-4 border-b border-[#e6e6e6] dark:border-[#2c2c2c]">
          <span class="font-bold text-[11px] tracking-wider text-[#8b8b8b] dark:text-[#71717a] uppercase mt-0.5">{{ $t('note.linked_mentions') }} ({{ backlinks.currentBacklinks.value.length }})</span>
      </div>
      <div class="flex-1 overflow-y-auto p-2 space-y-1">
          <div v-if="backlinks.currentBacklinks.value.length === 0" class="text-[13px] text-gray-400 text-center py-4">{{ $t('note.no_linked_mentions') }}</div>
          <div v-for="bl in backlinks.currentBacklinks.value" :key="bl.id" @click="handleOpenInternalNote({ id: bl.id, type: bl.node_type })" class="p-3 border border-transparent rounded-lg cursor-pointer hover:bg-white/50 dark:hover:bg-[#252525] hover:border-[#e6e6e6] dark:hover:border-[#2f2f2f] transition-all group">
            <h5 class="flex items-center gap-2 pr-2">
                <Calendar v-if="bl.node_type === 'event'" class="w-3.5 h-3.5 text-rose-500 shrink-0 opacity-80 group-hover:opacity-100 transition-colors"/>
                <CheckSquare v-else-if="bl.node_type === 'task'" class="w-3.5 h-3.5 text-emerald-500 shrink-0 opacity-80 group-hover:opacity-100 transition-colors"/>
                <FileText v-else class="w-3.5 h-3.5 text-gray-400 shrink-0 opacity-80 group-hover:text-purple-500 group-hover:opacity-100 transition-colors"/>
                <span class="text-[13px] font-medium text-[#1c1c1e] dark:text-[#f4f4f5] truncate">{{ bl.title }}</span>
                <span v-if="bl.node_type === 'event' && bl.properties && bl.properties.start_at" class="ml-auto text-[9px] text-gray-400 font-medium tracking-wider whitespace-nowrap">{{ (bl.properties.start_at as string).split('T')[0] }}</span>
                <button v-if="(bl as any)._is_outgoing_project" @click.stop="backlinks.unlinkProject(bl.id, bl.title)" class="ml-auto p-1.5 -mr-1.5 text-gray-400 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-900/30 rounded-md opacity-0 group-hover:opacity-100 transition-all" :title="$t('note.unlink_project')">
                   <X class="w-3.5 h-3.5" />
                </button>
            </h5>
          </div>
      </div>
    </aside>

    <!-- Rename Modal -->
    <Teleport to="body">
      <div v-if="rename.renameModal.value.show" class="fixed inset-0 z-[999] flex items-center justify-center bg-black/40 backdrop-blur-sm" @click.self="rename.renameModal.value.show = false">
        <div class="bg-white dark:bg-[#2a2a2a] rounded-2xl shadow-2xl p-6 w-80 border border-[#e6e6e6] dark:border-[#3a3a3a]">
          <h3 class="text-base font-semibold text-[#1c1c1e] dark:text-[#f4f4f5] mb-4">{{ $t('note.rename_note') }}</h3>
          <input v-model="rename.renameModal.value.value" type="text" class="w-full px-3 py-2 rounded-lg border border-[#e0e0e0] dark:border-[#444] bg-white dark:bg-[#1e1e1e] text-[#1c1c1e] dark:text-[#f4f4f5] text-sm focus:outline-none focus:ring-2 focus:ring-black/10 dark:focus:ring-white/20" @keydown.enter="rename.confirmRename" autofocus :aria-label="$t('note.rename_note')" />
          <div class="flex justify-end gap-2 mt-4">
            <button @click="rename.renameModal.value.show = false" class="px-4 py-1.5 text-sm rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-[#333] transition-colors">{{ $t('note.cancel') }}</button>
            <button @click="rename.confirmRename" class="px-4 py-1.5 text-sm rounded-lg bg-black dark:bg-white text-white dark:text-black font-medium hover:opacity-80 transition-opacity">{{ $t('note.rename') }}</button>
          </div>
        </div>
      </div>
    </Teleport>

    <!-- Notes that exist twice -->
    <NoteDuplicatesModal
      v-if="duplicatesModalVisible"
      :vault-path="vaultPath"
      @close="duplicatesModalVisible = false"
      @changed="scanVault"
    />

    <!-- Undo window on a deleted note -->
    <NoteUndoToast
      :pending="del.pending.value"
      :seconds="7"
      @undo="del.undoDelete"
    />

    <!-- Version History -->
    <NoteHistoryModal
      v-if="historyNoteId"
      :vault-path="vaultPath"
      :note-id="historyNoteId"
      :note-title="historyNote?.title || $t('note.untitled_note')"
      @close="historyNoteId = null"
      @restored="onVersionRestored"
    />

    <!-- Export Modal -->
    <NoteExportModal
      v-if="noteExport.exportModalVisible.value"
      @close="noteExport.exportModalVisible.value = false"
      @export="(o: any) => { flushActiveEditor(); noteExport.handleExportOption(o); }"
    />

    <!-- Per-Note Lock Screen -->
    <LockScreenComponent
      v-if="lock.showNoteLockScreen.value"
      :title="lock.noteLockTitle.value"
      @unlocked="lock.handleNoteLockUnlocked"
      @cancelled="lock.showNoteLockScreen.value = false; lock.pendingNoteId.value = null"
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
  font-size: 2.25rem;
  line-height: 2.5rem;
  font-weight: 700;
  word-break: break-word;
}
.grow-wrap > textarea {
  grid-area: 1 / 1 / 2 / 2;
}
</style>
