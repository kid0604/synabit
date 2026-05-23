<script setup lang="ts">
import { ref, computed, onMounted, watch, toRef, nextTick, inject } from 'vue';
import { VueFlow, useVueFlow, ConnectionMode, MarkerType, getRectOfNodes } from '@vue-flow/core';
import { Background } from '@vue-flow/background';
import { Controls } from '@vue-flow/controls';
import { Plus, Trash2, PenTool, PanelLeftClose, PanelLeft, Tag, X, FileText, Search, GripVertical, ChevronDown, ChevronRight } from 'lucide-vue-next';

// Edge, Shape, Text & Multi-select menus
import EdgeMenu from './components/EdgeMenu.vue';
import ShapeMenu from './components/ShapeMenu.vue';
import TextMenu from './components/TextMenu.vue';
import MultiSelectMenu from './components/MultiSelectMenu.vue';
import { toPng } from 'html-to-image';
import { ask } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import NavButtons from '../../shared/components/NavButtons.vue';

// Custom nodes
import ShapeNode from './nodes/ShapeNode.vue';
import StrokeNode from './nodes/StrokeNode.vue';
import MindmapNode from './nodes/MindmapNode.vue';
import TextNode from './nodes/TextNode.vue';
import NoteCardNode from './nodes/NoteCardNode.vue';

// Toolbar
import WhiteboardToolbar from './components/WhiteboardToolbar.vue';

// Composables
import { useWhiteboardStore } from './composables/useWhiteboardStore';
import { useFreeDrawing, getStroke, getSvgPathFromStroke } from './composables/useFreeDrawing';
import type { WBNode, WBEdge } from './composables/useWhiteboardStore';
import { SHAPES_MAP } from './shapes';
import { logger } from '../../utils/logger';
import type { NavEntry } from '../../stores/useNavigationStore';

// CSS
import '@vue-flow/core/dist/style.css';
import '@vue-flow/core/dist/theme-default.css';
import '@vue-flow/node-resizer/dist/style.css';

const props = defineProps<{
  vaultPath: string;
}>();

const vaultPathRef = toRef(props, 'vaultPath');
const store = useWhiteboardStore(vaultPathRef);

// ─── Intra-app navigation ──────────────────────────────────
const pushNavigation = inject<(entry?: NavEntry) => void>('pushNavigation');
let skipNavPush = false;

const switchBoard = (boardId: string) => {
    if (boardId !== store.currentBoardId.value && store.currentBoardId.value && !skipNavPush) {
        pushNavigation?.({ app: 'whiteboard', itemId: store.currentBoardId.value });
    }
    store.loadBoardData(boardId);
};

// ─── Vue Flow ───────────────────────────────────────────
const { setViewport, getViewport, getNodes } = useVueFlow({ id: 'whiteboard-flow' });

// Use refs (not computed) so VueFlow can track node identity for drag operations.
const vfNodes = ref<any[]>([]);
const vfEdges = ref<any[]>([]);

// ─── Clipboard for Ctrl+C / Ctrl+V ──────────────────────
let clipboard: { type: string; data: any; position: { x: number; y: number } } | null = null;

/**
 * Compute z-index for shape nodes based on area.
 * Smaller shapes get higher z-index so they are always clickable
 * above larger shapes that contain them (like Miro).
 */
function computeShapeZIndex(w: number, h: number): number {
  const area = w * h;
  // Max area ~1000×1000 = 1_000_000. Invert so small = high z.
  return Math.max(1, Math.round(10000 - area / 100));
}

// Sync store → VueFlow refs
function syncToVueFlow() {
  if (!store.currentBoardData.value) {
    vfNodes.value = [];
    vfEdges.value = [];
    return;
  }
  vfNodes.value = store.currentBoardData.value.nodes.map(n => {
    const node: any = {
      id: n.id,
      type: n.type,
      position: { ...n.position },
      data: { ...n.data },
      draggable: true,
    };
    // Shape nodes need explicit dimensions on the wrapper for NodeResizer
    if (n.type === 'shape') {
      const w = n.data.width || 160;
      const h = n.data.height || 80;
      node.style = {
        width: `${w}px`,
        height: `${h}px`,
      };
      node.zIndex = computeShapeZIndex(w, h);
    }
    return node;
  });
  vfEdges.value = store.currentBoardData.value.edges.map(e => {
    const d = e.data || {};
    const edgeObj: any = {
      id: e.id,
      source: e.source,
      sourceHandle: e.sourceHandle,
      target: e.target,
      targetHandle: e.targetHandle,
      type: e.type || 'default',
      animated: !!d.animated,
      label: d.label || '',
      style: {
        stroke: d.color || undefined,
        strokeWidth: d.strokeWidth ? `${d.strokeWidth}px` : undefined,
        strokeDasharray: d.dashStyle === 'dashed' ? '8 4' : d.dashStyle === 'dotted' ? '2 4' : undefined,
      },
      data: d,
    };
    // Apply markers
    if (d.markerEnd === 'arrow') {
      edgeObj.markerEnd = { type: MarkerType.ArrowClosed, color: d.color || undefined };
    }
    if (d.markerStart === 'arrow') {
      edgeObj.markerStart = { type: MarkerType.ArrowClosed, color: d.color || undefined };
    }
    // Set edge z-index ABOVE all shape z-indices so edges are always clickable
    edgeObj.zIndex = 10001;
    return edgeObj;
  });
}

// Only sync on board switch — NOT on every data mutation.
watch(
  () => store.currentBoardId.value,
  () => syncToVueFlow(),
);

const { viewport, screenToFlowCoordinate, removeEdges, addEdges, addSelectedNodes, updateNodeData: vfUpdateNodeData } = useVueFlow({ id: 'whiteboard-flow' });

// ─── VueFlow change handlers ────────────────────────────
// Handles all node changes from VueFlow: position (drag), remove (delete key)
function handleNodesChange(changes: any[]) {
  if (!store.currentBoardData.value) return;
  let dirty = false;
  for (const change of changes) {
    if (change.type === 'position' && change.position) {
      const wbNode = store.currentBoardData.value.nodes.find((n: WBNode) => n.id === change.id);
      if (wbNode) {
        wbNode.position = { x: change.position.x, y: change.position.y };
        dirty = true;
      }
    } else if (change.type === 'remove') {
      store.currentBoardData.value.nodes = store.currentBoardData.value.nodes.filter(
        (n: WBNode) => n.id !== change.id
      );
      store.currentBoardData.value.edges = store.currentBoardData.value.edges.filter(
        (e: WBEdge) => e.source !== change.id && e.target !== change.id
      );
      dirty = true;
    }
  }
  if (dirty) scheduleSave();
}

function handleEdgesChange(changes: any[]) {
  if (!store.currentBoardData.value) return;
  let dirty = false;
  for (const change of changes) {
    if (change.type === 'remove') {
      // Skip remove if we're just re-creating the edge for a marker update
      if (updatingEdgeId === change.id) continue;
      store.currentBoardData.value.edges = store.currentBoardData.value.edges.filter(
        (e: WBEdge) => e.id !== change.id
      );
      dirty = true;
    }
  }
  if (dirty) scheduleSave();
}

function handleConnect(params: any) {
  const edge: any = {
    id: store.generateId('e'),
    source: params.source,
    sourceHandle: params.sourceHandle,
    target: params.target,
    targetHandle: params.targetHandle,
    type: 'default',
    data: {},
    zIndex: 10001,
  };
  store.addEdge(edge);
  vfEdges.value = [...vfEdges.value, edge];
  scheduleSave();
}

// ─── Edge Menu ──────────────────────────────────────────
const selectedEdgeId = ref<string | null>(null);
const edgeMenuPos = ref({ x: 0, y: 0 });

const selectedEdgeData = computed(() => {
  if (!selectedEdgeId.value || !store.currentBoardData.value) return null;
  const edge = store.currentBoardData.value.edges.find((e: WBEdge) => e.id === selectedEdgeId.value);
  if (!edge) return null;
  return {
    type: edge.type || 'default',
    color: edge.data?.color || '',
    strokeWidth: edge.data?.strokeWidth || 2,
    animated: edge.data?.animated || false,
    label: edge.data?.label || '',
    markerEnd: edge.data?.markerEnd || 'none',
    markerStart: edge.data?.markerStart || 'none',
    dashStyle: edge.data?.dashStyle || 'solid',
  };
});

function handleEdgeClick({ edge, event }: any) {
  if (store.activeTool.value !== 'select') return;
  selectedEdgeId.value = edge.id;
  edgeMenuPos.value = { x: event.clientX, y: event.clientY };
}

// Guard flag to prevent handleEdgesChange from deleting the edge during update
let updatingEdgeId: string | null = null;

function handleEdgeUpdate(edgeId: string, data: Record<string, any>) {
  if (!store.currentBoardData.value) return;
  const wbEdge = store.currentBoardData.value.edges.find((e: WBEdge) => e.id === edgeId);
  if (!wbEdge) return;

  // Update store
  wbEdge.type = data.type || wbEdge.type;
  wbEdge.data = { ...wbEdge.data, ...data };

  // Rebuild the full VueFlow edge object
  const d = wbEdge.data || {};
  const newVfEdge: any = {
    id: wbEdge.id,
    source: wbEdge.source,
    sourceHandle: wbEdge.sourceHandle,
    target: wbEdge.target,
    targetHandle: wbEdge.targetHandle,
    type: wbEdge.type || 'default',
    animated: !!d.animated,
    label: d.label || '',
    style: {
      stroke: d.color || undefined,
      strokeWidth: d.strokeWidth ? `${d.strokeWidth}px` : undefined,
      strokeDasharray: d.dashStyle === 'dashed' ? '8 4' : d.dashStyle === 'dotted' ? '2 4' : undefined,
    },
    data: d,
    zIndex: 10001,
  };
  if (d.markerEnd === 'arrow') {
    newVfEdge.markerEnd = { type: MarkerType.ArrowClosed, color: d.color || undefined };
  }
  if (d.markerStart === 'arrow') {
    newVfEdge.markerStart = { type: MarkerType.ArrowClosed, color: d.color || undefined };
  }

  // Set guard flag, remove old edge, add new edge, clear flag
  updatingEdgeId = edgeId;
  removeEdges([edgeId]);
  addEdges([newVfEdge]);
  updatingEdgeId = null;

  scheduleSave();
}

function handleEdgeDelete(edgeId: string) {
  store.removeEdge(edgeId);
  vfEdges.value = vfEdges.value.filter((e: any) => e.id !== edgeId);
  selectedEdgeId.value = null;
  scheduleSave();
}

function closeEdgeMenu() {
  selectedEdgeId.value = null;
}

// ─── Shape Menu ─────────────────────────────────────────
const selectedShapeNodeId = ref<string | null>(null);
const shapeMenuPos = ref({ x: 0, y: 0 });

const selectedShapeData = computed(() => {
  if (!selectedShapeNodeId.value || !store.currentBoardData.value) return null;
  const node = store.currentBoardData.value.nodes.find((n: WBNode) => n.id === selectedShapeNodeId.value);
  if (!node || node.type !== 'shape') return null;
  return {
    shapeType: node.data.shapeType || 'rectangle',
    label: node.data.label || '',
    color: node.data.color || '#7c3aed',
    fillColor: node.data.fillColor || '',
    borderWidth: node.data.borderWidth || 2,
    dashStyle: node.data.dashStyle || 'solid',
    opacity: node.data.opacity ?? 100,
    fontSize: node.data.fontSize || 13,
  };
});

function handleShapeUpdate(nodeId: string, data: Record<string, any>) {
  if (!store.currentBoardData.value) return;
  const wbNode = store.currentBoardData.value.nodes.find((n: WBNode) => n.id === nodeId);
  if (!wbNode) return;
  wbNode.data = { ...wbNode.data, ...data };

  // Sync to VueFlow
  const vfNode = vfNodes.value.find((n: any) => n.id === nodeId);
  if (vfNode) {
    vfNode.data = { ...vfNode.data, ...data };
    vfNodes.value = [...vfNodes.value];
  }
  scheduleSave();
}

function handleShapeDelete(nodeId: string) {
  store.removeNode(nodeId);
  vfNodes.value = vfNodes.value.filter((n: any) => n.id !== nodeId);
  vfEdges.value = vfEdges.value.filter((e: any) => e.source !== nodeId && e.target !== nodeId);
  selectedShapeNodeId.value = null;
  scheduleSave();
}

function closeShapeMenu() {
  selectedShapeNodeId.value = null;
}

// ─── Text Menu ──────────────────────────────────────────
const selectedTextNodeId = ref<string | null>(null);

const selectedTextData = computed(() => {
  if (!selectedTextNodeId.value || !store.currentBoardData.value) return null;
  const node = store.currentBoardData.value.nodes.find((n: WBNode) => n.id === selectedTextNodeId.value);
  if (!node || node.type !== 'text') return null;
  return {
    label: node.data.label || '',
    fontSize: node.data.fontSize || 14,
    fontWeight: node.data.fontWeight || 'normal',
    fontStyle: node.data.fontStyle || 'normal',
    textAlign: node.data.textAlign || 'left',
    color: node.data.color || '#1e1e1e',
    backgroundColor: node.data.backgroundColor || '',
    opacity: node.data.opacity ?? 100,
    width: node.data.width || 240,
  };
});

function handleTextUpdate(nodeId: string, data: Record<string, any>) {
  if (!store.currentBoardData.value) return;
  const wbNode = store.currentBoardData.value.nodes.find((n: WBNode) => n.id === nodeId);
  if (!wbNode) return;
  wbNode.data = { ...wbNode.data, ...data };
  const vfNode = vfNodes.value.find((n: any) => n.id === nodeId);
  if (vfNode) {
    vfNode.data = { ...vfNode.data, ...data };
    vfNodes.value = [...vfNodes.value];
  }
  scheduleSave();
}

function handleTextDelete(nodeId: string) {
  store.removeNode(nodeId);
  vfNodes.value = vfNodes.value.filter((n: any) => n.id !== nodeId);
  vfEdges.value = vfEdges.value.filter((e: any) => e.source !== nodeId && e.target !== nodeId);
  selectedTextNodeId.value = null;
  scheduleSave();
}

function closeTextMenu() {
  selectedTextNodeId.value = null;
}

// ─── Multi-Select Menu ──────────────────────────────────
const multiSelectedNodes = computed(() =>
  vfNodes.value.filter((n: any) => n.selected)
);
const showMultiSelectMenu = computed(() =>
  multiSelectedNodes.value.length >= 2
);

function handleMultiGroup() {
  const selected = [...multiSelectedNodes.value];
  if (selected.length < 2) return;
  const groupId = `grp_${Date.now()}`;
  for (const n of selected) {
    // Use VueFlow's native API — doesn't reset selection
    vfUpdateNodeData(n.id, { groupId });
    store.updateNodeData(n.id, { groupId });
  }
  scheduleSave();
}

function handleMultiUngroup() {
  const selected = [...multiSelectedNodes.value];
  for (const n of selected) {
    if (n.data?.groupId) {
      const { groupId, ...rest } = n.data;
      vfUpdateNodeData(n.id, rest, { replace: true });
      store.updateNodeData(n.id, rest);
    }
  }
  scheduleSave();
}

function handleMultiDelete() {
  const ids = multiSelectedNodes.value.map((n: any) => n.id);
  for (const id of ids) {
    store.removeNode(id);
  }
  vfNodes.value = vfNodes.value.filter((n: any) => !ids.includes(n.id));
  vfEdges.value = vfEdges.value.filter((e: any) => !ids.includes(e.source) && !ids.includes(e.target));
  scheduleSave();
}

function handleMultiUpdateAll(data: Record<string, any>) {
  const selected = [...multiSelectedNodes.value];
  for (const n of selected) {
    vfUpdateNodeData(n.id, data);
    store.updateNodeData(n.id, data);
  }
  scheduleSave();
}

function closeMultiSelectMenu() {
  // Deselect all nodes
  for (const n of vfNodes.value) {
    n.selected = false;
  }
  vfNodes.value = [...vfNodes.value];
}

// ─── Auto-save (debounced 2s) ───────────────────────────
let saveTimer: ReturnType<typeof setTimeout> | null = null;
function scheduleSave() {
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    store.saveCurrentBoard();
  }, 2000);
}

// Auto-save is handled by scheduleSave() calls in each mutation handler.


// ─── Free Drawing ───────────────────────────────────────
const canvasRef = ref<HTMLElement | null>(null);

const freeDrawing = useFreeDrawing({
  color: store.activeColor,
  size: store.activeStrokeSize,
  onStrokeComplete: (svgPath, points, color, size, minX, minY) => {
    const isHighlighter = store.drawSubTool.value === 'highlighter';
    const node = {
      id: store.generateId('stroke'),
      type: 'stroke' as const,
      position: { x: minX, y: minY },
      data: {
        svgPath,
        points,
        color,
        size: isHighlighter ? size * 3 : size,
        opacity: isHighlighter ? 0.35 : 0.85,
      },
    };
    store.addNode(node);
    vfNodes.value = [...vfNodes.value, { ...node, draggable: true }];
    scheduleSave();
  },
});

const isErasing = ref(false);
const eraserPos = ref<{ x: number; y: number } | null>(null);

function handleCanvasPointerDown(e: PointerEvent) {
  if (store.activeTool.value !== 'draw') return;
  if (store.drawSubTool.value === 'eraser') {
    isErasing.value = true;
    eraseStrokesNear(e);
    return;
  }
  if (!canvasRef.value) return;
  const rect = canvasRef.value.getBoundingClientRect();
  freeDrawing.startDraw(e, rect, viewport.value);
}

function handleCanvasPointerMove(e: PointerEvent) {
  if (store.activeTool.value !== 'draw') return;
  if (store.drawSubTool.value === 'eraser') {
    eraserPos.value = { x: e.clientX, y: e.clientY };
    if (isErasing.value) eraseStrokesNear(e);
    return;
  }
  if (!canvasRef.value) return;
  const rect = canvasRef.value.getBoundingClientRect();
  freeDrawing.continueDraw(e, rect, viewport.value);
}

function handleCanvasPointerUp() {
  if (store.activeTool.value !== 'draw') return;
  if (store.drawSubTool.value === 'eraser') {
    isErasing.value = false;
    return;
  }
  freeDrawing.endDraw();
}

function eraseStrokesNear(e: PointerEvent) {
  if (!canvasRef.value) return;
  const rect = canvasRef.value.getBoundingClientRect();
  const cx = (e.clientX - rect.left - viewport.value.x) / viewport.value.zoom;
  const cy = (e.clientY - rect.top - viewport.value.y) / viewport.value.zoom;
  const r = store.activeStrokeSize.value;
  const r2 = r * r;

  const strokeNodes = (store.currentBoardData.value?.nodes || []).filter(n => n.type === 'stroke');
  let changed = false;

  for (const sn of strokeNodes) {
    const pts = sn.data.points as number[][] | undefined;
    if (!pts || pts.length < 2) continue;
    const origSize = sn.data.size as number || 3;
    const origColor = sn.data.color as string || '#000';
    const origOpacity = (sn.data.opacity as number) ?? 0.85;
    const nodeX = sn.position.x;
    const nodeY = sn.position.y;

    // Check if any point is within eraser radius
    let hasHit = false;
    const hitMap = pts.map(([px, py]) => {
      const dx = (nodeX + px) - cx;
      const dy = (nodeY + py) - cy;
      const hit = dx * dx + dy * dy < r2;
      if (hit) hasHit = true;
      return hit;
    });

    if (!hasHit) continue;

    // Split points into contiguous non-hit segments
    const segments: number[][][] = [];
    let currentSeg: number[][] = [];
    for (let i = 0; i < pts.length; i++) {
      if (!hitMap[i]) {
        currentSeg.push(pts[i]);
      } else {
        if (currentSeg.length >= 2) segments.push(currentSeg);
        currentSeg = [];
      }
    }
    if (currentSeg.length >= 2) segments.push(currentSeg);

    // Remove the original stroke
    store.removeNode(sn.id);
    vfNodes.value = vfNodes.value.filter(n => n.id !== sn.id);
    changed = true;

    // Create new stroke nodes from remaining segments
    for (const seg of segments) {
      // Normalize segment to its own bounding box
      let minSX = Infinity, minSY = Infinity;
      for (const [sx, sy] of seg) {
        if (sx < minSX) minSX = sx;
        if (sy < minSY) minSY = sy;
      }
      const normSeg = seg.map(([sx, sy, sp]) => [sx - minSX, sy - minSY, sp]);
      const stroke = getStroke(normSeg, { size: origSize, thinning: 0.5, smoothing: 0.5, streamline: 0.5 });
      const svgPath = getSvgPathFromStroke(stroke);
      if (!svgPath) continue;

      const newNode: WBNode = {
        id: store.generateId('stroke'),
        type: 'stroke',
        position: { x: nodeX + minSX, y: nodeY + minSY },
        data: { svgPath, points: normSeg, color: origColor, size: origSize, opacity: origOpacity },
      };
      store.addNode(newNode);
      vfNodes.value = [...vfNodes.value, { ...newNode, draggable: true }];
    }
  }
  if (changed) scheduleSave();
}

// ─── Canvas Click → Create Node ─────────────────────────
// Helper: add node to both store and VueFlow
function addNodeToCanvas(node: WBNode) {
  store.addNode(node);
  const vfNode: any = { ...node, position: { ...node.position }, data: { ...node.data }, draggable: true };
  // Shape nodes need explicit dimensions on the wrapper for NodeResizer
  if (node.type === 'shape') {
    const w = node.data.width || 160;
    const h = node.data.height || 80;
    vfNode.style = {
      width: `${w}px`,
      height: `${h}px`,
    };
    vfNode.zIndex = computeShapeZIndex(w, h);
  }
  vfNodes.value = [...vfNodes.value, vfNode];
  scheduleSave();
}

function handlePaneClick(event: any) {
  // Close menus when clicking on empty canvas
  selectedEdgeId.value = null;
  selectedShapeNodeId.value = null;
  selectedTextNodeId.value = null;

  const pos = screenToFlowCoordinate({ x: event.clientX, y: event.clientY });

  if (store.activeTool.value === 'shape') {
    const shape = store.activeShapeType.value;
    const def = SHAPES_MAP[shape];
    const defaultW = def?.defaultWidth || 160;
    const defaultH = def?.defaultHeight || 80;
    addNodeToCanvas({
      id: store.generateId('shape'),
      type: 'shape',
      position: pos,
      data: {
        shapeType: shape,
        label: '',
        color: store.activeColor.value,
        width: defaultW,
        height: defaultH,
      },
    });
    store.activeTool.value = 'select';
  } else if (store.activeTool.value === 'text') {
    addNodeToCanvas({
      id: store.generateId('text'),
      type: 'text',
      position: pos,
      data: { label: '' },
    });
    store.activeTool.value = 'select';
  } else if (store.activeTool.value === 'mindmap') {
    addNodeToCanvas({
      id: store.generateId('mind'),
      type: 'mindmap',
      position: pos,
      data: {
        label: 'Central Idea',
        color: store.getMindmapColor(0),
        level: 0,
        editing: true,
      },
    });
    store.activeTool.value = 'select';
  }
}

function handleDragOver(event: DragEvent) {
  event.preventDefault();
  if (event.dataTransfer) {
    event.dataTransfer.dropEffect = 'copy';
  }
}

function handleDrop(event: DragEvent) {
  event.preventDefault();
  if (!event.dataTransfer) return;
  
  const noteId = event.dataTransfer.getData('application/synabit-note-id');
  const noteTitle = event.dataTransfer.getData('application/synabit-note-title');
  const blockId = event.dataTransfer.getData('application/synabit-block-id');
  
  if (noteId) {
    const pos = screenToFlowCoordinate({ x: event.clientX, y: event.clientY });
    
    addNodeToCanvas({
      id: store.generateId('note'),
      type: 'note',
      position: pos,
      data: {
        noteId,
        noteTitle,
        blockId: blockId || undefined,
        width: 280,
        height: 180,
      },
    });
  }
}

// ─── Mindmap subtree drag (XMind-style) ─────────────────
let mindmapDragState: {
  nodeId: string;
  startPos: { x: number; y: number };
  descendants: { id: string; startX: number; startY: number }[];
} | null = null;

function handleNodeDragStart({ node }: any) {
  // Auto-select all group members BEFORE drag starts
  if (node.data?.groupId) {
    const groupId = node.data.groupId;
    const groupMembers = vfNodes.value.filter(
      (n: any) => n.data?.groupId === groupId && n.id !== node.id
    );
    if (groupMembers.length > 0) {
      addSelectedNodes(groupMembers);
    }
  }

  // Mindmap: record initial positions of all descendants
  if (node.type === 'mindmap') {
    const descendantIds = new Set<string>();
    const findDescendants = (parentId: string) => {
      for (const edge of vfEdges.value) {
        if (edge.source === parentId && !descendantIds.has(edge.target)) {
          descendantIds.add(edge.target);
          findDescendants(edge.target);
        }
      }
    };
    findDescendants(node.id);

    if (descendantIds.size > 0) {
      mindmapDragState = {
        nodeId: node.id,
        startPos: { x: node.position.x, y: node.position.y },
        descendants: [...descendantIds].map(id => {
          const n = vfNodes.value.find((nd: any) => nd.id === id);
          return { id, startX: n?.position?.x || 0, startY: n?.position?.y || 0 };
        }),
      };
    }
  }
}

function handleNodeDrag({ node }: any) {
  // Mindmap: apply delta to all descendants
  if (mindmapDragState && node.id === mindmapDragState.nodeId) {
    const dx = node.position.x - mindmapDragState.startPos.x;
    const dy = node.position.y - mindmapDragState.startPos.y;
    for (const desc of mindmapDragState.descendants) {
      const vfNode = vfNodes.value.find((n: any) => n.id === desc.id);
      if (vfNode) {
        vfNode.position = {
          x: desc.startX + dx,
          y: desc.startY + dy,
        };
      }
    }
  }
}

function handleNodeDragStop({ node }: any) {
  if (mindmapDragState && node.id === mindmapDragState.nodeId) {
    // Sync final positions to store
    for (const desc of mindmapDragState.descendants) {
      const vfNode = vfNodes.value.find((n: any) => n.id === desc.id);
      if (vfNode) {
        const wbNode = store.currentBoardData.value?.nodes.find((n: any) => n.id === desc.id);
        if (wbNode) {
          wbNode.position = { ...vfNode.position };
        }
      }
    }
    mindmapDragState = null;
    scheduleSave();
  }
}

function handleNodeClick({ node, event }: any) {
  // Close edge menu when clicking a node
  selectedEdgeId.value = null;

  if (store.activeTool.value === 'eraser') {
    store.removeNode(node.id);
    vfNodes.value = vfNodes.value.filter((n: any) => n.id !== node.id);
    vfEdges.value = vfEdges.value.filter((e: any) => e.source !== node.id && e.target !== node.id);
    selectedShapeNodeId.value = null;
    scheduleSave();
    return;
  }

  // Auto-select all group members when clicking a grouped node
  if (store.activeTool.value === 'select' && node.data?.groupId) {
    const groupId = node.data.groupId;
    const groupMembers = vfNodes.value.filter(
      (n: any) => n.data?.groupId === groupId && n.id !== node.id
    );
    if (groupMembers.length > 0) {
      addSelectedNodes(groupMembers);
    }
  }

  // Show property menu for the clicked node type
  if (store.activeTool.value === 'select') {
    if (node.type === 'shape') {
      selectedShapeNodeId.value = node.id;
      selectedTextNodeId.value = null;
      shapeMenuPos.value = { x: event.clientX, y: event.clientY };
    } else if (node.type === 'text') {
      selectedTextNodeId.value = node.id;
      selectedShapeNodeId.value = null;
    } else {
      selectedShapeNodeId.value = null;
      selectedTextNodeId.value = null;
    }
  }
}

function handleNodeDataUpdate(nodeId: string, data: any) {
  store.updateNodeData(nodeId, data);
  // Sync updated data into VueFlow node (critical for copy-paste accuracy)
  const vfNode = vfNodes.value.find((n: any) => n.id === nodeId);
  if (vfNode) {
    vfNode.data = { ...vfNode.data, ...data };
    // Update wrapper dimensions & z-index for shape nodes on resize
    if (vfNode.type === 'shape' && (data.width || data.height)) {
      const w = data.width || vfNode.data.width || 160;
      const h = data.height || vfNode.data.height || 80;
      vfNode.style = { ...vfNode.style, width: `${w}px`, height: `${h}px` };
      vfNode.zIndex = computeShapeZIndex(w, h);
    }
    vfNodes.value = [...vfNodes.value];
  }
  scheduleSave();
}

function handleMindmapAddChild({ parentId, direction }: { parentId: string; direction: 'right' | 'left' }) {
  const childId = store.addMindmapChild(parentId, direction);
  syncToVueFlow();
  scheduleSave();
  if (childId) focusMindmapNode(childId);
}

function handleMindmapRemoveNode(nodeId: string) {
  store.removeNode(nodeId);
  vfNodes.value = vfNodes.value.filter((n: any) => n.id !== nodeId);
  vfEdges.value = vfEdges.value.filter((e: any) => e.source !== nodeId && e.target !== nodeId);
  scheduleSave();
}

function handleMindmapAddSibling(nodeId: string) {
  const siblingId = store.addMindmapSibling(nodeId);
  syncToVueFlow();
  scheduleSave();
  if (siblingId) focusMindmapNode(siblingId);
}

/** Focus the input inside a newly created mindmap node after VueFlow renders it */
function focusMindmapNode(nodeId: string) {
  let attempts = 0;
  const maxAttempts = 10; // 10 × 50ms = 500ms max
  const tryFocus = () => {
    const nodeEl = document.querySelector(`[data-id="${nodeId}"]`);
    if (nodeEl) {
      const input = nodeEl.querySelector('input') as HTMLInputElement | null;
      if (input) {
        input.focus();
        input.select();
        return;
      }
    }
    attempts++;
    if (attempts < maxAttempts) {
      setTimeout(tryFocus, 50);
    }
  };
  // Start after a short delay to let VueFlow begin processing
  setTimeout(tryFocus, 50);
}

// ─── Keyboard Shortcuts ─────────────────────────────────
function handleKeydown(e: KeyboardEvent) {
  // Don't capture when editing text
  const target = e.target as HTMLElement;
  if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA') return;

  // Mindmap shortcuts: Tab = child, Enter = sibling (when a mindmap node is selected)
  if (e.key === 'Tab' || e.key === 'Enter') {
    const selectedNode = vfNodes.value.find((n: any) => n.selected && n.type === 'mindmap');
    if (selectedNode) {
      e.preventDefault();
      if (e.key === 'Tab') {
        const dir = selectedNode.data?.direction || 'right';
        handleMindmapAddChild({ parentId: selectedNode.id, direction: dir });
      } else {
        handleMindmapAddSibling(selectedNode.id);
      }
      return;
    }
  }

  // Tool shortcuts — only when no modifier key is held
  if (!e.ctrlKey && !e.metaKey) {
    if (e.key === 'v' || e.key === 'V') { store.activeTool.value = 'select'; return; }
    if (e.key === 'd' || e.key === 'D') { store.activeTool.value = 'draw'; return; }
    if (e.key === 's') { store.activeTool.value = 'shape'; return; }
    if (e.key === 't' || e.key === 'T') { store.activeTool.value = 'text'; return; }
    if (e.key === 'e' || e.key === 'E') { store.activeTool.value = 'draw'; store.drawSubTool.value = 'eraser'; return; }
    if (e.key === 'm' || e.key === 'M') { store.activeTool.value = 'mindmap'; return; }
  }

  if ((e.ctrlKey || e.metaKey) && e.key === 'z' && !e.shiftKey) {
    e.preventDefault();
    store.undo();
    syncToVueFlow();
    scheduleSave();
    return;
  }
  if ((e.ctrlKey || e.metaKey) && e.key === 'z' && e.shiftKey) {
    e.preventDefault();
    store.redo();
    syncToVueFlow();
    scheduleSave();
    return;
  }
  if ((e.ctrlKey || e.metaKey) && (e.key === 's' || e.key === 'S')) {
    e.preventDefault();
    store.saveCurrentBoard();
    return;
  }

  // Ctrl+C → copy selected node
  if ((e.ctrlKey || e.metaKey) && (e.key === 'c' || e.key === 'C') && !e.shiftKey) {
    const selectedNode = vfNodes.value.find((n: any) => n.selected);
    if (selectedNode) {
      e.preventDefault();
      clipboard = {
        type: selectedNode.type,
        data: JSON.parse(JSON.stringify(selectedNode.data)),
        position: { ...selectedNode.position },
      };
    }
    return;
  }

  // Ctrl+V → paste copied node with offset
  if ((e.ctrlKey || e.metaKey) && (e.key === 'v' || e.key === 'V') && !e.shiftKey) {
    if (clipboard) {
      e.preventDefault();
      const offset = 30;
      const newNode: WBNode = {
        id: store.generateId(clipboard.type === 'shape' ? 'sh' : 'n'),
        type: clipboard.type as any,
        position: {
          x: clipboard.position.x + offset,
          y: clipboard.position.y + offset,
        },
        data: JSON.parse(JSON.stringify(clipboard.data)),
      };
      addNodeToCanvas(newNode);
      // Move clipboard position so next paste offsets further
      clipboard.position.x += offset;
      clipboard.position.y += offset;
    }
    return;
  }

  // Ctrl+G → group selected nodes
  if ((e.ctrlKey || e.metaKey) && (e.key === 'g' || e.key === 'G') && !e.shiftKey) {
    e.preventDefault();
    handleMultiGroup();
    return;
  }

  // Ctrl+Shift+G → ungroup selected nodes
  if ((e.ctrlKey || e.metaKey) && (e.key === 'g' || e.key === 'G') && e.shiftKey) {
    e.preventDefault();
    handleMultiUngroup();
    return;
  }
}

// ─── Export PNG ──────────────────────────────────────────
const vueFlowRef = ref<HTMLElement | null>(null);

async function exportPng() {
  const el = document.querySelector('.vue-flow') as HTMLElement;
  const nodes = getNodes.value;
  if (!el || nodes.length === 0) return;
  try {
    const nodesBounds = getRectOfNodes(nodes);
    const padding = 50;
    const exportWidth = nodesBounds.width + padding * 2;
    const exportHeight = nodesBounds.height + padding * 2;

    const prevViewport = getViewport();

    // 1. Force the viewport to perfectly fit the export area
    setViewport({
      x: -nodesBounds.x + padding,
      y: -nodesBounds.y + padding,
      zoom: 1
    });

    // 2. Wait for VueFlow to apply transform to DOM
    await nextTick();
    await new Promise(r => setTimeout(r, 100)); // allow transitions to finish

    // 3. Inject explicit styles to fix html-to-image dropping CSS variables
    const isDark = document.documentElement.classList.contains('dark');
    const edgeColor = isDark ? '#71717a' : '#8b8b8b';
    const bgColor = store.backgroundColor.value === 'transparent' 
      ? (isDark ? '#242424' : '#ffffff') 
      : store.backgroundColor.value;
    const textColor = isDark ? '#a1a1aa' : '#52525b';
    
    const styleEl = document.createElement('style');
    styleEl.innerHTML = `
      .vue-flow__edge-path, .vue-flow__connection-path { stroke: ${edgeColor} !important; stroke-width: 2 !important; fill: none !important; }
      .vue-flow__edge-textbg { fill: ${bgColor} !important; }
      .vue-flow__edge-text { fill: ${textColor} !important; }
      .vue-flow__arrowhead { fill: ${edgeColor} !important; }
    `;
    el.appendChild(styleEl);

    // 4. Capture
    const dataUrl = await toPng(el, {
      backgroundColor: store.backgroundColor.value === 'transparent' ? '#ffffff' : store.backgroundColor.value,
      width: exportWidth,
      height: exportHeight,
      pixelRatio: 2,
      style: {
        width: `${exportWidth}px`,
        height: `${exportHeight}px`,
      },
      filter: (node) => {
        // Exclude UI controls
        if (node.classList?.contains('vue-flow__controls')) return false;
        if (node.classList?.contains('vue-flow__panel')) return false;
        return true;
      }
    });

    // 5. Cleanup and restore
    el.removeChild(styleEl);
    setViewport(prevViewport);

    // Trigger download
    const link = document.createElement('a');
    link.download = `${store.currentBoardData.value?.title || 'whiteboard'}.png`;
    link.href = dataUrl;
    link.click();
  } catch (err) {
    logger.error('Export PNG failed', err as string);
  }
}

// ─── Sidebar ────────────────────────────────────────────
const sidebarOpen = ref(true);
const sidebarTab = ref<'boards' | 'notes'>('boards');
const whiteboardNotes = ref<any[]>([]);
const noteSearch = ref('');
const dailyNotesExpanded = ref(false);

/** Detect daily notes (title is a date like 2026-05-04) */
const isDailyNote = (title: string) => /^\d{4}-\d{2}-\d{2}$/.test(title?.trim());

/** Extract a 1-line preview from note content (strip frontmatter + markdown) */
const notePreview = (content: string) => {
  if (!content) return '';
  let text = content;
  if (text.startsWith('---')) {
    const end = text.indexOf('---', 3);
    if (end > 3) text = text.substring(end + 3);
  }
  // Strip markdown syntax and get first meaningful line
  const line = text.split('\n').map(l => l.trim()).find(l => l && !l.startsWith('#') && !l.startsWith('---'));
  if (!line) return '';
  const clean = line.replace(/[\*\_\[\]\(\)\#\>\`]/g, '').trim();
  return clean.length > 60 ? clean.substring(0, 60) + '…' : clean;
};

const filteredRegularNotes = computed(() => {
  const q = noteSearch.value.toLowerCase().trim();
  return whiteboardNotes.value
    .filter(n => !isDailyNote(n.title))
    .filter(n => !q || (n.title || '').toLowerCase().includes(q) || (n.content || '').toLowerCase().includes(q));
});

const filteredDailyNotes = computed(() => {
  const q = noteSearch.value.toLowerCase().trim();
  return whiteboardNotes.value
    .filter(n => isDailyNote(n.title))
    .filter(n => !q || (n.title || '').toLowerCase().includes(q) || (n.content || '').toLowerCase().includes(q));
});

const editingTitle = ref(false);
const titleInput = ref('');

function startEditTitle() {
  if (!store.currentBoardData.value) return;
  editingTitle.value = true;
  titleInput.value = store.currentBoardData.value.title;
}

function handleNoteDragStart(event: DragEvent, note: any) {
  if (event.dataTransfer) {
    event.dataTransfer.setData('application/synabit-note-id', note.id);
    event.dataTransfer.setData('application/synabit-note-title', note.title);
    event.dataTransfer.effectAllowed = 'copy';
  }
}

function finishEditTitle() {
  editingTitle.value = false;
  if (store.currentBoardData.value && titleInput.value.trim()) {
    store.currentBoardData.value.title = titleInput.value.trim();
    store.saveCurrentBoard(); // Save immediately so sidebar updates
  }
}

// ─── Tags ────────────────────────────────────────────────
const isAddingTag = ref(false);
const newTagInput = ref('');

function addBoardTag() {
  if (!store.currentBoardData.value || !newTagInput.value.trim()) return;
  const tag = newTagInput.value.trim().toLowerCase();
  if (store.currentBoardData.value.tags.includes(tag)) {
    newTagInput.value = '';
    isAddingTag.value = false;
    return;
  }
  store.currentBoardData.value.tags.push(tag);
  newTagInput.value = '';
  isAddingTag.value = false;
  store.saveCurrentBoard();
}

function removeBoardTag(tag: string) {
  if (!store.currentBoardData.value) return;
  store.currentBoardData.value.tags = store.currentBoardData.value.tags.filter(t => t !== tag);
  store.saveCurrentBoard();
}

async function confirmDeleteBoard(boardId: string) {
  const yes = await ask('This board will be permanently deleted.', {
    title: 'Delete Whiteboard',
    kind: 'warning',
    okLabel: 'Delete',
    cancelLabel: 'Cancel',
  });
  if (yes) store.deleteBoard(boardId);
}

// ─── Sidebar Resizing ─────────────────────────────────────
const wSidebar = ref(250);
const isDraggingSidebar = ref(false);

const startDragSidebar = () => {
  isDraggingSidebar.value = true;
};

const onMouseMove = (e: MouseEvent) => {
  if (isDraggingSidebar.value) {
    wSidebar.value = Math.max(200, Math.min(e.clientX - 64, 500)); // 64 is rough app nav width
  }
};

const onMouseUp = () => {
  isDraggingSidebar.value = false;
};

// ─── Lifecycle ──────────────────────────────────────────
onMounted(async () => {
  await store.loadBoards();
  if (store.boards.value.length > 0) {
    await store.loadBoardData(store.boards.value[0].id);
  }
  
  try {
    const loadedNotes = await invoke<any[]>('get_nodes', { nodeType: 'note' });
    whiteboardNotes.value = loadedNotes.sort((a, b) => b.created_at.localeCompare(a.created_at));
  } catch (err) {
    logger.error('Failed to load notes for whiteboard sidebar', err);
  }

  window.addEventListener('keydown', handleKeydown);
  window.addEventListener('mousemove', onMouseMove);
  window.addEventListener('mouseup', onMouseUp);
});

import { onUnmounted } from 'vue';
onUnmounted(() => {
  if (saveTimer) clearTimeout(saveTimer);
  window.removeEventListener('keydown', handleKeydown);
  window.removeEventListener('mousemove', onMouseMove);
  window.removeEventListener('mouseup', onMouseUp);
});

async function openBoardById(boardId: string, _skipNavPush = false) {
  if (!_skipNavPush && store.currentBoardId.value && store.currentBoardId.value !== boardId && !skipNavPush) {
    pushNavigation?.({ app: 'whiteboard', itemId: store.currentBoardId.value });
  }
  if (!store.boards.value.length || !store.boards.value.find((b: any) => b.id === boardId)) {
    await store.loadBoards();
  }
  await store.loadBoardData(boardId);
}

async function refreshBoards() {
    await store.loadBoards();
}

defineExpose({ openBoardById, currentBoardId: store.currentBoardId, refreshBoards });
</script>

<template>
  <div class="flex flex-1 h-full overflow-hidden" :class="{'cursor-col-resize': isDraggingSidebar}">
    <!-- Sidebar: Board List -->
    <div
      v-if="sidebarOpen"
      class="wb-sidebar flex flex-col relative shrink-0"
      :style="{ width: wSidebar + 'px' }"
    >
      <div class="hidden md:block absolute top-0 right-0 w-1.5 h-full cursor-col-resize hover:bg-black/10 dark:hover:bg-white/10 z-10 opacity-0 hover:opacity-100 transition-opacity" @mousedown.stop="startDragSidebar"></div>

      <div class="flex items-center justify-between p-3 border-b border-border dark:border-border-dark" data-tauri-drag-region>
        <div class="flex gap-4">
          <button @click="sidebarTab = 'boards'" :class="sidebarTab === 'boards' ? 'text-sm font-bold text-text dark:text-text-dark' : 'text-sm font-semibold text-muted dark:text-muted-dark hover:text-text dark:hover:text-text-dark transition-colors'">Boards</button>
          <button @click="sidebarTab = 'notes'" :class="sidebarTab === 'notes' ? 'text-sm font-bold text-text dark:text-text-dark' : 'text-sm font-semibold text-muted dark:text-muted-dark hover:text-text dark:hover:text-text-dark transition-colors'">Notes</button>
        </div>
        <div class="flex items-center gap-1" @mousedown.stop>
          <button
            v-if="sidebarTab === 'boards'"
            @click="store.createBoard()"
            class="wb-icon-btn"
            title="New Board"
          >
            <Plus class="w-4 h-4" />
          </button>
          <button @click="sidebarOpen = false" class="wb-icon-btn" title="Close Sidebar">
            <PanelLeftClose class="w-4 h-4" />
          </button>
        </div>
      </div>

      <div v-if="sidebarTab === 'boards'" class="flex-1 overflow-y-auto p-2 space-y-1" @mousedown.stop>
        <button
          v-for="board in store.boards.value"
          :key="board.id"
          @click="switchBoard(board.id)"
          :class="[
            'w-full text-left px-3 py-2.5 rounded-lg text-sm transition-all group',
            store.currentBoardId.value === board.id
              ? 'bg-accent/10 text-accent dark:text-accent-dark font-semibold'
              : 'text-text-secondary dark:text-text-secondary-dark hover:bg-surface-hover dark:hover:bg-surface-hover-dark'
          ]"
        >
          <div class="flex items-center justify-between">
            <div class="flex items-center gap-2 min-w-0">
              <PenTool class="w-3.5 h-3.5 flex-shrink-0 opacity-50" />
              <span class="truncate">{{ board.title }}</span>
            </div>
            <button
              @click.stop="confirmDeleteBoard(board.id)"
              class="opacity-0 group-hover:opacity-60 hover:!opacity-100 transition-opacity p-1 rounded hover:bg-danger/10 hover:text-danger"
            >
              <Trash2 class="w-3 h-3" />
            </button>
          </div>
          <p class="text-[10px] opacity-40 mt-0.5 ml-5.5">{{ board.updated_at?.split(' ')[0] }}</p>
        </button>

        <div v-if="!store.boards.value.length" class="text-center text-xs text-muted dark:text-muted-dark py-8">
          <PenTool class="w-8 h-8 mx-auto mb-2 opacity-30" />
          <p>No whiteboards yet</p>
          <button @click="store.createBoard()" class="text-accent dark:text-accent-dark mt-1 hover:underline">
            Create one
          </button>
        </div>
      </div>

      <div v-else-if="sidebarTab === 'notes'" class="flex-1 overflow-y-auto flex flex-col" @mousedown.stop>
        <!-- Search -->
        <div class="p-2 border-b border-border dark:border-border-dark">
          <div class="relative">
            <Search class="absolute left-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-muted dark:text-muted-dark" />
            <input
              v-model="noteSearch"
              placeholder="Search notes…"
              class="w-full pl-8 pr-3 py-1.5 text-xs bg-surface-hover/50 dark:bg-surface-hover-dark/50 border border-border dark:border-border-dark rounded-md outline-none focus:ring-1 focus:ring-accent/40 text-text dark:text-text-dark placeholder:text-muted dark:placeholder:text-muted-dark transition-all"
            />
          </div>
        </div>

        <div class="flex-1 overflow-y-auto p-2 space-y-1">
          <!-- Regular Notes -->
          <div
            v-for="note in filteredRegularNotes"
            :key="note.id"
            draggable="true"
            @dragstart="(e) => handleNoteDragStart(e, note)"
            class="group px-3 py-2 rounded-lg transition-all hover:bg-surface-hover dark:hover:bg-surface-hover-dark cursor-grab active:cursor-grabbing border border-transparent hover:border-border dark:hover:border-border-dark"
          >
            <div class="flex items-center gap-2 min-w-0">
              <GripVertical class="w-3 h-3 flex-shrink-0 opacity-0 group-hover:opacity-40 transition-opacity text-muted" />
              <FileText class="w-3.5 h-3.5 flex-shrink-0 text-accent/60" />
              <span class="text-sm font-medium text-text dark:text-text-dark truncate">{{ note.title || 'Untitled' }}</span>
            </div>
            <p v-if="notePreview(note.content)" class="text-[11px] text-muted dark:text-muted-dark truncate mt-0.5 ml-[34px]">
              {{ notePreview(note.content) }}
            </p>
          </div>

          <!-- Daily Notes Group -->
          <div v-if="filteredDailyNotes.length > 0" class="mt-2">
            <button
              @click="dailyNotesExpanded = !dailyNotesExpanded"
              class="flex items-center gap-1.5 px-2 py-1.5 w-full text-left text-[11px] font-semibold uppercase tracking-wider text-muted dark:text-muted-dark hover:text-text dark:hover:text-text-dark transition-colors"
            >
              <component :is="dailyNotesExpanded ? ChevronDown : ChevronRight" class="w-3 h-3" />
              Daily Notes
              <span class="text-[10px] font-normal opacity-60">({{ filteredDailyNotes.length }})</span>
            </button>
            <div v-if="dailyNotesExpanded" class="space-y-0.5 mt-0.5">
              <div
                v-for="note in filteredDailyNotes"
                :key="note.id"
                draggable="true"
                @dragstart="(e) => handleNoteDragStart(e, note)"
                class="group flex items-center gap-2 px-3 py-1.5 rounded-md transition-all hover:bg-surface-hover dark:hover:bg-surface-hover-dark cursor-grab active:cursor-grabbing"
              >
                <GripVertical class="w-3 h-3 flex-shrink-0 opacity-0 group-hover:opacity-40 transition-opacity text-muted" />
                <FileText class="w-3 h-3 flex-shrink-0 text-muted/50" />
                <span class="text-xs text-text-secondary dark:text-text-secondary-dark truncate">{{ note.title }}</span>
              </div>
            </div>
          </div>

          <!-- Empty state -->
          <div v-if="filteredRegularNotes.length === 0 && filteredDailyNotes.length === 0" class="text-center text-xs text-muted dark:text-muted-dark py-8">
            <FileText class="w-8 h-8 mx-auto mb-2 opacity-30" />
            <p>{{ noteSearch ? 'No matching notes' : 'No notes found' }}</p>
          </div>
        </div>
      </div>
    </div>

    <!-- Toggle sidebar button when closed -->
    <button
      v-if="!sidebarOpen"
      @click="sidebarOpen = true"
      class="absolute top-3 left-1 z-50 wb-icon-btn bg-surface dark:bg-surface-dark border border-border dark:border-border-dark shadow-md"
      title="Open Sidebar"
    >
      <PanelLeft class="w-4 h-4" />
    </button>

    <!-- Main Canvas Area -->
    <div 
      class="flex-1 relative transition-colors" 
      ref="canvasRef"
      :style="{ backgroundColor: store.backgroundColor.value === 'transparent' ? '' : store.backgroundColor.value }"
    >
      <template v-if="store.currentBoardData.value">
        <!-- Title bar -->
        <div class="wb-title-bar">
          <div class="flex items-center gap-2 min-w-0 flex-1">
            <NavButtons />
            <input
              v-if="editingTitle"
              v-model="titleInput"
              @blur="finishEditTitle"
              @keydown.enter="finishEditTitle"
              class="text-sm font-bold bg-transparent border-b border-accent dark:border-accent-dark outline-none text-text dark:text-text-dark"
              autofocus
            />
            <h1
              v-else
              @click="startEditTitle"
              class="text-sm font-bold truncate text-text dark:text-text-dark cursor-text hover:text-accent dark:hover:text-accent-dark transition-colors"
            >
              {{ store.currentBoardData.value.title }}
            </h1>
          </div>
          <div class="flex items-center gap-1">
            <span v-if="store.isSaving.value" class="text-[10px] text-muted dark:text-muted-dark font-medium px-2">Saving…</span>
          </div>
        </div>

        <!-- Tags row -->
        <div class="wb-tags-bar">
          <Tag class="w-3 h-3 text-muted dark:text-muted-dark flex-shrink-0" />
          <div class="flex items-center gap-1 flex-wrap min-w-0">
            <span
              v-for="tag in store.currentBoardData.value.tags"
              :key="tag"
              class="wb-tag group"
            >
              #{{ tag }}
              <button @click.stop="removeBoardTag(tag)" class="ml-0.5 opacity-0 group-hover:opacity-100 hover:text-danger transition-opacity">
                <X class="w-2.5 h-2.5" />
              </button>
            </span>
            <input
              v-if="isAddingTag"
              v-model="newTagInput"
              @keydown.enter="addBoardTag"
              @keydown.escape="isAddingTag = false; newTagInput = ''"
              @blur="addBoardTag"
              type="text"
              placeholder="tag…"
              class="wb-tag-input"
              autofocus
            />
            <button v-else @click="isAddingTag = true" class="wb-tag-add" title="Add Tag">
              <Plus class="w-3 h-3" />
            </button>
          </div>
        </div>

        <!-- Vue Flow Canvas -->
        <VueFlow
          id="whiteboard-flow"
          ref="vueFlowRef"
          v-model:nodes="vfNodes"
          v-model:edges="vfEdges"
          :class="[
            'flex-1',
            store.activeTool.value === 'draw' && store.drawSubTool.value !== 'eraser' && 'cursor-crosshair',
            store.activeTool.value === 'draw' && store.drawSubTool.value === 'eraser' && 'wb-cursor-eraser',
            store.activeTool.value === 'eraser' && 'wb-cursor-eraser',
          ]"
          :default-viewport="store.currentBoardData.value.viewport"
          :snap-to-grid="true"
          :snap-grid="[10, 10]"
          :connection-mode="ConnectionMode.Loose"
          :delete-key-code="'Delete'"
          :pan-on-drag="store.activeTool.value === 'select' ? [1, 2] : false"
          :selection-on-drag="store.activeTool.value === 'select'"
          :pan-on-scroll="true"
          :zoom-on-scroll="true"
          :nodes-draggable="store.activeTool.value === 'select'"
          :nodes-connectable="store.activeTool.value === 'select'"
          :elevate-edges-on-select="true"
          @pane-click="handlePaneClick"
          @node-click="handleNodeClick"
          @node-drag-start="handleNodeDragStart"
          @node-drag="handleNodeDrag"
          @node-drag-stop="handleNodeDragStop"
          @edge-click="handleEdgeClick"
          @connect="handleConnect"
          @nodes-change="handleNodesChange"
          @edges-change="handleEdgesChange"
          @dragover.prevent="handleDragOver"
          @drop.prevent="handleDrop"
        >
          <template #node-shape="nodeProps">
            <ShapeNode
              v-bind="nodeProps"
              @update:data="(d: any) => handleNodeDataUpdate(nodeProps.id, d)"
            />
          </template>
          <template #node-stroke="nodeProps">
            <StrokeNode v-bind="nodeProps" />
          </template>
          <template #node-mindmap="nodeProps">
            <MindmapNode
              v-bind="nodeProps"
              @update:data="(d: any) => handleNodeDataUpdate(nodeProps.id, d)"
              @add-child="handleMindmapAddChild"
              @add-sibling="handleMindmapAddSibling"
              @remove-node="handleMindmapRemoveNode"
            />
          </template>
          <template #node-text="nodeProps">
            <TextNode
              v-bind="nodeProps"
              @update:data="(d: any) => handleNodeDataUpdate(nodeProps.id, d)"
            />
          </template>
          <template #node-note="nodeProps">
            <NoteCardNode
              v-bind="nodeProps"
              @update:data="(d: any) => handleNodeDataUpdate(nodeProps.id, d)"
            />
          </template>

          <!-- Dots Background -->
          <Background v-if="store.backgroundPattern.value === 'dots'" variant="dots" :gap="20" :size="1" />
          
          <!-- Lines Background (Miro style: very subtle opacity, major every 5 blocks) -->
          <template v-if="store.backgroundPattern.value === 'lines'">
            <!-- Minor lines -->
            <Background variant="lines" :gap="20" :size="1" pattern-color="currentColor" class="text-black/[0.03] dark:text-white/[0.03]" />
            <!-- Major lines -->
            <Background variant="lines" :gap="100" :size="1" pattern-color="currentColor" class="text-black/[0.08] dark:text-white/[0.08]" />
          </template>
          <Controls position="bottom-right" />
        </VueFlow>

        <!-- Drawing overlay -->
        <svg
          v-if="store.activeTool.value === 'draw'"
          class="absolute inset-0 w-full h-full z-40 pointer-events-auto"
          @pointerdown="handleCanvasPointerDown"
          @pointermove="handleCanvasPointerMove"
          @pointerup="handleCanvasPointerUp"
          @pointerleave="handleCanvasPointerUp"
          style="touch-action: none;"
        >
          <g :transform="`translate(${viewport.x}, ${viewport.y}) scale(${viewport.zoom})`">
            <path
              v-if="freeDrawing.previewPath.value"
              :d="freeDrawing.previewPath.value"
              :fill="store.activeColor.value"
              :opacity="store.drawSubTool.value === 'highlighter' ? 0.35 : 0.6"
            />
          </g>
        </svg>

        <!-- Eraser cursor overlay -->
        <div
          v-if="store.activeTool.value === 'draw' && store.drawSubTool.value === 'eraser' && eraserPos"
          class="wb-eraser-cursor"
          :style="{
            left: eraserPos.x + 'px',
            top: eraserPos.y + 'px',
            width: (store.activeStrokeSize.value * 2 * viewport.zoom) + 'px',
            height: (store.activeStrokeSize.value * 2 * viewport.zoom) + 'px',
          }"
        />

        <!-- Floating Toolbar -->
        <WhiteboardToolbar
          :active-tool="store.activeTool.value"
          :can-undo="store.undoStack.value.length > 0"
          :can-redo="store.redoStack.value.length > 0"
          :draw-sub-tool="store.drawSubTool.value"
          :draw-color="store.activeColor.value"
          :draw-size="store.activeStrokeSize.value"
          :background-pattern="store.backgroundPattern.value"
          :background-color="store.backgroundColor.value"
          @update:active-tool="store.activeTool.value = $event"
          @select-shape="store.activeShapeType.value = $event"
          @update:draw-sub-tool="store.drawSubTool.value = $event"
          @update:draw-color="store.activeColor.value = $event"
          @update:draw-size="store.activeStrokeSize.value = $event"
          @update:background-pattern="store.backgroundPattern.value = $event"
          @update:background-color="store.backgroundColor.value = $event"
          @undo="() => { store.undo(); syncToVueFlow(); scheduleSave(); }"
          @redo="() => { store.redo(); syncToVueFlow(); scheduleSave(); }"
          @export="exportPng"
        />

        <!-- Edge Properties Panel (docked right side) -->
        <Teleport to="body">
          <EdgeMenu
            v-if="selectedEdgeId && selectedEdgeData"
            :edge-id="selectedEdgeId"
            :edge-data="selectedEdgeData"
            @update="handleEdgeUpdate"
            @delete="handleEdgeDelete"
            @close="closeEdgeMenu"
          />
        </Teleport>

        <!-- Shape Properties Panel (docked right side) -->
        <Teleport to="body">
          <ShapeMenu
            v-if="selectedShapeNodeId && selectedShapeData"
            :node-id="selectedShapeNodeId"
            :node-data="selectedShapeData"
            @update="handleShapeUpdate"
            @delete="handleShapeDelete"
            @close="closeShapeMenu"
          />
        </Teleport>

        <!-- Text Properties Panel (docked right side) -->
        <Teleport to="body">
          <TextMenu
            v-if="selectedTextNodeId && selectedTextData"
            :node-id="selectedTextNodeId"
            :node-data="selectedTextData"
            @update="handleTextUpdate"
            @delete="handleTextDelete"
            @close="closeTextMenu"
          />
        </Teleport>

        <!-- Multi-Select Panel (docked right side) -->
        <Teleport to="body">
          <MultiSelectMenu
            v-if="showMultiSelectMenu"
            :selected-nodes="multiSelectedNodes"
            @group="handleMultiGroup"
            @ungroup="handleMultiUngroup"
            @delete="handleMultiDelete"
            @update-all="handleMultiUpdateAll"
            @close="closeMultiSelectMenu"
          />
        </Teleport>
      </template>

      <!-- No board selected -->
      <div v-else class="flex-1 flex items-center justify-center h-full">
        <div class="text-center text-muted dark:text-muted-dark">
          <PenTool class="w-12 h-12 mx-auto mb-3 opacity-20" />
          <p class="text-sm mb-3">Select or create a whiteboard to start</p>
          <button
            @click="store.createBoard()"
            class="px-4 py-2 rounded-lg bg-accent dark:bg-accent-dark text-white text-sm font-semibold hover:opacity-90 transition-opacity cursor-pointer"
          >
            New Board
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.wb-sidebar {
  width: 220px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  border-right: 1px solid var(--color-border, #e6e6e6);
  background: var(--color-surface-alt, #fbfbfc);
  height: 100%;
}
.dark .wb-sidebar {
  border-color: var(--color-border-dark, #2c2c2c);
  background: var(--color-surface-alt-dark, #191919);
}
.wb-title-bar {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  z-index: 45;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 72px 8px 72px;
  background: var(--color-surface, #fff);
  border-bottom: 1px solid var(--color-border, #e6e6e6);
  backdrop-filter: blur(8px);
  background: rgba(255,255,255,0.85);
}
.dark .wb-title-bar {
  background: rgba(30,30,30,0.85);
  border-color: var(--color-border-dark, #2c2c2c);
}
.wb-tags-bar {
  position: absolute;
  top: 37px;
  left: 0;
  right: 0;
  z-index: 45;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 72px;
  background: rgba(255,255,255,0.85);
  backdrop-filter: blur(8px);
  border-bottom: 1px solid var(--color-border, #e6e6e6);
}
.dark .wb-tags-bar {
  background: rgba(30,30,30,0.85);
  border-color: var(--color-border-dark, #2c2c2c);
}
.wb-tag {
  display: inline-flex;
  align-items: center;
  padding: 1px 6px;
  border-radius: 4px;
  font-size: 11px;
  font-weight: 500;
  background: rgba(124, 58, 237, 0.1);
  color: var(--color-accent, #7c3aed);
  cursor: default;
}
.dark .wb-tag {
  background: rgba(167, 139, 250, 0.12);
  color: #a78bfa;
}
.wb-tag-input {
  width: 60px;
  padding: 1px 6px;
  border-radius: 4px;
  font-size: 11px;
  font-weight: 500;
  border: 1px solid var(--color-accent, #7c3aed);
  background: transparent;
  color: inherit;
  outline: none;
}
.wb-tag-add {
  width: 20px;
  height: 20px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 4px;
  border: 1px dashed var(--color-border, #d4d4d8);
  background: transparent;
  color: var(--color-text-secondary, #a1a1aa);
  cursor: pointer;
  transition: all 0.15s;
}
.wb-tag-add:hover {
  border-color: var(--color-accent, #7c3aed);
  color: var(--color-accent, #7c3aed);
}
.dark .wb-tag-add {
  border-color: var(--color-border-dark, #3f3f46);
  color: var(--color-text-secondary-dark, #71717a);
}
.dark .wb-tag-add:hover {
  border-color: #a78bfa;
  color: #a78bfa;
}
.wb-icon-btn {
  width: 28px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 8px;
  border: none;
  background: transparent;
  color: var(--color-text-secondary, #52525b);
  cursor: pointer;
  transition: all 0.15s;
}
.dark .wb-icon-btn {
  color: var(--color-text-secondary-dark, #a1a1aa);
}
.wb-icon-btn:hover {
  background: var(--color-surface-hover, #f5f5f5);
}
.dark .wb-icon-btn:hover {
  background: var(--color-surface-hover-dark, #2a2a2a);
}

/* Override Vue Flow theme for our design system */
:deep(.vue-flow) {
  height: 100% !important;
}
:deep(.vue-flow__pane) {
  cursor: default;
}
:deep(.vue-flow__edge-path) {
  stroke: var(--color-muted, #8b8b8b);
  stroke-width: 2;
}
.dark :deep(.vue-flow__edge-path) {
  stroke: var(--color-muted-dark, #71717a);
}
:deep(.vue-flow__controls) {
  border-radius: 10px;
  overflow: hidden;
  border: 1px solid var(--color-border, #e6e6e6);
  box-shadow: 0 2px 8px rgba(0,0,0,0.05);
}
.dark :deep(.vue-flow__controls) {
  border-color: var(--color-border-dark, #2c2c2c);
}
:deep(.vue-flow__controls-button) {
  background: var(--color-surface, #fff);
  border: none;
  color: var(--color-text-secondary, #52525b);
}
.dark :deep(.vue-flow__controls-button) {
  background: var(--color-surface-dark, #1e1e1e);
  color: var(--color-text-secondary-dark, #a1a1aa);
}
:deep(.vue-flow__controls-button:hover) {
  background: var(--color-surface-hover, #f5f5f5);
}
.dark :deep(.vue-flow__controls-button:hover) {
  background: var(--color-surface-hover-dark, #2a2a2a);
}
:deep(.vue-flow__background) {
  background: var(--color-base, #fdfdfc);
}
.dark :deep(.vue-flow__background) {
  background: var(--color-base-dark, #242424);
}
.dark :deep(.vue-flow__background pattern circle) {
  fill: #333;
}
.cursor-crosshair :deep(.vue-flow__pane) {
  cursor: crosshair !important;
}
/* Remove ALL VueFlow default selection visuals — handled inside custom nodes */
:deep(.vue-flow__node.selected),
:deep(.vue-flow__node.selected:focus),
:deep(.vue-flow__node.selected:focus-visible),
:deep(.vue-flow__node:focus),
:deep(.vue-flow__node:focus-visible) {
  box-shadow: none !important;
  outline: none !important;
  border: none !important;
}
:deep(.vue-flow__node) {
  border: none !important;
  outline: none !important;
  box-shadow: none !important;
}
.wb-cursor-eraser :deep(.vue-flow__pane) {
  cursor: none !important;
}
.wb-eraser-cursor {
  position: fixed;
  pointer-events: none;
  z-index: 9999;
  border-radius: 50%;
  border: 2px solid rgba(100, 100, 100, 0.7);
  background: rgba(200, 200, 200, 0.15);
  transform: translate(-50%, -50%);
}
:deep(.vue-flow__edge.selected .vue-flow__edge-path) {
  stroke: var(--color-accent, #7c3aed) !important;
  filter: drop-shadow(0 0 3px rgba(124, 58, 237, 0.4));
}
:deep(.vue-flow__edge-interaction) {
  stroke-width: 20px;
}
/*
 * Make shape node fills click-through so edges underneath can be selected.
 * pointer-events: none on the wrapper makes the fill area transparent to clicks.
 * Children (SVG stroke, handles, label) re-enable pointer-events so they're
 * still interactive. VueFlow drag still works via event bubbling from children.
 */
:deep(.vue-flow__node-shape) {
  pointer-events: none !important;
}
/* Re-enable pointer-events on resize handles so they remain interactive */
:deep(.vue-flow__node-shape .vue-flow__resize-control) {
  pointer-events: auto !important;
}
</style>
