import { ref, computed, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { open, ask, message } from '@tauri-apps/plugin-dialog';
import { listen } from '@tauri-apps/api/event';
import { i18n } from '../../../i18n';

// The repo's convention for composables: `useI18n()` needs a component setup
// context, and this is called from one — but the store outlives any single
// call site, so it takes the global instance like every other composable here.
const t = i18n.global.t;
import { logger } from '../../../utils/logger';

export interface FileMetadata {
  id: string;
  path: string;
  filename: string;
  extension: string;
  size: number;
  created_at: string;
  modified_at: string;
  tags: string[];
  people: string[];
  source_type: string;
  /** What the camera wrote, for pictures that carried it. */
  camera?: string | null;
  shot_at?: string | null;
  width?: number | null;
  height?: number | null;
  /** A colour mark — a rating without words. */
  label?: string | null;
}

export interface FileSource {
  id: string;
  path: string;
  name: string;
}

export interface DuplicateGroup {
  filename: string;
  extension: string;
  size: number;
  count: number;
  files: FileMetadata[];
  wasted_bytes: number;
}

export interface DuplicateReport {
  groups: DuplicateGroup[];
  total_groups: number;
  total_duplicate_files: number;
  total_wasted_bytes: number;
}

export interface ScanProgress {
  source: string;
  /** Files reached so far in this folder. */
  indexed: number;
  /** Of those, how many had to be read rather than recognised from the cache. */
  hashed: number;
  cancelled: boolean;
}

export interface TextProgress {
  /** Documents read in this run. */
  done: number;
  /** Documents still waiting. */
  remaining: number;
  cancelled: boolean;
}

export interface FileReference {
  node_id: string;
  node_type: string;
  title: string;
  /** `attachment` when the note shows the file, a link type when it points at it. */
  edge_type: string;
}

export function useFileStore(vaultPath: () => string) {
  const sources = ref<FileSource[]>([]);
  const isLoading = ref(true);
  const isScanning = ref(false);

  const activeSourceId = ref<string | null>(null);
  const activeType = ref<string | null>(null);
  const activeTag = ref<string | null>(null);
  const activeCamera = ref<string | null>(null);
  const searchQuery = ref('');

  /**
   * How the list is ordered.
   *
   * There was no choice before: the list came back in one order and stayed in
   * it. Browsing a photo library means asking "what is biggest", "what is
   * newest", "what did I shoot that day" — none of which a fixed order answers.
   */
  const sortBy = ref<'modified' | 'name' | 'size' | 'shot' | 'pixels'>('modified');
  const sortDescending = ref(true);

  /** Which copies the user has picked out, keyed by path — one row, one copy. */
  const selection = ref<Set<string>>(new Set());
  const lastPicked = ref<string | null>(null);

  const isSelected = (file: FileMetadata) => selection.value.has(file.path);
  const clearSelection = () => {
    selection.value = new Set();
    lastPicked.value = null;
    allMatchingIds.value = null;
  };

  /**
   * Extend a selection the way a file manager does: a plain click replaces,
   * cmd/ctrl toggles one, shift takes everything in between.
   */
  const selectFile = (file: FileMetadata, modifiers: { toggle?: boolean; range?: boolean } = {}) => {
    const visible = loadedFiles.value;
    if (modifiers.range && lastPicked.value) {
      const from = visible.findIndex(f => f.path === lastPicked.value);
      const to = visible.findIndex(f => f.path === file.path);
      if (from !== -1 && to !== -1) {
        const [lo, hi] = from < to ? [from, to] : [to, from];
        const next = new Set(selection.value);
        for (const f of visible.slice(lo, hi + 1)) next.add(f.path);
        selection.value = next;
        return;
      }
    }
    allMatchingIds.value = null;
    if (modifiers.toggle) {
      const next = new Set(selection.value);
      if (next.has(file.path)) next.delete(file.path); else next.add(file.path);
      selection.value = next;
    } else {
      selection.value = new Set([file.path]);
    }
    lastPicked.value = file.path;
  };

  /**
   * Select everything the current filter matches — not merely what is loaded.
   *
   * Kept as identities rather than paths because that is what the list no
   * longer holds: most of it has never been fetched. Tagging works on
   * identities anyway, so this is also the shape the bulk command wants.
   */
  const allMatchingIds = ref<string[] | null>(null);

  const selectAllMatching = async () => {
    try {
      allMatchingIds.value = await invoke<string[]>('query_file_ids', { filter: currentFilter() });
      selection.value = new Set(loadedFiles.value.map(f => f.path));
    } catch (e) {
      logger.error("Failed to select everything", e);
    }
  };

  /** How many files the current selection covers. */
  const selectionSize = computed(() =>
    allMatchingIds.value ? allMatchingIds.value.length : selection.value.size
  );

  /** The selected rows, as files. */
  const selectedFiles = computed(() => loadedFiles.value.filter(f => selection.value.has(f.path)));

  /**
   * Tag everything selected in one call.
   *
   * Sends identities rather than paths, deduplicated: two copies of one photo
   * are one item, and tagging both would ask the backend to do the same work
   * twice and write the same vault file twice.
   */
  const tagSelection = async (add: string[], remove: string[] = []) => {
    const ids = allMatchingIds.value ?? [...new Set(selectedFiles.value.map(f => f.id))];
    if (ids.length === 0) return 0;
    try {
      const changed = await invoke<number>('bulk_tag_files', {
        vaultPath: vaultPath(), nodeIds: ids, add, remove,
      });
      await fetchFiles();
      return changed;
    } catch (e) {
      logger.error("Failed to tag selection", e);
      return 0;
    }
  };

  /** Colour marks, ordered so they read as a scale rather than a set. */
  const LABELS = ['red', 'orange', 'yellow', 'green', 'blue', 'purple'] as const;
  const activeLabel = ref<string | null>(null);

  const labelSelection = async (label: string | null) => {
    const ids = allMatchingIds.value ?? [...new Set(selectedFiles.value.map(f => f.id))];
    if (ids.length === 0) return 0;
    try {
      const changed = await invoke<number>('set_file_label', {
        vaultPath: vaultPath(), nodeIds: ids, label,
      });
      await reload();
      return changed;
    } catch (e) {
      logger.error("Failed to label the selection", e);
      return 0;
    }
  };

  const revealFile = async (path: string) => {
    try {
      await invoke('reveal_in_file_manager', { vaultPath: vaultPath(), path });
    } catch (e) {
      logger.error("Failed to reveal the file", e);
    }
  };

  // ─── Saved collections ─────────────────────────────────────
  //
  // A collection stores the question, not the answer, so it stays true as files
  // come and go — and it is a vault node, so it travels between devices.
  const collections = ref<{ id: string; name: string; filter: Record<string, unknown> }[]>([]);

  const fetchCollections = async () => {
    try {
      collections.value = await invoke('list_file_collections');
    } catch (e) {
      logger.error("Failed to load collections", e);
    }
  };

  const saveCollection = async (name: string) => {
    try {
      await invoke('save_file_collection', { vaultPath: vaultPath(), name, filter: currentFilter() });
      await fetchCollections();
    } catch (e) {
      logger.error("Failed to save the collection", e);
    }
  };

  const deleteCollection = async (id: string) => {
    try {
      await invoke('delete_file_collection', { vaultPath: vaultPath(), id });
      await fetchCollections();
    } catch (e) {
      logger.error("Failed to delete the collection", e);
    }
  };

  /** Put the list back into the state a collection describes. */
  const applyCollection = (collection: { filter: Record<string, unknown> }) => {
    const f = collection.filter as Record<string, string | null>;
    activeSourceId.value = f.sourceKind === 'gdrive'
      ? 'gdrive'
      : sources.value.find(s => s.path === f.sourcePath)?.id ?? null;
    activeTag.value = f.tag ?? null;
    activeCamera.value = f.camera ?? null;
    activeLabel.value = f.label ?? null;
    searchQuery.value = typeof f.nameContains === 'string' ? f.nameContains : '';
  };

  /** The cameras that actually appear in this library, for the filter to offer. */
  const cameras = ref<string[]>([]);
  const fetchCameras = async () => {
    try {
      cameras.value = await invoke<string[]>('list_cameras');
    } catch (e) {
      logger.error("Failed to list cameras", e);
    }
  };
  const fileBackendSearchIds = ref<string[] | null>(null);

  // ─── A connected account ───────────────────────────────────
  //
  // Metadata only. What comes back is a listing, not a download: a Drive full
  // of holiday video is not something to copy onto a laptop because it appeared
  // in a list. Opening a cloud file opens its page.
  const isGDriveConnected = ref(false);
  const gdriveEmail = ref('');
  const isConnectingGDrive = ref(false);
  const gdriveFileCount = ref(0);

  const checkGDriveStatus = async () => {
    try {
      isGDriveConnected.value = await invoke<boolean>('is_gdrive_connected', { vaultPath: vaultPath() });
      if (isGDriveConnected.value) {
        gdriveEmail.value = await invoke<string>('get_gdrive_user_info', { vaultPath: vaultPath() });
      }
    } catch (e) {
      logger.error("Failed to check Drive status", e);
    }
  };

  const syncGDrive = async () => {
    if (!isGDriveConnected.value) return;
    isScanning.value = true;
    try {
      gdriveFileCount.value = await invoke<number>('get_gdrive_files', { vaultPath: vaultPath() });
      await fetchFiles();
    } catch (e) {
      logger.error("Drive listing failed", e);
      await message(String(e), { title: 'Google Drive', kind: 'error' });
    } finally {
      isScanning.value = false;
    }
  };

  const connectGDrive = async () => {
    if (isConnectingGDrive.value) return;
    isConnectingGDrive.value = true;
    try {
      const response = await invoke<string>('connect_gdrive', { vaultPath: vaultPath() });
      // The browser flow finishes through a deep link, not here.
      if (response !== 'SUCCESS') return;
      isGDriveConnected.value = true;
      gdriveEmail.value = await invoke<string>('get_gdrive_user_info', { vaultPath: vaultPath() });
      await syncGDrive();
    } catch (e) {
      logger.error("Failed to connect Drive", e);
      await message(String(e), { title: 'Google Drive', kind: 'error' });
    } finally {
      isConnectingGDrive.value = false;
    }
  };

  const disconnectGDrive = async () => {
    const confirmed = await ask(t('file.gdrive_disconnect_body'), {
      title: t('file.gdrive_disconnect_title'),
      kind: 'warning',
      okLabel: t('file.gdrive_disconnect'),
      cancelLabel: t('file.cancel'),
    });
    if (!confirmed) return;
    try {
      await invoke('disconnect_gdrive', { vaultPath: vaultPath() });
      isGDriveConnected.value = false;
      gdriveEmail.value = '';
      if (activeSourceId.value === 'gdrive') activeSourceId.value = null;
      await fetchFiles();
    } catch (e) {
      logger.error("Failed to disconnect Drive", e);
    }
  };

  // ─── Sources & Files ───────────────────────────────────────
  const fetchSources = async () => {
    try {
      sources.value = await invoke<FileSource[]>('get_file_sources', { vaultPath: vaultPath() });
    } catch (e) {
      logger.error("Failed to load sources", e);
    }
  };

  const fetchFiles = async () => {
    await reload();
    void fetchTags();
  };

  /**
   * How far the running scan has got.
   *
   * A scan is no longer a spinner you wait out. It commits in batches and says
   * so after each one, so the app stays usable throughout and the user can see
   * that something is happening to a folder of forty thousand photos — and stop
   * it if they would rather not.
   */
  const scanProgress = ref<ScanProgress | null>(null);
  let unlistenScan: (() => void) | null = null;

  const watchScanProgress = async () => {
    unlistenScan?.();
    unlistenScan = await listen<ScanProgress>('file-scan-progress', (event) => {
      scanProgress.value = event.payload;
    });
  };

  const stopScanning = async () => {
    try {
      await invoke('cancel_file_scan');
    } catch (e) {
      logger.error("Failed to cancel scan", e);
    }
  };

  /**
   * How much of the library still has to be read.
   *
   * Reading the words out of a thousand PDFs takes minutes, so it is not part
   * of the scan and nothing waits on it. The scan makes files findable by name
   * immediately; this makes them findable by what is written inside them,
   * shortly afterwards.
   */
  const textProgress = ref<TextProgress | null>(null);
  let unlistenText: (() => void) | null = null;

  const watchTextProgress = async () => {
    unlistenText?.();
    unlistenText = await listen<TextProgress>('file-text-progress', (event) => {
      textProgress.value = event.payload.remaining > 0 ? event.payload : null;
    });
  };

  /** Read the documents that have not been read yet. Never awaited by the UI. */
  const readDocumentText = async () => {
    try {
      const backlog = await invoke<number>('file_text_backlog');
      if (backlog === 0) return;
      textProgress.value = { done: 0, remaining: backlog, cancelled: false };
      await invoke('extract_file_text');
    } catch (e) {
      logger.error("Failed to read document text", e);
    } finally {
      textProgress.value = null;
    }
  };

  const syncAllSources = async () => {
    if (isScanning.value) return;
    isScanning.value = true;
    scanProgress.value = null;
    try {
      await invoke('reindex_sources', { vaultPath: vaultPath() });
      await fetchFiles();
    } catch (e) {
      logger.error("Failed to sync sources", e);
    } finally {
      isScanning.value = false;
      scanProgress.value = null;
    }
    // Deliberately after the spinner stops and deliberately not awaited: the
    // list is usable now, and this only makes it better.
    void readDocumentText();
  };

  /**
   * Notice when a watched folder changes on disk.
   *
   * Registered folders update by themselves now. Until this, a photo dropped
   * into a synced folder stayed invisible until somebody thought to press the
   * sync button — which is not something anyone thinks to do about a folder
   * they added precisely so they would not have to think about it.
   *
   * The rescan covers the folders that changed, not everything, and the backend
   * has already waited for the burst of events to settle.
   */
  let unlistenSources: (() => void) | null = null;

  const watchSourceFolders = async () => {
    try {
      await invoke('watch_file_sources', { paths: sources.value.map(s => s.path) });
    } catch (e) {
      logger.error("Failed to watch source folders", e);
    }
    if (unlistenSources) return;
    unlistenSources = await listen<string[]>('file-source-changed', async (event) => {
      if (isScanning.value) return;
      isScanning.value = true;
      try {
        for (const folder of event.payload) {
          await invoke('scan_directory', { vaultPath: vaultPath(), sourcePath: folder });
        }
        await fetchFiles();
      } catch (e) {
        logger.error("Rescan after a folder changed failed", e);
      } finally {
        isScanning.value = false;
      }
      void readDocumentText();
    });
  };

  const addNewSource = async () => {
    try {
      const selectedPath = await open({
        directory: true,
        multiple: false,
        title: "Select a folder to sync"
      });
      if (selectedPath && typeof selectedPath === 'string') {
        const folderName = selectedPath.split('/').pop() || selectedPath.split('\\').pop() || "Unknown Folder";
        await invoke('add_file_source', { vaultPath: vaultPath(), path: selectedPath, name: folderName });
        await fetchSources();
        void watchSourceFolders();
        isScanning.value = true;
        await invoke('scan_directory', { vaultPath: vaultPath(), sourcePath: selectedPath });
        await fetchFiles();
        isScanning.value = false;
      }
    } catch (e) {
      logger.error("Failed to add source", e);
      isScanning.value = false;
    }
  };

  /** Copy files into the vault and index them. Shared by the button and by drop. */
  const importPaths = async (paths: string[]) => {
    if (paths.length === 0) return 0;
    isScanning.value = true;
    try {
      const count = await invoke<number>('import_files', { vaultPath: vaultPath(), filePaths: paths });
      if (count > 0) {
        await fetchFiles();
        void readDocumentText();
      }
      return count;
    } catch (e) {
      logger.error("Failed to import files", e);
      return 0;
    } finally {
      isScanning.value = false;
    }
  };

  /**
   * Import files dropped onto the window.
   *
   * Bytes rather than paths, because that is all a browser drop event carries:
   * the OS hands over the contents, not a location on disk. `save_asset` names
   * each one after its own contents, so dropping the same screenshot twice
   * stores it once.
   *
   * This replaces an attempt to use Tauri's own drag-drop event, which never
   * fired: the window sets `dragDropEnabled: false`, so the webview keeps the
   * OS drop and Tauri is never told about it. That configuration is also what
   * makes dragging a file *out* of this list and into a note work at all, so
   * the fix belongs on this side.
   */
  const importDroppedFiles = async (dropped: File[]) => {
    if (dropped.length === 0) return 0;
    isScanning.value = true;
    let saved = 0;
    try {
      for (const file of dropped) {
        try {
          const bytes = new Uint8Array(await file.arrayBuffer());
          await invoke<string>('save_asset', {
            vaultPath: vaultPath(),
            filename: file.name,
            bytes: Array.from(bytes),
          });
          saved += 1;
        } catch (e) {
          logger.error(`Failed to import ${file.name}`, e);
        }
      }
      if (saved > 0) {
        // The assets folder is a registered source, so a scan of it is what
        // turns the new bytes into indexed files.
        await invoke('reindex_sources', { vaultPath: vaultPath() });
        await fetchFiles();
        void readDocumentText();
      }
    } finally {
      isScanning.value = false;
    }
    return saved;
  };

  const importFiles = async () => {
    try {
      const selected = await open({
        multiple: true,
        title: "Select files to import",
      });
      if (!selected) return;
      await importPaths(Array.isArray(selected) ? selected : [selected]);
    } catch (e) {
      logger.error("Failed to import files", e);
    }
  };

  const removeSource = async (id: string) => {
    try {
      await invoke('remove_file_source', { vaultPath: vaultPath(), sourceId: id });
      if (activeSourceId.value === id) activeSourceId.value = null;
      await fetchSources();
      void watchSourceFolders();
      await fetchFiles();
    } catch (e) {
      logger.error("Failed to remove source", e);
    }
  };

  // ─── File Operations ───────────────────────────────────────
  /**
   * Every row showing the same file as this one — itself included.
   *
   * The list holds one row per copy on disk, and copies of one file share an
   * id because they share an identity. Tagging is therefore not an edit to a
   * row: it is an edit to the item, and every row showing that item has to move
   * with it or the second copy sits there claiming the tag was never added.
   */
  const sameItem = (file: FileMetadata) => loadedFiles.value.filter(f => f.id === file.id);

  const saveFileName = async (file: FileMetadata, newName: string) => {
    let finalName = newName.trim();
    if (!finalName) return;
    if (file.extension && !finalName.endsWith(`.${file.extension}`)) {
      finalName = `${finalName}.${file.extension}`;
    }
    if (finalName === file.filename) return;
    try {
      const newPath = await invoke<string>('update_file_metadata', {
        vaultPath: vaultPath(), path: file.path, newFilename: finalName, newTags: file.tags, newPeople: file.people || []
      });
      file.filename = finalName;
      file.path = newPath;
      for (const row of sameItem(file)) { row.filename = finalName; }
      const moved = loadedFiles.value.find(f => f.path === newPath) ?? file;
      moved.path = newPath;
    } catch (e) {
      logger.error("Failed to rename file", e);
    }
  };

  const addTag = async (file: FileMetadata, tag: string) => {
    const t = tag.trim().toLowerCase();
    if (!t || file.tags.includes(t)) return;
    const updatedTags = [...file.tags, t];
    try {
      await invoke('update_file_metadata', {
        vaultPath: vaultPath(), path: file.path, newFilename: file.filename, newTags: updatedTags, newPeople: file.people || []
      });
      for (const row of sameItem(file)) row.tags = updatedTags;
    } catch (e) {
      logger.error("Failed to add tag", e);
    }
  };

  const removeTag = async (file: FileMetadata, tag: string) => {
    const updatedTags = file.tags.filter(t => t !== tag);
    try {
      await invoke('update_file_metadata', {
        vaultPath: vaultPath(), path: file.path, newFilename: file.filename, newTags: updatedTags, newPeople: file.people || []
      });
      for (const row of sameItem(file)) row.tags = updatedTags;
    } catch (e) {
      logger.error("Failed to remove tag", e);
    }
  };

  const addPerson = async (file: FileMetadata, personInternalLink: string) => {
    if (!personInternalLink || (file.people && file.people.includes(personInternalLink))) return;
    const updatedPeople = [...(file.people || []), personInternalLink];
    try {
      await invoke('update_file_metadata', {
        vaultPath: vaultPath(), path: file.path, newFilename: file.filename, newTags: file.tags, newPeople: updatedPeople
      });
      for (const row of sameItem(file)) row.people = updatedPeople;
    } catch (e) {
      logger.error("Failed to add person", e);
    }
  };

  const removePerson = async (file: FileMetadata, personInternalLink: string) => {
    const updatedPeople = (file.people || []).filter(p => p !== personInternalLink);
    try {
      await invoke('update_file_metadata', {
        vaultPath: vaultPath(), path: file.path, newFilename: file.filename, newTags: file.tags, newPeople: updatedPeople
      });
      for (const row of sameItem(file)) row.people = updatedPeople;
    } catch (e) {
      logger.error("Failed to remove person", e);
    }
  };

  const openLocalFile = async (path: string) => {
    try {
      await invoke('open_local_file', { vaultPath: vaultPath(), path });
    } catch (e) {
      logger.error("Failed to open file", e);
    }
  };

  // ─── Helpers ───────────────────────────────────────────────
  const TYPE_GROUPS: Record<string, string[]> = {
    Images: ['jpg', 'jpeg', 'png', 'gif', 'svg', 'webp', 'bmp', 'ico', 'tiff', 'heic', 'avif'],
    Documents: ['pdf', 'txt', 'md', 'doc', 'docx', 'odt', 'epub', 'rtf'],
    Videos: ['mp4', 'mov', 'avi', 'webm', 'mkv', 'flv', 'wmv', 'm4v'],
    Audio: ['mp3', 'wav', 'ogg', 'm4a', 'flac', 'aac', 'wma', 'alac'],
    Archives: ['zip', 'rar', 'gz', '7z', 'tar'],
    Code: ['js', 'ts', 'vue', 'json', 'html', 'css', 'rs', 'py', 'go', 'java', 'sh'],
  };

  /**
   * The extensions a category covers.
   *
   * The list travels to SQL now instead of driving a filter over an array in
   * the browser, which is why it is written this way round: the categories are
   * defined here, so the mapping stays here and the database stays incurious.
   */
  const extensionsForGroup = (group: string): string[] => TYPE_GROUPS[group] ?? [];

  const getFileTypeGroup = (ext: string) => {
    const e = ext.toLowerCase();
    for (const [group, extensions] of Object.entries(TYPE_GROUPS)) {
      if (extensions.includes(e)) return group;
    }
    return 'Other';
  };

  const formatSize = (bytes: number) => {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
  };

  // ─── The list ──────────────────────────────────────────────
  //
  // Narrowed and ordered by the database, a page at a time.
  //
  // What this replaces held every indexed file in the browser and filtered the
  // array on each keystroke. It worked up to a point and then stopped: fifty
  // thousand files is seventeen megabytes of JSON across the IPC bridge before
  // the webview has parsed any of it, on every open, to show forty rows.
  //
  // `rows` is as long as the filtered set but mostly empty. That array never
  // crosses the bridge — it is placeholders, and a screenful of real rows is
  // fetched around wherever the reader has scrolled to. Virtualisation keeps
  // working unchanged, because it only ever needed to know how many there are.

  /** How many rows are fetched in one request. */
  const PAGE_SIZE = 100;

  /** One slot per matching file; `null` until that page has been fetched. */
  const rows = ref<(FileMetadata | null)[]>([]);
  const total = ref(0);
  /** Pages already asked for, so scrolling back does not ask again. */
  let requestedPages = new Set<number>();
  /** Bumped on every reload, so a page arriving late cannot land in a new list. */
  let generation = 0;

  const allTags = ref<string[]>([]);

  const currentFilter = () => {
    const source = sources.value.find(s => s.id === activeSourceId.value);
    const query = searchQuery.value.trim();
    // `#thuế` searches tags rather than text, which is the one search shape the
    // full-text index is the wrong tool for.
    const tagQuery = query.startsWith('#') ? query.slice(1).toLowerCase() : null;

    return {
      sourcePath: activeSourceId.value && activeSourceId.value !== 'gdrive' ? source?.path ?? null : null,
      sourceKind: activeSourceId.value === 'gdrive' ? 'gdrive' : null,
      extensions: activeType.value ? extensionsForGroup(activeType.value) : null,
      tag: tagQuery || activeTag.value,
      camera: activeCamera.value,
      label: activeLabel.value,
      searchIds: tagQuery ? null : fileBackendSearchIds.value,
      // Until the index answers — and if it never does — the filename is
      // something to narrow by, so the box does not appear to be ignored.
      nameContains: !tagQuery && query && fileBackendSearchIds.value === null ? query : null,
    };
  };

  /** Start the list again: new count, nothing loaded, first page on its way. */
  const reload = async () => {
    const mine = ++generation;
    requestedPages = new Set();
    clearSelection();
    isLoading.value = true;
    try {
      const page = await invoke<{ files: FileMetadata[]; total: number }>('query_file_page', {
        filter: currentFilter(),
        sort: searchIsRanked() ? 'relevance' : sortBy.value,
        descending: sortDescending.value,
        offset: 0,
        limit: PAGE_SIZE,
      });
      if (mine !== generation) return;
      total.value = page.total;
      rows.value = new Array(page.total).fill(null);
      placePage(0, page.files);
      requestedPages.add(0);
    } catch (e) {
      logger.error("Failed to load files", e);
      rows.value = [];
      total.value = 0;
    } finally {
      if (mine === generation) isLoading.value = false;
    }
  };

  /**
   * A search returns its results already ranked, and re-sorting them by name
   * would discard the ranking it just computed.
   */
  const searchIsRanked = () => fileBackendSearchIds.value !== null;

  const placePage = (offset: number, files: FileMetadata[]) => {
    for (let i = 0; i < files.length; i++) rows.value[offset + i] = files[i];
    // Vue tracks the array, not its holes.
    rows.value = [...rows.value];
  };

  /** Fetch whatever pages the rows between `from` and `to` need. */
  const ensureLoaded = async (from: number, to: number) => {
    const first = Math.max(0, Math.floor(from / PAGE_SIZE));
    const last = Math.min(Math.floor(Math.max(to, 0) / PAGE_SIZE), Math.floor(Math.max(total.value - 1, 0) / PAGE_SIZE));

    for (let page = first; page <= last; page++) {
      if (requestedPages.has(page)) continue;
      requestedPages.add(page);
      const mine = generation;
      const offset = page * PAGE_SIZE;
      invoke<{ files: FileMetadata[]; total: number }>('query_file_page', {
        filter: currentFilter(),
        sort: searchIsRanked() ? 'relevance' : sortBy.value,
        descending: sortDescending.value,
        offset,
        limit: PAGE_SIZE,
      })
        .then(result => {
          if (mine !== generation) return;
          placePage(offset, result.files);
        })
        .catch(e => {
          // Let it be asked for again rather than leaving a permanent hole.
          requestedPages.delete(page);
          logger.error("Failed to load a page of files", e);
        });
    }
  };

  const fetchTags = async () => {
    try {
      const counts = await invoke<[string, number][]>('file_tag_counts');
      allTags.value = counts.map(([tag]) => tag);
    } catch (e) {
      logger.error("Failed to load tags", e);
    }
  };

  /** The rows on screen right now — never the whole library. */
  const loadedFiles = computed(() => rows.value.filter((f): f is FileMetadata => f !== null));

  /**
   * One file by identity or path, for opening something the list may not have
   * fetched. Asks the database when it is not already in hand.
   */
  const findFile = async (idOrPath: string): Promise<FileMetadata | null> => {
    const loaded = loadedFiles.value.find(f => f.id === idOrPath || f.path === idOrPath);
    if (loaded) return loaded;
    try {
      const page = await invoke<{ files: FileMetadata[] }>('query_file_page', {
        filter: { searchIds: [idOrPath] },
        sort: 'relevance',
        descending: false,
        offset: 0,
        limit: 1,
      });
      return page.files[0] ?? null;
    } catch (e) {
      logger.error("Failed to find file", e);
      return null;
    }
  };

  // Any change to what is being asked for starts the list again.
  watch(
    [activeSourceId, activeType, activeTag, activeCamera, activeLabel, sortBy, sortDescending, fileBackendSearchIds, searchQuery],
    () => { void reload(); }
  );

  // ─── Duplicate Finder ─────────────────────────────────────
  const duplicateGroups = ref<DuplicateGroup[]>([]);
  const duplicateSummary = ref<{ total_groups: number; total_duplicate_files: number; total_wasted_bytes: number } | null>(null);
  const isScanningDuplicates = ref(false);

  // Computed for backward-compatible template access
  const duplicateReport = computed<DuplicateReport | null>(() => {
    if (duplicateGroups.value.length === 0 && !duplicateSummary.value) return null;
    const summary = duplicateSummary.value || {
      total_groups: duplicateGroups.value.length,
      total_duplicate_files: duplicateGroups.value.reduce((acc, g) => acc + g.count - 1, 0),
      total_wasted_bytes: duplicateGroups.value.reduce((acc, g) => acc + g.wasted_bytes, 0),
    };
    return {
      groups: duplicateGroups.value,
      ...summary,
    };
  });

  let unlistenGroupFound: (() => void) | null = null;
  let unlistenScanComplete: (() => void) | null = null;

  const scanDuplicates = async () => {
    if (isScanningDuplicates.value) return;
    isScanningDuplicates.value = true;
    duplicateGroups.value = [];
    duplicateSummary.value = null;

    // Clean up previous listeners
    unlistenGroupFound?.();
    unlistenScanComplete?.();

    // Listen for streamed groups
    unlistenGroupFound = await listen<DuplicateGroup>('duplicate-group-found', (event) => {

      duplicateGroups.value.push(event.payload);
    });

    // Listen for scan completion
    unlistenScanComplete = await listen<{ total_groups: number; total_duplicate_files: number; total_wasted_bytes: number }>('duplicate-scan-complete', (event) => {

      duplicateSummary.value = event.payload;
      isScanningDuplicates.value = false;
      // Clean up listeners
      unlistenGroupFound?.();
      unlistenScanComplete?.();
      unlistenGroupFound = null;
      unlistenScanComplete = null;
    });

    // Safety timeout: if scan-complete never arrives, stop spinner after 60s
    const safetyTimeout = setTimeout(() => {
      if (isScanningDuplicates.value) {
        logger.warn('[DupFinder] Safety timeout reached — forcing scan complete with', duplicateGroups.value.length, 'groups');
        duplicateSummary.value = {
          total_groups: duplicateGroups.value.length,
          total_duplicate_files: duplicateGroups.value.reduce((acc, g) => acc + g.count - 1, 0),
          total_wasted_bytes: duplicateGroups.value.reduce((acc, g) => acc + g.wasted_bytes, 0),
        };
        isScanningDuplicates.value = false;
        unlistenGroupFound?.();
        unlistenScanComplete?.();
        unlistenGroupFound = null;
        unlistenScanComplete = null;
      }
    }, 60_000);

    try {
      await invoke('find_duplicate_files', { vaultPath: vaultPath() });
    } catch (e) {
      logger.error("[DupFinder] Failed to scan duplicates", e);
      clearTimeout(safetyTimeout);
      isScanningDuplicates.value = false;
      unlistenGroupFound?.();
      unlistenScanComplete?.();
    }
  };

  /**
   * Which notes use this file, asked by identity rather than by name.
   *
   * The name was never a safe question: a file called `note.pdf` came back
   * used by every note containing the word "note".
   */
  const getFileReferences = async (nodeId: string): Promise<FileReference[]> => {
    try {
      return await invoke<FileReference[]>('get_file_references', { vaultPath: vaultPath(), nodeId });
    } catch (e) {
      logger.error('Failed to get file references', e);
      return [];
    }
  };

  const deleteFile = async (file: FileMetadata): Promise<boolean> => {
    const confirmed = await ask(t('file.delete_body', { name: file.filename }), {
      title: t('file.delete_title'),
      kind: 'warning',
      okLabel: t('file.delete_confirm'),
      cancelLabel: t('file.cancel'),
    });
    if (!confirmed) return false;

    try {
      await invoke('delete_file', { vaultPath: vaultPath(), fileId: file.id, filePath: file.path });
      // Remove from local state
      rows.value = rows.value.filter(f => f?.id !== file.id);
      total.value = Math.max(0, total.value - 1);
      return true;
    } catch (e) {
      logger.error('Failed to delete file', e);
      await message(`Failed to delete: ${e}`, { title: 'Error', kind: 'error' });
      return false;
    }
  };

  // ─── Init ──────────────────────────────────────────────────
  const init = async () => {
    await watchScanProgress();
    await watchTextProgress();
    await fetchSources();
    void watchSourceFolders();
    void checkGDriveStatus();
    void fetchCameras();
    void fetchCollections();
    await syncAllSources(); // index sources + fetch files
  };

  const dispose = () => {
    unlistenScan?.();
    unlistenText?.();
    unlistenSources?.();
    unlistenSources = null;
    unlistenScan = null;
    unlistenText = null;
  };

  return {
    // State
    sources, isLoading, isScanning, scanProgress, stopScanning, textProgress,
    activeSourceId, activeType, activeTag, activeCamera, activeLabel, searchQuery,
    LABELS, labelSelection, revealFile,
    collections, fetchCollections, saveCollection, deleteCollection, applyCollection,
    sortBy, sortDescending,
    // The list: as long as the filtered set, loaded a page at a time.
    rows, total, loadedFiles, ensureLoaded, reload, findFile, allTags,
    // Selection and bulk work
    selection, isSelected, selectFile, selectAllMatching, clearSelection,
    selectedFiles, selectionSize, tagSelection,
    // Cameras
    cameras, fetchCameras,
    // A connected account
    isGDriveConnected, gdriveEmail, isConnectingGDrive, gdriveFileCount,
    connectGDrive, syncGDrive, disconnectGDrive,
    // Sources
    fetchSources, fetchFiles, syncAllSources, addNewSource, removeSource, importFiles, importPaths, importDroppedFiles,
    // File ops
    saveFileName, addTag, removeTag,
    addPerson,
    removePerson,
    openLocalFile,
    // Duplicates
    duplicateReport, isScanningDuplicates, scanDuplicates, getFileReferences, deleteFile,
    // Helpers
    getFileTypeGroup, formatSize,
    // Init
    init, dispose,
  };
}
