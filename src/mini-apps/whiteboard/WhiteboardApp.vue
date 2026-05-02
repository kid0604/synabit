<script setup lang="ts">
import { ref, onMounted, watch, nextTick, toRef } from 'vue';
import { VueFlow, useVueFlow, ConnectionMode } from '@vue-flow/core';
import { Background } from '@vue-flow/background';
import { Controls } from '@vue-flow/controls';
import { Plus, Trash2, PenTool, Save, PanelLeftClose, PanelLeft } from 'lucide-vue-next';
import { toPng } from 'html-to-image';
import { ask } from '@tauri-apps/plugin-dialog';

// Custom nodes
import ShapeNode from './nodes/ShapeNode.vue';
import StrokeNode from './nodes/StrokeNode.vue';
import MindmapNode from './nodes/MindmapNode.vue';
import TextNode from './nodes/TextNode.vue';

// Toolbar
import WhiteboardToolbar from './components/WhiteboardToolbar.vue';

// Composables
import { useWhiteboardStore } from './composables/useWhiteboardStore';
import { useFreeDrawing, getStroke, getSvgPathFromStroke } from './composables/useFreeDrawing';
import type { ToolMode, ShapeType, WBNode, WBEdge } from './composables/useWhiteboardStore';
import { SHAPES_MAP } from './shapes';
import { logger } from '../../utils/logger';

// CSS
import '@vue-flow/core/dist/style.css';
import '@vue-flow/core/dist/theme-default.css';
import '@vue-flow/node-resizer/dist/style.css';

const props = defineProps<{
  vaultPath: string;
}>();

const vaultPathRef = toRef(props, 'vaultPath');
const store = useWhiteboardStore(vaultPathRef);

// ─── Vue Flow ───────────────────────────────────────────
// Use refs (not computed) so VueFlow can track node identity for drag operations.
const vfNodes = ref<any[]>([]);
const vfEdges = ref<any[]>([]);

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
      node.style = {
        width: `${n.data.width || 160}px`,
        height: `${n.data.height || 80}px`,
      };
    }
    return node;
  });
  vfEdges.value = store.currentBoardData.value.edges.map(e => ({
    id: e.id,
    source: e.source,
    sourceHandle: e.sourceHandle,
    target: e.target,
    targetHandle: e.targetHandle,
    type: e.type || 'smoothstep',
    data: e.data || {},
  }));
}

// Only sync on board switch — NOT on every data mutation.
watch(
  () => store.currentBoardId.value,
  () => syncToVueFlow(),
);

const { viewport, screenToFlowCoordinate } = useVueFlow({ id: 'whiteboard-flow' });

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
      store.currentBoardData.value.edges = store.currentBoardData.value.edges.filter(
        (e: WBEdge) => e.id !== change.id
      );
      dirty = true;
    }
  }
  if (dirty) scheduleSave();
}

function handleConnect(params: any) {
  const edge = {
    id: store.generateId('e'),
    source: params.source,
    sourceHandle: params.sourceHandle,
    target: params.target,
    targetHandle: params.targetHandle,
    type: 'smoothstep',
    data: {},
  };
  store.addEdge(edge);
  vfEdges.value = [...vfEdges.value, edge];
  scheduleSave();
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
    vfNode.style = {
      width: `${node.data.width || 160}px`,
      height: `${node.data.height || 80}px`,
    };
  }
  vfNodes.value = [...vfNodes.value, vfNode];
  scheduleSave();
}

function handlePaneClick(event: any) {
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

// ─── Node Events ────────────────────────────────────────
function handleNodeClick({ node }: any) {
  if (store.activeTool.value === 'eraser') {
    store.removeNode(node.id);
    vfNodes.value = vfNodes.value.filter((n: any) => n.id !== node.id);
    vfEdges.value = vfEdges.value.filter((e: any) => e.source !== node.id && e.target !== node.id);
    scheduleSave();
  }
}

function handleNodeDataUpdate(nodeId: string, data: any) {
  store.updateNodeData(nodeId, data);
  scheduleSave();
}

function handleMindmapAddChild({ parentId, direction }: { parentId: string; direction: 'right' | 'bottom' }) {
  const childId = store.addMindmapChild(parentId, direction);
  // Sync new child node + edge to VueFlow
  syncToVueFlow();
  scheduleSave();
}

function handleMindmapRemoveNode(nodeId: string) {
  store.removeNode(nodeId);
  vfNodes.value = vfNodes.value.filter((n: any) => n.id !== nodeId);
  vfEdges.value = vfEdges.value.filter((e: any) => e.source !== nodeId && e.target !== nodeId);
  scheduleSave();
}

function handleMindmapAddSibling(nodeId: string) {
  store.addMindmapSibling(nodeId);
  syncToVueFlow();
  scheduleSave();
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
        handleMindmapAddChild({ parentId: selectedNode.id, direction: 'right' });
      } else {
        handleMindmapAddSibling(selectedNode.id);
      }
      return;
    }
  }

  if (e.key === 'v' || e.key === 'V') { store.activeTool.value = 'select'; return; }
  if (e.key === 'd' || e.key === 'D') { store.activeTool.value = 'draw'; return; }
  if (e.key === 's' && !e.ctrlKey && !e.metaKey) { store.activeTool.value = 'shape'; return; }
  if (e.key === 't' || e.key === 'T') { store.activeTool.value = 'text'; return; }
  if (e.key === 'e' || e.key === 'E') { store.activeTool.value = 'draw'; store.drawSubTool.value = 'eraser'; return; }
  if (e.key === 'm' || e.key === 'M') { store.activeTool.value = 'mindmap'; return; }

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
}

// ─── Export PNG ──────────────────────────────────────────
const vueFlowRef = ref<HTMLElement | null>(null);

async function exportPng() {
  const el = document.querySelector('.vue-flow__viewport') as HTMLElement;
  if (!el) return;
  try {
    const dataUrl = await toPng(el, {
      backgroundColor: '#ffffff',
      pixelRatio: 2,
    });
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
const editingTitle = ref(false);
const titleInput = ref('');

function startEditTitle() {
  if (!store.currentBoardData.value) return;
  editingTitle.value = true;
  titleInput.value = store.currentBoardData.value.title;
}

function finishEditTitle() {
  editingTitle.value = false;
  if (store.currentBoardData.value && titleInput.value.trim()) {
    store.currentBoardData.value.title = titleInput.value.trim();
    store.saveCurrentBoard(); // Save immediately so sidebar updates
  }
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
        <h2 class="text-sm font-bold text-text dark:text-text-dark">Boards</h2>
        <div class="flex items-center gap-1" @mousedown.stop>
          <button
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

      <div class="flex-1 overflow-y-auto p-2 space-y-1" @mousedown.stop>
        <button
          v-for="board in store.boards.value"
          :key="board.id"
          @click="store.loadBoardData(board.id)"
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
            <span v-if="store.isSaving.value" class="text-[10px] text-muted dark:text-muted-dark">Saving…</span>
            <button @click="store.saveCurrentBoard()" class="wb-icon-btn" title="Save (Ctrl+S)">
              <Save class="w-3.5 h-3.5" />
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
          :pan-on-drag="store.activeTool.value === 'select'"
          :zoom-on-scroll="true"
          :nodes-draggable="store.activeTool.value === 'select'"
          :nodes-connectable="store.activeTool.value === 'select'"
          @pane-click="handlePaneClick"
          @node-click="handleNodeClick"
          @connect="handleConnect"
          @nodes-change="handleNodesChange"
          @edges-change="handleEdgesChange"
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
</style>
