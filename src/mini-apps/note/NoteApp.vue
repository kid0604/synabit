<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch, nextTick, inject, defineAsyncComponent, toRef } from 'vue';
import { Type, FileText, Search, PanelLeft, PanelLeftClose, PanelRight, PanelRightClose, Hash, Plus, MoreVertical, Pin, X, ArrowLeft, ArrowRight, CalendarDays, ChevronDown, CaseSensitive, Globe, Calendar, CheckSquare, Monitor, Download, History, Copy, Trash2, LayoutTemplate } from 'lucide-vue-next';
import { invoke } from '@tauri-apps/api/core';
import { useI18n } from 'vue-i18n';
import { useEventBus } from '../../composables/useEventBus';
import { useNodeService } from '../../composables/useNodeService';
import { showAppNotice } from '../../composables/useAppNotice';

import TiptapEditor from './TiptapEditor.vue';
import NoteGraph from './NoteGraph.vue';
import NavButtons from '../../shared/components/NavButtons.vue';
import NoteExportModal from './NoteExportModal.vue';
import NoteHistoryModal from './NoteHistoryModal.vue';
import NoteListItem from './components/NoteListItem.vue';
import NoteContextMenu from './components/NoteContextMenu.vue';
import UndoToast from '../../shared/components/UndoToast.vue';
import AppDialog from '../../shared/components/AppDialog.vue';
import NoteDuplicatesModal from './NoteDuplicatesModal.vue';
import TemplatePickerModal from './components/TemplatePickerModal.vue';
import { userTemplatesFrom, instantiateTemplate, isTemplatePath, TEMPLATE_FOLDER, type NoteTemplate } from './templates/noteTemplates';
import { i18n } from '../../i18n';

import { useAppStore } from '../../stores/useAppStore';
import { storeToRefs } from 'pinia';
import { logger } from '../../utils/logger';
import { routeForNode } from '../../shared/nodeRoutes';
import type { NavEntry } from '../../stores/useNavigationStore';
import { useAppLockStore } from '../../stores/useAppLockStore';

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
import { useNoteDelete, UNDO_WINDOW_MS as NOTE_UNDO_WINDOW_MS } from './composables/useNoteDelete';
import { confirmDelete } from '../../composables/useConfirmDelete';
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
const { enableDailyNotes, dailyNoteFormat, dailyNoteTag, simpleMode, noteToolbarVisible } = storeToRefs(appStore);
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

const lock = useNoteLock(appLockStore, handleNoteSelect, (id) => openHistory(id));

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
 * One press, then the undo toast, like every delete in the app. The only
 * question is the app-wide "Ask before deleting" (`confirmDelete`), asked here
 * before anything leaves the list — `useNoteDelete` itself asks nothing.
 */
const del = useNoteDelete({
    notes, currentNoteId, recentNoteIds,
    tabContents: tabs.tabContents,
    activeTabs: tabs.activeTabs,
    tabAccessTime: tabs.tabAccessTime,
    flushSave: save.flushSave,
    ns, scanVault,
    onFailed: (note) => {
        showAppNotice(t('note.delete_failed_restored', { title: note.title || t('note.untitled_note') }), 'error');
    },
});

const deleteNote = async (id: string) => {
    activeContextMenu.value = null;
    const note = notes.value.find((n) => n.id === id);
    if (!note) return;
    if (!(await confirmDelete({ name: note.title || t('note.untitled_note') }))) return;
    return del.deleteNote(id);
};

/**
 * What the undo toast says. One note is named; several are counted — listing
 * thirteen titles in a bar this size means truncating twelve of them, which
 * tells the reader less than the number does.
 */
const undoMessage = computed(() => {
    const held = del.pending.value?.notes ?? [];
    return held.length > 1
        ? t('note.deleted_many_toast', { count: held.length })
        : t('note.deleted_toast', { title: held[0]?.note.title || t('note.untitled_note') });
});

/** Restarts the countdown bar when one deletion follows another with no gap. */
const undoKey = computed(() => (del.pending.value?.notes ?? []).map((e) => e.note.id).join('|'));

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
    if (!(await confirmDelete({ count: ids.length }))) return;

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
    activeContextMenu.value = null;
    // Every version of a note is the note, so a protected one asks for the PIN
    // here exactly as it does to open it. Without this the history was a way
    // round the lock: right-click, History, and every version was readable.
    if (appLockStore.isEnabled && appLockStore.isNoteProtected(id) && !appLockStore.isNoteAccessible(id)) {
        lock.pendingNoteId.value = id;
        lock.pendingNoteAction.value = 'history';
        lock.noteLockTitle.value = 'note.pin_to_view_history';
        lock.showNoteLockScreen.value = true;
        return;
    }
    historyNoteId.value = id;
};

const historyNote = computed(() => notes.value.find(n => n.id === historyNoteId.value) || null);

/**
 * Save what the open tab is holding before a restore replaces it.
 *
 * The restore keeps the version it replaces, but a version is what reached
 * disk. Anything still waiting on the autosave would be in none of them.
 */
const saveBeforeRestore = async () => {
    const id = historyNoteId.value;
    if (id && tabs.tabContents.value[id] !== undefined) await save.flushSave(id);
};

/**
 * Read the restored note back into the open tab.
 *
 * The restore has already written the file and brought the database in line,
 * so this is only about the copy the editor is holding. Without it the editor
 * would still show the old text and the next autosave — 600ms after the next
 * keystroke — would write it straight back over the restore.
 *
 * Read from disk rather than from anything the restore handed back. The
 * restore has a whole file, frontmatter included, and the editor holds only a
 * body; putting one in the other wrote the frontmatter into the note, and the
 * next save gave it a second. Read back the way any note is read, the
 * frontmatter lands in the row's properties where it belongs.
 *
 * The rows go first, so a save the reload sets off carries the restored title
 * and tags rather than the ones the restore replaced.
 */
const onVersionRestored = async () => {
    const id = historyNoteId.value;
    await scanVault();
    // Only the tab actually holding this note needs telling. Restoring from a
    // context menu can target a note that is not open, and the file plus the
    // database are already in line by the time this runs.
    if (!id || tabs.tabContents.value[id] === undefined) return;
    await tabs.reloadNoteFile(id);
    bus.emit('note:updated-external', { id, content: tabs.tabContents.value[id] });
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

/*
 * Daily notes written before `date:` existed carry their day only in the title,
 * under whatever pattern was set at the time. Give each one its date once per
 * vault on this device, before any note is loaded into a tab, so an open
 * editor never saves over the line being added. The title is read in Rust,
 * which skips any title two devices could read differently; see
 * `src-tauri/src/utils/daily_note_date.rs`.
 */
const giveDailyNotesTheirDate = async () => {
    if (!props.vaultPath) return;
    const key = `daily-note-date-v1:${props.vaultPath}`;
    try {
        if (await invoke<string | null>('get_migration_flag', { key })) return;

        const formatStr = isValidDailyFormat.value ? dailyNoteFormat.value : 'YYYY-MM-DD';
        const report = await invoke<{ changed: number; unchanged: number; failed: number }>(
            'migrate_daily_note_dates',
            { vaultPath: props.vaultPath, formatStr },
        );
        logger.info(
            `Daily note dates: ${report.changed} dated, ${report.unchanged} already current, ${report.failed} failed`,
        );

        // Only a clean pass is recorded, as with the other storage repairs.
        if (report.failed === 0) {
            await invoke('set_migration_flag', { key, value: new Date().toISOString() });
        }
    } catch (e) {
        logger.error('Daily note dates failed', e);
    }
};

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
        lock.noteLockTitle.value = 'note.pin_to_view';
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

// ── Templates ───────────────────────────────────────────────
/**
 * The template picker, for a new note ("New from template…" beside the new
 * note button) or for the open one (`/template` in the editor). See
 * `templates/noteTemplates.ts` for where templates come from.
 */
const templatePicker = ref<{ show: boolean; mode: 'create' | 'insert' }>({ show: false, mode: 'create' });

/** Every note in `Templates/`. Built from the list, so it is never stale. */
const userTemplates = computed<NoteTemplate[]>(() => userTemplatesFrom(notes.value, t('note.untitled_note')));

const openTemplatePicker = (mode: 'create' | 'insert') => {
    templatePicker.value = { show: true, mode };
};

/** A template note's body: the open tab's copy if it has one, else the file's. */
const readNoteBody = async (id: string): Promise<string> => {
    if (id === currentNoteId.value) flushActiveEditor();
    const open = tabs.tabContents.value[id];
    if (open !== undefined) return open;
    const full = await ns.getNode(id);
    return typeof full?.content === 'string' ? full.content : '';
};

const appLocale = () => String(i18n.global.locale.value);

const onTemplateChosen = async ({ template, body }: { template: NoteTemplate; body: string }) => {
    const mode = templatePicker.value.mode;
    templatePicker.value.show = false;
    if (mode === 'insert') insertTemplateIntoOpenNote(template, body);
    else await createNoteFromTemplate(template, body);
};

/**
 * A new note, already written.
 *
 * Made the way any new note is — `createNode`, so it gets its identity and
 * creation date from the same place — and then given the template's title,
 * body and tags in one write, before it is opened. Opening first and filling
 * after would have the editor's autosave racing the fill.
 */
async function createNoteFromTemplate(template: NoteTemplate, body: string) {
    if (!props.vaultPath || isCreatingNote) return;
    isCreatingNote = true;
    save.setSuppressWatcherUntil(Date.now() + 3000);
    try {
        const filled = instantiateTemplate(template, body, new Date(), appLocale());
        const newPath = await ns.createNode({ directory: 'Notes', nodeType: 'note' });
        if (!newPath) return;
        await ns.writeNode({
            relPath: newPath,
            nodeType: 'note',
            title: filled.title || t('note.untitled_note'),
            properties: { tags: template.tags },
            content: filled.body,
        });
        await scanVault();
        currentNoteId.value = newPath;
        manager.viewMode.value = 'editor';
        if (window.innerWidth < 768) sidebar.showLeft.value = false;
    } catch (e) {
        logger.error('Failed to create a note from a template:', e);
    } finally {
        isCreatingNote = false;
    }
}

/** At the caret of the open note, with `{{title}}` meaning that note's title. */
function insertTemplateIntoOpenNote(template: NoteTemplate, body: string) {
    const id = currentNoteId.value;
    if (!id) return;
    const title = notes.value.find((n) => n.id === id)?.title || template.name;
    const filled = instantiateTemplate({ name: title, titlePattern: title }, body, new Date(), appLocale());
    const refs = (save.editorRefs.value || save.editorRefs) as Record<string, { insertMarkdown?: (md: string) => void }>;
    refs[id]?.insertMarkdown?.(filled.body);
}

/** "Saved as template" — said briefly, then gone. */
const templateSavedMessage = ref<string | null>(null);
let templateSavedTimer: ReturnType<typeof setTimeout> | null = null;

/**
 * Copy a note into `Templates/`.
 *
 * A copy, not a move: the note someone has been writing in stays where it
 * was, and the template is free to be pared down without touching it. Its
 * tags go along, so a note made from it starts in the same place in the tag
 * tree.
 */
const saveAsTemplate = async (id: string) => {
    activeContextMenu.value = null;
    const note = notes.value.find((n) => n.id === id);
    if (!note || !props.vaultPath) return;
    save.setSuppressWatcherUntil(Date.now() + 3000);
    try {
        const body = await readNoteBody(id);
        const title = note.title || t('note.untitled_note');
        const newPath = await ns.createNode({ directory: TEMPLATE_FOLDER, nodeType: 'note' });
        if (!newPath) return;
        await ns.writeNode({ relPath: newPath, nodeType: 'note', title, properties: { tags: note.tags }, content: body });
        await scanVault();
        templateSavedMessage.value = t('note.templates.saved_toast', { title });
        if (templateSavedTimer) clearTimeout(templateSavedTimer);
        templateSavedTimer = setTimeout(() => { templateSavedMessage.value = null; }, 4000);
    } catch (e) {
        logger.error('Failed to save a note as a template:', e);
        showAppNotice(t('note.templates.save_failed'), 'error');
    }
};

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

        // Nothing of that id anywhere, so there is nothing to open.
        //
        // This used to fall through and open the editor on it. That does not
        // fail — `loadNoteFile` waits on a file that is not there — so the
        // reader gets a spinner that never stops, which is what a note deleted
        // after somebody linked to it looked like from every direction: a Syn
        // source chip, a `[[wikilink]]`, a reminder, or the button on a
        // diagram this app has just started offering.
        //
        // The manager — the list of notes — is where somebody who has just
        // deleted one expects to be, and it is the one state this app can
        // always show honestly.
        if (!node) {
            logger.warn(`NoteApp: nothing in the vault has the id ${id}`);
            manager.viewMode.value = 'manager';
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
defineExpose({ openNoteById, scanVault, notes, tabContents: tabs.tabContents, loadNoteFile: tabs.loadNoteFile, currentNoteId, deleteNote, openHistory });

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
    if (templateSavedTimer) clearTimeout(templateSavedTimer);
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

    if (props.vaultPath && !props.isFloatingView) { await giveDailyNotesTheirDate(); }
    if (props.vaultPath) { await scanVault(); }

    bus.on('note:updated-external', (data) => {
        if (currentNoteId.value === data.id) return;
        if (tabs.tabContents.value[data.id] !== undefined) {
            tabs.tabContents.value[data.id] = data.content;
        }
    });

    /*
     * A write this editor did not make.
     *
     * Syn's tools run in Rust and their `node:updated` reaches the bus through
     * the bridge in `useEventBus`. Every other app already reloads on it — this
     * one did not, because an open tab keeps its own copy of the body and never
     * went back for another. So Syn would add a line to the note on screen, the
     * file on disk would have it, and the screen would not until the app was
     * restarted.
     *
     * `note:updated-external` is the neighbouring event and deliberately skips
     * the note you are looking at, because it is emitted by this editor's own
     * saves. This one is for the writes that came from somewhere else, which is
     * exactly the case that one steps around.
     *
     * The guard is a pending save, not a timer: a save in flight means the
     * buffer is newer than the file, and re-reading would take the sentence
     * being typed with it. Our own echo needs no guard at all — it fetches the
     * same text back and assigns a string that is already there.
     */
    bus.on('node:updated', ({ id, nodeType }) => {
        if (!id || (nodeType && nodeType !== 'note')) return;
        if (save.saveTimeouts.has(id)) return;
        tabs.reloadNoteFile(id, () => !save.saveTimeouts.has(id));
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
      class="border-r border-border dark:border-border-dark bg-surface-alt dark:bg-surface-alt-dark flex flex-col relative shrink-0 max-md:!w-full max-md:absolute max-md:inset-0 max-md:z-50"
      :style="{ width: sidebar.leftWidth.value + 'px' }"
    >
      <div class="hidden md:block absolute top-0 right-0 w-1.5 h-full cursor-col-resize hover:bg-black/10 dark:hover:bg-white/10 z-10 opacity-0 hover:opacity-100 transition-opacity" @mousedown.stop="sidebar.startDragLeft($event)"></div>

      <!--
        Read left to right: which app this is, the secondary way in (today's
        note), and last the primary action — the same order as AppHeader in
        the other apps. Every control is 32px so the row sits on one line.
        The sidebar starts about 300px wide, which fits the title and the
        split button but not "Today" in words as well; there it keeps its
        calendar icon, its name for screen readers and its tooltip, and the
        words come back once the sidebar is dragged wider.
        "New" and "from a template" are one split button: they are two ways
        of doing the same thing, and a separate icon beside the primary
        button made the primary look like it was in the middle of the row.
      -->
      <div class="@container h-10 flex-shrink-0 flex items-center gap-2 px-3 border-b border-border dark:border-border-dark" data-tauri-drag-region>
         <button type="button" @click="sidebar.showLeft.value = false" class="btn-icon !w-8 !h-8 md:hidden -ml-1" :title="$t('note.close_sidebar')" :aria-label="$t('note.close_sidebar')">
            <X class="w-4 h-4" aria-hidden="true" />
         </button>
         <h2 class="text-sm font-semibold text-text dark:text-text-dark whitespace-nowrap shrink-0">{{ $t('shell.apps.note') }}</h2>
         <div class="flex items-center gap-1.5 ml-auto shrink-0" @mousedown.stop>
           <button v-if="enableDailyNotes" type="button" @click="handleOpenDailyNote" class="h-8 px-2 flex items-center gap-1.5 rounded-lg text-sm font-medium text-text-secondary dark:text-text-secondary-dark hover:bg-surface-hover dark:hover:bg-surface-hover-dark hover:text-text dark:hover:text-text-dark transition-colors cursor-pointer focus-visible:outline-2 focus-visible:outline-accent" :title="$t('note.todays_daily_note')" :aria-label="$t('note.todays_daily_note')">
             <CalendarDays class="w-4 h-4" aria-hidden="true" />
             <span class="hidden @[22rem]:inline">{{ $t('note.today') }}</span>
           </button>
           <div class="flex items-stretch">
             <button type="button" @click="handleCreateNewNote" class="btn-primary !h-8 !px-3 !rounded-r-none" :title="$t('note.new_note')">
               <Plus class="w-4 h-4" aria-hidden="true" />
               <span>{{ $t('note.new_note') }}</span>
             </button>
             <button type="button" @click="openTemplatePicker('create')" class="btn-primary !h-8 !w-7 !px-0 !rounded-l-none border-l border-white/25" :aria-label="$t('note.templates.new_from_template_ellipsis')" :title="$t('note.templates.new_from_template_ellipsis')">
               <ChevronDown class="w-4 h-4" aria-hidden="true" />
             </button>
           </div>
         </div>
      </div>

      <div class="px-3 pt-3 pb-2 sticky top-0 bg-surface-alt dark:bg-surface-alt-dark z-10" @mousedown.stop>
          <div class="relative w-full">
            <Search class="absolute left-2.5 top-1/2 -translate-y-1/2 w-4 h-4 text-muted dark:text-muted-dark" />
            <input v-model="search.searchQuery.value" type="text" :placeholder="$t('note.search_placeholder')" class="w-full pl-8 pr-14 py-1.5 bg-white dark:bg-[#2c2c2c] border border-border dark:border-transparent mx-auto block rounded-md text-sm focus:outline-none focus:ring-1 focus:ring-black dark:focus:ring-white transition-shadow text-text dark:text-text-dark placeholder:text-gray-500 dark:placeholder:text-gray-500">
            <button v-if="search.searchQuery.value" @click="search.searchQuery.value = ''" class="absolute right-7 top-1/2 -translate-y-1/2 p-0.5 rounded-full hover:bg-gray-100 dark:hover:bg-[#3f3f46] text-gray-500 dark:text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors" :aria-label="$t('note.clear_search')" :title="$t('note.clear_search')">
              <X class="w-3.5 h-3.5" />
            </button>
            <button @click="search.isCaseSensitiveSearch.value = !search.isCaseSensitiveSearch.value" :class="['absolute right-2 top-1/2 -translate-y-1/2 p-0.5 rounded-sm transition-colors', search.isCaseSensitiveSearch.value ? 'bg-accent/10 text-accent dark:text-accent-dark' : 'text-gray-500 hover:text-gray-600 dark:hover:text-gray-300 hover:bg-gray-100 dark:hover:bg-[#3f3f46]']" :title="$t('note.match_case')">
              <CaseSensitive class="w-3.5 h-3.5" />
            </button>
          </div>
      </div>

      <div class="flex-1 overflow-y-auto" @mousedown.stop>
         <!-- Pinned Section -->
         <div class="mb-4" v-if="search.allPinnedNotes.value.length > 0">
             <div class="flex justify-between items-center px-4 mb-2 mt-3">
                 <span class="text-xs font-semibold text-muted dark:text-muted-dark uppercase tracking-wider">{{ $t('note.pinned_notes') }}</span>
                 <button @click="manager.openNoteManager('pinned', () => { sidebar.showLeft.value = false; })" class="text-xs text-accent dark:text-accent-dark hover:underline font-medium p-2 -m-2">{{ $t('note.show_all') }}</button>
             </div>
             <div class="px-2 space-y-0.5">
                 <NoteListItem v-for="note in search.topPinnedNotes.value" :key="note.id"
                    :note="note" :is-active="currentNoteId === note.id" :show-context-menu="activeContextMenu === note.id" :is-pinned-section="true"
                    @select="handleNoteSelect" @toggle-context="toggleContext"
                    @pin="togglePin" @open-window="openInNewWindow" @rename="rename.handleRenamePrompt($event, closeContextMenu)" @toggle-lock="lock.toggleNoteLock($event, closeContextMenu)" @history="openHistory" @save-as-template="saveAsTemplate" @delete="deleteNote"
                 />
                 <button v-if="search.allPinnedNotes.value.length > 5" @click="manager.openNoteManager('pinned', () => { sidebar.showLeft.value = false; })" class="w-full text-center py-2.5 mt-2 text-xs font-medium text-accent dark:text-accent-dark hover:bg-accent/10 rounded-lg transition-colors">
                     {{ $t('note.show_more', { count: search.allPinnedNotes.value.length - 5 }) }}
                 </button>
             </div>
         </div>

         <!-- Tags Section -->
         <div class="mb-4">
             <div class="flex justify-between items-center px-4 mb-2 mt-2">
                 <span class="text-xs font-semibold text-muted dark:text-muted-dark uppercase tracking-wider">{{ $t('note.top_tags') }}</span>
                 <button @click="manager.openNoteManager('tags', () => { sidebar.showLeft.value = false; })" class="text-xs text-accent dark:text-accent-dark hover:underline font-medium p-2 -m-2">{{ $t('note.show_all') }}</button>
             </div>
             <div class="px-2 space-y-0.5" v-if="tags.topTags.value.length > 0">
                 <div v-for="tag in tags.topTags.value" :key="tag.name"
                      @click="tags.toggleTagSelection(tag.name)"
                      class="w-full flex items-center justify-between px-3 py-1.5 rounded-lg text-sm transition-colors cursor-pointer group"
                      :class="tags.selectedTags.value.has(tag.name) ? 'bg-black/5 dark:bg-white/10' : 'hover:bg-gray-100 dark:hover:bg-surface-hover-dark text-text-secondary dark:text-text-secondary-dark'">
                      <div class="flex items-center gap-2 truncate">
                          <Hash class="w-3.5 h-3.5 opacity-70 group-hover:text-black dark:group-hover:text-white transition-colors" />
                          <span class="truncate select-none group-hover:text-black dark:group-hover:text-white transition-colors">{{ tag.name.split('/').pop() }}</span>
                      </div>
                      <span class="text-xs opacity-50 bg-black/5 dark:bg-white/10 px-1.5 py-0.5 rounded-full min-w-[20px] text-center">{{ tag.count }}</span>
                 </div>
             </div>
             <div v-else class="text-center p-4 text-xs text-gray-500 dark:text-gray-400">{{ $t('note.no_tags_found') }}</div>
         </div>

         <!-- Recent Notes -->
         <div class="mb-4">
             <div class="flex justify-between items-center px-4 mb-2 mt-2">
                 <span class="text-xs font-semibold text-muted dark:text-muted-dark uppercase tracking-wider">{{ $t('note.recent_notes') }}</span>
                 <button @click="manager.openNoteManager('notes', () => { sidebar.showLeft.value = false; })" class="text-xs text-accent dark:text-accent-dark hover:underline font-medium p-2 -m-2">{{ $t('note.show_all') }}</button>
             </div>
             <div class="px-2 space-y-0.5">
                 <NoteListItem v-for="note in search.recentNotes.value" :key="note.id"
                    :note="note" :is-active="currentNoteId === note.id" :show-context-menu="activeContextMenu === note.id" :is-pinned-section="false"
                    @select="handleNoteSelect" @toggle-context="toggleContext"
                    @pin="togglePin" @open-window="openInNewWindow" @rename="rename.handleRenamePrompt($event, closeContextMenu)" @toggle-lock="lock.toggleNoteLock($event, closeContextMenu)" @history="openHistory" @save-as-template="saveAsTemplate" @delete="deleteNote"
                 />
             </div>
             <div v-if="search.recentNotes.value.length === 0" class="p-8 text-center text-sm text-text-secondary dark:text-text-secondary-dark">
               {{ $t('note.no_notes_match') }}
             </div>
         </div>
      </div>
    </aside>

    <!-- Main Area: Editor / Manager -->
    <main class="flex-1 flex flex-col bg-base dark:bg-base-dark min-w-[300px] max-md:min-w-0" @mousedown.stop>
      <template v-if="manager.viewMode.value === 'editor'">
          <div v-if="!isFloatingView" class="h-10 flex-shrink-0 w-full flex items-center justify-between px-4" data-tauri-drag-region>
            <div class="flex items-center gap-1">
              <NavButtons />
              <button type="button" @click="sidebar.showLeft.value = !sidebar.showLeft.value" class="btn-icon" :title="$t('note.toggle_sidebar')" :aria-label="$t('note.toggle_sidebar')" :aria-expanded="sidebar.showLeft.value">
                <PanelLeftClose v-if="sidebar.showLeft.value" class="w-4 h-4" aria-hidden="true" />
                <PanelLeft v-else class="w-4 h-4" aria-hidden="true" />
              </button>
            </div>
            <!--
              One row of the same 36px icon buttons, all grey. A setting that is
              on shows as pressed (a soft fill), not in the accent colour: the
              accent is for the one thing to do on a screen, and a row of
              toggles where one is purple reads as a mistake. The panel toggle
              sits after a divider, because it opens a pane rather than
              changing the note.
            -->
            <div v-if="currentNoteId && manager.viewMode.value === 'editor'" class="flex items-center gap-1">
              <!-- Show or hide the formatting row; remembered, and also in Settings → Notes. -->
              <button v-if="!zenMode" type="button" @click="noteToolbarVisible = !noteToolbarVisible" class="btn-icon" :class="noteToolbarVisible ? 'bg-surface-hover dark:bg-surface-hover-dark text-text dark:text-text-dark' : ''" :title="noteToolbarVisible ? $t('note.hide_toolbar') : $t('note.show_toolbar')" :aria-label="$t('note.formatting_toolbar')" :aria-pressed="noteToolbarVisible">
                <Type class="w-4 h-4" aria-hidden="true" />
              </button>
              <button type="button" @click="zenMode = !zenMode" class="btn-icon hidden md:inline-flex" :title="zenMode ? $t('note.exit_zen_mode') : $t('note.zen_mode')" :aria-label="zenMode ? $t('note.exit_zen_mode') : $t('note.zen_mode')">
                <Monitor class="w-4 h-4" aria-hidden="true" />
              </button>
              <button type="button" @click="editorFullWidth = !editorFullWidth" class="btn-icon hidden md:inline-flex" :title="editorFullWidth ? $t('note.standard_width') : $t('note.full_width')" :aria-label="editorFullWidth ? $t('note.standard_width') : $t('note.full_width')">
                <span v-if="editorFullWidth" class="flex items-center gap-px" aria-hidden="true">
                  <ArrowRight class="w-3 h-3" />
                  <ArrowLeft class="w-3 h-3" />
                </span>
                <span v-else class="flex items-center gap-px" aria-hidden="true">
                  <ArrowLeft class="w-3 h-3" />
                  <ArrowRight class="w-3 h-3" />
                </span>
              </button>
              <button type="button" @click="openHistory(currentNoteId)" class="btn-icon hidden md:inline-flex" :title="$t('note.history_title')" :aria-label="$t('note.history_title')">
                <History class="w-4 h-4" aria-hidden="true" />
              </button>
              <button type="button" @click="noteExport.exportModalVisible.value = true" class="btn-icon hidden md:inline-flex" :title="$t('note.export_note')" :aria-label="$t('note.export_note')">
                <Download class="w-4 h-4" aria-hidden="true" />
              </button>
              <!-- The graph and linked mentions are power tools; simple mode leaves out the way in. -->
              <template v-if="!simpleMode">
                <span class="w-px h-5 mx-1 bg-border dark:bg-border-dark" aria-hidden="true" />
                <button type="button" @click="sidebar.showRight.value = !sidebar.showRight.value" class="btn-icon" :title="$t('note.toggle_right_sidebar')" :aria-label="$t('note.toggle_right_sidebar')" :aria-expanded="sidebar.showRight.value">
                  <PanelRightClose v-if="sidebar.showRight.value" class="w-4 h-4" aria-hidden="true" />
                  <PanelRight v-else class="w-4 h-4" aria-hidden="true" />
                </button>
              </template>
            </div>
          </div>

          <div v-if="zenMode" class="absolute top-4 right-4 z-50">
             <button @click="zenMode = false" class="p-2 bg-black/10 dark:bg-white/10 hover:bg-black/20 dark:hover:bg-white/20 rounded-full text-gray-500 dark:text-gray-400 hover:text-black dark:hover:text-white transition-all shadow-sm backdrop-blur-md opacity-0 hover:opacity-100 group-hover:opacity-100 group-focus-within:opacity-100 pointer-coarse:opacity-100" :title="$t('note.exit_zen_mode')">
                <Monitor class="w-4 h-4" />
             </button>
          </div>

          <div v-else-if="!isFloatingView && manager.viewMode.value !== 'editor'" class="h-8 flex-shrink-0 w-full z-50 bg-base dark:bg-base-dark" data-tauri-drag-region></div>

          <template v-if="tabs.activeTabs.value.length > 0">
            <template v-for="tabId in tabs.activeTabs.value" :key="tabId">
              <div v-show="currentNoteId === tabId" class="flex-1 overflow-y-auto w-full relative">
                <div v-if="tabs.tabContents.value[tabId] === undefined" class="absolute inset-0 flex items-center justify-center bg-base dark:bg-base-dark">
                    <div class="w-8 h-8 rounded-full border-2 border-gray-200 border-t-gray-400 animate-spin"></div>
                </div>
                <div v-else class="px-4 md:px-12 pb-12 mx-auto w-full cursor-text transition-all duration-300" :class="editorFullWidth ? 'max-w-none' : 'max-w-4xl'">
                <div class="mb-4 pt-4">
                   <div class="flex gap-2 mb-4 flex-wrap items-center">
                      <span v-for="tag in notes.find(n => n.id === tabId)?.tags" :key="tag" class="text-xs px-2 py-1 rounded-md bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-300 flex items-center gap-1 group/tag">
                          <Hash class="w-3 h-3 opacity-50"/>
                          {{ tag }}
                          <button @click="tags.removeTag(tag)" class="opacity-0 group-hover/tag:opacity-100 group-focus-within/tag:opacity-100 pointer-coarse:opacity-100 hover:text-red-500 transition-opacity ml-1 p-0.5" :aria-label="$t('note.remove_tag', { tag })" :title="$t('note.remove_tag', { tag })"><X class="w-3 h-3"/></button>
                       </span>
                       <div class="relative flex items-center">
                          <Plus class="w-3 h-3 absolute left-1.5 text-gray-500 dark:text-gray-400" />
                          <input v-model="tags.newTagInput.value" @keydown="tags.addTag" :placeholder="$t('note.add_tag')" class="text-xs bg-transparent border border-dashed border-gray-300 dark:border-gray-600 rounded-md py-1 pl-5 pr-2 w-24 focus:w-32 focus:outline-none focus:border-gray-400 transition-all text-text dark:text-text-dark" />
                       </div>
                   </div>
                   <div class="w-full grid grow-wrap" :data-replicated-value="(tabs.focusedTitles.value[tabId] !== undefined ? tabs.focusedTitles.value[tabId] : notes.find(n => n.id === tabId)?.title) || ''">
                     <textarea class="note-title-input w-full text-4xl font-bold bg-transparent border-none outline-none text-text dark:text-text-dark placeholder:text-gray-300 dark:placeholder:text-gray-700 resize-none overflow-hidden col-start-1 row-start-1 h-full"
                       rows="1"
                       :value="tabs.focusedTitles.value[tabId] !== undefined ? tabs.focusedTitles.value[tabId] : notes.find(n => n.id === tabId)?.title"
                       @focus="tabs.focusedTitles.value[tabId] = ($event.target as HTMLTextAreaElement).value"
                       @input="tabs.focusedTitles.value[tabId] = ($event.target as HTMLTextAreaElement).value"
                       @blur="rename.renameTopTitle"
                       @keydown.enter.prevent="rename.renameTopTitle"
                       :placeholder="$t('note.note_title')"></textarea>
                   </div>
                </div>
                <div class="mt-4 pb-20 w-full text-text dark:text-text-dark" :class="{'zen-editor-container': zenMode && !editorFullWidth}">
                   <TiptapEditor :ref="(el) => { const refs = save.editorRefs.value || save.editorRefs; if (el) refs[tabId] = el; else delete refs[tabId]; }" :model-value="tabs.tabContents.value[tabId]" :vault-path="vaultPath" :zen-mode="zenMode" :current-note-id="tabId" :toolbar="!zenMode && noteToolbarVisible" templates @update:model-value="(val: string) => save.onEditorUpdate(val, tabId)" @open-internal-note="handleOpenInternalNote" @insert-template="openTemplatePicker('insert')" />
                </div>
                </div>
              </div>
            </template>
          </template>
          <div v-else class="flex-1 flex items-center justify-center text-text-secondary dark:text-text-secondary-dark">
            <div class="text-center">
              <FileText class="w-12 h-12 mx-auto mb-4 opacity-20" />
              <p>{{ $t('note.select_to_start') }}</p>
            </div>
          </div>
      </template>
      <template v-else-if="manager.viewMode.value === 'manager'">
          <div class="flex-1 flex flex-col bg-base dark:bg-base-dark h-full relative z-0 overflow-y-auto">
             <div class="flex items-center justify-between px-6 h-10 border-b border-border dark:border-border-dark shrink-0 sticky top-0 bg-base dark:bg-base-dark z-10" data-tauri-drag-region>
                <div class="flex items-center gap-3">
                   <button @click="manager.viewMode.value = 'editor'" class="p-1.5 rounded-md hover:bg-gray-100 dark:hover:bg-[#2c2c2c] transition-colors text-gray-500 dark:text-gray-400" :aria-label="$t('note.back_to_editor')" :title="$t('note.back_to_editor')">
                      <ArrowLeft class="w-5 h-5" />
                   </button>
                   <h1 class="text-xl font-bold text-text dark:text-text-dark flex items-center gap-2">
                      {{ manager.managerFilter.value === 'tags' && !manager.managerSearchQuery.value ? $t('note.all_tags') : manager.managerSearchQuery.value ? $t('note.search_results') : manager.managerFilter.value === 'notes' || !manager.managerFilter.value ? $t('note.all_notes') : manager.managerFilter.value === 'pinned' ? $t('note.pinned_notes') : $t('note.tag_heading', { tag: manager.managerFilter.value.split('/').pop() }) }}
                      <span class="text-[12px] font-medium px-2 py-0.5 mt-0.5 rounded-full bg-gray-100 dark:bg-[#333] text-gray-500 dark:text-gray-400">
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
                     class="ml-auto flex items-center gap-2 px-3 py-1.5 text-[13px] rounded-lg text-gray-500 dark:text-gray-400 hover:text-text dark:hover:text-text-dark hover:bg-gray-100 dark:hover:bg-[#2c2c2c] transition-colors"
                     :title="$t('note.duplicates_hint')"
                   >
                     <Copy class="w-4 h-4" />
                     <span class="hidden sm:inline">{{ $t('note.duplicates_title') }}</span>
                   </button>
                </div>
             </div>

             <div class="flex-1 flex flex-col p-8 md:p-12 lg:p-16 w-full max-w-6xl mx-auto">
                 <div class="relative w-full mb-8">
                   <Search class="absolute left-4 top-1/2 -translate-y-1/2 w-5 h-5 text-muted dark:text-muted-dark" />
                   <input v-model="manager.managerSearchQuery.value" type="text" :placeholder="$t('note.search_manager_placeholder')" class="w-full pl-12 pr-20 py-3 bg-white dark:bg-[#1a1a1a] border border-border dark:border-border-dark rounded-xl text-base shadow-sm focus:outline-none focus:ring-2 focus:ring-accent/50 transition-shadow placeholder:text-gray-500 dark:placeholder:text-gray-400 manager-search-input">
                   <button v-if="manager.managerSearchQuery.value" @click="manager.managerSearchQuery.value = ''" class="absolute right-12 top-1/2 -translate-y-1/2 p-1.5 rounded-full hover:bg-gray-100 dark:hover:bg-[#2c2c2c] text-gray-500 dark:text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors" :aria-label="$t('note.clear_search')" :title="$t('note.clear_search')">
                     <X class="w-4 h-4" />
                   </button>
                   <button @click="search.isCaseSensitiveSearch.value = !search.isCaseSensitiveSearch.value" :class="['absolute right-3 top-1/2 -translate-y-1/2 p-1.5 rounded-md transition-colors', search.isCaseSensitiveSearch.value ? 'bg-accent/10 text-accent dark:text-accent-dark' : 'text-gray-500 hover:text-gray-600 dark:hover:text-gray-300 hover:bg-gray-100 dark:hover:bg-[#2c2c2c]']" :title="$t('note.match_case')">
                     <CaseSensitive class="w-4 h-4" />
                   </button>
                 </div>

                 <!-- Tags View -->
                 <div v-if="manager.managerFilter.value === 'tags' && !manager.managerSearchQuery.value" class="w-full">
                    <div class="flex flex-wrap gap-3">
                       <div v-for="tag in tags.allTags.value" :key="tag.name" @click="manager.managerFilter.value = tag.name" class="px-4 py-2 bg-white dark:bg-[#1f1f1f] border border-border dark:border-border-dark rounded-lg cursor-pointer hover:border-[#d4d4d8] dark:hover:border-[#444] transition-all flex items-center gap-2 group">
                          <Hash class="w-4 h-4 text-gray-500 group-hover:text-text dark:group-hover:text-white transition-colors" />
                          <span class="font-medium text-text dark:text-text-dark">{{ tag.name.split('/').pop() }}</span>
                          <span class="text-xs bg-gray-100 dark:bg-[#2c2c2c] px-2 py-0.5 rounded text-gray-500 dark:text-gray-400">{{ tag.count }}</span>
                       </div>
                    </div>
                 </div>

                 <!-- Notes Table View -->
                 <div v-else class="w-full">
                   <!--
                     Only on screen while something is ticked. A bar that is
                     always there, greyed out, teaches people to stop reading it.
                   -->
                   <div v-if="selection.active.value" class="mb-3 flex items-center gap-3 px-4 py-2.5 rounded-xl bg-accent/10 border border-accent/30">
                      <span class="text-[13px] font-medium text-text dark:text-text-dark">{{ $t('note.selected_count', { count: selection.count.value }) }}</span>
                      <button @click="deleteSelected" class="ml-auto flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-[13px] font-medium text-white bg-red-500 hover:bg-red-600 transition-colors">
                         <Trash2 class="w-3.5 h-3.5" />
                         {{ $t('note.delete_selected') }}
                      </button>
                      <button @click="selection.clear()" class="px-3 py-1.5 rounded-lg text-[13px] font-medium text-text dark:text-text-dark hover:bg-accent/10 transition-colors">
                         {{ $t('note.clear_selection') }}
                      </button>
                   </div>
                   <div class="bg-white dark:bg-[#252525] border border-border dark:border-[#333] rounded-xl overflow-hidden shadow-sm">
                      <table class="w-full text-left border-collapse">
                         <thead>
                            <tr class="bg-gray-50 dark:bg-[#1a1a1a] border-b border-border dark:border-[#333]">
                               <th class="py-2.5 px-4 w-8">
                                  <input
                                     type="checkbox"
                                     class="w-3.5 h-3.5 align-middle accent-accent cursor-pointer"
                                     :checked="selection.allVisibleSelected(visibleManagerIds)"
                                     :indeterminate="selection.someVisibleSelected(visibleManagerIds)"
                                     :aria-label="$t('note.select_all_on_page')"
                                     :title="$t('note.select_all_on_page')"
                                     @click="selection.toggleAll(visibleManagerIds)"
                                  />
                               </th>
                               <th class="py-2.5 px-4 text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase w-5/12">{{ $t('note.title_col') }}</th>
                               <th class="py-2.5 px-4 text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase">{{ $t('note.tags_col') }}</th>
                               <th class="py-2.5 px-4 text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase whitespace-nowrap text-right">{{ $t('note.modified_col') }}</th>
                               <th class="py-2.5 px-4 text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase w-12 text-center">{{ $t('note.action_col') }}</th>
                            </tr>
                         </thead>
                         <tbody class="divide-y divide-border dark:divide-[#333] text-sm">
                            <tr v-for="note in manager.managerPaginatedNotes.value" :key="note.id" @click="handleManagerRowClick(note.id, $event)" class="cursor-pointer transition-colors group"
                                :class="selection.isSelected(note.id) ? 'bg-accent/10' : 'hover:bg-gray-50 dark:hover:bg-surface-hover-dark'">
                               <!--
                                 One cell, two things. The tick takes over from
                                 the icon on hover, or as soon as anything is
                                 selected — so the column costs nothing while
                                 the reader is only reading, and is already
                                 under the pointer when they are not.
                               -->
                               <td class="py-3 px-4 w-8" @click.stop>
                                  <div class="relative w-3.5 h-3.5">
                                     <Pin v-if="note.pinned" class="w-3.5 h-3.5 text-orange-500 fill-orange-500/20 transition-opacity group-hover:opacity-0 group-focus-within:opacity-0 pointer-coarse:opacity-0" :class="{ '!opacity-0': selection.active.value }" />
                                     <FileText v-else class="w-3.5 h-3.5 text-gray-500 dark:text-gray-400 opacity-50 transition-opacity group-hover:opacity-0 group-focus-within:opacity-0 pointer-coarse:opacity-0" :class="{ '!opacity-0': selection.active.value }" />
                                     <input
                                        type="checkbox"
                                        class="absolute inset-0 w-3.5 h-3.5 accent-accent cursor-pointer opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100 pointer-coarse:opacity-100"
                                        :class="{ '!opacity-100': selection.active.value }"
                                        :checked="selection.isSelected(note.id)"
                                        :aria-label="$t('note.select_note')"
                                        @click.stop="selection.toggle(note.id, visibleManagerIds, $event.shiftKey)"
                                     />
                                  </div>
                               </td>
                               <td class="py-3 px-4 font-medium text-text dark:text-text-dark max-w-[250px] truncate">{{ note.title || $t('note.untitled_note') }}<span v-if="isTemplatePath(note.id)" class="ml-2 text-xs px-1.5 rounded bg-accent/10 text-accent dark:bg-accent-dark/15 dark:text-accent-dark font-medium">{{ $t('note.templates.badge') }}</span></td>
                               <td class="py-3 px-4">
                                  <div class="flex flex-wrap gap-1" v-if="note.tags.length">
                                     <span v-for="tag in note.tags.slice(0, 3)" :key="tag" class="text-xs px-1.5 py-0.5 rounded bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-300">{{ tag.split('/').pop() }}</span>
                                     <span v-if="note.tags.length > 3" class="text-xs px-1.5 py-0.5 rounded bg-gray-100 dark:bg-gray-800 text-gray-500 dark:text-gray-400">+{{ note.tags.length - 3 }}</span>
                                  </div>
                                  <span v-else class="text-xs text-gray-500 dark:text-gray-400 italic">{{ $t('note.no_tags') }}</span>
                               </td>
                               <td class="py-3 px-4 text-xs text-gray-500 dark:text-gray-400 whitespace-nowrap text-right">{{ formatDate(note.date) }}</td>
                               <td class="py-3 px-4 w-12 text-center" @click.stop>
                                  <div class="relative flex justify-center">
                                     <button @click="(e) => toggleContext('manager_'+note.id, e)" :aria-label="$t('note.more_actions')" :title="$t('note.more_actions')" class="p-1 rounded md:opacity-0 opacity-100 group-hover:opacity-100 group-focus-within:opacity-100 pointer-coarse:opacity-100 hover:bg-gray-200 dark:hover:bg-[#444] transition">
                                        <MoreVertical class="w-4 h-4 text-gray-500 dark:text-gray-400" />
                                     </button>
                                     <NoteContextMenu v-if="activeContextMenu === 'manager_'+note.id" :note-id="note.id" :is-pinned="note.pinned" variant="manager"
                                        @pin="togglePin($event); activeContextMenu = null;"
                                        @history="openHistory"
                                        @save-as-template="saveAsTemplate"
                                        @delete="deleteNote($event); activeContextMenu = null;"
                                     />
                                  </div>
                               </td>
                            </tr>
                            <tr v-if="manager.managerFilteredNotes.value.length === 0">
                               <td colspan="5" class="py-12 text-center text-gray-500 dark:text-gray-400">{{ $t('note.no_notes_found') }}</td>
                            </tr>
                         </tbody>
                      </table>
                   </div>

                   <!-- Pagination Controls -->
                   <div v-if="manager.managerTotalPages.value > 1" class="mt-4 flex items-center justify-between text-[13px] text-gray-500 dark:text-gray-400">
                      <div>{{ $t('note.pagination_range', { from: (manager.managerCurrentPage.value - 1) * manager.managerItemsPerPage + 1, to: Math.min(manager.managerCurrentPage.value * manager.managerItemsPerPage, manager.managerFilteredNotes.value.length), total: manager.managerFilteredNotes.value.length }) }}</div>
                      <div class="flex items-center gap-2">
                         <button @click="manager.managerPrevPage()" :disabled="manager.managerCurrentPage.value === 1" class="px-3 py-1.5 rounded-lg border border-border dark:border-[#333] hover:bg-gray-50 dark:hover:bg-[#2c2c2c] disabled:opacity-50 disabled:cursor-not-allowed transition-colors text-text dark:text-text-dark">{{ $t('note.previous') }}</button>
                         <span class="font-medium px-2 text-text dark:text-text-dark">{{ $t('note.page_of', { page: manager.managerCurrentPage.value, total: manager.managerTotalPages.value }) }}</span>
                         <button @click="manager.managerNextPage()" :disabled="manager.managerCurrentPage.value === manager.managerTotalPages.value" class="px-3 py-1.5 rounded-lg border border-border dark:border-[#333] hover:bg-gray-50 dark:hover:bg-[#2c2c2c] disabled:opacity-50 disabled:cursor-not-allowed transition-colors text-text dark:text-text-dark">{{ $t('note.next') }}</button>
                      </div>
                   </div>
                 </div>
             </div>
          </div>
      </template>
    </main>

    <!-- Right Sidebar: Graph & Backlinks -->
    <aside v-if="currentNoteId && !isFloatingView && manager.viewMode.value === 'editor' && !simpleMode" v-show="sidebar.showRight.value" class="shrink-0 relative border-l border-border dark:border-border-dark bg-surface-alt dark:bg-surface-alt-dark flex flex-col overflow-hidden max-md:!w-full max-md:absolute max-md:inset-0 max-md:z-[60]" :style="{ width: sidebar.rightWidth.value + 'px' }">
      <div class="hidden md:block absolute top-0 left-0 w-1.5 h-full cursor-col-resize hover:bg-black/10 dark:hover:bg-white/10 z-10 opacity-0 hover:opacity-100 transition-opacity" @mousedown.stop="sidebar.startDragRight"></div>
      <div class="h-10 flex-shrink-0 flex items-center px-4 border-b border-border dark:border-border-dark" data-tauri-drag-region>
          <Globe class="w-4 h-4 text-gray-500 dark:text-gray-400 mr-2" />
          <span class="font-bold text-xs tracking-wider text-gray-500 dark:text-gray-400 uppercase mt-0.5">{{ $t('note.graph_view') }}</span>
          <button @click="sidebar.showRight.value = false" class="p-1 ml-auto rounded-md hover:bg-gray-200 dark:hover:bg-gray-800 text-gray-500 dark:text-gray-400 transition-colors" :aria-label="$t('note.close_right_sidebar')" :title="$t('note.close_right_sidebar')">
             <X class="w-3.5 h-3.5" />
          </button>
      </div>
      <div class="h-1/2 border-b border-border dark:border-border-dark overflow-hidden">
          <NoteGraph v-if="activeNote" :current-note-id="currentNoteId || ''" :current-note-title="activeNote.title || $t('note.untitled_note')" :tags="activeNote.tags || []" :outgoing-links="backlinks.currentOutgoingLinks.value" :backlinks="backlinks.currentBacklinks.value" :all-notes="notes" @open-note="handleOpenInternalNote" />
      </div>
      <div class="h-10 flex-shrink-0 flex items-center px-4 border-b border-border dark:border-border-dark">
          <span class="font-bold text-xs tracking-wider text-muted dark:text-muted-dark uppercase mt-0.5">{{ $t('note.linked_mentions_count', { count: backlinks.currentBacklinks.value.length }) }}</span>
      </div>
      <div class="flex-1 overflow-y-auto p-2 space-y-1">
          <div v-if="backlinks.currentBacklinks.value.length === 0" class="text-[13px] text-gray-500 dark:text-gray-400 text-center py-4">{{ $t('note.no_linked_mentions') }}</div>
          <div v-for="bl in backlinks.currentBacklinks.value" :key="bl.id" @click="handleOpenInternalNote({ id: bl.id, type: bl.node_type })" class="p-3 border border-transparent rounded-lg cursor-pointer hover:bg-white/50 dark:hover:bg-[#252525] hover:border-border dark:hover:border-[#2f2f2f] transition-all group">
            <h5 class="flex items-center gap-2 pr-2">
                <Calendar v-if="bl.node_type === 'event'" class="w-3.5 h-3.5 text-rose-500 shrink-0 opacity-80 group-hover:opacity-100 transition-colors"/>
                <CheckSquare v-else-if="bl.node_type === 'task'" class="w-3.5 h-3.5 text-emerald-500 shrink-0 opacity-80 group-hover:opacity-100 transition-colors"/>
                <FileText v-else class="w-3.5 h-3.5 text-gray-500 shrink-0 opacity-80 group-hover:text-accent dark:group-hover:text-accent-dark group-hover:opacity-100 transition-colors"/>
                <span class="text-[13px] font-medium text-text dark:text-text-dark truncate">{{ bl.title }}</span>
                <span v-if="bl.node_type === 'event' && bl.properties && bl.properties.start_at" class="ml-auto text-xs text-gray-500 dark:text-gray-400 font-medium tracking-wider whitespace-nowrap">{{ (bl.properties.start_at as string).split('T')[0] }}</span>
                <button v-if="(bl as any)._is_outgoing_project" @click.stop="backlinks.unlinkProject(bl.id, bl.title)" class="ml-auto p-1.5 -mr-1.5 text-gray-500 dark:text-gray-400 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-900/30 rounded-md opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 pointer-coarse:opacity-100 transition-all" :title="$t('note.unlink_project')">
                   <X class="w-3.5 h-3.5" />
                </button>
            </h5>
          </div>
      </div>
    </aside>

    <!-- Rename Modal -->
    <AppDialog :show="rename.renameModal.value.show" labelledby="note-rename-title" size="sm" panel-class="p-6" @close="rename.renameModal.value.show = false">
      <h3 id="note-rename-title" class="text-base font-semibold text-text dark:text-text-dark mb-4">{{ $t('note.rename_note') }}</h3>
      <input v-model="rename.renameModal.value.value" type="text" class="w-full px-3 py-2 rounded-lg border border-border-subtle dark:border-[#444] bg-white dark:bg-surface-dark text-text dark:text-text-dark text-sm focus:outline-none focus:ring-2 focus:ring-black/10 dark:focus:ring-white/20" @keydown.enter="rename.confirmRename" autofocus :aria-label="$t('note.rename_note')" />
      <div class="flex justify-end gap-2 mt-4">
        <button @click="rename.renameModal.value.show = false" class="btn-secondary">{{ $t('note.cancel') }}</button>
        <button @click="rename.confirmRename" class="btn-primary">{{ $t('note.rename') }}</button>
      </div>
    </AppDialog>

    <!-- Templates -->
    <TemplatePickerModal
      v-if="templatePicker.show"
      :show="templatePicker.show"
      :mode="templatePicker.mode"
      :user-templates="userTemplates"
      :load-body="readNoteBody"
      @close="templatePicker.show = false"
      @choose="onTemplateChosen"
    />
    <Teleport to="body">
      <div v-if="templateSavedMessage" class="fixed bottom-5 left-1/2 -translate-x-1/2 z-[300] flex items-center gap-2.5 px-3.5 py-2.5 rounded-xl shadow-2xl max-w-[420px] bg-surface dark:bg-surface-dark border border-border dark:border-border-dark" role="status" aria-live="polite">
        <LayoutTemplate class="w-3.5 h-3.5 text-gray-500 dark:text-gray-400 shrink-0" />
        <span class="text-[13px] text-text dark:text-text-dark truncate min-w-0">{{ templateSavedMessage }}</span>
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
    <UndoToast
      :show="del.pending.value !== null"
      :restart-key="undoKey"
      :message="undoMessage"
      :hint="$t('common.in_trash_hint')"
      :undo-label="$t('common.undo')"
      :seconds="NOTE_UNDO_WINDOW_MS / 1000"
      @undo="del.undoDelete"
      @pause="del.pause"
      @resume="del.resume"
    />

    <!-- Version History -->
    <NoteHistoryModal
      v-if="historyNoteId"
      :vault-path="vaultPath"
      :note-id="historyNoteId"
      :note-title="historyNote?.title || $t('note.untitled_note')"
      :before-restore="saveBeforeRestore"
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
      :title="$t(lock.noteLockTitle.value)"
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
