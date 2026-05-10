import { ref, computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { emit as tauriEmit } from '@tauri-apps/api/event';
import { logger } from '../../../utils/logger';
import type { WhiteboardMetadata } from '../../../types/ipc';

export interface WBNode {
  id: string;
  type: 'shape' | 'stroke' | 'mindmap' | 'text';
  position: { x: number; y: number };
  data: Record<string, any>;
}

export interface WBEdge {
  id: string;
  source: string;
  sourceHandle?: string;
  target: string;
  targetHandle?: string;
  type: string;
  data?: Record<string, any>;
}

export interface WhiteboardData {
  title: string;
  tags: string[];
  created_at: string;
  viewport: { x: number; y: number; zoom: number };
  nodes: WBNode[];
  edges: WBEdge[];
}

export type ToolMode = 'select' | 'draw' | 'shape' | 'mindmap' | 'text' | 'eraser';
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
      currentBoardData.value = JSON.parse(raw);
      currentBoardId.value = boardId;
      undoStack.value = [];
      redoStack.value = [];
    } catch (err) {
      logger.error('Failed to load whiteboard data', err as string);
    }
  }

  async function createBoard(title: string = 'Untitled Board') {
    try {
      const data: WhiteboardData = {
        title,
        tags: [],
        created_at: new Date().toISOString(),
        viewport: { x: 0, y: 0, zoom: 1 },
        nodes: [],
        edges: [],
      };
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
    } catch (err) {
      logger.error('Failed to create whiteboard', err as string);
    }
  }

  async function saveCurrentBoard() {
    if (!currentBoardData.value || !currentBoardId.value) return;
    const board = boards.value.find(b => b.id === currentBoardId.value);
    if (!board) return;

    try {
      isSaving.value = true;
      const content = JSON.stringify(currentBoardData.value, null, 2);
      await invoke('update_whiteboard', {
        vaultPath: vaultPath.value,
        path: board.path,
        title: currentBoardData.value.title,
        tags: currentBoardData.value.tags,
        content,
      });
      // Update local meta
      board.title = currentBoardData.value.title;
      board.tags = currentBoardData.value.tags;
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
  function pushUndoState() {
    if (!currentBoardData.value) return;
    const snapshot = JSON.stringify({
      nodes: currentBoardData.value.nodes,
      edges: currentBoardData.value.edges,
    });
    undoStack.value.push(snapshot);
    if (undoStack.value.length > MAX_UNDO) undoStack.value.shift();
    redoStack.value = [];
  }

  function undo() {
    if (!undoStack.value.length || !currentBoardData.value) return;
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
    currentBoardData.value.nodes.push(node);
  }

  function addEdge(edge: WBEdge) {
    if (!currentBoardData.value) return;
    pushUndoState();
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
      node.data = { ...node.data, ...data };
    }
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
    undoStack,
    redoStack,
    loadBoards,
    loadBoardData,
    createBoard,
    saveCurrentBoard,
    deleteBoard,
    pushUndoState,
    undo,
    redo,
    generateId,
    addNode,
    addEdge,
    removeNode,
    removeEdge,
    updateNodeData,
    getMindmapColor,
    addMindmapChild,
    addMindmapSibling,
    findParentId,
  };
}
