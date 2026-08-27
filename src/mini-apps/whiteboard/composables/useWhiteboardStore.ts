import { ref, computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { emit as tauriEmit } from '@tauri-apps/api/event';
import { logger } from '../../../utils/logger';
import type { WhiteboardMetadata } from '../../../types/ipc';
import {
  BOARD_SCHEMA_VERSION,
  newBoardData,
  readBoardFile,
  stampElement,
} from '../boardFile';
import type { WBEdge, WBNode, WhiteboardData } from '../boardFile';

// The file format lives in `boardFile`, which is the only thing that reads or
// writes one. Re-exported because every component in this app names these
// types through the store.
export type { WBEdge, WBNode, WhiteboardData };

export type ToolMode = 'select' | 'pan' | 'draw' | 'shape' | 'mindmap' | 'text' | 'eraser';
export type DrawSubTool = 'pen' | 'highlighter' | 'eraser';
export type ShapeType = string;

const MINDMAP_COLORS = [
  '#7c3aed', '#3b82f6', '#10b981', '#f59e0b', '#ef4444',
  '#ec4899', '#8b5cf6', '#06b6d4', '#84cc16', '#f97316',
];

export function useWhiteboardStore(vaultPath: { value: string }) {
  const boards = ref<WhiteboardMetadata[]>([]);
  const currentBoardId = ref<string | null>(null);
  const currentBoardData = ref<WhiteboardData | null>(null);
  const activeTool = ref<ToolMode>('select');
  const activeShapeType = ref<ShapeType>('rectangle');
  const activeColor = ref('#7c3aed');
  const backgroundPattern = ref<'dots' | 'lines' | 'none'>('dots');
  const backgroundColor = ref('transparent');
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

  // Undo/Redo
  const undoStack = ref<string[]>([]);
  const redoStack = ref<string[]>([]);
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
    } finally {
      isLoading.value = false;
    }
  }

  async function loadBoardData(boardId: string) {
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
      undoStack.value = [];
      redoStack.value = [];
    } catch (err) {
      logger.error('Failed to load whiteboard data', err as string);
    }
  }

  async function createBoard(title: string = 'Untitled Board') {
    try {
      const data = newBoardData(title);
      const content = JSON.stringify(data, null, 2);
      const meta = await invoke<WhiteboardMetadata>('create_whiteboard', {
        vaultPath: vaultPath.value,
        title,
        tags: [] as string[],
        content,
      });
      boards.value.unshift(meta);
      currentBoardId.value = meta.id;
      currentBoardData.value = data;
      currentBoardUnsupported.value = false;
    } catch (err) {
      logger.error('Failed to create whiteboard', err as string);
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

  async function saveCurrentBoard() {
    if (currentBoardUnsupported.value) return;
    if (!currentBoardData.value || !currentBoardId.value) return;
    const board = boards.value.find(b => b.id === currentBoardId.value);
    if (!board) return;

    try {
      isSaving.value = true;
      stampSave(currentBoardData.value);
      const content = JSON.stringify(currentBoardData.value, null, 2);
      await invoke('update_whiteboard', {
        vaultPath: vaultPath.value,
        path: board.path,
        title: currentBoardData.value.title || 'Untitled',
        tags: currentBoardData.value.tags || [],
        content,
      });
      // Update local meta
      board.title = currentBoardData.value.title || 'Untitled';
      board.tags = currentBoardData.value.tags || [];
      // Notify embedded previews in notes to reload
      tauriEmit('whiteboard-updated', { path: board.path, id: board.id });
    } catch (err) {
      logger.error('Failed to save whiteboard', err as string);
    } finally {
      isSaving.value = false;
    }
  }

  async function deleteBoard(boardId: string) {
    const board = boards.value.find(b => b.id === boardId);
    if (!board) return;
    try {
      await invoke('delete_whiteboard', {
        vaultPath: vaultPath.value,
        path: board.path,
      });
      boards.value = boards.value.filter(b => b.id !== boardId);
      if (currentBoardId.value === boardId) {
        currentBoardId.value = boards.value[0]?.id || null;
        if (currentBoardId.value) {
          await loadBoardData(currentBoardId.value);
        } else {
          currentBoardData.value = null;
        }
      }
    } catch (err) {
      logger.error('Failed to delete whiteboard', err as string);
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

    const snapshot = JSON.stringify({
      nodes: currentBoardData.value.nodes,
      edges: currentBoardData.value.edges,
    });
    // An operation that changed nothing is not a step back to anywhere.
    if (snapshot === undoStack.value[undoStack.value.length - 1]) return;

    undoStack.value.push(snapshot);
    if (undoStack.value.length > MAX_UNDO) undoStack.value.shift();
    redoStack.value = [];
  }

  function undo() {
    if (!undoStack.value.length || !currentBoardData.value) return;
    lastPushKey = null;
    const currentSnapshot = JSON.stringify({
      nodes: currentBoardData.value.nodes,
      edges: currentBoardData.value.edges,
    });
    redoStack.value.push(currentSnapshot);
    const prev = JSON.parse(undoStack.value.pop()!);
    currentBoardData.value.nodes = prev.nodes;
    currentBoardData.value.edges = prev.edges;
  }

  function redo() {
    if (!redoStack.value.length || !currentBoardData.value) return;
    lastPushKey = null;
    const currentSnapshot = JSON.stringify({
      nodes: currentBoardData.value.nodes,
      edges: currentBoardData.value.edges,
    });
    undoStack.value.push(currentSnapshot);
    const next = JSON.parse(redoStack.value.pop()!);
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
    currentBoard,
    activeTool,
    activeShapeType,
    activeColor,
    activeStrokeSize,
    backgroundPattern,
    backgroundColor,
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
