import { ref, computed, watch, markRaw } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { emit as tauriEmit } from '@tauri-apps/api/event';
import { logger } from '../../../utils/logger';
import { i18n } from '../../../i18n';
import { showAppNotice } from '../../../composables/useAppNotice';
import type { WhiteboardMetadata } from '../../../types/ipc';
import {
  BOARD_SCHEMA_VERSION,
  mergeBoards,
  newBoardData,
  recordDeletions,
  stampFields,
  readBoardFile,
  stampElement,
} from '../boardFile';
import type { WBEdge, WBNode, WhiteboardData } from '../boardFile';
import { restoreState, sameState, takeState, type BoardState } from '../undoSnapshots';
import { boardText, isStale, MAX_WRITE_TRIES, versionOf } from '../boardDisk';

// The file format lives in `boardFile`, which is the only thing that reads or
// writes one. Re-exported because every component in this app names these
// types through the store.
export type { WBEdge, WBNode, WhiteboardData };

export type ToolMode = 'select' | 'pan' | 'draw' | 'shape' | 'mindmap' | 'text' | 'eraser' | 'sticky' | 'frame';
export type DrawSubTool = 'pen' | 'highlighter' | 'eraser';
export type ShapeType = string;

const MINDMAP_COLORS = [
  '#7c3aed', '#3b82f6', '#10b981', '#f59e0b', '#ef4444',
  '#ec4899', '#8b5cf6', '#06b6d4', '#84cc16', '#f97316',
];

/** A yes/no kept in this device's storage; a blocked storage just forgets. */
function devicePreference(key: string, fallback: boolean) {
  let initial = fallback;
  try {
    const saved = localStorage.getItem(key);
    if (saved !== null) initial = saved === '1';
  } catch { /* storage unavailable */ }
  const value = ref(initial);
  watch(value, (v) => {
    try { localStorage.setItem(key, v ? '1' : '0'); } catch { /* storage unavailable */ }
  });
  return value;
}

export function useWhiteboardStore(vaultPath: { value: string }) {
  const boards = ref<WhiteboardMetadata[]>([]);
  const currentBoardId = ref<string | null>(null);
  const currentBoardData = ref<WhiteboardData | null>(null);
  const activeTool = ref<ToolMode>('select');
  const activeShapeType = ref<ShapeType>('rectangle');
  const activeColor = ref('#7c3aed');
  const backgroundPattern = ref<'dots' | 'lines' | 'none'>('dots');
  const backgroundColor = ref('transparent');
  /**
   * The canvas's two pulls on a dragged item: to the 10px grid, and into line
   * with other items. A preference of this device rather than of a board,
   * kept between sessions.
   */
  const snapToGrid = devicePreference('whiteboard:snap-to-grid', true);
  const smartGuides = devicePreference('whiteboard:smart-guides', true);
  const showMinimap = devicePreference('whiteboard:minimap', true);
  const drawSubTool = ref<DrawSubTool>('pen');
  const drawSizes = ref<Record<DrawSubTool, number>>({ pen: 3, highlighter: 12, eraser: 20 });
  const activeStrokeSize = computed({
    get: () => drawSizes.value[drawSubTool.value],
    set: (v: number) => { drawSizes.value[drawSubTool.value] = v; },
  });
  const isLoading = ref(false);
  const isSaving = ref(false);
  /**
   * Set when the open board was written by a newer build of the app.
   *
   * Nothing is loaded in that case and nothing may be saved: writing back a
   * file we only half understand would drop whatever the newer build put in
   * it, and the user would find out much later.
   */
  const currentBoardUnsupported = ref(false);

  /**
   * The open board's file as this window last read or wrote it: the text, to
   * notice when someone else has written since, and the board, as the common
   * ancestor a merge with their copy needs.
   */
  let diskRaw: string | null = null;
  let diskBase: WhiteboardData | null = null;

  function rememberDisk(raw: string) {
    const read = readBoardFile(raw);
    diskRaw = raw;
    diskBase = read.ok ? read.data : null;
  }

  /**
   * Counts the times the open board was changed from outside — merged with a
   * copy someone else wrote. The canvas redraws from the board when it moves.
   */
  const externalRevision = ref(0);

  /**
   * Why the open board's last save did not land, until one does.
   *
   * `failed` — the write went wrong (a full disk, a file held open by another
   * program); the next save tries again. `missing` — the file is no longer
   * where it was: deleted, moved or renamed somewhere else, and writing it
   * back would undo that. Either way the work is still on screen, and the
   * user has to be told it is not on disk.
   */
  const saveProblem = ref<null | 'failed' | 'missing'>(null);

  /**
   * Reads and writes of the open board, one at a time.
   *
   * A save is read, merge, write; a check for changes is read, merge. Two of
   * them interleaved would each merge against a file the other was about to
   * replace.
   */
  let diskQueue: Promise<unknown> = Promise.resolve();
  function queued<T>(work: () => Promise<T>): Promise<T> {
    const run = diskQueue.then(work, work);
    diskQueue = run.catch(() => {});
    return run;
  }

  // Undo/Redo
  const undoStack = ref<BoardState[]>([]);
  const redoStack = ref<BoardState[]>([]);
  const MAX_UNDO = 50;

  const currentBoard = computed(() =>
    boards.value.find(b => b.id === currentBoardId.value) || null
  );

  // ─── CRUD ──────────────────────────────────────────────
  async function loadBoards() {
    try {
      isLoading.value = true;
      boards.value = await invoke<WhiteboardMetadata[]>('scan_whiteboards', {
        vaultPath: vaultPath.value,
      });
    } catch (err) {
      logger.error('Failed to scan whiteboards', err as string);
      showAppNotice(i18n.global.t('whiteboard.fail.list'), 'error');
    } finally {
      isLoading.value = false;
    }
  }

  /**
   * Open a board. In the disk queue, behind any save or check still running
   * for the board being left: let in between, it swapped the open board out
   * from under that save, which then merged one board into the other.
   */
  function loadBoardData(boardId: string) {
    return queued(() => loadBoardDataNow(boardId));
  }

  async function loadBoardDataNow(boardId: string) {
    try {
      const board = boards.value.find(b => b.id === boardId);
      if (!board) return;
      const raw = await invoke<string>('read_whiteboard', {
        vaultPath: vaultPath.value,
        path: board.path,
      });
      const read = readBoardFile(raw);
      if (!read.ok) {
        currentBoardData.value = null;
        currentBoardId.value = boardId;
        currentBoardUnsupported.value = read.reason === 'too-new';
        logger.error(
          read.reason === 'too-new'
            ? `Whiteboard ${board.path} is version ${read.fileVersion}; this build reads ${BOARD_SCHEMA_VERSION}`
            : `Whiteboard ${board.path} is not readable JSON`
        );
        return;
      }
      currentBoardUnsupported.value = false;
      currentBoardData.value = read.data;
      currentBoardId.value = boardId;
      rememberDisk(raw);
      saveProblem.value = null;
      undoStack.value = [];
      redoStack.value = [];
    } catch (err) {
      logger.error('Failed to load whiteboard data', err as string);
      showAppNotice(i18n.global.t('whiteboard.fail.open'), 'error');
    }
  }

  /** A new board, opened. In the disk queue for the same reason as `loadBoardData`. */
  function createBoard(
    title: string = i18n.global.t('whiteboard.untitled_board'),
    start?: { nodes: WBNode[]; edges: WBEdge[] },
  ) {
    return queued(() => createBoardNow(title, start));
  }

  /** A new board, opened — empty, or with `start` on it (a template). */
  async function createBoardNow(title: string, start?: { nodes: WBNode[]; edges: WBEdge[] }) {
    try {
      const data = newBoardData(title);
      if (start) {
        for (const item of [...start.nodes, ...start.edges]) stampElement(item);
        data.nodes = start.nodes;
        data.edges = start.edges;
      }
      const content = boardText(data);
      const meta = await invoke<WhiteboardMetadata>('create_whiteboard', {
        vaultPath: vaultPath.value,
        title,
        tags: [] as string[],
        content,
      });
      boards.value.unshift(meta);
      currentBoardId.value = meta.id;
      currentBoardData.value = data;
      rememberDisk(content);
      currentBoardUnsupported.value = false;
      saveProblem.value = null;
      // The history belongs to the board that was open before; stepping back
      // here would put its items on this one.
      undoStack.value = [];
      redoStack.value = [];
    } catch (err) {
      logger.error('Failed to create whiteboard', err as string);
      showAppNotice(i18n.global.t('whiteboard.fail.create'), 'error');
    }
  }

  /**
   * Record when this save happened, inside the file.
   *
   * Sync settles two copies of a board by comparing `metadata.updated_at` as
   * a string. Boards never wrote one, so both sides read as empty, and the
   * comparison is `remote >= local` — an empty string is not greater than an
   * empty string, so the remote copy won every time, including when it was
   * the older of the two. A board edited here could be replaced by a stale
   * copy from another device, silently.
   *
   * UTC, in RFC 3339, because the two devices being compared need not share
   * a time zone and the comparison is lexicographic.
   */
  function stampSave(data: WhiteboardData) {
    data.schemaVersion = BOARD_SCHEMA_VERSION;
    data.metadata = { ...(data.metadata || {}), updated_at: new Date().toISOString() };
  }

  /**
   * Bring someone else's copy of the open board in, keeping what is here.
   *
   * Returns true when the file had changed. The history is cleared when it
   * had: every step in it is a snapshot from before their change, and
   * stepping back to one would quietly take their work off the board again.
   */
  function absorb(raw: string): boolean {
    if (raw === diskRaw || !currentBoardData.value) return false;
    const read = readBoardFile(raw);
    // A file this build cannot read is not something to merge with. Unreadable
    // — half a file, from before writes were atomic — is healed by the save
    // that follows; too new is refused there.
    if (!read.ok) return false;
    const merged = diskBase ? mergeBoards(diskBase, currentBoardData.value, read.data) : read.data;
    currentBoardData.value = merged;
    // Their copy is now the common ancestor; parsed afresh, so it shares no
    // objects with the board being edited.
    rememberDisk(raw);
    undoStack.value = [];
    redoStack.value = [];
    externalRevision.value++;
    return true;
  }

  async function readOpenBoard(path: string): Promise<string | null> {
    try {
      return await invoke<string>('read_whiteboard', { vaultPath: vaultPath.value, path });
    } catch {
      // Gone, or unreadable for now: the caller carries on with what it has.
      return null;
    }
  }

  /**
   * Check the open board's file and take in whatever someone else wrote.
   *
   * Called when the vault reports a change, and when the app comes back on
   * screen. Cheap when nothing changed: one read and a string comparison.
   */
  function syncWithDisk(): Promise<boolean> {
    return queued(async () => {
      if (currentBoardUnsupported.value || !currentBoardData.value || !currentBoardId.value) return false;
      const board = boards.value.find(b => b.id === currentBoardId.value);
      if (!board) return false;
      const open = currentBoardData.value;
      const raw = await readOpenBoard(board.path);
      // The board on screen may have changed while the file was read; this
      // file belongs to the one that was open when it started.
      if (currentBoardData.value !== open || currentBoardId.value !== board.id) return false;
      if (raw === null) return false;
      const read = readBoardFile(raw);
      if (!read.ok && read.reason === 'too-new') {
        // Saved by a newer build since it was opened here. Nothing more may
        // be written from this window.
        await keepAsideUnsaved(open);
        currentBoardUnsupported.value = true;
        currentBoardData.value = null;
        return true;
      }
      return absorb(raw);
    });
  }

  /**
   * Boards this window wrote, and when. The vault reports every write back as
   * a change, ours included, and each report used to cost a rescan of every
   * board, a read of this one and a re-ask of every live frame — for a change
   * that was already on screen.
   */
  const ownWrites = new Map<string, number>();
  /** The next save keeps the file it replaces as a version, whenever it lands (a restore). */
  /**
   * The board whose next save keeps the file it replaces (a restore). By
   * board, not for whatever is saved next: a restore whose save did not go
   * through used to make a later save — of another board — keep a version.
   */
  let keepNextVersionFor: string | null = null;
  const OWN_ECHO_MS = 3000;
  function wroteRecently(path: string): boolean {
    for (const [mine, at] of ownWrites) {
      if (Date.now() - at > OWN_ECHO_MS) ownWrites.delete(mine);
      else if (path === mine || path.endsWith(`/${mine}`)) return true;
    }
    return false;
  }

  /**
   * The title of a board the open board's unsaved changes were kept in, when
   * the board itself turned out to have been saved by a newer build.
   */
  const keptAside = ref<string | null>(null);

  /**
   * Keep what was changed here as a board of its own, before the open board
   * is closed for having been saved by a newer build. It cannot be merged into
   * that file — this build does not know all of what is in it — and dropping
   * it would lose the work.
   */
  async function keepAsideUnsaved(data: WhiteboardData) {
    if (!diskBase) return;
    if (sameState(takeState(diskBase), takeState(data)) && diskBase.title === data.title) return;
    const title = i18n.global.t('whiteboard.kept_aside_title', { title: data.title || i18n.global.t('whiteboard.untitled') });
    const copy: WhiteboardData = { ...JSON.parse(JSON.stringify(data)), title };
    stampSave(copy);
    try {
      const meta = await invoke<WhiteboardMetadata>('create_whiteboard', {
        vaultPath: vaultPath.value,
        title,
        tags: copy.tags || [],
        content: boardText(copy),
      });
      boards.value.unshift(meta);
      keptAside.value = title;
    } catch (err) {
      logger.error('Could not keep the unsaved changes aside', err as string);
    }
  }

  /**
   * Write the open board.
   *
   * Reads the file first: anything written there since this window last
   * looked is merged in before this copy goes over it, so the save keeps both.
   */
  function saveCurrentBoard() {
    return queued(async () => {
      if (currentBoardUnsupported.value) return;
      if (!currentBoardData.value || !currentBoardId.value) return;
      const board = boards.value.find(b => b.id === currentBoardId.value);
      if (!board) {
        // Gone from the list: the file was deleted or moved elsewhere.
        saveProblem.value = 'missing';
        return;
      }
      // The board being saved. Taking in someone else's copy replaces the
      // object (see `absorb`), so it is followed here: compared against the
      // copy from before the merge, a second refused write looked like the
      // board had been closed, and the save gave up with the merged board
      // only in memory and nothing said.
      let open = currentBoardData.value;
      const stillOpen = () => currentBoardData.value === open && currentBoardId.value === board.id;

      try {
        isSaving.value = true;
        let content = '';
        let data: WhiteboardData;
        // The file this write is made from: the copy last read or written, or
        // what a read below found — taken in, or (half a file) to be healed.
        let madeFrom = diskRaw;
        // The copy last read or written is trusted to be the file: the write
        // says so, and is refused if it is not. Only then is the file read,
        // and what is in it combined with this copy before trying again.
        for (let attempt = 1; ; attempt++) {
          if (attempt > 1 || madeFrom === null) {
            const onDisk = await readOpenBoard(board.path);
            if (!stillOpen()) return;
            if (onDisk === null) {
              // Not there to merge with, and not to be written back either: a
              // board deleted or moved elsewhere would come back from the dead.
              saveProblem.value = 'missing';
              return;
            }
            const read = readBoardFile(onDisk);
            if (!read.ok && read.reason === 'too-new') {
              await keepAsideUnsaved(open);
              currentBoardUnsupported.value = true;
              currentBoardData.value = null;
              logger.error(`Whiteboard ${board.path} was saved by a newer build; not writing over it`);
              return;
            }
            absorb(onDisk);
            open = currentBoardData.value;
            madeFrom = onDisk;
          }

          data = currentBoardData.value!;
          recordDeletions(diskBase, data);
          stampFields(diskBase, data);
          stampSave(data);
          content = boardText(data);
          try {
            await invoke('update_whiteboard', {
              vaultPath: vaultPath.value,
              path: board.path,
              title: data.title || i18n.global.t('whiteboard.untitled'),
              tags: data.tags || [],
              content,
              expected: madeFrom === null ? undefined : await versionOf(madeFrom),
              keep: keepNextVersionFor === board.id || undefined,
            });
            if (keepNextVersionFor === board.id) keepNextVersionFor = null;
            ownWrites.set(board.path, Date.now());
            break;
          } catch (err) {
            if (!isStale(err) || attempt >= MAX_WRITE_TRIES) throw err;
            if (!stillOpen()) return;
          }
        }
        saveProblem.value = null;
        // Unless the board was closed while the write was out (a delete does
        // that outside the queue), this is the copy both sides now share.
        if (currentBoardData.value === data) rememberDisk(content);
        board.title = data.title || i18n.global.t('whiteboard.untitled');
        board.tags = data.tags || [];
        // Notify embedded previews in notes to reload
        tauriEmit('whiteboard-updated', { path: board.path, id: board.id });
      } catch (err) {
        saveProblem.value = 'failed';
        logger.error('Failed to save whiteboard', err as string);
      } finally {
        isSaving.value = false;
      }
    });
  }

  /**
   * Move a board to the trash. A failed move throws: the caller holds this
   * behind an undo window, and only a thrown error lets it put the board back
   * and say so — swallowed here, the board just vanished until the next
   * rescan brought it back unexplained. Opening another board afterwards is a
   * refresh, not the delete, so its failure is only logged.
   */
  async function deleteBoard(boardId: string) {
    const board = boards.value.find(b => b.id === boardId);
    if (!board) return;
    await invoke('delete_whiteboard', {
      vaultPath: vaultPath.value,
      path: board.path,
    });
    boards.value = boards.value.filter(b => b.id !== boardId);
    if (currentBoardId.value === boardId) {
      currentBoardId.value = boards.value[0]?.id || null;
      try {
        if (currentBoardId.value) {
          await loadBoardData(currentBoardId.value);
        } else {
          currentBoardData.value = null;
        }
      } catch (err) {
        logger.error('Failed to open a board after deleting one', err as string);
      }
    }
  }

  // ─── Undo/Redo ─────────────────────────────────────────
  //
  // One entry per thing the user did. Getting there needs two guards, because
  // the code below calls `pushUndoState` far more often than a person acts:
  //
  //   - a batch, for an action that is many operations. Erasing across a
  //     drawing removes and rebuilds a stroke per pointer event, and each one
  //     used to push; a single wipe could push every other entry off the end
  //     of a fifty-deep stack, leaving nothing to go back to.
  //   - coalescing, for an action that arrives as a stream. A colour slider
  //     and the label field both emit on `input`, so dragging one produces a
  //     push per pixel. Repeats against the same target inside the window
  //     below fold into the first, which is the state the user wants back.
  const COALESCE_MS = 700;
  let batchDepth = 0;
  let batchPushed = false;
  let lastPushKey: string | null = null;
  let lastPushAt = 0;

  /**
   * Treat everything until `endUndoBatch` as one action.
   *
   * Nests: only the outermost pair records anything, so a batched operation
   * can call another one without splitting the entry.
   */
  function beginUndoBatch() {
    if (batchDepth === 0) batchPushed = false;
    batchDepth++;
  }

  function endUndoBatch() {
    if (batchDepth > 0) batchDepth--;
  }

  /**
   * Record the state to come back to, before changing it.
   *
   * `coalesceKey` names what is being changed — a node id, usually. Two
   * pushes with the same key in quick succession keep only the first.
   */
  function pushUndoState(coalesceKey?: string) {
    if (!currentBoardData.value) return;

    if (batchDepth > 0) {
      if (batchPushed) return;
      batchPushed = true;
    } else if (coalesceKey) {
      const now = Date.now();
      const repeat = coalesceKey === lastPushKey && now - lastPushAt < COALESCE_MS;
      lastPushKey = coalesceKey;
      lastPushAt = now;
      if (repeat) return;
    } else {
      lastPushKey = null;
    }

    const snapshot = markRaw(takeState(currentBoardData.value));
    // An operation that changed nothing is not a step back to anywhere.
    if (sameState(undoStack.value[undoStack.value.length - 1], snapshot)) return;

    undoStack.value.push(snapshot);
    if (undoStack.value.length > MAX_UNDO) undoStack.value.shift();
    redoStack.value = [];
  }

  function undo() {
    if (!undoStack.value.length || !currentBoardData.value) return;
    lastPushKey = null;
    redoStack.value.push(markRaw(takeState(currentBoardData.value)));
    const prev = restoreState(undoStack.value.pop()!, currentBoardData.value);
    currentBoardData.value.nodes = prev.nodes;
    currentBoardData.value.edges = prev.edges;
  }

  /**
   * Put an earlier version of the open board back, as a change made now: one
   * step back undoes it, and what it changes is stamped so a sync carries it
   * to other devices instead of undoing it.
   */
  function restoreVersion(version: WhiteboardData) {
    if (!currentBoardData.value) return;
    keepNextVersionFor = currentBoardId.value;
    pushUndoState();
    const next = restoreState(takeState(version), currentBoardData.value);
    currentBoardData.value.nodes = next.nodes;
    currentBoardData.value.edges = next.edges;
    if (version.title && version.title !== currentBoardData.value.title) currentBoardData.value.title = version.title;
  }

  function redo() {
    if (!redoStack.value.length || !currentBoardData.value) return;
    lastPushKey = null;
    undoStack.value.push(markRaw(takeState(currentBoardData.value)));
    const next = restoreState(redoStack.value.pop()!, currentBoardData.value);
    currentBoardData.value.nodes = next.nodes;
    currentBoardData.value.edges = next.edges;
  }

  // ─── Node Helpers ──────────────────────────────────────
  function generateId(prefix: string = 'node') {
    return `${prefix}_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
  }

  function addNode(node: WBNode) {
    if (!currentBoardData.value) return;
    pushUndoState();
    stampElement(node);
    currentBoardData.value.nodes.push(node);
  }

  function addEdge(edge: WBEdge) {
    if (!currentBoardData.value) return;
    pushUndoState();
    stampElement(edge);
    currentBoardData.value.edges.push(edge);
  }

  function removeNode(nodeId: string) {
    if (!currentBoardData.value) return;
    pushUndoState();
    currentBoardData.value.nodes = currentBoardData.value.nodes.filter(n => n.id !== nodeId);
    currentBoardData.value.edges = currentBoardData.value.edges.filter(
      e => e.source !== nodeId && e.target !== nodeId
    );
  }

  function removeEdge(edgeId: string) {
    if (!currentBoardData.value) return;
    pushUndoState();
    currentBoardData.value.edges = currentBoardData.value.edges.filter(e => e.id !== edgeId);
  }

  function updateNodeData(nodeId: string, data: Record<string, any>) {
    if (!currentBoardData.value) return;
    const node = currentBoardData.value.nodes.find(n => n.id === nodeId);
    if (node) {
      // Before the change, keyed on the node: a slider dragged across its
      // range is one step back, not two hundred.
      pushUndoState(`data:${nodeId}`);
      node.data = { ...node.data, ...data };
      stampElement(node);
    }
  }

  /** Record that a node moved. Position is written by the canvas itself. */
  function stampNode(nodeId: string) {
    const node = currentBoardData.value?.nodes.find(n => n.id === nodeId);
    if (node) stampElement(node);
  }

  function getMindmapColor(level: number): string {
    return MINDMAP_COLORS[level % MINDMAP_COLORS.length];
  }

  function addMindmapChild(parentId: string, direction: 'right' | 'left' = 'right') {
    if (!currentBoardData.value) return;
    const parent = currentBoardData.value.nodes.find(n => n.id === parentId);
    if (!parent) return;

    const parentLevel = parent.data.level || 0;
    const childLevel = parentLevel + 1;

    // Count only siblings in the same direction
    const allChildEdges = currentBoardData.value.edges.filter(e => e.source === parentId);
    const sameDirectionChildren = allChildEdges.filter(e => {
      const childNode = currentBoardData.value!.nodes.find(n => n.id === e.target);
      return childNode?.data?.direction === direction;
    });
    const offsetIndex = sameDirectionChildren.length;

    let childPos: { x: number; y: number };
    if (direction === 'left') {
      childPos = {
        x: parent.position.x - 220,
        y: parent.position.y + offsetIndex * 80,
      };
    } else {
      childPos = {
        x: parent.position.x + 220,
        y: parent.position.y + offsetIndex * 80,
      };
    }

    const childId = generateId('mind');
    const childNode: WBNode = {
      id: childId,
      type: 'mindmap',
      position: childPos,
      data: {
        label: '',
        color: getMindmapColor(childLevel),
        level: childLevel,
        editing: true,
        direction, // preserve direction for sub-children
      },
    };

    const edge: WBEdge = {
      id: generateId('e'),
      source: parentId,
      target: childId,
      sourceHandle: direction === 'left' ? 'left-source' : 'right-source',
      targetHandle: direction === 'left' ? 'right-target' : 'left-target',
      type: 'default',
      data: {},
    };

    pushUndoState();
    stampElement(childNode);
    stampElement(edge);
    currentBoardData.value.nodes.push(childNode);
    currentBoardData.value.edges.push(edge);

    return childId;
  }

  function findParentId(nodeId: string): string | null {
    if (!currentBoardData.value) return null;
    const parentEdge = currentBoardData.value.edges.find(e => e.target === nodeId);
    return parentEdge ? parentEdge.source : null;
  }

  function addMindmapSibling(nodeId: string) {
    if (!currentBoardData.value) return;
    const node = currentBoardData.value.nodes.find(n => n.id === nodeId);
    if (!node) return;
    const parentId = findParentId(nodeId);
    if (!parentId) {
      // Root node — create sibling as another root below
      const siblingId = generateId('mind');
      const siblingNode: WBNode = {
        id: siblingId,
        type: 'mindmap',
        position: { x: node.position.x, y: node.position.y + 120 },
        data: {
          label: '',
          color: getMindmapColor(0),
          level: 0,
          editing: true,
        },
      };
      pushUndoState();
      stampElement(siblingNode);
      currentBoardData.value.nodes.push(siblingNode);
      return siblingId;
    }
    // Has parent — add another child to the same parent, preserving direction
    const direction = node.data?.direction || 'right';
    return addMindmapChild(parentId, direction);
  }

  return {
    boards,
    currentBoardId,
    currentBoardData,
    keptAside,
    wroteRecently,
    restoreVersion,
    currentBoard,
    activeTool,
    activeShapeType,
    activeColor,
    activeStrokeSize,
    backgroundPattern,
    backgroundColor,
    snapToGrid,
    smartGuides,
    showMinimap,
    drawSubTool,
    isLoading,
    isSaving,
    currentBoardUnsupported,
    undoStack,
    redoStack,
    loadBoards,
    loadBoardData,
    createBoard,
    saveCurrentBoard,
    syncWithDisk,
    externalRevision,
    saveProblem,
    deleteBoard,
    pushUndoState,
    beginUndoBatch,
    endUndoBatch,
    undo,
    redo,
    generateId,
    addNode,
    addEdge,
    removeNode,
    removeEdge,
    updateNodeData,
    stampNode,
    getMindmapColor,
    addMindmapChild,
    addMindmapSibling,
    findParentId,
  };
}
