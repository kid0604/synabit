<script setup lang="ts">
import { ref, computed, nextTick, onMounted, onUnmounted, watch, toRef, provide, inject, onActivated, onDeactivated } from 'vue';
import { VueFlow, useVueFlow, ConnectionMode } from '@vue-flow/core';
import { Background } from '@vue-flow/background';
import { Controls } from '@vue-flow/controls';
import { FileWarning, PenTool } from 'lucide-vue-next';

// ── Existing Components ─────────────────────────────────────
import EdgeMenu from './components/EdgeMenu.vue';
import ShapeMenu from './components/ShapeMenu.vue';
import TextMenu from './components/TextMenu.vue';
import MultiSelectMenu from './components/MultiSelectMenu.vue';
import ShapeNode from './nodes/ShapeNode.vue';
import StrokeNode from './nodes/StrokeNode.vue';
import MindmapNode from './nodes/MindmapNode.vue';
import TextNode from './nodes/TextNode.vue';
import NoteCardNode from './nodes/NoteCardNode.vue';
import ImageNode from './nodes/ImageNode.vue';
import WaypointEdge from './components/WaypointEdge.vue';
import WhiteboardToolbar from './components/WhiteboardToolbar.vue';

// ── New Extracted Components ────────────────────────────────
import WhiteboardSidebar from './components/WhiteboardSidebar.vue';
import WhiteboardTitleBar from './components/WhiteboardTitleBar.vue';
import ConfirmModal from '../../shared/components/ConfirmModal.vue';

// ── Composables ─────────────────────────────────────────────
import { useWhiteboardStore } from './composables/useWhiteboardStore';
import { useFreeDrawing } from './composables/useFreeDrawing';
import { useNodeOperations } from './composables/useNodeOperations';
import { useEdgeMenu } from './composables/useEdgeMenu';
import { useShapeMenu } from './composables/useShapeMenu';
import { useTextMenu } from './composables/useTextMenu';
import { useMultiSelect } from './composables/useMultiSelect';
import { useEraser } from './composables/useEraser';
import { useMindmapDrag } from './composables/useMindmapDrag';
import { useClipboardExport } from './composables/useClipboardExport';
import { useWhiteboardKeyboard } from './composables/useWhiteboardKeyboard';

import type { WBNode, WBEdge } from './composables/useWhiteboardStore';
import { SHAPES_MAP } from './shapes';
import { useEventBus } from '../../composables/useEventBus';
import { forgetViewport, recallViewport, rememberViewport } from './viewportMemory';
import {
  assetUrl,
  fitWithin,
  importImagePath,
  naturalSize,
  naturalSizeOfUrl,
  saveImageToVault,
} from './imageAssets';
import { open as openFileDialog } from '@tauri-apps/plugin-dialog';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../../utils/logger';
import type { NavEntry } from '../../stores/useNavigationStore';

// CSS
import '@vue-flow/core/dist/style.css';
import '@vue-flow/core/dist/theme-default.css';
import '@vue-flow/node-resizer/dist/style.css';

// ── Props & Services ────────────────────────────────────────
const props = defineProps<{ vaultPath: string }>();

const bus = useEventBus();
const vaultPathRef = toRef(props, 'vaultPath');
const store = useWhiteboardStore(vaultPathRef);

// ── Navigation ──────────────────────────────────────────────
const pushNavigation = inject<(entry?: NavEntry) => void>('pushNavigation');
const skipNavPush = false;

const isMobile = ref(window.innerWidth < 768);

const switchBoard = async (boardId: string) => {
  if (boardId !== store.currentBoardId.value && store.currentBoardId.value && !skipNavPush) {
    pushNavigation?.({ app: 'whiteboard', itemId: store.currentBoardId.value });
  }
  // The board being left may still have a change waiting on the save timer,
  // and loading the next one over the top of it is how that change is lost.
  await flushSave();
  store.loadBoardData(boardId);
};

// ── Keep-alive tracking ─────────────────────────────────────
const isAppActive = ref(true);
onActivated(() => { isAppActive.value = true; });
onDeactivated(() => { isAppActive.value = false; void flushSave(); });

// ── VueFlow Core ────────────────────────────────────────────
const {
  viewport,
  screenToFlowCoordinate,
  addSelectedNodes,
  fitView,
  setViewport,
} = useVueFlow({ id: 'whiteboard-flow' });

const vfNodes = ref<any[]>([]);
const vfEdges = ref<any[]>([]);
const canvasRef = ref<HTMLElement | null>(null);
const vueFlowRef = ref<HTMLElement | null>(null);

// ── Auto-save ───────────────────────────────────────────────
// A board is written two seconds after the last change. That delay is what
// keeps a drag from writing the file sixty times, and it is also a two-second
// window in which the work exists only in memory — so every way out of this
// component closes the window rather than dropping it.
let saveTimer: ReturnType<typeof setTimeout> | null = null;
function scheduleSave() {
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = setTimeout(() => { saveTimer = null; store.saveCurrentBoard(); }, 2000);
}

/**
 * Write the pending change now. No-op when there is nothing waiting.
 *
 * Awaitable, and worth awaiting before the open board is swapped: the save
 * reads which board it is writing when it starts, so a swap that does not
 * wait can hand one board's title to another board's row.
 */
async function flushSave() {
  if (!saveTimer) return;
  clearTimeout(saveTimer);
  saveTimer = null;
  await store.saveCurrentBoard();
}

/**
 * Forget the pending change without writing it.
 *
 * Only for the case where what is in memory is about to be replaced by what
 * is on disk: letting the timer fire afterwards would write the copy the
 * reload just discarded, back over the one it just read.
 */
function cancelPendingSave() {
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = null;
}

function handleVisibilityChange() {
  if (document.hidden) void flushSave();
}

// ── Composable Wiring ───────────────────────────────────────
const { applySize, computeShapeZIndex, deleteNodes, updateNodeData, buildVfEdge } =
  useNodeOperations(store, vfNodes, vfEdges, scheduleSave);

const {
  selectedEdgeId, selectedEdgeData,
  handleEdgeClick, handleEdgeUpdate, handleEdgeDelete,
  closeEdgeMenu, updateEdgeWaypoints, getUpdatingEdgeId,
} = useEdgeMenu(store, vfEdges, vfNodes, buildVfEdge, scheduleSave);

provide('updateEdgeWaypoints', updateEdgeWaypoints);
// Pictures hold a path inside the vault and need to know which vault.
provide('whiteboardVaultPath', vaultPathRef);

const {
  selectedShapeNodeId, shapeMenuPos, selectedShapeData,
  handleShapeUpdate, handleShapeDelete, closeShapeMenu,
} = useShapeMenu(store, updateNodeData, deleteNodes);

const {
  selectedTextNodeId, selectedTextData,
  handleTextUpdate, handleTextDelete, closeTextMenu,
} = useTextMenu(store, vfNodes, updateNodeData, deleteNodes);

const {
  multiSelectedNodes, showMultiSelectMenu,
  handleMultiGroup, handleMultiUngroup,
  handleMultiDelete, handleMultiUpdateAll,
  closeMultiSelectMenu,
} = useMultiSelect(store, vfNodes, vfEdges, deleteNodes, updateNodeData, scheduleSave);

const { isErasing, eraserPos, eraseStrokesNear } =
  useEraser(store, vfNodes, viewport, scheduleSave);

const { handleNodeDragStart: mindmapDragStart, handleNodeDrag: mindmapDrag, handleNodeDragStop: mindmapDragStop } =
  useMindmapDrag(store, vfNodes, vfEdges, scheduleSave);

// ── VueFlow Sync ────────────────────────────────────────────
function syncToVueFlow() {
  if (!store.currentBoardData.value) {
    vfNodes.value = [];
    vfEdges.value = [];
    return;
  }
  vfNodes.value = store.currentBoardData.value.nodes.map((n: WBNode) => {
    const node: any = {
      id: n.id, type: n.type,
      position: { ...n.position },
      data: { ...n.data },
      draggable: true,
    };
    applySize(node, n.data.width, n.data.height);
    return node;
  });
  vfEdges.value = store.currentBoardData.value.edges.map((e: WBEdge) =>
    buildVfEdge(e, store.currentBoardData.value!.nodes)
  );
}

watch(() => store.currentBoardId.value, (boardId) => {
  syncToVueFlow();

  // Where this board was left, on this device. `default-viewport` only
  // applies to the first board the canvas ever shows, so switching boards
  // needs the camera moved by hand.
  const remembered = boardId ? recallViewport(boardId) : null;
  if (remembered) {
    // After the swapped-in nodes are in the document, so the canvas is
    // moving a board it already has rather than an empty one. The very first
    // board of the session is placed by `default-viewport` instead, which
    // avoids a visible jump on open.
    nextTick(() => setViewport(remembered));
    return;
  }
  // Never opened here: fit the whole board on a phone, where the file's own
  // viewport is likely to have been written on a much wider screen.
  if (isMobile.value) {
    setTimeout(() => {
      fitView({ padding: 0.1, duration: 500 });
    }, 150);
  }
});

/**
 * Remember the camera when the user stops moving it.
 *
 * `move-end` is one event per gesture — the end of a drag, or of a burst of
 * wheel zooming — rather than one per frame.
 */
/**
 * Where the canvas should sit the moment it appears.
 *
 * Only read at mount, which is why switching boards needs `setViewport` as
 * well — but reading it here is what keeps the first board of the session
 * from opening at the origin and sliding into place afterwards.
 */
const initialViewport = computed(() => {
  const boardId = store.currentBoardId.value;
  return (
    (boardId ? recallViewport(boardId) : null) ??
    store.currentBoardData.value?.viewport ?? { x: 0, y: 0, zoom: 1 }
  );
});

function handleMoveEnd() {
  const boardId = store.currentBoardId.value;
  if (boardId) rememberViewport(boardId, { ...viewport.value });
}

// ── VueFlow Change Handlers ─────────────────────────────────
function handleNodesChange(changes: any[]) {
  if (!store.currentBoardData.value) return;
  let dirty = false;
  for (const change of changes) {
    if (change.type === 'position' && change.position) {
      const wbNode = store.currentBoardData.value.nodes.find((n: WBNode) => n.id === change.id);
      if (wbNode) {
        wbNode.position = { x: change.position.x, y: change.position.y };
        store.stampNode(change.id);
        dirty = true;
      }
    } else if (change.type === 'remove') {
      store.currentBoardData.value.nodes = store.currentBoardData.value.nodes.filter((n: WBNode) => n.id !== change.id);
      store.currentBoardData.value.edges = store.currentBoardData.value.edges.filter((e: WBEdge) => e.source !== change.id && e.target !== change.id);
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
      if (getUpdatingEdgeId() === change.id) continue;
      store.currentBoardData.value.edges = store.currentBoardData.value.edges.filter((e: WBEdge) => e.id !== change.id);
      dirty = true;
    }
  }
  if (dirty) scheduleSave();
}

function handleConnect(params: any) {
  const edge: any = {
    id: store.generateId('e'), source: params.source, sourceHandle: params.sourceHandle,
    target: params.target, targetHandle: params.targetHandle, type: 'default', data: {}, zIndex: 10001,
  };
  store.addEdge(edge);
  vfEdges.value = [...vfEdges.value, edge];
  scheduleSave();
}

// ── Canvas Node Creation ────────────────────────────────────
function addNodeToCanvas(node: WBNode) {
  store.addNode(node);
  const vfNode: any = { ...node, position: { ...node.position }, data: { ...node.data }, draggable: true };
  applySize(vfNode, node.data.width, node.data.height);
  vfNodes.value = [...vfNodes.value, vfNode];
  scheduleSave();
}

function handlePaneClick(event: any) {
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
      id: store.generateId('shape'), type: 'shape', position: pos,
      data: { shapeType: shape, label: '', color: store.activeColor.value, width: defaultW, height: defaultH },
    });
    store.activeTool.value = 'select';
  } else if (store.activeTool.value === 'text') {
    addNodeToCanvas({ id: store.generateId('text'), type: 'text', position: pos, data: { label: '' } });
    store.activeTool.value = 'select';
  } else if (store.activeTool.value === 'mindmap') {
    addNodeToCanvas({
      id: store.generateId('mind'), type: 'mindmap', position: pos,
      data: { label: 'Central Idea', color: store.getMindmapColor(0), level: 0, editing: true },
    });
    store.activeTool.value = 'select';
  }
}

// ── Pictures ────────────────────────────────────────────────
// Where the pointer was last seen over the canvas, so a pasted picture lands
// under it rather than in the middle of wherever the user happens to be
// looking. Deliberately not a ref: it is written on every pointer move and
// nothing renders from it.
let lastCanvasPointer: { x: number; y: number } | null = null;

/** Board coordinates for something the user is adding without a drop point. */
function placementPoint(): { x: number; y: number } {
  if (lastCanvasPointer) return screenToFlowCoordinate(lastCanvasPointer);
  const rect = (document.querySelector('.vue-flow') as HTMLElement | null)?.getBoundingClientRect();
  if (!rect) return { x: 0, y: 0 };
  return screenToFlowCoordinate({ x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 });
}

/**
 * Copy pictures into the vault and put them on the board.
 *
 * One at a time and offset from each other, so a multiple selection does not
 * arrive as one stack the user has to pull apart.
 */
async function addImageFiles(files: File[], at: { x: number; y: number }) {
  const images = files.filter((f) => f.type.startsWith('image/'));
  if (!images.length || !props.vaultPath) return;

  let offset = 0;
  for (const file of images) {
    try {
      // Measured from the bytes already in hand. Reading the file back out
      // of the vault to measure it would be a round trip for something that
      // is right here.
      const size = fitWithin(await naturalSize(file));
      const assetPath = await saveImageToVault(props.vaultPath, file);
      addNodeToCanvas({
        id: store.generateId('img'),
        type: 'image',
        position: { x: at.x + offset, y: at.y + offset },
        data: { assetPath, alt: file.name || '', width: size.width, height: size.height },
      });
      offset += 24;
    } catch (err) {
      logger.error('Failed to put an image on the board', err as string);
    }
  }
}

/**
 * Choose pictures from disk.
 *
 * Paste and drop are how most pictures will arrive, and neither is visible:
 * this is the toolbar's answer to "can I put a picture on here at all". The
 * file is copied by path rather than read into the page first, so a photo
 * straight off a camera does not cross the bridge as a list of numbers.
 */
async function pickImages() {
  if (!props.vaultPath) return;
  try {
    const chosen = await openFileDialog({
      multiple: true,
      filters: [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'avif', 'bmp'] }],
    });
    const paths = Array.isArray(chosen) ? chosen : chosen ? [chosen] : [];
    if (!paths.length) return;

    const at = placementPoint();
    let offset = 0;
    for (const sourcePath of paths) {
      const assetPath = await importImagePath(props.vaultPath, sourcePath);
      const size = fitWithin(await naturalSizeOfUrl(assetUrl(props.vaultPath, assetPath)));
      addNodeToCanvas({
        id: store.generateId('img'),
        type: 'image',
        position: { x: at.x + offset, y: at.y + offset },
        data: { assetPath, alt: sourcePath.split(/[\\/]/).pop() || '', width: size.width, height: size.height },
      });
      offset += 24;
    }
  } catch (err) {
    logger.error('Failed to import images', err as string);
  }
}

/**
 * A paste on the board.
 *
 * The clipboard may hold a picture from anywhere, or the board's own copied
 * item, and only this event can tell — which is why `Ctrl+V` is not in the
 * keyboard handler: cancelling the key there would stop this event from ever
 * arriving. A paste inside a text field is the field's business.
 */
function handlePaste(event: ClipboardEvent) {
  if (!isAppActive.value) return;
  const target = event.target as HTMLElement | null;
  if (target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable)) {
    return;
  }

  const files = Array.from(event.clipboardData?.items ?? [])
    .filter((item) => item.kind === 'file' && item.type.startsWith('image/'))
    .map((item) => item.getAsFile())
    .filter((file): file is File => !!file);

  event.preventDefault();
  if (files.length) {
    void addImageFiles(files, placementPoint());
    return;
  }
  pasteClipboard();
}

function handleDragOver(event: DragEvent) {
  event.preventDefault();
  if (event.dataTransfer) event.dataTransfer.dropEffect = 'copy';
}

function handleDrop(event: DragEvent) {
  event.preventDefault();
  if (!event.dataTransfer) return;

  // Picture files from outside the app — a screenshot, a photo, a folder
  // full of both.
  const dropped = Array.from(event.dataTransfer.files ?? []);
  if (dropped.some((f) => f.type.startsWith('image/'))) {
    void addImageFiles(dropped, screenToFlowCoordinate({ x: event.clientX, y: event.clientY }));
    return;
  }

  const noteId = event.dataTransfer.getData('application/synabit-note-id');
  const noteTitle = event.dataTransfer.getData('application/synabit-note-title');
  const blockId = event.dataTransfer.getData('application/synabit-block-id');
  if (noteId) {
    const pos = screenToFlowCoordinate({ x: event.clientX, y: event.clientY });
    addNodeToCanvas({
      id: store.generateId('note'), type: 'note', position: pos,
      data: { noteId, noteTitle, blockId: blockId || undefined, width: 280, height: 180 },
    });
  }
}

// ── Free Drawing ────────────────────────────────────────────
const freeDrawing = useFreeDrawing({
  color: store.activeColor,
  size: store.activeStrokeSize,
  onStrokeComplete: (svgPath, points, color, size, minX, minY) => {
    const isHighlighter = store.drawSubTool.value === 'highlighter';
    const node: WBNode = {
      id: store.generateId('stroke'), type: 'stroke',
      position: { x: minX, y: minY },
      data: { svgPath, points, color, size: isHighlighter ? size * 3 : size, opacity: isHighlighter ? 0.35 : 0.85 },
    };
    store.addNode(node);
    vfNodes.value = [...vfNodes.value, { ...node, draggable: true }];
    scheduleSave();
  },
});

// Open while an eraser gesture is in progress, so that the batch is closed by
// whichever of pointerup or pointerleave arrives — and by neither twice.
let eraseBatchOpen = false;

function handleCanvasPointerDown(e: PointerEvent) {
  if (store.activeTool.value !== 'draw') return;
  if (store.drawSubTool.value === 'eraser') {
    isErasing.value = true;
    if (!eraseBatchOpen) { store.beginUndoBatch(); eraseBatchOpen = true; }
    eraseStrokesNear(e);
    return;
  }
  const vfEl = document.querySelector('.vue-flow') as HTMLElement | null;
  if (!vfEl) return;
  freeDrawing.startDraw(e, vfEl.getBoundingClientRect(), viewport.value);
}

function handleCanvasPointerMove(e: PointerEvent) {
  lastCanvasPointer = { x: e.clientX, y: e.clientY };
  if (store.activeTool.value !== 'draw') return;
  if (store.drawSubTool.value === 'eraser') {
    eraserPos.value = { x: e.clientX, y: e.clientY };
    if (isErasing.value) eraseStrokesNear(e);
    return;
  }
  const vfEl = document.querySelector('.vue-flow') as HTMLElement | null;
  if (!vfEl) return;
  freeDrawing.continueDraw(e, vfEl.getBoundingClientRect(), viewport.value);
}

function handleCanvasPointerUp() {
  if (eraseBatchOpen) { store.endUndoBatch(); eraseBatchOpen = false; }
  if (store.activeTool.value !== 'draw') return;
  if (store.drawSubTool.value === 'eraser') { isErasing.value = false; return; }
  freeDrawing.endDraw();
}

// ── Node Click ──────────────────────────────────────────────
function handleNodeClick({ node, event }: any) {
  selectedEdgeId.value = null;

  if (store.activeTool.value === 'eraser') {
    deleteNodes([node.id]);
    selectedShapeNodeId.value = null;
    return;
  }

  if (store.activeTool.value === 'select' && node.data?.groupId) {
    const groupMembers = vfNodes.value.filter((n: any) => n.data?.groupId === node.data.groupId && n.id !== node.id);
    if (groupMembers.length > 0) addSelectedNodes(groupMembers);
  }

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

/**
 * A picture being resized, then the resize itself.
 *
 * While the pointer is down this writes one property on one node and stops.
 * That is the whole budget: the canvas is a component tree, and anything more
 * — replacing the node list, or writing the same node twice through two
 * different doors — is another full pass over it, several times per frame,
 * which is seen as the picture shivering rather than turning.
 *
 * `useMindmapDrag` has done it this way all along: move the canvas node
 * during the drag, write the board when it stops.
 */
function handleNodeBoxUpdate(
  nodeId: string,
  box: { x: number; y: number; width: number; height: number },
  final: boolean
) {
  const vfNode = vfNodes.value.find((n: any) => n.id === nodeId);
  if (vfNode) {
    // In place, property by property. Handing out fresh objects here makes
    // the canvas rebuild what it holds for the node.
    vfNode.position.x = box.x;
    vfNode.position.y = box.y;
    vfNode.data.width = box.width;
    vfNode.data.height = box.height;
  }
  if (!final) return;

  const wbNode = store.currentBoardData.value?.nodes.find((n: WBNode) => n.id === nodeId);
  if (!wbNode) return;

  // One step to undo, one save, one stamp — for the gesture, not the frame.
  store.pushUndoState();
  wbNode.position = { x: box.x, y: box.y };
  wbNode.data = { ...wbNode.data, width: box.width, height: box.height };
  store.stampNode(nodeId);
  // Smaller sits on top: worth redoing now the size has settled.
  if (vfNode) vfNode.zIndex = computeShapeZIndex(box.width, box.height);
  scheduleSave();
}

/** The angle of a picture. Same division of labour as the box above. */
function handleImageRotation(nodeId: string, degrees: number, final: boolean) {
  const vfNode = vfNodes.value.find((n: any) => n.id === nodeId);
  if (vfNode) vfNode.data.rotation = degrees;
  if (!final) return;

  const wbNode = store.currentBoardData.value?.nodes.find((n: WBNode) => n.id === nodeId);
  if (!wbNode) return;

  store.pushUndoState();
  wbNode.data = { ...wbNode.data, rotation: degrees };
  store.stampNode(nodeId);
  scheduleSave();
}

function handleNodeDataUpdate(nodeId: string, data: any) {
  store.updateNodeData(nodeId, data);
  const vfNode = vfNodes.value.find((n: any) => n.id === nodeId);
  if (vfNode) {
    vfNode.data = { ...vfNode.data, ...data };
    if (data.width || data.height) applySize(vfNode, data.width, data.height);
    vfNodes.value = [...vfNodes.value];
  }
  scheduleSave();
}

// ── Mindmap Helpers ─────────────────────────────────────────
function handleMindmapAddChild({ parentId, direction }: { parentId: string; direction: 'right' | 'left' }) {
  const childId = store.addMindmapChild(parentId, direction);
  syncToVueFlow();
  scheduleSave();
  if (childId) focusMindmapNode(childId);
}

function handleMindmapRemoveNode(nodeId: string) {
  deleteNodes([nodeId]);
}

function handleMindmapAddSibling(nodeId: string) {
  const siblingId = store.addMindmapSibling(nodeId);
  syncToVueFlow();
  scheduleSave();
  if (siblingId) focusMindmapNode(siblingId);
}

function focusMindmapNode(nodeId: string) {
  let attempts = 0;
  const tryFocus = () => {
    const nodeEl = document.querySelector(`[data-id="${nodeId}"]`);
    if (nodeEl) {
      const input = nodeEl.querySelector('input') as HTMLInputElement | null;
      if (input) { input.focus(); input.select(); return; }
    }
    if (++attempts < 10) setTimeout(tryFocus, 50);
  };
  setTimeout(tryFocus, 50);
}

// ── Node Drag (delegates to mindmap drag) ───────────────────
function handleNodeDragStart(event: any) {
  const { node } = event;
  // Before anything moves. A drag reports a position per frame, so the step
  // to come back to is the one recorded here, once, at the start of it.
  store.pushUndoState();
  if (node.data?.groupId) {
    const groupMembers = vfNodes.value.filter((n: any) => n.data?.groupId === node.data.groupId && n.id !== node.id);
    if (groupMembers.length > 0) addSelectedNodes(groupMembers);
  }
  mindmapDragStart(event);
}

// ── Clipboard & Export ──────────────────────────────────────
const { copySelected, pasteClipboard, exportPng, isExporting } =
  useClipboardExport(store, vfNodes, vfEdges, addNodeToCanvas, scheduleSave, vaultPathRef);

// ── Keyboard Shortcuts ──────────────────────────────────────
const { handleKeydown } = useWhiteboardKeyboard({
  store, vfNodes, vfEdges,
  deleteNodes, syncToVueFlow, scheduleSave,
  copySelected,
  focusMindmapNode, handleMindmapAddChild, handleMindmapAddSibling, handleMindmapRemoveNode,
  handleMultiGroup, handleMultiUngroup,
  closeEdgeMenu, closeShapeMenu, closeTextMenu,
});

// ── Title & Tags (handled by TitleBar component) ────────────
function handleUpdateTitle(title: string) {
  if (store.currentBoardData.value) {
    store.currentBoardData.value.title = title;
    store.saveCurrentBoard();
  }
}
function handleAddTag(tag: string) {
  if (store.currentBoardData.value && !store.currentBoardData.value.tags.includes(tag)) {
    store.currentBoardData.value.tags.push(tag);
    store.saveCurrentBoard();
  }
}
function handleRemoveTag(tag: string) {
  if (store.currentBoardData.value) {
    store.currentBoardData.value.tags = store.currentBoardData.value.tags.filter((t: string) => t !== tag);
    store.saveCurrentBoard();
  }
}

// ── Sidebar ref & notes ─────────────────────────────────────
const sidebarRef = ref<InstanceType<typeof WhiteboardSidebar> | null>(null);
const whiteboardNotes = ref<any[]>([]);

// ── Deleting a board ────────────────────────────────────────
// The delete icon sits inside each row in the sidebar and appears on hover,
// which is about as easy to hit by accident as a control gets. Ask first: a
// board is hours of work, and the answer to "where did it go" has to be a
// place rather than an apology.
const pendingDeleteBoardId = ref<string | null>(null);
const pendingDeleteBoard = computed(() =>
  store.boards.value.find((b: any) => b.id === pendingDeleteBoardId.value) || null
);

function confirmDeleteBoard() {
  const id = pendingDeleteBoardId.value;
  pendingDeleteBoardId.value = null;
  if (!id) return;
  if (id === store.currentBoardId.value) cancelPendingSave();
  forgetViewport(id);
  store.deleteBoard(id);
}

// ── Lifecycle ───────────────────────────────────────────────
onMounted(async () => {
  await store.loadBoards();
  if (store.boards.value.length > 0) {
    await store.loadBoardData(store.boards.value[0].id);
  }

  try {
    // The sidebar is a drag source: it shows titles and one-line previews.
    // Loading every note's body for that was the bulk of opening the board.
    const loadedNotes = await invoke<any[]>('get_node_summaries', { nodeType: 'note' });
    whiteboardNotes.value = loadedNotes.sort((a: any, b: any) => b.created_at.localeCompare(a.created_at));
  } catch (err) {
    logger.error('Failed to load notes for whiteboard sidebar', err);
  }

  const handleResize = () => { isMobile.value = window.innerWidth < 768; };
  window.addEventListener('resize', handleResize);
  window.addEventListener('keydown', handleKeydown);

  document.addEventListener('visibilitychange', handleVisibilityChange);
  window.addEventListener('paste', handlePaste);

  bus.on('vault:file-modified', () => { store.loadBoards(); });
  bus.on('vault:file-created-deleted', () => { store.loadBoards(); });

  // A pull rewrites the board's file on disk. Refreshing only the list left
  // the open board held in memory as it was before the pull, and the next
  // edit wrote that copy back over what had just arrived — the other device's
  // work, gone, with nothing on screen to say so. Read the board again when
  // it is one of the files that changed.
  bus.on('vault:sync-completed', (payload: any) => {
    const pulled = payload?.pulled_files as string[] | undefined;
    const openId = store.currentBoardId.value;
    store.loadBoards().then(() => {
      if (openId && pulled?.includes(openId)) {
        cancelPendingSave();
        store.loadBoardData(openId);
      }
    });
  });
});

onUnmounted(() => {
  void flushSave();
  window.removeEventListener('keydown', handleKeydown);
  window.removeEventListener('paste', handlePaste);
  document.removeEventListener('visibilitychange', handleVisibilityChange);
});

// ── Expose ──────────────────────────────────────────────────
async function openBoardById(boardId: string, _skipNavPush = false) {
  if (!_skipNavPush && store.currentBoardId.value && store.currentBoardId.value !== boardId && !skipNavPush) {
    pushNavigation?.({ app: 'whiteboard', itemId: store.currentBoardId.value });
  }
  await flushSave();
  if (!store.boards.value.length || !store.boards.value.find((b: any) => b.id === boardId)) {
    await store.loadBoards();
  }
  await store.loadBoardData(boardId);
}

async function refreshBoards() { await store.loadBoards(); }

defineExpose({ openBoardById, currentBoardId: store.currentBoardId, refreshBoards });
</script>

<template>
  <div class="flex flex-1 h-full overflow-hidden bg-base dark:bg-base-dark text-text dark:text-text-dark" :class="{'cursor-col-resize': sidebarRef?.isDraggingSidebar}">
    <!-- Sidebar -->
    <WhiteboardSidebar
      ref="sidebarRef"
      :boards="store.boards.value"
      :currentBoardId="store.currentBoardId.value || ''"
      :currentBoardData="store.currentBoardData.value"
      :notes="whiteboardNotes"
      @switch-board="switchBoard"
      @create-board="store.createBoard()"
      @delete-board="pendingDeleteBoardId = $event"
      @note-drag-start="() => {}"
    />

    <!-- Main Canvas Area -->
    <div
      class="flex-1 relative transition-colors bg-base dark:bg-base-dark"
      ref="canvasRef"
      :style="{ backgroundColor: store.backgroundColor.value === 'transparent' ? '' : store.backgroundColor.value }"
    >
      <template v-if="store.currentBoardData.value">
        <!-- Title Bar -->
        <WhiteboardTitleBar
          :boardData="store.currentBoardData.value"
          :isSaving="store.isSaving.value"
          @update-title="handleUpdateTitle"
          @add-tag="handleAddTag"
          @remove-tag="handleRemoveTag"
          @open-sidebar="sidebarRef && (sidebarRef.sidebarOpen = true)"
        />

        <!-- Vue Flow Canvas -->
        <VueFlow
          id="whiteboard-flow"
          ref="vueFlowRef"
          v-model:nodes="vfNodes"
          v-model:edges="vfEdges"
          :class="[
            'flex-1',
            store.activeTool.value === 'pan' && 'wb-cursor-grab',
            store.activeTool.value === 'draw' && store.drawSubTool.value !== 'eraser' && 'cursor-crosshair',
            store.activeTool.value === 'draw' && store.drawSubTool.value === 'eraser' && 'wb-cursor-eraser',
            store.activeTool.value === 'eraser' && 'wb-cursor-eraser',
          ]"
          :default-viewport="initialViewport"
          :snap-to-grid="true"
          :snap-grid="[10, 10]"
          :only-render-visible-elements="!isExporting"
          :connection-mode="ConnectionMode.Loose"
          :delete-key-code="['Delete', 'Backspace']"
          :pan-on-drag="store.activeTool.value === 'pan' || (isMobile && store.activeTool.value === 'select') ? [0, 1, 2] : (store.activeTool.value === 'select' ? [1, 2] : false)"
          :selection-on-drag="!isMobile && store.activeTool.value === 'select'"
          :pan-on-scroll="true"
          :zoom-on-scroll="true"
          :zoom-on-pinch="true"
          :nodes-draggable="store.activeTool.value === 'select'"
          :nodes-connectable="store.activeTool.value === 'select'"
          :elevate-edges-on-select="true"
          @move-end="handleMoveEnd"
          @pane-click="handlePaneClick"
          @node-click="handleNodeClick"
          @node-drag-start="handleNodeDragStart"
          @node-drag="mindmapDrag"
          @node-drag-stop="mindmapDragStop"
          @edge-click="handleEdgeClick"
          @connect="handleConnect"
          @nodes-change="handleNodesChange"
          @edges-change="handleEdgesChange"
          @dragover.prevent="handleDragOver"
          @drop.prevent="handleDrop"
        >
          <template #edge-default="edgeProps"><WaypointEdge v-bind="(edgeProps as any)" edge-type="default" /></template>
          <template #edge-straight="edgeProps"><WaypointEdge v-bind="(edgeProps as any)" edge-type="straight" /></template>
          <template #edge-step="edgeProps"><WaypointEdge v-bind="(edgeProps as any)" edge-type="step" /></template>
          <template #node-shape="nodeProps"><ShapeNode v-bind="nodeProps" @update:data="(d: any) => handleNodeDataUpdate(nodeProps.id, d)" /></template>
          <template #node-stroke="nodeProps"><StrokeNode v-bind="nodeProps" /></template>
          <template #node-mindmap="nodeProps">
            <MindmapNode v-bind="nodeProps" @update:data="(d: any) => handleNodeDataUpdate(nodeProps.id, d)" @add-child="handleMindmapAddChild" @add-sibling="handleMindmapAddSibling" @remove-node="handleMindmapRemoveNode" />
          </template>
          <template #node-text="nodeProps"><TextNode v-bind="nodeProps" @update:data="(d: any) => handleNodeDataUpdate(nodeProps.id, d)" /></template>
          <template #node-note="nodeProps"><NoteCardNode v-bind="nodeProps" @update:data="(d: any) => handleNodeDataUpdate(nodeProps.id, d)" /></template>
          <template #node-image="nodeProps">
            <ImageNode
              v-bind="nodeProps"
              @update:box="(b: any, final: boolean) => handleNodeBoxUpdate(nodeProps.id, b, final)"
              @update:rotation="(deg: number, final: boolean) => handleImageRotation(nodeProps.id, deg, final)"
            />
          </template>

          <Background v-if="store.backgroundPattern.value === 'dots'" variant="dots" :gap="20" :size="1" pattern-color="currentColor" class="text-[#a1a1aa] dark:text-[#52525b]" />
          <template v-if="store.backgroundPattern.value === 'lines'">
            <Background variant="lines" :gap="20" :size="1" pattern-color="currentColor" class="text-black/[0.03] dark:text-white/[0.03]" />
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
          @add-image="pickImages"
        />

        <!-- Property Menus (Teleported) -->
        <Teleport to="body">
          <EdgeMenu v-if="isAppActive && selectedEdgeId && selectedEdgeData" :edge-id="selectedEdgeId" :edge-data="selectedEdgeData" @update="handleEdgeUpdate" @delete="handleEdgeDelete" @close="closeEdgeMenu" />
        </Teleport>
        <Teleport to="body">
          <ShapeMenu v-if="isAppActive && selectedShapeNodeId && selectedShapeData" :node-id="selectedShapeNodeId" :node-data="selectedShapeData" @update="handleShapeUpdate" @delete="handleShapeDelete" @close="closeShapeMenu" />
        </Teleport>
        <Teleport to="body">
          <TextMenu v-if="isAppActive && selectedTextNodeId && selectedTextData" :node-id="selectedTextNodeId" :node-data="selectedTextData" @update="handleTextUpdate" @delete="handleTextDelete" @close="closeTextMenu" />
        </Teleport>
        <Teleport to="body">
          <MultiSelectMenu v-if="isAppActive && showMultiSelectMenu" :selected-nodes="multiSelectedNodes" @group="handleMultiGroup" @ungroup="handleMultiUngroup" @delete="handleMultiDelete" @update-all="handleMultiUpdateAll" @close="closeMultiSelectMenu" />
        </Teleport>
      </template>

      <!-- Written by a newer build: shown, not opened, and never saved over -->
      <div v-else-if="store.currentBoardUnsupported.value" class="flex-1 flex items-center justify-center h-full p-6">
        <div class="text-center max-w-sm">
          <FileWarning class="w-12 h-12 mx-auto mb-3 opacity-30 text-text-secondary dark:text-text-secondary-dark" />
          <p class="text-sm font-semibold text-text dark:text-text-dark mb-1">{{ $t('whiteboard.too_new_title') }}</p>
          <p class="text-xs text-muted dark:text-muted-dark leading-relaxed">{{ $t('whiteboard.too_new_body') }}</p>
        </div>
      </div>

      <!-- No board selected -->
      <div v-else class="flex-1 flex items-center justify-center h-full">
        <div class="text-center text-muted dark:text-muted-dark">
          <PenTool class="w-12 h-12 mx-auto mb-3 opacity-20" />
          <p class="text-sm mb-3">{{ $t('whiteboard.select_to_start') }}</p>
          <button @click="store.createBoard()" class="px-4 py-2 rounded-lg bg-accent dark:bg-accent-dark text-white text-sm font-semibold hover:opacity-90 transition-opacity cursor-pointer">
            {{ $t('whiteboard.new_board') }}
          </button>
        </div>
      </div>
    </div>

    <!-- Deleting a board: where it goes, before it goes -->
    <ConfirmModal
      :show="!!pendingDeleteBoardId"
      :title="$t('whiteboard.delete_board_title', { title: pendingDeleteBoard?.title || '' })"
      :message="$t('whiteboard.delete_board_body')"
      :confirmText="$t('whiteboard.delete_board_confirm')"
      :cancelText="$t('whiteboard.delete_board_cancel')"
      isDestructive
      @confirm="confirmDeleteBoard"
      @cancel="pendingDeleteBoardId = null"
    />
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
@media (max-width: 767px) {
  :deep(.vue-flow__controls) {
    bottom: auto !important;
    top: 70px !important;
  }
}
:deep(.vue-flow__background) {
  background: transparent !important;
}
.cursor-crosshair :deep(.vue-flow__pane) {
  cursor: crosshair !important;
}
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
.wb-cursor-grab :deep(.vue-flow__pane) {
  cursor: grab !important;
}
.wb-cursor-grab:active :deep(.vue-flow__pane) {
  cursor: grabbing !important;
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
:deep(.vue-flow__node-shape) {
  pointer-events: none !important;
}
:deep(.vue-flow__node-shape .vue-flow__resize-control) {
  pointer-events: auto !important;
}
:deep(.vue-flow__resize-control.line) {
  display: none !important;
}
</style>
