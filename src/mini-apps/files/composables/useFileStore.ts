import { ref, computed, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { open, ask, message } from '@tauri-apps/plugin-dialog';
import { listen } from '@tauri-apps/api/event';
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

export interface FileReference {
  node_id: string;
  node_type: string;
  title: string;
}

export function useFileStore(vaultPath: () => string) {
  const files = ref<FileMetadata[]>([]);
  const sources = ref<FileSource[]>([]);
  const isLoading = ref(true);
  const isScanning = ref(false);

  const activeSourceId = ref<string | null>(null);
  const activeType = ref<string | null>(null);
  const activeTag = ref<string | null>(null);
  const searchQuery = ref('');
  const fileBackendSearchIds = ref<string[] | null>(null);
  let fileSearchTimeout: ReturnType<typeof setTimeout>;

  // ─── GDrive ────────────────────────────────────────────────
  const isGDriveConnected = ref(false);
  const gdriveEmail = ref('');
  const isConnectingGDrive = ref(false);

  const checkGDriveStatus = async () => {
    try {
      isGDriveConnected.value = await invoke<boolean>('is_gdrive_connected', { vaultPath: vaultPath() });
      if (isGDriveConnected.value) {
        gdriveEmail.value = await invoke<string>('get_gdrive_user_info', { vaultPath: vaultPath() });
      }
    } catch (e) {
      logger.error("Failed to check GDrive status", e);
    }
  };

  const connectGDrive = async () => {
    if (isConnectingGDrive.value) return;
    isConnectingGDrive.value = true;
    try {
      const resp = await invoke<string>('connect_gdrive', { vaultPath: vaultPath() });
      if (resp === 'WAITING_DEEP_LINK') return;
      if (resp === 'SUCCESS') {
        isGDriveConnected.value = true;
        gdriveEmail.value = await invoke<string>('get_gdrive_user_info', { vaultPath: vaultPath() });
        await invoke('get_gdrive_files', { vaultPath: vaultPath() });
        await fetchFiles();
        activeSourceId.value = 'gdrive';
      }
    } catch (e: any) {
      logger.error("Failed to connect GDrive", JSON.stringify(e));
      const errStr = typeof e === 'object' ? JSON.stringify(e) : String(e);
      await message(`Failed to connect Google Drive: ${errStr}`, { title: 'Error', kind: 'error' });
    } finally {
      isConnectingGDrive.value = false;
    }
  };

  const syncGDrive = async () => {
    isScanning.value = true;
    try {
      await invoke('get_gdrive_files', { vaultPath: vaultPath() });
      await fetchFiles();
    } catch (e: any) {
      logger.error("GDrive sync failed", e);
    } finally {
      isScanning.value = false;
    }
  };

  const disconnectGDrive = async () => {
    const isConfirmed = await ask('All cloud files will be removed from your view. Your actual files on Google Drive will not be deleted.', {
      title: 'Disconnect Google Drive?',
      kind: 'warning',
      okLabel: 'Disconnect',
      cancelLabel: 'Cancel'
    });
    if (!isConfirmed) return;
    try {
      await invoke('disconnect_gdrive', { vaultPath: vaultPath() });
      isGDriveConnected.value = false;
      gdriveEmail.value = '';
      if (activeSourceId.value === 'gdrive') activeSourceId.value = null;
      await fetchFiles();
    } catch (e: any) {
      logger.error("Failed to disconnect", e);
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
    isLoading.value = true;
    try {
      files.value = await invoke<FileMetadata[]>('query_files', { vaultPath: vaultPath() });
    } catch (e) {
      logger.error("Failed to load files", e);
    } finally {
      isLoading.value = false;
    }
  };

  const syncAllSources = async () => {
    if (isScanning.value) return;
    isScanning.value = true;
    try {
      await invoke('reindex_sources', { vaultPath: vaultPath() });
      if (isGDriveConnected.value) {
        await invoke('get_gdrive_files', { vaultPath: vaultPath() });
      }
      await fetchFiles();
    } catch (e) {
      logger.error("Failed to sync sources", e);
    } finally {
      isScanning.value = false;
    }
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

  const importFiles = async () => {
    try {
      const selected = await open({
        multiple: true,
        title: "Select files to import",
      });
      if (!selected) return;
      const paths = Array.isArray(selected) ? selected : [selected];
      if (paths.length === 0) return;
      isScanning.value = true;
      const count = await invoke<number>('import_files', { vaultPath: vaultPath(), filePaths: paths });
      if (count > 0) await fetchFiles();
    } catch (e) {
      logger.error("Failed to import files", e);
    } finally {
      isScanning.value = false;
    }
  };

  const removeSource = async (id: string) => {
    try {
      await invoke('remove_file_source', { vaultPath: vaultPath(), sourceId: id });
      if (activeSourceId.value === id) activeSourceId.value = null;
      await fetchSources();
      await fetchFiles();
    } catch (e) {
      logger.error("Failed to remove source", e);
    }
  };

  // ─── File Operations ───────────────────────────────────────
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
      const idx = files.value.findIndex(f => f.id === file.id);
      if (idx !== -1) { files.value[idx].filename = finalName; files.value[idx].path = newPath; }
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
      file.tags = updatedTags;
      const idx = files.value.findIndex(f => f.id === file.id);
      if (idx !== -1) files.value[idx].tags = updatedTags;
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
      file.tags = updatedTags;
      const idx = files.value.findIndex(f => f.id === file.id);
      if (idx !== -1) files.value[idx].tags = updatedTags;
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
      file.people = updatedPeople;
      const idx = files.value.findIndex(f => f.id === file.id);
      if (idx !== -1) files.value[idx].people = updatedPeople;
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
      file.people = updatedPeople;
      const idx = files.value.findIndex(f => f.id === file.id);
      if (idx !== -1) files.value[idx].people = updatedPeople;
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
  const getFileTypeGroup = (ext: string) => {
    const e = ext.toLowerCase();
    if (['jpg', 'jpeg', 'png', 'gif', 'svg', 'webp', 'bmp', 'ico', 'tiff', 'heic'].includes(e)) return 'Images';
    if (['pdf', 'txt', 'md', 'doc', 'docx'].includes(e)) return 'Documents';
    if (['mp4', 'mov', 'avi', 'webm', 'mkv', 'flv', 'wmv', 'm4v'].includes(e)) return 'Videos';
    if (['mp3', 'wav', 'ogg', 'm4a', 'flac', 'aac', 'wma', 'alac'].includes(e)) return 'Audio';
    if (['zip', 'rar', 'gz'].includes(e)) return 'Archives';
    if (['js', 'ts', 'vue', 'json', 'html', 'css', 'rs', 'py'].includes(e)) return 'Code';
    return 'Other';
  };

  const formatSize = (bytes: number) => {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
  };

  // ─── All unique tags ───────────────────────────────────────
  const allTags = computed(() => {
    const tagSet = new Set<string>();
    for (const f of files.value) {
      for (const t of f.tags) tagSet.add(t);
    }
    return Array.from(tagSet).sort();
  });

  // ─── Filtering ─────────────────────────────────────────────
  const filteredFiles = computed(() => {
    let result = files.value;

    if (activeSourceId.value) {
      if (activeSourceId.value === 'gdrive') {
        result = result.filter(f => f.source_type === 'gdrive');
      } else {
        const source = sources.value.find(s => s.id === activeSourceId.value);
        if (source) {
          result = result.filter(f => f.path.startsWith(source.path));
        }
      }
    }

    if (activeType.value) {
      result = result.filter(f => getFileTypeGroup(f.extension) === activeType.value);
    }

    if (activeTag.value) {
      result = result.filter(f => f.tags.includes(activeTag.value!));
    }

    if (searchQuery.value) {
      let q = searchQuery.value.toLowerCase().trim();
      const isTagSearch = q.startsWith('#');

      if (isTagSearch) {
        q = q.slice(1);
        result = result.filter(f => f.tags.some(t => t.toLowerCase().includes(q)));
      } else if (fileBackendSearchIds.value !== null) {
        const idSet = new Set(fileBackendSearchIds.value);
        result = result.filter(f => idSet.has(f.id));
        const orderMap = new Map(fileBackendSearchIds.value.map((id, i) => [id, i]));
        result = result.sort((a, b) => (orderMap.get(a.id) ?? 999) - (orderMap.get(b.id) ?? 999));
      } else {
        result = result.filter(f =>
          f.filename.toLowerCase().includes(q) ||
          f.tags.some(t => t.toLowerCase().includes(q)) ||
          f.extension.toLowerCase().includes(q)
        );
      }
    }

    return result;
  });

  // Debounced backend search
  watch(searchQuery, (q) => {
    clearTimeout(fileSearchTimeout);
    const trimmed = q.trim();
    if (!trimmed || trimmed.startsWith('#')) {
      fileBackendSearchIds.value = null;
      return;
    }
    fileSearchTimeout = setTimeout(async () => {
      try {
        const resp = await invoke<{ results: { id: string }[], total_count: number, query_time_ms: number }>('search_files', {
          vaultPath: vaultPath(), query: trimmed
        });
        if (searchQuery.value.trim() === trimmed) {
          fileBackendSearchIds.value = resp.results.map(r => r.id);
        }
      } catch (e) {
        console.error('File backend search error', e);
      }
    }, 200);
  });

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

  const getFileReferences = async (filename: string): Promise<FileReference[]> => {
    try {
      return await invoke<FileReference[]>('get_file_references', { vaultPath: vaultPath(), filename });
    } catch (e) {
      logger.error('Failed to get file references', e);
      return [];
    }
  };

  const deleteFile = async (file: FileMetadata): Promise<boolean> => {
    const confirmed = await ask(`Delete "${file.filename}"?\n\nThis will permanently remove the file from disk.`, {
      title: 'Delete File',
      kind: 'warning',
      okLabel: 'Delete',
      cancelLabel: 'Cancel'
    });
    if (!confirmed) return false;

    try {
      await invoke('delete_file', { fileId: file.id, filePath: file.path });
      // Remove from local state
      files.value = files.value.filter(f => f.id !== file.id);
      return true;
    } catch (e) {
      logger.error('Failed to delete file', e);
      await message(`Failed to delete: ${e}`, { title: 'Error', kind: 'error' });
      return false;
    }
  };

  // ─── Init ──────────────────────────────────────────────────
  const init = async () => {
    await fetchSources();
    await checkGDriveStatus();
    await syncAllSources(); // index sources + fetch files
  };

  const setupAuthListener = async () => {
    return listen('omnidrive-auth-code', async (e: any) => {
      const code = e.payload.code;
      try {
        const success = await invoke('connect_gdrive_complete', { authCode: code, vaultPath: vaultPath() });
        if (success) {
          isGDriveConnected.value = true;
          gdriveEmail.value = await invoke<string>('get_gdrive_user_info', { vaultPath: vaultPath() });
          await invoke('get_gdrive_files', { vaultPath: vaultPath() });
          await fetchFiles();
          activeSourceId.value = 'gdrive';
        }
      } catch (err: any) {
        logger.error("OmniDrive auth complete failed", err);
        const errStr = typeof err === 'object' ? JSON.stringify(err) : String(err);
        await message(`Failed to connect Google Drive: ${errStr}`, { title: 'Error', kind: 'error' });
      } finally {
        isConnectingGDrive.value = false;
      }
    });
  };

  return {
    // State
    files, sources, isLoading, isScanning,
    activeSourceId, activeType, activeTag, searchQuery,
    filteredFiles, allTags,
    // GDrive
    isGDriveConnected, gdriveEmail, isConnectingGDrive,
    connectGDrive, syncGDrive, disconnectGDrive,
    // Sources
    fetchSources, fetchFiles, syncAllSources, addNewSource, removeSource, importFiles,
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
    init, setupAuthListener,
  };
}
