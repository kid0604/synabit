<script setup lang="ts">
import { ref, computed, nextTick, onMounted, onUnmounted, watch, toRef, provide, inject, onActivated, onDeactivated, shallowRef } from 'vue';
import { VueFlow, useVueFlow, ConnectionMode } from '@vue-flow/core';
import { Background } from '@vue-flow/background';
import { Controls, ControlButton } from '@vue-flow/controls';
import { FileWarning, PenTool, Plus, Minus, Maximize } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';

// ── Existing Components ─────────────────────────────────────
import EdgeMenu from './components/EdgeMenu.vue';
import ShapeMenu from './components/ShapeMenu.vue';
import TextMenu from './components/TextMenu.vue';
import MultiSelectMenu from './components/MultiSelectMenu.vue';
import ContextMenu, { type ContextMenuItem } from './components/ContextMenu.vue';
import EdgeMarkerDefs from './components/EdgeMarkerDefs.vue';
import { isMarker } from './edgeMarkers';
import { carryWaypoints } from './waypoints';
import { fromDrawio, fromExcalidraw, looksLikeMermaid, type Imported } from './importers';
import { renderDiagram, diagramId } from '../../shared/mermaid';
import { boardFromDiagram } from '../../shared/diagramToBoard';
import { childrenOf, descendantsOf, estimateSize, fromOutline, hiddenByCollapse, rootOf, tidyTree, toOutline, type OutlineItem } from './mindmap';
import { stampElement } from './boardFile';
import ShapeNode from './nodes/ShapeNode.vue';
import StrokeNode from './nodes/StrokeNode.vue';
import InkLayer from './components/InkLayer.vue';
import { sidesFacing, type Side } from './routing';
import BoardSearch from './components/BoardSearch.vue';
import BoardHistory from './components/BoardHistory.vue';
import MiniMap from './components/MiniMap.vue';
import PresentationBar from './components/PresentationBar.vue';
import LaserPointer from './components/LaserPointer.vue';
import { usePresentation } from './composables/usePresentation';
import { handedToCanvas, inkBox, inkInside, unionBox, withLooseInk, type Box } from './inkLayer';
import MindmapNode from './nodes/MindmapNode.vue';
import TextNode from './nodes/TextNode.vue';
import NoteCardNode from './nodes/NoteCardNode.vue';
import StickyNode from './nodes/StickyNode.vue';
import FrameNode from './nodes/FrameNode.vue';
import VaultCardNode from './nodes/VaultCardNode.vue';
import CommentNode from './nodes/CommentNode.vue';
import VaultPicker from './components/VaultPicker.vue';
import LiveFrameDialog from './components/LiveFrameDialog.vue';
import { useLiveFrames } from './composables/useLiveFrames';
import { TASK_STATUS_ORDER } from './liveLayouts';
import { useBoardAssist } from './composables/useBoardAssist';
import { CARD_SIZE, parseVaultLink } from './vaultCards';
import { routeForNodeType } from '../../shared/nodeRoutes';
import { STICKY_SIZE } from './sticky';
import ImageNode from './nodes/ImageNode.vue';
import WaypointEdge from './components/WaypointEdge.vue';
import WhiteboardToolbar from './components/WhiteboardToolbar.vue';

// ── New Extracted Components ────────────────────────────────
import WhiteboardSidebar from './components/WhiteboardSidebar.vue';
import WhiteboardTitleBar from './components/WhiteboardTitleBar.vue';
import UndoToast from '../../shared/components/UndoToast.vue';
import { useUndoableAction } from '../../composables/useUndoableAction';
import { confirmDelete } from '../../composables/useConfirmDelete';

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
import { useSmartGuides } from './composables/useSmartGuides';
import { useClipboardExport } from './composables/useClipboardExport';
import { useNodeService } from '../../composables/useNodeService';
import type { NodeType } from '../../composables/useNodeService';
import { showAppNotice } from '../../composables/useAppNotice';
import { onBeforeQuit } from '../../composables/useBeforeQuit';
import { openUrl } from '@tauri-apps/plugin-opener';
import { followLink as followWebLink } from '../../shared/syn/pane';
import { cleanLink, vaultTarget } from './itemLinks';
import LinkDialog from './components/LinkDialog.vue';
import AltTextDialog from './components/AltTextDialog.vue';
import ShortcutsDialog from './components/ShortcutsDialog.vue';
import GenerateDialog from './components/GenerateDialog.vue';
import { useArrange, parseClip, rekeyClip, type AlignMode } from './composables/useArrange';
import TemplatePicker from './components/TemplatePicker.vue';
import { TEMPLATES } from './templates';
import { EDGE_Z, EDGE_GREY } from './composables/useNodeOperations';
import { useWhiteboardKeyboard } from './composables/useWhiteboardKeyboard';

import type { WBNode, WBEdge } from './composables/useWhiteboardStore';
import type { WhiteboardData } from './boardFile';
import { SHAPES_MAP } from './shapes';
import { GLYPH_SHAPE, GLYPH_SIZE, type Glyph } from './glyph';
import { toDrawio, toExcalidraw } from './exporters';
import { itemFrom, libraryPath, listLibraries, parseLibrary, saveLibrary, trashLibrary, LIBRARY_DIR, type LibraryItem, type ShapeLibrary } from './shapeLibraries';
import { isDarkPaper, paint } from './ink';
import { useEventBus } from '../../composables/useEventBus';
import { forgetViewport, recallViewport, rememberViewport } from './viewportMemory';
import {
  assetDataUri,
  assetUrl,
  dataUrlBlob,
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
/** Open a thing from the vault in its own app — the shell routes it (App.vue). */
const emit = defineEmits<{ (e: 'open-node', id: string, route: string): void }>();

const { t, locale: i18nLocale } = useI18n();
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
  // Until none is waiting: a change made while the last one was being written
  // sets the timer again, and was lost to the swap.
  for (let i = 0; i < 5 && saveTimer; i++) await flushSave();
  await store.loadBoardData(boardId);
};

/**
 * A new board. Whatever was still waiting to be saved belongs to the board
 * being left, and goes to it first.
 */
async function createBoard(templateId = 'blank') {
  if (store.currentBoardId.value && !skipNavPush) {
    pushNavigation?.({ app: 'whiteboard', itemId: store.currentBoardId.value });
  }
  for (let i = 0; i < 5 && saveTimer; i++) await flushSave();
  const template = TEMPLATES.find((tpl) => tpl.id === templateId);
  if (!template) {
    await store.createBoard();
    return;
  }
  // Fresh ids, so two boards from one template share none.
  const start = rekeyClip(template.build((k) => t(k)), (prefix) => store.generateId(prefix));
  await store.createBoard(t(`whiteboard.templates.${template.id}.name`), start);
  // A template fills more than one screen: show all of it to begin with.
  setTimeout(() => fitAll(motion(300)), 250);
}

/** Which template choice is open, if any: for a new board, or into this one. */
const templatePicker = ref<null | { mode: 'new' | 'insert'; at?: { x: number; y: number } }>(null);

async function handleTemplatePick(id: string) {
  const picker = templatePicker.value;
  templatePicker.value = null;
  if (!picker) return;
  if (picker.mode === 'new') {
    await createBoard(id);
    return;
  }
  const template = TEMPLATES.find((tpl) => tpl.id === id);
  if (template) arrange.paste(template.build((k) => t(k)), picker.at ?? viewportCentre());
}

// ── Keep-alive tracking ─────────────────────────────────────
const isAppActive = ref(true);
// Back on screen: whatever changed the board while it was hidden — Syn, a
// project link, a sync — is taken in before the user touches it.
onActivated(() => { isAppActive.value = true; void refreshFromDisk(); });
onDeactivated(() => { isAppActive.value = false; void flushSave(); });

// ── VueFlow Core ────────────────────────────────────────────
const {
  viewport,
  screenToFlowCoordinate,
  fitBounds,
  setCenter,
  addSelectedNodes,
  removeSelectedElements,
  setViewport,
  getViewport,
  zoomIn,
  zoomOut,
  findNode,
  dimensions,
  userSelectionRect,
} = useVueFlow({ id: 'whiteboard-flow' });

/**
 * What stepped lines that find their own way go around: every canvas item's
 * box, frames and ink aside. Worked out once here and read by every such line
 * (WaypointEdge), rather than once per line — on a board with many of them,
 * every frame of a drag measured every item once for each line. Only worked
 * out at all when such a line reads it.
 */
const { getNodes: canvasItems } = useVueFlow({ id: 'whiteboard-flow' });
/**
 * True while items are being dragged. The obstacles are then held as they
 * were when the drag began, and worked out again on the drop: every frame of
 * a drag moved an item, so every line that goes around things — on the whole
 * board, not only the dragged item's — was routed again, at every frame.
 */
const holdObstacles = ref(false);
let heldObstacles: Box[] = [];
provide('wbObstacles', computed<Box[]>(() => {
  if (holdObstacles.value) return heldObstacles;
  const out: Box[] = [];
  for (const n of canvasItems.value) {
    if (n.type === 'frame' || n.type === 'stroke') continue;
    const width = n.dimensions?.width || Number(n.data?.width) || 0;
    const height = n.dimensions?.height || Number(n.data?.height) || 0;
    if (width && height) out.push({ x: n.computedPosition?.x ?? n.position.x, y: n.computedPosition?.y ?? n.position.y, width, height });
  }
  heldObstacles = out;
  return out;
}));

/**
 * The board's own background, and the ink that reads on it. The paper, not
 * the theme, decides the ink once the board has a colour of its own.
 */
const paperStyle = computed(() => {
  const paper = store.backgroundColor.value;
  if (!paper || paper === 'transparent') return {};
  const dark = isDarkPaper(paper);
  return {
    backgroundColor: paper,
    '--wb-paper': paper,
    '--wb-ink': dark ? '#e4e4e7' : '#1e1e1e',
    '--wb-edge': dark ? '#a1a1aa' : '#8b8b8b',
  };
});

/** How long a camera move takes: none for someone who asked for less motion. */
const motion = (ms: number) => (window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ? 0 : ms);

const vfNodes = ref<any[]>([]);
const vfEdges = ref<any[]>([]);

// ── Ink layer ───────────────────────────────────────────────
// `vfNodes` is every item on the board. The canvas is handed all but the loose
// ink, which the ink layer draws instead (see inkLayer.ts): a stroke becomes a
// canvas item when it is selected, and goes back when it no longer is.
const canvasNodes = computed(() => handedToCanvas(vfNodes.value));
const handedIds = computed(() => new Set(canvasNodes.value.map((n: any) => n.id)));
/** The ink the canvas is not drawing — including any it has just let go of. */
const looseInk = computed(() => vfNodes.value.filter((n: any) => n.type === 'stroke' && !handedIds.value.has(n.id) && !findNode(n.id)));
/** Selected ink kept on the ink layer (a large selection): one box around it. */
const looseSelectionBox = computed(() => unionBox(looseInk.value.filter((n: any) => n.selected).map(inkBox)));

let handBackTimer: ReturnType<typeof setTimeout> | null = null;
/**
 * The canvas reports its items — after a drag, a selection, a deletion. Its
 * list is put together with the ink it was never handed. A stroke it reports
 * that it should no longer hold (no longer selected) is let go once the
 * canvas is listening again (it ignores its own echo for a tick).
 */
function onCanvasNodes(fromCanvas: any[]) {
  vfNodes.value = withLooseInk(fromCanvas, vfNodes.value, handedIds.value);
  const keep = new Set(handedToCanvas(vfNodes.value).map((n: any) => n.id));
  if (fromCanvas.some((n) => !keep.has(n.id)) && !handBackTimer) {
    handBackTimer = setTimeout(() => {
      handBackTimer = null;
      vfNodes.value = [...vfNodes.value];
    }, 0);
  }
}

/**
 * Delete the selection when some of it is ink the canvas does not hold: the
 * canvas's Delete key reaches only its own items, so the whole selection is
 * deleted here, as one step.
 */
function deleteLooseSelection(): boolean {
  if (!vfNodes.value.some((n: any) => n.selected && n.type === 'stroke' && !findNode(n.id))) return false;
  arrange.removeSelection();
  return true;
}

/** Every item in the group `id` is in, itself included; just itself when ungrouped. */
function groupOf(id: string): string[] {
  const group = vfNodes.value.find((n: any) => n.id === id)?.data?.groupId;
  if (!group) return [id];
  return vfNodes.value.filter((n: any) => n.data?.groupId === group).map((n: any) => n.id);
}

/**
 * Select some items, keeping the selection when `add` (a shift-click).
 *
 * Only the selection changes, so only the selection is told: the canvas's own
 * items through the canvas, loose ink by its flag (and handed over, or kept on
 * the ink layer, as `handedToCanvas` decides). This used to redraw the whole
 * board — every item rebuilt — for a click, and for every letter typed into
 * the search box, which selects each match it finds.
 */
function selectItems(ids: string[], add: boolean) {
  const want = new Set(add ? [...arrange.selectedIds(), ...ids] : ids);
  const onCanvas = [...want].map((id) => findNode(id)).filter((n): n is NonNullable<typeof n> => !!n);
  if (onCanvas.length) addSelectedNodes(onCanvas);
  else removeSelectedElements();
  // Every entry says so at once, so whatever reads the selection next — the
  // menu a right-click opens straight after — reads this one.
  let inkChanged = false;
  for (const n of vfNodes.value) {
    const on = want.has(n.id);
    if (!!n.selected === on) continue;
    n.selected = on;
    if (n.type === 'stroke' && !findNode(n.id)) inkChanged = true;
  }
  // After the canvas has taken its own change in (it ignores the list for a
  // tick after it reports one), so the ink it is handed is not lost.
  if (inkChanged) setTimeout(() => { vfNodes.value = [...vfNodes.value]; }, 0);
}

/**
 * A press on loose ink: select it, and move it if the pointer moves. The
 * stroke is moved on the ink layer and only then made a canvas item — the
 * canvas cannot take over a drag that started before the item existed.
 */
function pressInk(id: string, e: PointerEvent) {
  if (store.activeTool.value !== 'select' || e.button !== 0) return;
  e.stopPropagation();
  const entry = vfNodes.value.find((n: any) => n.id === id);
  if (!entry) return;
  const add = e.shiftKey || e.metaKey || e.ctrlKey;
  const start = { x: e.clientX, y: e.clientY };
  const group = groupOf(id);
  // Pressed on ink already selected: the whole selection moves and stays
  // selected. Otherwise the stroke, and the rest of its group.
  const keepSelection = !!entry.selected && !add;
  const moving = new Set(keepSelection ? vfNodes.value.filter((n: any) => n.selected).map((n: any) => n.id) : []);
  for (const g of group) moving.add(g);
  // Canvas items and loose ink alike — minus anything locked.
  const carried = vfNodes.value
    .filter((n: any) => moving.has(n.id) && !n.data?.locked)
    .map((n: any) => ({ entry: n, from: { ...n.position } }));
  let moved = false;
  const move = (ev: PointerEvent) => {
    if (!carried.length) return;
    const dx = (ev.clientX - start.x) / viewport.value.zoom;
    const dy = (ev.clientY - start.y) / viewport.value.zoom;
    if (!moved && Math.hypot(ev.clientX - start.x, ev.clientY - start.y) < 3) return;
    if (!moved) { moved = true; store.pushUndoState(); }
    for (const c of carried) c.entry.position = { x: c.from.x + dx, y: c.from.y + dy };
  };
  const up = () => {
    window.removeEventListener('pointermove', move);
    window.removeEventListener('pointerup', up);
    window.removeEventListener('pointercancel', up);
    if (moved && store.currentBoardData.value) {
      // Snapped as one: the same nudge for every item, so the group keeps its shape.
      const snap = (v: number) => (store.snapToGrid.value ? Math.round(v / 10) * 10 : v);
      const first = carried[0];
      const nudge = { x: snap(first.entry.position.x) - first.entry.position.x, y: snap(first.entry.position.y) - first.entry.position.y };
      const moves = new Map<string, { dx: number; dy: number }>();
      for (const c of carried) {
        const wb = store.currentBoardData.value.nodes.find((n: WBNode) => n.id === c.entry.id);
        if (!wb) continue;
        wb.position = { x: c.entry.position.x + nudge.x, y: c.entry.position.y + nudge.y };
        moves.set(wb.id, { dx: wb.position.x - c.from.x, dy: wb.position.y - c.from.y });
        store.stampNode(wb.id);
      }
      carryWaypoints(store.currentBoardData.value.edges, moves);
      scheduleSave();
    }
    const chosen = keepSelection ? [...moving] : group;
    // Moved: the board changed, and the canvas is drawn again from it. Not
    // moved: a click, and only the selection changes.
    if (moved) syncToVueFlow(add ? [...arrange.selectedIds(), ...chosen] : chosen);
    else selectItems(chosen, add);
  };
  window.addEventListener('pointermove', move);
  window.addEventListener('pointerup', up);
  window.addEventListener('pointercancel', up);
}

function inkMenu(id: string, e: MouseEvent) {
  if (store.activeTool.value !== 'select') return;
  if (!vfNodes.value.find((n: any) => n.id === id)?.selected) selectItems([id], false);
  openContextMenu(e, true);
}

/** Where the ink layer goes: inside the canvas's moving layer, once it exists. */
const inkTarget = ref<HTMLElement | null>(null);
function handlePaneReady() {
  inkTarget.value = flowEl()?.querySelector<HTMLElement>('.vue-flow__transformationpane') ?? null;
}

/** The marquee being dragged, in board coordinates. */
const marquee = computed<Box | null>(() => {
  const r = userSelectionRect.value;
  if (!r || !r.width || !r.height) return null;
  const { x, y, zoom } = viewport.value;
  return { x: (r.x - x) / zoom, y: (r.y - y) / zoom, width: r.width / zoom, height: r.height / zoom };
});
let lastMarquee: Box | null = null;
watch(marquee, (m) => { if (m) lastMarquee = m; });
/** The marquee was let go: the loose ink wholly inside it joins what it selected. */
function handleSelectionEnd() {
  const area = lastMarquee;
  lastMarquee = null;
  if (!area) return;
  const ids = inkInside(looseInk.value, area).map((n: any) => n.id);
  if (ids.length) selectItems(ids, true);
}

/**
 * Fit the whole board in view. The canvas's own fit sees only what it was
 * handed and has measured — not the ink layer, and not items it has never
 * built because they were off screen — so the box is worked out here.
 */
/** Every item's box, as drawn where the canvas measured it, else by what it carries. */
function itemBoxes(withComments = true): Box[] {
  return vfNodes.value
    .filter((n: any) => !n.hidden && (withComments || n.type !== 'comment'))
    .map((n: any) => (n.type === 'stroke' ? inkBox(n) : {
      x: n.position.x,
      y: n.position.y,
      width: findNode(n.id)?.dimensions?.width || n.data?.width || 160,
      height: findNode(n.id)?.dimensions?.height || n.data?.height || 80,
    }));
}
function boardBox(): Box | null {
  return unionBox(itemBoxes());
}
/**
 * The minimap's boxes, worked out at most ten times a second. The list of
 * items changes on every frame of a drag, and measuring every item each time
 * was work the minimap, a picture 200 pixels wide, has no use for.
 */
const minimapBoxes = shallowRef<Box[]>([]);
let minimapTimer: ReturnType<typeof setTimeout> | null = null;
watch([vfNodes, () => store.showMinimap.value], () => {
  if (!store.showMinimap.value) { minimapBoxes.value = []; return; }
  if (minimapTimer) return;
  minimapTimer = setTimeout(() => { minimapTimer = null; minimapBoxes.value = itemBoxes(); }, 100);
}, { immediate: true });
onUnmounted(() => { if (minimapTimer) clearTimeout(minimapTimer); });

// ── A diagram from a description ────────────────────────────
const generateDialog = ref(false);
/**
 * What Syn is asked to draw, with a vault item's text as the material when
 * one was picked: a note's body, a task's or project's notes, under its title.
 */
async function generateDiagram(ask: { request: string; useSelection: boolean; source?: { id: string; title: string } }) {
  generateDialog.value = false;
  let source: string | undefined;
  if (ask.source) {
    try {
      const n = await nodeService.getNode(ask.source.id);
      source = n ? `# ${n.title ?? ask.source.title}\n\n${n.content ?? ''}` : undefined;
    } catch (err) {
      logger.error('Could not read the note for the diagram', err as string);
      showAppNotice(t('whiteboard.generate.source_failed'), 'error');
      return;
    }
  }
  void assist.run('generate', { request: ask.request, useSelection: ask.useSelection, source });
}

// ── Links ───────────────────────────────────────────────────
const linkDialog = ref<{ id: string; link?: string } | null>(null);
/** A picture's words being written (AltTextDialog). */
const altDialog = ref<{ id: string; alt?: string } | null>(null);
/** The list of keys the board answers to (ShortcutsDialog). */
const showShortcuts = ref(false);
function setAlt(id: string, alt: string) {
  store.updateNodeData(id, { alt });
  altDialog.value = null;
  syncToVueFlow([id]);
  scheduleSave();
}
const selectedItem = (): WBNode | undefined => {
  const id = arrange.selectedIds()[0];
  return store.currentBoardData.value?.nodes.find((n: WBNode) => n.id === id);
};
function setLink(id: string, link: string | null) {
  store.updateNodeData(id, { link: link ?? undefined });
  linkDialog.value = null;
  syncToVueFlow([id]);
  scheduleSave();
}
/** A link followed: into the vault in the app, out to the web by the system. */
function followLink(link: string) {
  const inVault = vaultTarget(link);
  if (inVault) { openInApp(inVault.id, inVault.kind); return; }
  if (cleanLink(link) !== link) return;
  // A web page goes the way every link in the app goes — the same checks
  // (no addresses on this computer or its network, which a board from
  // elsewhere could point at) and the same choice of where it opens.
  if (/^https?:/i.test(link)) void followWebLink(link);
  else openUrl(link).catch((e) => logger.error('Could not open that link', e));
}
// Links written in a text box take the same way as an item's link.
provide('wbFollowLink', followLink);

// ── Copy as a picture ───────────────────────────────────────
/**
 * The selection, on the clipboard as a picture. Offered only where the
 * webview can put a picture there; elsewhere the menu simply has no such
 * line, and export is the way, as before.
 */
const canCopyImage = typeof window !== 'undefined' && 'ClipboardItem' in window && !!navigator.clipboard?.write;
function copySelectionAsImage() {
  const ids = arrange.selectedIds();
  if (!ids.length) return;
  // Handed the promise at once, inside the click: a clipboard write that
  // waits for the picture first is refused as no longer the user's doing.
  const png = exportBoard({ format: 'png', only: ids, keep: true }).then(async (url) => {
    if (!url) throw new Error('nothing to copy');
    return dataUrlBlob(url);
  });
  navigator.clipboard
    .write([new ClipboardItem({ 'image/png': png })])
    .then(() => showAppNotice(t('whiteboard.copied_image'), 'info'))
    .catch((err) => {
      logger.error('Could not copy the picture', err as string);
      showAppNotice(t('whiteboard.fail.copy_image'), 'error');
    });
}

// ── Comments ────────────────────────────────────────────────
/** A comment, at a point or beside an item (and about it), ready to type into. */
function addComment(at: { x: number; y: number } | null, on?: string) {
  let position = at ?? viewportCentre();
  if (on) {
    const i = vfNodes.value.filter((n: any) => !n.hidden).findIndex((n: any) => n.id === on);
    const box = itemBoxes()[i];
    if (box) position = { x: box.x + box.width + 16, y: box.y - 8 };
  }
  const id = store.generateId('comment');
  addNodeToCanvas({ id, type: 'comment', position, data: { label: '', editing: true, at: Date.now(), ...(on ? { on } : {}) } });
  syncToVueFlow([id]);
}

// ── Versions ────────────────────────────────────────────────
const showHistory = ref(false);
/**
 * Put an earlier version back. What is on the board now is written first, so
 * that the save putting the old version back finds it on disk and keeps it
 * as a version of its own (`keep_version`, forced): restoring is never the
 * end of the work it replaces.
 */
async function restoreVersion(version: WhiteboardData) {
  showHistory.value = false;
  await flushSave();
  store.restoreVersion(version);
  syncToVueFlow([]);
  await store.saveCurrentBoard();
  showAppNotice(t('whiteboard.history.restored'), 'info');
}

// ── Find on the board ───────────────────────────────────────
const showSearch = ref(false);
const hiddenItems = computed(() => new Set(vfNodes.value.filter((n: any) => n.hidden).map((n: any) => n.id)));
/** A match: selected, and brought to the middle of the view. */
function showFound(id: string) {
  const box = itemBoxes()[vfNodes.value.filter((n: any) => !n.hidden).findIndex((n: any) => n.id === id)];
  selectItems([id], false);
  if (box) void setCenter(box.x + box.width / 2, box.y + box.height / 2, { zoom: Math.max(viewport.value.zoom, 0.8), duration: motion(250) });
}
function fitAll(duration = 0) {
  const box = boardBox();
  if (box) void fitBounds(box, { padding: 0.1, duration });
}

// ── Presenting ──────────────────────────────────────────────
const presentation = usePresentation({
  nodes: () => store.currentBoardData.value?.nodes ?? [],
  boardTitle: () => store.currentBoardData.value?.title ?? '',
  // Comments are not shown when presenting, so the view is not made to
  // leave room for them.
  boardBox: () => unionBox(itemBoxes(false)),
  fitBounds,
  getViewport,
  setViewport,
  size: dimensions,
  // Nothing selected and no panel open on a slide: every panel floats above
  // the stage, and each still edits its item.
  deselect: () => {
    syncToVueFlow([]);
    contextMenu.value = null;
    cancelPanel();
    closeEdgeMenu();
    closeShapeMenu();
    closeTextMenu();
    closeMultiSelectMenu();
  },
});
const presenting = presentation.active;
// Leaving the app, or the board, ends the show.
onDeactivated(() => presentation.stop());
watch(() => store.currentBoardId.value, () => presentation.stop());

const canvasRef = ref<HTMLElement | null>(null);
const vueFlowRef = ref<HTMLElement | null>(null);

/**
 * This board's canvas element.
 *
 * Looked up inside this app rather than across the document: a note's board
 * preview, the board pane beside a chat and the Nexus graph are all canvases
 * of the same kind, and the first one in the document is not always this one.
 */
const flowEl = () => canvasRef.value?.querySelector<HTMLElement>('.vue-flow') ?? null;

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
let stopQuitSave: (() => void) | null = null;
async function flushSave() {
  if (!saveTimer) return;
  clearTimeout(saveTimer);
  saveTimer = null;
  await store.saveCurrentBoard();
}

/**
 * Take in what someone else wrote to the open board's file.
 *
 * Skipped while a change of ours is waiting to be written: that save reads
 * the file and merges first, and redrawing the canvas under a drag that has
 * not finished would only get in the way of it.
 */
async function refreshFromDisk() {
  // A save waiting to run reads the file and merges before it writes: what
  // arrived is taken in then.
  if (saveTimer) return;
  // Typing: asked again once the field is left, rather than never — a pull
  // or Syn's edit that arrived mid-sentence used to wait for the next change
  // to anything in the vault.
  if (isEditingOnCanvas()) {
    if (!diskCheckWaiting) {
      diskCheckWaiting = true;
      document.addEventListener('focusout', () => setTimeout(() => {
        diskCheckWaiting = false;
        void refreshFromDisk();
      }, 0), { once: true });
    }
    return;
  }
  await store.syncWithDisk();
}
let diskCheckWaiting = false;

/**
 * Whether the user is typing into something on the board — a label, a text
 * box. A redraw then would pull the field out from under the caret; the save
 * that follows the edit merges anyway.
 */
function isEditingOnCanvas(): boolean {
  const el = document.activeElement as HTMLElement | null;
  if (!el || !canvasRef.value?.contains(el)) return false;
  return el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.isContentEditable;
}

function handleVisibilityChange() {
  if (document.hidden) void flushSave();
}

// ── Composable Wiring ───────────────────────────────────────
/** Each list of items by id, made once per list: lines look up their ends in it. */
const nodeIndex = new WeakMap<WBNode[], Map<string, WBNode>>();
const { applySize, applyState, computeShapeZIndex, deleteNodes, updateNodeData, buildVfEdge } =
  useNodeOperations(store, vfNodes, vfEdges, scheduleSave, (edge, nodes) => {
    let byId = nodeIndex.get(nodes);
    if (!byId) nodeIndex.set(nodes, (byId = new Map(nodes.map((n) => [n.id, n]))));
    const end = (id: string) => { const n = byId!.get(id); return n ? spokenName(n) : ''; };
    const words = String(edge.data?.label ?? '').trim();
    return [t('whiteboard.a11y.edge', { from: end(edge.source), to: end(edge.target) }), words].filter(Boolean).join(', ');
  });

const arrange = useArrange({ store, vfNodes, refresh: (select) => syncToVueFlow(select), scheduleSave, moved: showMoved });

/**
 * Items moved by a nudge, shown without redrawing the board: each moved
 * item's place is set where it is drawn, and lines are rebuilt only when a
 * bend went with the items. A held arrow key used to rebuild every item and
 * every line at the key's repeat rate.
 */
function showMoved(ids: string[]) {
  const board = store.currentBoardData.value;
  if (!board) return;
  const byId = new Map(board.nodes.map((n: WBNode) => [n.id, n]));
  let ink = false;
  for (const id of ids) {
    const stored = byId.get(id);
    if (!stored) continue;
    const entry = vfNodes.value.find((n: any) => n.id === id);
    if (entry) entry.position = { ...stored.position };
    const onCanvas = findNode(id);
    if (onCanvas) onCanvas.position = { ...stored.position };
    else ink = true;
  }
  const moving = new Set(ids);
  if (board.edges.some((e: WBEdge) => e.data?.waypoints?.length && moving.has(e.source) && moving.has(e.target))) {
    vfEdges.value = board.edges.map((e: WBEdge) => buildVfEdge(e, board.nodes));
  }
  // Ink the canvas does not hold is drawn from the list: handed a new one.
  if (ink) vfNodes.value = [...vfNodes.value];
}

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
  handleMultiUpdateAll,
  closeMultiSelectMenu,
} = useMultiSelect(store, vfNodes, vfEdges, deleteNodes, updateNodeData, scheduleSave);

const { isErasing, eraserPos, eraseStrokesNear, endWipe } =
  useEraser(store, vfNodes, viewport, scheduleSave, flowEl);

const { handleNodeDragStart: mindmapDragStart, handleNodeDrag: mindmapDrag, handleNodeDragStop: mindmapDragStop } =
  useMindmapDrag(store, vfNodes, vfEdges, scheduleSave);

/** What a screen reader says for an item: its kind, its words, whether it is locked. */
function spokenName(n: WBNode): string {
  const kind = t(`whiteboard.a11y.kind.${n.type}`);
  const words = String(n.data?.label ?? n.data?.title ?? n.data?.noteTitle ?? (n.type === 'image' ? n.data?.alt : '') ?? '').replace(/[#*_`>-]/g, ' ').replace(/\s+/g, ' ').trim().slice(0, 120);
  // An icon without words is named by what it is a picture of.
  const icon = !words && n.data?.shapeType === GLYPH_SHAPE ? String(n.data?.glyph?.name ?? '').replace(/-/g, ' ') : '';
  return [kind, words || icon, n.data?.locked ? t('whiteboard.a11y.locked') : ''].filter(Boolean).join(', ');
}

// ── VueFlow Sync ────────────────────────────────────────────
function syncToVueFlow(select?: Iterable<string>) {
  if (!store.currentBoardData.value) {
    vfNodes.value = [];
    vfEdges.value = [];
    return;
  }
  // A redraw — after an undo, or a change merged in from another copy — keeps
  // what was selected, so the panel open for it still has something to edit.
  // An operation that made new items (a paste) selects those instead.
  const selected = new Set(select ?? vfNodes.value.filter((n: any) => n.selected).map((n: any) => n.id));
  // What folded mind-map branches hide, and each mind-map item's children.
  const hidden = hiddenByCollapse(store.currentBoardData.value.nodes, store.currentBoardData.value.edges);
  const children = childrenOf(store.currentBoardData.value.nodes, store.currentBoardData.value.edges);
  vfNodes.value = store.currentBoardData.value.nodes.map((n: WBNode) => {
    const node: any = {
      id: n.id, type: n.type,
      position: { ...n.position },
      data: { ...n.data },
      draggable: true,
      selected: selected.has(n.id),
    };
    applySize(node, n.data.width, n.data.height);
    node.ariaLabel = spokenName(n);
    if (hidden.has(n.id)) node.hidden = true;
    if (n.type === 'mindmap') {
      // For the fold button: drawn from these, never saved.
      const kids = children.get(n.id) ?? [];
      node.data._childCount = kids.length;
      if (n.data?.collapsed) node.data._hiddenCount = descendantsOf(n.id, children).length;
    }
    return node;
  });
  vfEdges.value = store.currentBoardData.value.edges.map((e: WBEdge) => {
    const edge = buildVfEdge(e, store.currentBoardData.value!.nodes);
    if (hidden.has(e.source) || hidden.has(e.target)) edge.hidden = true;
    return edge;
  });
}

watch(() => store.currentBoardId.value, (boardId) => {
  syncToVueFlow();
  // Live frames ask their questions again whenever their board opens.
  void live.refreshAll();

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
    setTimeout(() => fitAll(motion(500)), 150);
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

// A panel belongs to one selected item. When the selection moves on — to a
// frame just made, to everything — the panel for the item it left closes.
watch(
  () => vfNodes.value.filter((n: any) => n.selected).map((n: any) => n.id).join(','),
  (ids) => {
    const selected = new Set(ids ? ids.split(',') : []);
    if (selectedShapeNodeId.value && !selected.has(selectedShapeNodeId.value)) selectedShapeNodeId.value = null;
    if (selectedTextNodeId.value && !selected.has(selectedTextNodeId.value)) selectedTextNodeId.value = null;
  },
);

// The open board turned out to be a newer build's, and what was unsaved here
// went into a board of its own: say where.
watch(() => store.keptAside.value, (title) => {
  if (title) showAppNotice(t('whiteboard.kept_aside', { title }), 'info');
});

// The board was merged with a copy someone else wrote: draw what it is now.
watch(() => store.externalRevision.value, () => syncToVueFlow());

// ── VueFlow Change Handlers ─────────────────────────────────
/**
 * The step to come back to before a deletion the canvas made itself.
 *
 * Delete and Backspace are the canvas's own keys. It reports the nodes it
 * removed and, separately, the edges that went with them, so both reports
 * share a key that lasts until the end of the task: one key press, one step
 * back. These used to change the board without recording anything, so the
 * one deletion that could not be undone was the commonest one.
 */
let deletionKey: string | null = null;
function recordDeletion() {
  if (!deletionKey) {
    deletionKey = store.generateId('delete');
    setTimeout(() => { deletionKey = null; });
  }
  store.pushUndoState(deletionKey);
}

function handleNodesChange(changes: any[]) {
  if (!store.currentBoardData.value) return;
  let dirty = false;
  for (const change of changes) {
    if (change.type === 'position' && change.position) {
      const wbNode = store.currentBoardData.value.nodes.find((n: WBNode) => n.id === change.id);
      // A move that is not a drag is an arrow key on a focused item: one step
      // back for a run of presses, as for the selection's own nudge.
      if (wbNode && change.dragging !== true) store.pushUndoState('nudge');
      if (wbNode) {
        wbNode.position = { x: change.position.x, y: change.position.y };
        store.stampNode(change.id);
        dirty = true;
      }
    } else if (change.type === 'remove') {
      recordDeletion();
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
      recordDeletion();
      store.currentBoardData.value.edges = store.currentBoardData.value.edges.filter((e: WBEdge) => e.id !== change.id);
      dirty = true;
    }
  }
  if (dirty) scheduleSave();
}

function handleConnect(params: any) {
  const edge: any = {
    id: store.generateId('e'), source: params.source, sourceHandle: params.sourceHandle,
    // A new line picks the sides that face each other, and keeps doing so as
    // its ends move (see routing.ts); the panel can pin it to these handles.
    target: params.target, targetHandle: params.targetHandle, type: 'default', data: { sides: 'auto' }, zIndex: EDGE_Z,
  };
  store.addEdge(edge);
  // Built the way every other line is: with its name for a screen reader,
  // not the canvas's own English "Edge from <id> to <id>".
  vfEdges.value = [...vfEdges.value, buildVfEdge(edge, store.currentBoardData.value?.nodes ?? [])];
  scheduleSave();
}

/** Items a line can be drawn to: what has handles for it. */
const CONNECTABLE = new Set(['shape', 'sticky', 'card', 'frame', 'image', 'note', 'mindmap']);

/**
 * The handle a line is attached by, on an item of this kind, on this side.
 * Lines drawn from the keyboard choose their sides as they go (`sides:
 * 'auto'`), so this is only where they start out.
 */
function handleFor(type: string, side: Side, role: 'source' | 'target'): string | undefined {
  if (type === 'mindmap') return `${side === 'left' || side === 'right' ? side : role === 'source' ? 'right' : 'left'}-${role}`;
  if (type === 'shape' || type === 'sticky' || type === 'card' || type === 'frame') return side;
  return undefined;
}

/**
 * Join the two selected items with a line — the keyboard's way to do what
 * dragging from a handle does. It runs in reading order, from the one higher
 * up (or further left) to the other, and takes the sides that face each other.
 */
function connectSelection() {
  const board = store.currentBoardData.value;
  if (!board) return;
  const picked = arrange.selectedIds()
    .map((id) => board.nodes.find((n: WBNode) => n.id === id))
    .filter((n): n is WBNode => !!n && CONNECTABLE.has(n.type));
  if (picked.length !== 2) return;
  const [a, b] = picked.sort((p, q) => (Math.abs(p.position.y - q.position.y) > 40 ? p.position.y - q.position.y : p.position.x - q.position.x));
  if (board.edges.some((e: WBEdge) => (e.source === a.id && e.target === b.id) || (e.source === b.id && e.target === a.id))) {
    showAppNotice(t('whiteboard.connect.already'), 'info');
    return;
  }
  const box = (n: WBNode) => ({
    x: n.position.x, y: n.position.y,
    width: findNode(n.id)?.dimensions?.width || Number(n.data?.width) || 160,
    height: findNode(n.id)?.dimensions?.height || Number(n.data?.height) || 80,
  });
  const { from, to } = sidesFacing(box(a), box(b));
  store.pushUndoState();
  handleConnect({
    source: a.id, sourceHandle: handleFor(a.type, from, 'source'),
    target: b.id, targetHandle: handleFor(b.type, to, 'target'),
  });
  showAppNotice(t('whiteboard.connect.done'), 'info');
}

// ── Canvas Node Creation ────────────────────────────────────
function addNodeToCanvas(node: WBNode) {
  store.addNode(node);
  const vfNode: any = { ...node, position: { ...node.position }, data: { ...node.data }, draggable: true, ariaLabel: spokenName(node) };
  applySize(vfNode, node.data.width, node.data.height);
  vfNodes.value = [...vfNodes.value, vfNode];
  scheduleSave();
}

const PLACING_TOOLS = new Set(['shape', 'text', 'sticky', 'frame', 'mindmap']);
/** What a click on the board would put down, put down in the middle of the view. */
function placeWithTool(): boolean {
  if (!PLACING_TOOLS.has(store.activeTool.value)) return false;
  const rect = flowEl()?.getBoundingClientRect();
  if (!rect) return false;
  handlePaneClick({ clientX: rect.left + rect.width / 2, clientY: rect.top + rect.height / 2 });
  return true;
}

function handlePaneClick(event: any) {
  cancelPanel();
  selectedEdgeId.value = null;
  selectedShapeNodeId.value = null;
  selectedTextNodeId.value = null;

  const pos = screenToFlowCoordinate({ x: event.clientX, y: event.clientY });

  if (store.activeTool.value === 'shape' && store.activeShapeType.value === GLYPH_SHAPE && activeGlyph.value) {
    // An icon, centred where the click was, its words under it.
    addNodeToCanvas({
      id: store.generateId('shape'), type: 'shape',
      position: { x: pos.x - GLYPH_SIZE / 2, y: pos.y - GLYPH_SIZE / 2 },
      data: { shapeType: GLYPH_SHAPE, glyph: activeGlyph.value, label: '', color: store.activeColor.value, width: GLYPH_SIZE, height: GLYPH_SIZE },
    });
    store.activeTool.value = 'select';
  } else if (store.activeTool.value === 'shape') {
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
    addNodeToCanvas({ id: store.generateId('text'), type: 'text', position: pos, data: { label: '', editing: true } });
    store.activeTool.value = 'select';
  } else if (store.activeTool.value === 'sticky') {
    // Centred where the click was, in the colour of the last one placed, and
    // ready to type into.
    const id = store.generateId('sticky');
    addNodeToCanvas({
      id, type: 'sticky',
      position: { x: pos.x - STICKY_SIZE / 2, y: pos.y - STICKY_SIZE / 2 },
      data: { label: '', color: lastStickyColor(), width: STICKY_SIZE, height: STICKY_SIZE, editing: true },
    });
    store.activeTool.value = 'select';
    focusMindmapNode(id);
  } else if (store.activeTool.value === 'frame') {
    addNodeToCanvas({
      id: store.generateId('frame'), type: 'frame',
      position: { x: pos.x - 240, y: pos.y - 160 },
      data: { label: '', width: 480, height: 320 },
    });
    store.activeTool.value = 'select';
  } else if (store.activeTool.value === 'mindmap') {
    addNodeToCanvas({
      id: store.generateId('mind'), type: 'mindmap', position: pos,
      data: { label: t('whiteboard.central_idea'), color: store.getMindmapColor(0), level: 0, editing: true },
    });
    store.activeTool.value = 'select';
  }
}

/** The paper colour of the most recent sticky note, for the next one. */
function lastStickyColor(): string {
  const stickies = (store.currentBoardData.value?.nodes ?? []).filter((n: WBNode) => n.type === 'sticky');
  return stickies.length ? stickies[stickies.length - 1].data.color ?? 'yellow' : 'yellow';
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
  return viewportCentre();
}

/** Board coordinates of the middle of what is on screen. */
// ── Icons and shape libraries ───────────────────────────────

/** The icon the shape tool puts down next, chosen in the picker. */
const activeGlyph = shallowRef<Glyph | null>(null);
function pickIcon(glyph: Glyph) {
  activeGlyph.value = glyph;
  store.activeShapeType.value = GLYPH_SHAPE;
  store.activeTool.value = 'shape';
}

const libraries = ref<ShapeLibrary[]>([]);
async function loadLibraries() {
  if (!props.vaultPath) { libraries.value = []; return; }
  try {
    libraries.value = await listLibraries(props.vaultPath);
  } catch (err) {
    logger.error('Could not read the shape libraries', err as string);
  }
}

/** A library piece, put down in the middle of the view: copies of its items, or its picture. */
async function placeLibraryItem(item: LibraryItem) {
  // Put down at once, so the shape tool the picker was opened with is done.
  store.activeTool.value = 'select';
  const at = viewportCentre();
  if (item.image) {
    let blob: Blob;
    try {
      blob = dataUrlBlob(item.image.dataUri);
    } catch (err) {
      logger.error('A library picture could not be read', err as string);
      showAppNotice(t('whiteboard.fail.images', { count: 1 }, 1), 'error');
      return;
    }
    const ext = blob.type.split('/')[1]?.replace('svg+xml', 'svg').replace('jpeg', 'jpg') || 'png';
    await addImageFiles([new File([blob], `${item.title || 'library'}.${ext}`, { type: blob.type })], at);
    return;
  }
  const w = Math.max(...item.nodes.map((n) => n.position.x + (Number(n.data?.width) || 160)));
  const h = Math.max(...item.nodes.map((n) => n.position.y + (Number(n.data?.height) || 80)));
  // Copies: the library keeps its own pieces as they are.
  const piece = JSON.parse(JSON.stringify({ nodes: item.nodes, edges: item.edges }));
  arrange.paste(await settlePictures(piece), { x: at.x - w / 2, y: at.y - h / 2 });
}

/** "Import a library…": an Excalidraw or draw.io library file, kept as one of the vault's own. */
function importLibrary() {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = '.excalidrawlib,.xml,.drawio,.json';
  input.onchange = async () => {
    const file = input.files?.[0];
    if (!file || !props.vaultPath) return;
    const parsed = await parseLibrary(await file.text(), file.name).catch(() => null);
    if (!parsed) {
      showAppNotice(t('whiteboard.library.import_failed', { name: file.name }), 'error');
      return;
    }
    const library: ShapeLibrary = { path: libraryPath(parsed.name, libraries.value.map((l) => l.path)), name: parsed.name, items: parsed.items };
    try {
      await saveLibrary(props.vaultPath, library);
      await loadLibraries();
      showAppNotice(t('whiteboard.library.imported', { name: library.name, count: library.items.length }), 'info');
    } catch (err) {
      logger.error('Could not save the library', err as string);
      showAppNotice(t('whiteboard.library.save_failed'), 'error');
    }
  };
  input.click();
}

async function removeLibrary(path: string) {
  const lib = libraries.value.find((l) => l.path === path);
  if (!lib || !props.vaultPath) return;
  if (!(await confirmDelete({ name: lib.name }))) return;
  try {
    await trashLibrary(props.vaultPath, path);
    await loadLibraries();
    showAppNotice(t('whiteboard.library.removed', { name: lib.name }), 'info');
  } catch (err) {
    logger.error('Could not remove the library', err as string);
    showAppNotice(t('whiteboard.library.save_failed'), 'error');
  }
}

/** The selection, as a piece of the user's own library ("My shapes"), to put on any board later. */
async function saveSelectionToLibrary() {
  const board = store.currentBoardData.value;
  if (!board || !props.vaultPath) return;
  const ids = new Set(arrange.selectedIds());
  const nodes = board.nodes.filter((n: WBNode) => ids.has(n.id) && n.type !== 'comment');
  if (!nodes.length) return;
  const name = t('whiteboard.library.mine');
  const own = libraries.value.find((l) => l.name === name)
    ?? { path: libraryPath(name, libraries.value.map((l) => l.path)), name, items: [] };
  const title = String(nodes.find((n: WBNode) => typeof n.data?.label === 'string' && n.data.label.trim())?.data?.label ?? '').trim().slice(0, 60)
    || t('whiteboard.library.piece', { n: own.items.length + 1 });
  try {
    await saveLibrary(props.vaultPath, { ...own, items: [...own.items, itemFrom(title, nodes, board.edges)] });
    await loadLibraries();
    showAppNotice(t('whiteboard.library.added', { name, folder: LIBRARY_DIR }), 'info');
  } catch (err) {
    logger.error('Could not save to the library', err as string);
    showAppNotice(t('whiteboard.library.save_failed'), 'error');
  }
}
watch(() => props.vaultPath, () => void loadLibraries(), { immediate: true });

/**
 * The board as a file another app opens and edits — draw.io or Excalidraw —
 * so what is drawn here is never kept here only.
 */
async function exportToApp(format: 'drawio' | 'excalidraw') {
  const board = store.currentBoardData.value;
  if (!board) return;
  await flushSave();
  try {
    const input = { nodes: board.nodes, edges: board.edges, title: board.title };
    const opts = { imageData: (assetPath: string) => assetDataUri(props.vaultPath, assetPath) };
    const text = format === 'drawio' ? await toDrawio(input, opts) : await toExcalidraw(input, opts);
    const url = URL.createObjectURL(new Blob([text], { type: format === 'drawio' ? 'application/xml' : 'application/json' }));
    const link = document.createElement('a');
    link.href = url;
    link.download = `${board.title || 'whiteboard'}.${format}`;
    link.click();
    setTimeout(() => URL.revokeObjectURL(url), 10_000);
  } catch (err) {
    logger.error('Could not export for another app', err as string);
    showAppNotice(t('whiteboard.export_other.failed'), 'error');
  }
}

function viewportCentre(): { x: number; y: number } {
  const rect = flowEl()?.getBoundingClientRect();
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
  let failed = 0;
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
      failed++;
    }
  }
  if (failed) showAppNotice(t('whiteboard.fail.images', { count: failed }, failed), 'error');
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
      filters: [{ name: t('whiteboard.images'), extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'avif', 'bmp'] }],
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
    showAppNotice(t('whiteboard.fail.images', { count: 1 }, 1), 'error');
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
  // A board being shown is not a board being edited.
  if (!isAppActive.value || presenting.value) return;
  const target = event.target as HTMLElement | null;
  if (target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable)) {
    return;
  }

  const files = Array.from(event.clipboardData?.items ?? [])
    .filter((item) => item.kind === 'file' && item.type.startsWith('image/'))
    .map((item) => item.getAsFile())
    .filter((file): file is File => !!file);

  if (files.length) {
    event.preventDefault();
    void addImageFiles(files, placementPoint());
    return;
  }

  const text = event.clipboardData?.getData('text/plain') ?? '';
  const clip = parseClip(text);
  if (clip) {
    event.preventDefault();
    arrange.paste(clip, placementPoint());
    return;
  }
  // A drawing in another tool's words: Mermaid source, a draw.io diagram,
  // Excalidraw's clipboard.
  if (looksLikeMermaid(text) || /^\s*<mxfile|^\s*<mxGraphModel/.test(text) || /^\s*\{[\s\S]*"type"\s*:\s*"excalidraw/.test(text)) {
    event.preventDefault();
    void importDrawing(text, '', placementPoint());
    return;
  }

  // An indented list — an outline from a note, a list from anywhere — becomes
  // a mind map.
  const outline = fromOutline(text);
  if (outline) {
    event.preventDefault();
    addMindmapFromOutline(outline, placementPoint());
    return;
  }

  // A link to something in the vault — copied from a note, from Tasks —
  // becomes its card.
  const link = parseVaultLink(text);
  if (link) {
    event.preventDefault();
    void addVaultCard(link.id, link.kind, '', placementPoint());
    return;
  }

  // Words from anywhere else become a text box where the pointer is.
  if (text.trim()) {
    event.preventDefault();
    addNodeToCanvas({
      id: store.generateId('text'), type: 'text', position: placementPoint(),
      data: { label: text.trim().slice(0, 5000), width: 240 },
    });
  }
}

/** Whether a copy or cut is the board's to handle rather than a text field's. */
function ownsClipboardEvent(event: ClipboardEvent): boolean {
  if (!isAppActive.value || presenting.value) return false;
  const target = event.target as HTMLElement | null;
  if (target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable)) return false;
  // Text selected on the page — a note card's preview — copies as text.
  if (window.getSelection()?.toString()) return false;
  return arrange.selectedIds().length > 0;
}

/**
 * Copy the selection onto the system clipboard, edges among it included, so
 * it pastes into this board, another board, or this board after a restart.
 * Through the `copy` event rather than the clipboard API: inside the event
 * the page may always write, without asking for permission.
 */
function handleCopy(event: ClipboardEvent) {
  if (!ownsClipboardEvent(event)) return;
  const clip = arrange.clipOf();
  if (!clip || !event.clipboardData) return;
  event.clipboardData.setData('text/plain', JSON.stringify(clip));
  event.preventDefault();
}

function handleCut(event: ClipboardEvent) {
  if (!ownsClipboardEvent(event)) return;
  handleCopy(event);
  if (event.defaultPrevented) arrange.removeSelection();
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

  // Drawings from other tools: a draw.io, Excalidraw or Mermaid file.
  const drawings = dropped.filter((f) => /\.(drawio|excalidraw|mmd|mermaid|xml|json)$/i.test(f.name));
  if (drawings.length) {
    const at = screenToFlowCoordinate({ x: event.clientX, y: event.clientY });
    void (async () => {
      for (const file of drawings) await importDrawing(await file.text(), file.name, at);
    })();
    return;
  }

  const noteId = event.dataTransfer.getData('application/synabit-note-id');
  const noteTitle = event.dataTransfer.getData('application/synabit-note-title');
  const blockId = event.dataTransfer.getData('application/synabit-block-id');
  if (noteId) {
    addNoteCard(noteId, noteTitle, blockId || undefined, screenToFlowCoordinate({ x: event.clientX, y: event.clientY }));
  }
}

function addNoteCard(noteId: string, noteTitle: string, blockId: string | undefined, pos: { x: number; y: number }) {
  addNodeToCanvas({
    id: store.generateId('note'), type: 'note', position: pos,
    data: { noteId, noteTitle, blockId, width: 280, height: 180 },
  });
}

/**
 * The sidebar's "Add to board", for anyone who cannot drag: a touch screen,
 * a keyboard. The card lands in the middle of the view, centred on it rather
 * than hanging off its top-left corner.
 */
function addNoteFromSidebar(note: { id: string; title?: string }) {
  const centre = viewportCentre();
  addNoteCard(note.id, note.title || '', undefined, { x: centre.x - 140, y: centre.y - 90 });
  // On a phone the sidebar covers the board; get out of the way of the result.
  if (window.matchMedia('(max-width: 767px)').matches && sidebarRef.value) sidebarRef.value.sidebarOpen = false;
}

// ── Free Drawing ────────────────────────────────────────────
// The highlighter is a wide, faint pen. Its width and its opacity are decided
// here, once, for both the stroke being drawn and the one that is saved, so
// what the user sees while drawing is what the board keeps.
const isHighlighter = computed(() => store.drawSubTool.value === 'highlighter');
const drawingSize = computed(() => store.activeStrokeSize.value * (isHighlighter.value ? 3 : 1));
const drawingOpacity = computed(() => (isHighlighter.value ? 0.35 : 0.85));

const freeDrawing = useFreeDrawing({
  color: store.activeColor,
  size: drawingSize,
  onStrokeComplete: (stroke, color, size, realPressure) => {
    const node: WBNode = {
      id: store.generateId('stroke'), type: 'stroke',
      position: { x: stroke.x, y: stroke.y },
      data: {
        svgPath: stroke.svgPath,
        points: stroke.points,
        width: stroke.width,
        height: stroke.height,
        color,
        size,
        opacity: drawingOpacity.value,
        ...(realPressure ? { realPressure: true } : {}),
      },
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
  const vfEl = flowEl();
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
  const vfEl = flowEl();
  if (!vfEl) return;
  freeDrawing.continueDraw(e, vfEl.getBoundingClientRect(), viewport.value);
}

function handleCanvasPointerUp(e: PointerEvent) {
  if (eraseBatchOpen) { store.endUndoBatch(); eraseBatchOpen = false; }
  endWipe();
  if (store.activeTool.value !== 'draw') return;
  if (store.drawSubTool.value === 'eraser') { isErasing.value = false; return; }
  freeDrawing.endDraw(e);
}

/**
 * The system took the pointer away — a palm on the glass, a gesture of the
 * OS. What was being drawn was not meant, so it is dropped rather than kept.
 */
function handleCanvasPointerCancel(e: PointerEvent) {
  if (eraseBatchOpen) { store.endUndoBatch(); eraseBatchOpen = false; }
  endWipe();
  isErasing.value = false;
  freeDrawing.cancelDraw(e);
}

// The tool changed in the middle of a stroke (a key pressed with the pen
// down): the drawing overlay goes away and its pointer-up never comes. The
// stroke so far is kept, and the next one starts fresh — it used to be joined
// onto this one.
watch(() => [store.activeTool.value, store.drawSubTool.value], () => {
  if (freeDrawing.isDrawing.value) freeDrawing.endDraw();
  if (eraseBatchOpen) { store.endUndoBatch(); eraseBatchOpen = false; }
  endWipe();
  isErasing.value = false;
});

/**
 * The pointer left the canvas.
 *
 * Ends a wipe, but not a stroke: a stroke holds its pointer, so it keeps
 * drawing past the edge of the overlay and ends when the pen is lifted.
 */
function handleCanvasPointerLeave() {
  if (eraseBatchOpen) { store.endUndoBatch(); eraseBatchOpen = false; }
  endWipe();
  if (isErasing.value) isErasing.value = false;
}

// ── Node Click ──────────────────────────────────────────────
function handleNodeClick({ node, event }: any) {
  selectedEdgeId.value = null;

  // Alt/Option-click follows an item's link. Cmd and Ctrl are taken: they
  // add to the selection.
  if (event?.altKey && node.data?.link) {
    followLink(node.data.link);
    return;
  }

  if (store.activeTool.value === 'eraser') {
    deleteNodes([node.id]);
    selectedShapeNodeId.value = null;
    return;
  }

  // A grouped item is clicked as its group: all of it selected, the item
  // clicked included, and no single item's panel.
  if (store.activeTool.value === 'select' && node.data?.groupId) {
    const members = groupOf(node.id);
    if (members.length > 1) {
      selectItems(members, !!(event?.shiftKey || event?.metaKey || event?.ctrlKey));
      cancelPanel();
      selectedShapeNodeId.value = null;
      selectedTextNodeId.value = null;
      return;
    }
  }

  if (store.activeTool.value === 'select') {
    if (node.type === 'shape' || node.type === 'text') {
      openPanelSoon(node.type, node.id);
      shapeMenuPos.value = { x: event.clientX, y: event.clientY };
    } else {
      cancelPanel();
      selectedShapeNodeId.value = null;
      selectedTextNodeId.value = null;
    }
  }
}

/**
 * Open an item's panel a moment after the click that selected it.
 *
 * The panel sits over the right of the canvas. Opened at once, it lands on
 * top of an item near that edge between the two clicks of a double-click,
 * and the second click — the one that starts editing — goes to the panel.
 * Once open, the canvas moves the item out from under it.
 */
let panelTimer: ReturnType<typeof setTimeout> | null = null;
function cancelPanel() {
  if (panelTimer) clearTimeout(panelTimer);
  panelTimer = null;
}
function openPanelSoon(type: 'shape' | 'text', id: string) {
  cancelPanel();
  panelTimer = setTimeout(() => {
    panelTimer = null;
    if (!vfNodes.value.find((n: any) => n.id === id)?.selected) return;
    selectedShapeNodeId.value = type === 'shape' ? id : null;
    selectedTextNodeId.value = type === 'text' ? id : null;
    nextTick(() => keepClearOfPanel(id));
  }, 260);
}

/** Pan the canvas so an item is not under the panel on the right. */
function keepClearOfPanel(id: string) {
  const el = canvasRef.value?.querySelector<HTMLElement>(`.vue-flow__node[data-id="${CSS.escape(id)}"]`);
  const panel = document.querySelector<HTMLElement>('.sp-panel, .ep-panel');
  if (!el || !panel) return;
  const item = el.getBoundingClientRect();
  const left = panel.getBoundingClientRect().left;
  const overlap = item.right + 24 - left;
  if (overlap <= 0) return;
  const { x, y, zoom } = viewport.value;
  setViewport({ x: x - overlap, y, zoom }, { duration: motion(200) });
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
  if (vfNode) {
    vfNode.zIndex = computeShapeZIndex(box.width, box.height);
    applyState(vfNode);
  }
  scheduleSave();
}

/**
 * The angle of an item — a picture, a shape, a sticky note, a text box. Same
 * division of labour as the box above: the canvas while turning, the board
 * when the turn ends.
 */
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
    // What a screen reader says for it follows its words as they change.
    const stored = store.currentBoardData.value?.nodes.find((n: WBNode) => n.id === nodeId);
    if (stored) vfNode.ariaLabel = spokenName(stored);
    vfNodes.value = [...vfNodes.value];
  }
  scheduleSave();
}

// ── Mindmap Helpers ─────────────────────────────────────────
function handleMindmapAddChild({ parentId, direction }: { parentId: string; direction: 'right' | 'left' }) {
  const parent = store.currentBoardData.value?.nodes.find((n: WBNode) => n.id === parentId);
  // A locked item is left as it is, its branches included.
  if (!parent || parent.data?.locked) return;
  // A child under a folded branch would be added out of sight: unfolded in
  // the same step as the add, so one undo takes back both.
  store.beginUndoBatch();
  if (parent.data?.collapsed) store.updateNodeData(parentId, { collapsed: undefined });
  const childId = store.addMindmapChild(parentId, direction);
  store.endUndoBatch();
  tidyMindmap(parentId);
  scheduleSave();
  if (childId) focusMindmapNode(childId);
}

/**
 * Lay a mind map out as a tree, from its root.
 *
 * Done whenever an item is added, as a mind-map tool does, so siblings never
 * land on top of one another. New items have not been drawn yet, so their
 * size is a guess; the layout runs again a moment later with the sizes as
 * drawn. Part of the step that added the item: nothing to undo separately.
 */
function tidyMindmap(anyId: string, settle = true) {
  const board = store.currentBoardData.value;
  if (!board) return;
  const root = rootOf(anyId, board.nodes, board.edges);
  const sizeOf = (n: WBNode) => {
    const d = findNode(n.id)?.dimensions;
    return d?.width && d?.height ? { width: d.width, height: d.height } : estimateSize(n);
  };
  const placed = tidyTree(root, board.nodes, board.edges, sizeOf);
  for (const node of board.nodes) {
    const at = placed.get(node.id);
    if (!at || (at.x === node.position.x && at.y === node.position.y)) continue;
    node.position = at;
    stampElement(node);
  }
  syncToVueFlow();
  if (settle) setTimeout(() => { tidyMindmap(anyId, false); scheduleSave(); }, 120);
}

/** Fold or unfold the branch under a mind-map item. */
function toggleMindmapCollapse(nodeId: string) {
  const node = store.currentBoardData.value?.nodes.find((n: WBNode) => n.id === nodeId);
  if (!node) return;
  store.updateNodeData(nodeId, { collapsed: node.data?.collapsed ? undefined : true });
  tidyMindmap(nodeId, false);
  scheduleSave();
}

function handleMindmapRemoveNode(nodeId: string) {
  deleteNodes([nodeId]);
}

function handleMindmapAddSibling(nodeId: string) {
  if (store.currentBoardData.value?.nodes.find((n: WBNode) => n.id === nodeId)?.data?.locked) return;
  const siblingId = store.addMindmapSibling(nodeId);
  tidyMindmap(nodeId);
  scheduleSave();
  if (siblingId) focusMindmapNode(siblingId);
}

/**
 * Put the caret in a new item's editor.
 *
 * After a short wait and with retries: the item is created by the click that
 * placed it, and the canvas finishes handling that click — and moves focus —
 * after the item's own attempt to focus itself has already run.
 */
function focusMindmapNode(nodeId: string) {
  let attempts = 0;
  const tryFocus = () => {
    const nodeEl = canvasRef.value?.querySelector(`[data-id="${nodeId}"]`);
    if (nodeEl) {
      const input = nodeEl.querySelector('input, textarea') as HTMLInputElement | HTMLTextAreaElement | null;
      if (input) { input.focus(); input.select(); return; }
    }
    if (++attempts < 10) setTimeout(tryFocus, 50);
  };
  setTimeout(tryFocus, 50);
}

// ── Node Drag (delegates to mindmap drag) ───────────────────
/**
 * Open an item's editor from the keyboard. Each kind of item opens on a
 * double-click on the part that holds its words; this sends it one.
 */
function startEditing(id: string) {
  const el = canvasRef.value?.querySelector(`.vue-flow__node[data-id="${CSS.escape(id)}"]`);
  const target = el?.querySelector('.wb-shape-node, .wb-text-node, .wb-sticky, .wb-frame__title, .wb-comment');
  target?.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
}

/** Where everything was when a drag began, to know what it moved and by how much. */
let positionsAtDragStart = new Map<string, { x: number; y: number }>();

function handleNodeDragStart(event: any) {
  const { node } = event;
  positionsAtDragStart = new Map(
    (store.currentBoardData.value?.nodes ?? []).map((n: WBNode) => [n.id, { ...n.position }]),
  );
  // Before anything moves. A drag reports a position per frame, so the step
  // to come back to is the one recorded here, once, at the start of it.
  store.pushUndoState();
  holdObstacles.value = true;
  // A group's other members are carried along with it (useMindmapDrag).
  mindmapDragStart(event);
  guides.onDragStart(event.nodes ?? [node]);
}

function handleNodeDrag(event: any) {
  guides.onDrag(event.nodes ?? [event.node]);
  mindmapDrag(event);
}

function handleNodeDragStop(event: any) {
  holdObstacles.value = false;
  // The guides first: they may pull the dropped item a few pixels into line,
  // and a mind map's branch has to follow where it ends up.
  const pulled = guides.onDragStop(event.nodes ?? [event.node]);
  if (pulled.dx || pulled.dy) mindmapDrag(event);
  mindmapDragStop(event);

  // Lines whose two ends both moved take their bends along.
  const board = store.currentBoardData.value;
  if (board) {
    const moved = new Map<string, { dx: number; dy: number }>();
    for (const n of board.nodes) {
      const was = positionsAtDragStart.get(n.id);
      if (was && (was.x !== n.position.x || was.y !== n.position.y)) {
        moved.set(n.id, { dx: n.position.x - was.x, dy: n.position.y - was.y });
      }
    }
    if (carryWaypoints(board.edges, moved)) {
      vfEdges.value = board.edges.map((e: WBEdge) => buildVfEdge(e, board.nodes));
    }
  }
  positionsAtDragStart = new Map();
  scheduleSave();

  // A card dropped in a kanban column or on a timeline day changes its item in
  // the vault (useLiveFrames.moveCard); the frame then puts it in its place.
  for (const dropped of event.nodes ?? [event.node]) {
    const card = store.currentBoardData.value?.nodes.find((n: WBNode) => n.id === dropped.id);
    const frame = card?.data?.fromQuery && store.currentBoardData.value?.nodes.find((n: WBNode) => n.id === card.data.fromQuery);
    if (card && frame && (frame.data?.layout === 'kanban' || frame.data?.layout === 'timeline')) void live.moveCard(card);
  }
}

/**
 * A line's end dragged onto another item, or another side of the same one.
 * The line keeps everything else — its style, its label, its bends.
 */
function handleEdgeReconnect({ edge, connection }: { edge: any; connection: any }) {
  const wbEdge = store.currentBoardData.value?.edges.find((e: WBEdge) => e.id === edge.id);
  if (!wbEdge || !connection?.source || !connection?.target) return;
  store.pushUndoState();
  wbEdge.source = connection.source;
  wbEdge.target = connection.target;
  wbEdge.sourceHandle = connection.sourceHandle ?? undefined;
  wbEdge.targetHandle = connection.targetHandle ?? undefined;
  stampElement(wbEdge);
  syncToVueFlow();
  scheduleSave();
}

const { getNodes: allCanvasNodes } = useVueFlow({ id: 'whiteboard-flow' });
const guides = useSmartGuides({
  store,
  zoom: () => viewport.value.zoom,
  allNodes: () => allCanvasNodes.value,
  enabled: () => store.smartGuides.value,
});

// ── Clipboard & Export ──────────────────────────────────────
const { exportBoard, isExporting, exportOnly } =
  useClipboardExport(store, vaultPathRef, flowEl, {
    lang: () => String(i18nLocale.value),
    labels: () => ({
      prev: t('whiteboard.present.prev'),
      next: t('whiteboard.present.next'),
      fit: t('whiteboard.share.fit'),
      zoomIn: t('whiteboard.share.zoom_in'),
      zoomOut: t('whiteboard.share.zoom_out'),
      hint: t('whiteboard.share.page_hint'),
      untitled: t('whiteboard.untitled_board'),
    }),
  }, (ids) => syncToVueFlow(ids), () => showAppNotice(t('whiteboard.fail.export'), 'error'));

// ── Keyboard Shortcuts ──────────────────────────────────────
const { handleKeydown, handleKeydownCapture } = useWhiteboardKeyboard({
  isActive: () => isAppActive.value && !presenting.value,
  startEditing,
  store, vfNodes, vfEdges,
  deleteNodes, syncToVueFlow, scheduleSave,
  deleteLooseSelection,
  arrange,
  focusMindmapNode, handleMindmapAddChild, handleMindmapAddSibling, handleMindmapRemoveNode,
  handleMultiGroup, handleMultiUngroup,
  closeEdgeMenu, closeShapeMenu, closeTextMenu,
  placeWithTool,
  openMenu: openMenuFromKeyboard,
  openSearch: () => { showSearch.value = true; },
  openShortcuts: () => { showShortcuts.value = true; },
});

/** Every line end on the board, by kind and colour, for the marker definitions. */
const markerUses = computed(() => {
  const uses: { kind: string; color: string }[] = [];
  for (const edge of store.currentBoardData.value?.edges ?? []) {
    const color = paint(edge.data?.color) || EDGE_GREY;
    for (const kind of [edge.data?.markerStart, edge.data?.markerEnd]) {
      if (isMarker(kind)) uses.push({ kind, color });
    }
  }
  return uses;
});

// ── Context menu ────────────────────────────────────────────
// The visible home of everything the keyboard can do to a selection, and the
// only home of it on a touch screen.
const isMac = /Mac|iPhone|iPad/.test(navigator.platform);
const keys = (mac: string, other: string) => (isMac ? mac : other);
const contextMenu = ref<{ x: number; y: number; at: { x: number; y: number }; items: ContextMenuItem[] } | null>(null);

/**
 * The right-click menu, opened some other way: from the keyboard, from a
 * panel's "More" button, from the toolbar's Insert button. Focus goes back to
 * whatever had it once the menu closes.
 */
let menuOpener: HTMLElement | null = null;
function openMenuAt(screen: { x: number; y: number }, onSelection: boolean, opener?: HTMLElement | null) {
  menuOpener = opener ?? (document.activeElement as HTMLElement | null);
  openContextMenu({ clientX: screen.x, clientY: screen.y, preventDefault() {} } as MouseEvent, onSelection);
  // Not opened at a point on the board: what it inserts goes in the middle of the view.
  if (!onSelection && contextMenu.value) contextMenu.value.at = viewportCentre();
}
function openMenuFromKeyboard() {
  const ids = arrange.selectedIds();
  const rect = flowEl()?.getBoundingClientRect();
  if (!rect) return;
  const el = ids.length ? flowEl()?.querySelector(`.vue-flow__node[data-id="${CSS.escape(ids[0])}"]`) : null;
  const box = el?.getBoundingClientRect();
  const point = box ? { x: box.left + box.width / 2, y: box.top + box.height / 2 } : { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
  openMenuAt(point, ids.length > 0);
}
function openMenuFrom(el: HTMLElement | undefined, onSelection: boolean) {
  const box = el?.getBoundingClientRect();
  if (!box) return openMenuFromKeyboard();
  openMenuAt({ x: box.left, y: box.bottom + 4 }, onSelection, el);
}
watch(contextMenu, (menu) => {
  if (menu || !menuOpener) return;
  const back = menuOpener;
  menuOpener = null;
  if (back.isConnected) back.focus();
});

function openContextMenu(event: MouseEvent, onSelection: boolean) {
  event.preventDefault();
  const at = screenToFlowCoordinate({ x: event.clientX, y: event.clientY });
  const ids = arrange.selectedIds();
  const nodes = ids.map((id) => store.currentBoardData.value?.nodes.find((n: WBNode) => n.id === id)).filter(Boolean) as WBNode[];
  const allLocked = nodes.length > 0 && nodes.every((n) => n.data?.locked);
  const grouped = nodes.length > 1 && nodes.every((n) => n.data?.groupId && n.data.groupId === nodes[0].data.groupId);
  // A single mind-map item gets the mind map's own actions first.
  const mindItem = nodes.length === 1 && nodes[0].type === 'mindmap' ? nodes[0] : null;
  const mindHasKids = mindItem ? (childrenOf(store.currentBoardData.value!.nodes, store.currentBoardData.value!.edges).get(mindItem.id)?.length ?? 0) > 0 : false;
  const mindItems: ContextMenuItem[] = mindItem
    ? [
        { id: 'mind-tidy', label: t('whiteboard.mindmap.tidy') },
        ...(mindHasKids ? [{ id: 'mind-fold', label: t(mindItem.data?.collapsed ? 'whiteboard.mindmap.expand' : 'whiteboard.mindmap.collapse') }] : []),
        { id: 'mind-outline', label: t('whiteboard.mindmap.copy_outline') },
        { id: 'mind-note', label: t('whiteboard.mindmap.save_note') },
      ]
    : [];
  // Syn's actions, when there is something for it to read or look at.
  const hasWords = nodes.some((n) => n.type !== 'stroke' && n.type !== 'image' && n.type !== 'frame');
  const hasInk = nodes.some((n) => n.type === 'stroke');
  const synItems: ContextMenuItem[] = [
    ...(hasWords
      ? [
          { id: 'syn-summarize', label: t('whiteboard.ctx.syn_summarize'), separated: true, disabled: !!assist.busy.value },
          { id: 'syn-expand', label: t('whiteboard.ctx.syn_expand'), disabled: !!assist.busy.value },
          ...(nodes.length > 2 ? [{ id: 'syn-cluster', label: t('whiteboard.ctx.syn_cluster'), disabled: !!assist.busy.value }] : []),
          { id: 'syn-tasks', label: t('whiteboard.ctx.syn_tasks'), disabled: !!assist.busy.value },
        ]
      : []),
    ...(hasInk ? [{ id: 'syn-sketch', label: t('whiteboard.ctx.syn_sketch'), separated: !hasWords, disabled: !!assist.busy.value }] : []),
    ...(hasWords ? [{ id: 'syn-generate', label: t('whiteboard.ctx.syn_generate_from'), disabled: !!assist.busy.value }] : []),
    // While Syn works, the way to stop waiting for it.
    ...(assist.busy.value ? [{ id: 'syn-cancel', label: t('whiteboard.syn.cancel') }] : []),
  ];
  const liveFrame = nodes.length === 1 && nodes[0].type === 'frame' && nodes[0].data?.query;
  const items: ContextMenuItem[] = onSelection && ids.length
    ? [
        ...(liveFrame ? [{ id: 'refresh-live', label: t('whiteboard.ctx.refresh_live') }] : []),
        ...(liveFrame ? (['grid', 'kanban', 'timeline'] as const)
          .filter((l) => (nodes[0].data?.layout ?? 'grid') !== l)
          .map((l) => ({ id: `live-layout-${l}`, label: t('whiteboard.live.show_as', { layout: t(`whiteboard.live.layouts.${l}`) }) })) : []),
        ...(nodes.length === 1 && nodes[0].type === 'frame' ? [{ id: 'present-here', label: t('whiteboard.present.from_here') }] : []),
        ...(nodes.length === 1 && nodes[0].type !== 'comment' ? [{ id: 'comment-on', label: t('whiteboard.comment.add') }] : []),
        ...(nodes.length === 1 && nodes[0].data?.link ? [{ id: 'link-open', label: t('whiteboard.link.open'), shortcut: keys('⌥-click', 'Alt+click') }] : []),
        ...(nodes.length === 2 && nodes.every((n) => CONNECTABLE.has(n.type)) ? [{ id: 'connect', label: t('whiteboard.connect.menu') }] : []),
        ...(nodes.length === 1 && nodes[0].type === 'image' ? [{ id: 'alt-edit', label: t('whiteboard.alt.edit') }] : []),
        ...(nodes.length === 1 && nodes[0].type !== 'stroke' ? [{ id: 'link-edit', label: t(nodes[0].data?.link ? 'whiteboard.link.edit' : 'whiteboard.link.add') }] : []),
        ...(nodes.length === 1 && nodes[0].data?.link ? [{ id: 'link-remove', label: t('whiteboard.link.remove') }] : []),
        ...mindItems,
        { id: 'duplicate', label: t('whiteboard.ctx.duplicate'), shortcut: keys('⌘D', 'Ctrl+D'), separated: mindItems.length > 0 },
        { id: 'copy', label: t('whiteboard.ctx.copy'), shortcut: keys('⌘C', 'Ctrl+C') },
        { id: 'cut', label: t('whiteboard.ctx.cut'), shortcut: keys('⌘X', 'Ctrl+X'), disabled: allLocked },
        { id: 'front', label: t('whiteboard.ctx.to_front'), shortcut: keys('⇧⌘]', 'Ctrl+Shift+]'), separated: true },
        { id: 'back', label: t('whiteboard.ctx.to_back'), shortcut: keys('⇧⌘[', 'Ctrl+Shift+[') },
        { id: 'lock', label: t(allLocked ? 'whiteboard.ctx.unlock' : 'whiteboard.ctx.lock'), shortcut: keys('⇧⌘L', 'Ctrl+Shift+L') },
        ...(nodes.length > 1
          ? [{ id: grouped ? 'ungroup' : 'group', label: t(grouped ? 'whiteboard.ungroup' : 'whiteboard.group'), shortcut: grouped ? keys('⇧⌘G', 'Ctrl+Shift+G') : keys('⌘G', 'Ctrl+G') }]
          : []),
        { id: 'frame', label: t('whiteboard.ctx.frame_selection'), shortcut: keys('⌥⌘G', 'Ctrl+Alt+G') },
        { id: 'copy-style', label: t('whiteboard.ctx.copy_style'), shortcut: keys('⌥⌘C', 'Ctrl+Alt+C'), separated: true, disabled: nodes.length !== 1 },
        { id: 'paste-style', label: t('whiteboard.ctx.paste_style'), shortcut: keys('⌥⌘V', 'Ctrl+Alt+V'), disabled: !arrange.hasCopiedStyle() },
        ...synItems,
        { id: 'save-to-library', label: t('whiteboard.library.save_selection'), separated: true },
        ...(canCopyImage ? [{ id: 'copy-image', label: t('whiteboard.ctx.copy_image') }] : []),
        { id: 'export-png', label: t('whiteboard.ctx.export_png'), separated: !canCopyImage },
        { id: 'export-pdf', label: t('whiteboard.ctx.export_pdf') },
        { id: 'delete', label: t('whiteboard.delete'), shortcut: keys('⌫', 'Del'), danger: true, separated: true, disabled: allLocked },
      ]
    : [
        { id: 'paste', label: t('whiteboard.ctx.paste'), shortcut: keys('⌘V', 'Ctrl+V') },
        { id: 'select-all', label: t('whiteboard.ctx.select_all'), shortcut: keys('⌘A', 'Ctrl+A'), disabled: !store.currentBoardData.value?.nodes.length },
        { id: 'template', label: t('whiteboard.templates.insert'), separated: true },
        { id: 'add-vault', label: t('whiteboard.ctx.add_vault') },
        { id: 'add-live', label: t('whiteboard.ctx.add_live') },
        { id: 'syn-generate', label: t('whiteboard.ctx.syn_generate'), separated: true, disabled: !!assist.busy.value },
        ...(assist.busy.value ? [{ id: 'syn-cancel', label: t('whiteboard.syn.cancel') }] : []),
        { id: 'comment-here', label: t('whiteboard.comment.add') },
        { id: 'present', label: t('whiteboard.present.start'), separated: true, disabled: !store.currentBoardData.value?.nodes.length },
        { id: 'shortcuts', label: t('whiteboard.keys.title'), shortcut: '?' },
      ];
  contextMenu.value = { x: event.clientX, y: event.clientY, at, items };
}

/** Right-click on an item: it becomes the selection, unless it is already part of it. */
function handleNodeContextMenu({ event, node }: { event: MouseEvent | TouchEvent; node: any }) {
  if (!node.selected) selectItems([node.id], false);
  openContextMenu(event as MouseEvent, true);
}

async function handleContextAction(action: string) {
  const at = contextMenu.value?.at;
  const id = action;
  switch (id) {
    case 'duplicate': arrange.duplicate(); break;
    case 'copy':
    case 'cut': {
      const clip = arrange.clipOf();
      if (!clip) break;
      // A menu click is a gesture the page may write the clipboard on.
      try { await navigator.clipboard.writeText(JSON.stringify(clip)); } catch (err) { logger.error('Copy failed', err as string); showAppNotice(t('whiteboard.fail.clipboard'), 'error'); break; }
      if (id === 'cut') arrange.removeSelection();
      break;
    }
    case 'paste': {
      let text = '';
      try { text = await navigator.clipboard.readText(); } catch (err) { logger.error('Paste failed', err as string); showAppNotice(t('whiteboard.fail.clipboard'), 'error'); break; }
      const clip = parseClip(text);
      if (clip) arrange.paste(clip, at);
      else if (text.trim() && at) {
        addNodeToCanvas({ id: store.generateId('text'), type: 'text', position: at, data: { label: text.trim().slice(0, 5000), width: 240 } });
      }
      break;
    }
    case 'front': arrange.reorder('front'); break;
    case 'back': arrange.reorder('back'); break;
    case 'lock': arrange.toggleLock(); break;
    case 'group': handleMultiGroup(); break;
    case 'ungroup': handleMultiUngroup(); break;
    case 'copy-style': arrange.copyStyle(); break;
    case 'paste-style': arrange.pasteStyle(); break;
    case 'delete': arrange.removeSelection(); break;
    case 'select-all': arrange.selectAll(); break;
    case 'shortcuts': showShortcuts.value = true; break;
    case 'frame': arrange.frameSelection(); break;
    case 'template': templatePicker.value = { mode: 'insert', at }; break;
    case 'add-vault': vaultPickerAt.value = at ?? viewportCentre(); break;
    case 'add-live': liveDialogAt.value = at ?? viewportCentre(); break;
    case 'present': presentation.start(); break;
    case 'link-open': { const n = selectedItem(); if (n?.data?.link) followLink(n.data.link); break; }
    case 'link-edit': { const n = selectedItem(); if (n) linkDialog.value = { id: n.id, link: n.data?.link }; break; }
    case 'connect': connectSelection(); break;
    case 'alt-edit': { const n = selectedItem(); if (n) altDialog.value = { id: n.id, alt: n.data?.alt }; break; }
    case 'link-remove': { const n = selectedItem(); if (n) setLink(n.id, null); break; }
    case 'copy-image': copySelectionAsImage(); break;
    case 'save-to-library': void saveSelectionToLibrary(); break;
    case 'comment-here': if (at) addComment(at); break;
    case 'comment-on': { const id = arrange.selectedIds()[0]; if (id) addComment(null, id); break; }
    case 'present-here': presentation.start(arrange.selectedIds()[0]); break;
    case 'live-layout-grid':
    case 'live-layout-kanban':
    case 'live-layout-timeline': {
      const frame = store.currentBoardData.value?.nodes.find((n: WBNode) => n.id === arrange.selectedIds()[0]);
      if (frame) void live.setLayout(frame, id.slice('live-layout-'.length) as 'grid' | 'kanban' | 'timeline');
      break;
    }
    case 'refresh-live': { const id = arrange.selectedIds()[0]; if (id) refreshOneLive(id); break; }
    case 'syn-summarize': void assist.run('summarize'); break;
    case 'syn-cancel': assist.cancel(); break;
    case 'syn-expand': void assist.run('expand'); break;
    case 'syn-cluster': void assist.run('cluster'); break;
    case 'syn-tasks': void assist.run('tasks'); break;
    case 'syn-sketch': void assist.run('sketch'); break;
    case 'syn-generate': generateDialog.value = true; break;
    case 'export-png':
    case 'export-pdf':
      await exportBoard({ format: id === 'export-pdf' ? 'pdf' : 'png', only: selectionWithFrameContents() });
      break;
    case 'mind-tidy': { const id = arrange.selectedIds()[0]; if (id) { store.pushUndoState(); tidyMindmap(id, false); scheduleSave(); } break; }
    case 'mind-fold': { const id = arrange.selectedIds()[0]; if (id) toggleMindmapCollapse(id); break; }
    case 'mind-outline':
    case 'mind-note': {
      const id = arrange.selectedIds()[0];
      const board = store.currentBoardData.value;
      if (!id || !board) break;
      const root = rootOf(id, board.nodes, board.edges);
      const outline = toOutline(root, board.nodes, board.edges);
      if (action === 'mind-outline') {
        try { await navigator.clipboard.writeText(outline); } catch (err) { logger.error('Copy failed', err as string); showAppNotice(t('whiteboard.fail.clipboard'), 'error'); }
      } else {
        await saveOutlineAsNote(board.nodes.find((n: WBNode) => n.id === root)?.data?.label || board.title, outline);
      }
      break;
    }
  }
}

/**
 * The selection, and for each frame in it, what the frame holds — "export
 * this frame" means the frame and its contents, not an empty rectangle.
 */
function selectionWithFrameContents(): string[] {
  const ids = new Set(arrange.selectedIds());
  const board = store.currentBoardData.value;
  if (!board) return [...ids];
  const boxOf = (n: WBNode) => {
    const d = findNode(n.id)?.dimensions;
    return { x: n.position.x, y: n.position.y, w: d?.width || n.data?.width || 0, h: d?.height || n.data?.height || 0 };
  };
  for (const frame of board.nodes.filter((n: WBNode) => n.type === 'frame' && ids.has(n.id))) {
    const f = boxOf(frame);
    for (const n of board.nodes) {
      const b = boxOf(n);
      if (b.x >= f.x && b.y >= f.y && b.x + b.w <= f.x + f.w && b.y + b.h <= f.y + f.h) ids.add(n.id);
    }
  }
  return [...ids];
}

/** A button in a single item's panel; the item is the selection. */
function handleItemAction(id: string, el?: HTMLElement) {
  if (id === 'more') openMenuFrom(el, true);
  else if (id === 'copy-style') arrange.copyStyle();
  else if (id === 'paste-style') arrange.pasteStyle();
  else handleArrangeAction(id);
}

/** A button in the selection panel. */
function handleArrangeAction(id: string, el?: HTMLElement) {
  if (id === 'more') openMenuFrom(el, true);
  else if (id.startsWith('align-')) arrange.align(id.slice(6) as AlignMode);
  else if (id === 'distribute-x') arrange.distribute('x');
  else if (id === 'distribute-y') arrange.distribute('y');
  else if (id === 'same-width') arrange.matchSize('width');
  else if (id === 'same-height') arrange.matchSize('height');
  else if (id === 'front' || id === 'back') arrange.reorder(id);
  else if (id === 'lock') arrange.toggleLock();
  else if (id === 'duplicate') arrange.duplicate();
}

/**
 * A mind map, written out as a note: its root is the title and the rest an
 * outline — the same tree, in the form the rest of the vault reads and links.
 */
const nodeService = useNodeService();
async function saveOutlineAsNote(title: string, outline: string) {
  try {
    const relPath = await nodeService.createNode({ directory: 'Notes', nodeType: 'note', silent: true });
    // The outline under its own title line is a note with a heading; the
    // first line of the outline is the root, which the title already names.
    const body = outline.split('\n').slice(1).map((line) => line.slice(2)).join('\n');
    await nodeService.writeNode({ relPath, nodeType: 'note', title, properties: {}, content: body, eventType: 'created' });
    showAppNotice(t('whiteboard.mindmap.saved_note', { title }), 'info');
  } catch (err) {
    logger.error('Could not save the mind map as a note', err as string);
    showAppNotice(t('whiteboard.fail.save_note'), 'error');
  }
}

/**
 * An indented list pasted onto the board becomes a mind map: its first item
 * the root, each level of indent a level of branch. Laid out at once.
 */
function addMindmapFromOutline(tree: OutlineItem, at: { x: number; y: number }) {
  const board = store.currentBoardData.value;
  if (!board) return;
  store.beginUndoBatch();
  store.pushUndoState();
  const rootId = store.generateId('mind');
  const add = (item: OutlineItem, id: string, level: number, parentId: string | null) => {
    const node: WBNode = {
      id, type: 'mindmap', position: { ...at },
      data: { label: item.label, color: store.getMindmapColor(level), level, direction: 'right' },
    };
    stampElement(node);
    board.nodes.push(node);
    if (parentId) {
      const edge: WBEdge = {
        id: store.generateId('e'), source: parentId, target: id, type: 'default', data: {},
        sourceHandle: 'right-source', targetHandle: 'left-target',
      };
      stampElement(edge);
      board.edges.push(edge);
    }
    for (const child of item.children) add(child, store.generateId('mind'), level + 1, id);
  };
  add(tree, rootId, 0, null);
  store.endUndoBatch();
  tidyMindmap(rootId);
  scheduleSave();
}

/**
 * Bring a drawing from another tool onto the board, its top-left at `at`.
 * Goes through the same door as a paste: fresh ids, one step to undo, the
 * new items selected.
 */
/**
 * Pictures that came with a drawing or a library piece, put in the vault so
 * the board can show them: until then they are bytes on the item
 * (`pendingPicture`). A picture that cannot be saved is left out, with the
 * lines to it, and the user is told.
 */
async function settlePictures(clip: { nodes: WBNode[]; edges: WBEdge[] }): Promise<{ nodes: WBNode[]; edges: WBEdge[] }> {
  const pending = clip.nodes.filter((n) => n.type === 'image' && typeof n.data?.pendingPicture === 'string');
  if (!pending.length) return clip;
  const dropped = new Set<string>();
  for (const n of pending) {
    try {
      if (!props.vaultPath) throw new Error('no vault');
      const blob = dataUrlBlob(n.data.pendingPicture);
      const ext = blob.type.split('/')[1]?.replace('svg+xml', 'svg').replace('jpeg', 'jpg') || 'png';
      const assetPath = await saveImageToVault(props.vaultPath, new File([blob], `${n.data.alt || 'picture'}.${ext}`, { type: blob.type }));
      const { pendingPicture: _bytes, ...rest } = n.data;
      n.data = { ...rest, assetPath };
    } catch (err) {
      logger.error('A picture from the drawing could not be saved', err as string);
      dropped.add(n.id);
    }
  }
  if (dropped.size) showAppNotice(t('whiteboard.fail.images', { count: dropped.size }, dropped.size), 'error');
  return {
    nodes: clip.nodes.filter((n) => !dropped.has(n.id)),
    edges: clip.edges.filter((e) => !dropped.has(e.source) && !dropped.has(e.target)),
  };
}

async function importDrawing(text: string, name: string, at: { x: number; y: number }): Promise<boolean> {
  let drawing: Imported | null = null;
  try {
    if (/\.excalidraw$/i.test(name) || /"type"\s*:\s*"excalidraw/.test(text)) drawing = fromExcalidraw(text);
    else if (/\.(drawio|xml)$/i.test(name) || /<mxfile|<mxGraphModel/.test(text)) drawing = await fromDrawio(text);
    else if (/\.(mmd|mermaid)$/i.test(name) || looksLikeMermaid(text)) {
      const drawn = await renderDiagram(diagramId('board-import'), text);
      if ('svg' in drawn) {
        const draft = boardFromDiagram(drawn.svg);
        drawing = draft.nodes.length ? draft : null;
      }
    }
  } catch (err) {
    logger.error('Could not import a drawing', err as string);
  }
  if (!drawing) {
    showAppNotice(t('whiteboard.import_failed', { name: name || t('whiteboard.untitled') }), 'error');
    return false;
  }
  arrange.paste(await settlePictures(drawing), at);
  return true;
}

/** The toolbar's "Import a drawing…": a file chosen from disk, read in the page. */
function pickDrawing() {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = '.drawio,.xml,.excalidraw,.json,.mmd,.mermaid';
  input.onchange = async () => {
    const file = input.files?.[0];
    if (file) await importDrawing(await file.text(), file.name, placementPoint());
  };
  input.click();
}

// ── Things from the vault ───────────────────────────────────

/** Open something from the vault in its own app. */
function openInApp(id: string, kind: string) {
  const route = routeForNodeType(kind);
  if (route) emit('open-node', id, route);
}

/**
 * A card wrote to its board item. A card noticing its thing was renamed is
 * not something the user did, so it is not a step to undo.
 */
function handleCardUpdate(nodeId: string, data: Record<string, any>) {
  const node = store.currentBoardData.value?.nodes.find((n: WBNode) => n.id === nodeId);
  if (!node) return;
  const onlyTitle = Object.keys(data).every((k) => k === 'title' || k === 'kind');
  if (!onlyTitle) {
    handleNodeDataUpdate(nodeId, data);
    return;
  }
  node.data = { ...node.data, ...data };
  stampElement(node);
  const vfNode = vfNodes.value.find((n: any) => n.id === nodeId);
  if (vfNode) {
    vfNode.data = { ...vfNode.data, ...data };
    vfNode.ariaLabel = spokenName(node);
  }
  scheduleSave();
}

/** Put a card for a vault thing on the board, its top-left at `at`. */
async function addVaultCard(id: string, kind: string, title: string, at: { x: number; y: number }) {
  let found = { kind, title };
  if (!kind || !title) {
    try {
      const n = await nodeService.getNode(id);
      if (n) found = { kind: n.node_type, title: n.title };
    } catch { /* placed anyway; the card says it cannot find it */ }
  }
  addNodeToCanvas({
    id: store.generateId('card'), type: 'card', position: at,
    data: { ref: id, kind: found.kind || 'note', title: found.title || id, ...CARD_SIZE },
  });
}

const vaultPickerAt = ref<{ x: number; y: number } | null>(null);
let pickedOffset = 0;
/** Picked in the vault search: placed in a column, one under another, while it stays open. */
function handleVaultPick(item: { id: string; kind: string; title: string }) {
  const at = vaultPickerAt.value ?? viewportCentre();
  void addVaultCard(item.id, item.kind, item.title, { x: at.x, y: at.y + pickedOffset });
  pickedOffset += CARD_SIZE.height + 12;
}
watch(vaultPickerAt, (v) => { if (!v) pickedOffset = 0; });

const liveDialogAt = ref<{ x: number; y: number } | null>(null);
async function handleLiveCreate(value: { query: string; title: string; layout: 'grid' | 'kanban' | 'timeline'; field?: string }) {
  const at = liveDialogAt.value ?? viewportCentre();
  liveDialogAt.value = null;
  await live.addLiveFrame(value.query, value.title, at, value.layout, value.field);
}

let liveNotified = false;
const live = useLiveFrames({
  store,
  refresh: () => syncToVueFlow(),
  scheduleSave,
  // A card dropped in another column or on another day: the change made in
  // the vault, said the way the Tasks app says it, so streaks, projects and
  // the calendar hear it too.
  writeProperty: async (ref, kind, title, patch, was, now) => {
    // The title as the item has it now, not as the card last showed it: the
    // write carries a title, and a stale one would undo a rename made since.
    const current = await nodeService.getNode(ref).catch(() => null);
    title = String(current?.title ?? title);
    await nodeService.writeNode({ relPath: ref, nodeType: kind as NodeType, title, properties: patch, eventType: 'updated' });
    if (kind === 'task' && 'status' in patch) {
      bus.emit('task:status-changed', { id: ref, oldStatus: was, newStatus: now, title });
      if (now === 'done') bus.emit('task:completed', { id: ref, title });
    }
  },
  onMoved: ({ title, field, now, undo }) => {
    const value = field === 'status' && TASK_STATUS_ORDER.includes(now) ? t(`whiteboard.live.status.${now}`) : now || t('whiteboard.live.no_value');
    void undoCardMove.run(
      t('whiteboard.live.moved', { title, value }),
      () => {},
      () => { undo().catch((err) => showAppNotice(t('whiteboard.live.move_failed', { error: String(err) }), 'error')); },
    );
  },
  onMoveFailed: (error) => showAppNotice(t('whiteboard.live.move_failed', { error }), 'error'),
  onError: (error) => {
    // Once per board, not once per refresh: a wrong question stays wrong
    // until it is changed.
    if (liveNotified) return;
    liveNotified = true;
    showAppNotice(t('whiteboard.live.failed', { error }), 'error');
  },
});
watch(() => store.currentBoardId.value, () => { liveNotified = false; });

let liveTimer: ReturnType<typeof setTimeout> | null = null;
/** Ask the live frames again, once the vault has been quiet for a moment. */
function refreshLiveSoon() {
  if (liveTimer) clearTimeout(liveTimer);
  liveTimer = setTimeout(() => { liveTimer = null; void live.refreshAll(); }, 1500);
}

async function refreshOneLive(frameId: string) {
  const frame = store.currentBoardData.value?.nodes.find((n: WBNode) => n.id === frameId);
  if (!frame) return;
  liveNotified = false;
  if (await live.refreshFrame(frame)) {
    syncToVueFlow();
    scheduleSave();
  }
}

// ── Syn on the canvas ───────────────────────────────────────
const { locale } = useI18n();
const assist = useBoardAssist({
  store,
  vaultPath: () => props.vaultPath,
  locale: () => locale.value,
  selectedIds: () => arrange.selectedIds(),
  boxOf: (n: WBNode) => {
    const d = findNode(n.id)?.dimensions;
    return { x: n.position.x, y: n.position.y, w: d?.width || n.data?.width || 160, h: d?.height || n.data?.height || 80 };
  },
  paste: (clip, at) => arrange.paste(clip, at),
  commit: (select) => { syncToVueFlow(select); scheduleSave(); },
  pictureOf: async (ids) => {
    const url = await exportBoard({ format: 'png', only: ids, keep: true });
    return url ? url.slice(url.indexOf(',') + 1) : null;
  },
  addMindmapChildren: (parentId, labels) => {
    const parent = store.currentBoardData.value?.nodes.find((n: WBNode) => n.id === parentId);
    const direction = parent?.data?.direction === 'left' ? 'left' : 'right';
    store.beginUndoBatch();
    for (const label of labels) {
      const childId = store.addMindmapChild(parentId, direction);
      const child = store.currentBoardData.value?.nodes.find((n: WBNode) => n.id === childId);
      if (child) child.data = { ...child.data, label, editing: undefined };
    }
    store.endUndoBatch();
    tidyMindmap(parentId);
    scheduleSave();
  },
  createTasks: async (tasks) => {
    const made: { id: string; title: string }[] = [];
    for (const task of tasks) {
      const relPath = `Tasks/${crypto.randomUUID()}.md`;
      try {
        await nodeService.writeNode({
          relPath, nodeType: 'task', title: task.title,
          properties: { status: 'todo', ...(task.due_date ? { due_date: task.due_date } : {}) },
          content: '', eventType: 'created',
        });
        made.push({ id: relPath, title: task.title });
      } catch (err) {
        logger.error('Could not create a task from the board', err as string);
      }
    }
    if (made.length < tasks.length) showAppNotice(t('whiteboard.fail.tasks', { count: tasks.length - made.length }, tasks.length - made.length), 'error');
    return made;
  },
  notify: (message, kind) => showAppNotice(message, kind),
  viewCentre: () => viewportCentre(),
  t: (key, values) => t(key, values ?? {}),
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
// One press, like every delete in the app; the only question is the app-wide
// "Ask before deleting" (`confirmDelete`). The board leaves the sidebar at once
// and only goes to the vault trash once the undo window closes. Nothing is written inside the
// window, so Undo is a cancelled timer rather than a restore.
const undoBoardDelete = useUndoableAction();
/** A card moved to another column or day changed its item: this takes it back. */
const undoCardMove = useUndoableAction();

/** Boards waiting to go. Hidden here, because a rescan would list them again. */
const hiddenBoardIds = ref<Set<string>>(new Set());
const visibleBoards = computed(() =>
  store.boards.value.filter((b: any) => !hiddenBoardIds.value.has(b.id))
);

function setBoardHidden(id: string, hidden: boolean) {
  const next = new Set(hiddenBoardIds.value);
  if (hidden) next.add(id);
  else next.delete(id);
  hiddenBoardIds.value = next;
}

async function deleteBoard(id: string) {
  const board = store.boards.value.find((b: any) => b.id === id);
  if (!board) return;
  if (!(await confirmDelete({ name: board.title }))) return;
  const wasCurrent = id === store.currentBoardId.value;

  if (wasCurrent) {
    // Whatever was still waiting on the save timer belongs to this board, and
    // is part of what Undo has to bring back.
    await flushSave();
  }
  setBoardHidden(id, true);
  if (wasCurrent) {
    const next = visibleBoards.value[0];
    if (next) {
      await store.loadBoardData(next.id);
    } else {
      store.currentBoardId.value = null;
      store.currentBoardData.value = null;
    }
  }

  await undoBoardDelete.run(
    t('common.deleted_item', { name: board.title }),
    async () => {
      // The move first: a failure throws to the undo, which puts the board
      // back and says so, with its remembered viewport still in place.
      await store.deleteBoard(id);
      forgetViewport(id);
      setBoardHidden(id, false);
    },
    () => {
      setBoardHidden(id, false);
      if (wasCurrent) void switchBoard(id);
    },
  );
}

// ── Lifecycle ───────────────────────────────────────────────
function handleResize() { isMobile.value = window.innerWidth < 768; }

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

  window.addEventListener('resize', handleResize);
  window.addEventListener('keydown', handleKeydown);
  window.addEventListener('keydown', handleKeydownCapture, true);
  // Quitting waits for the autosave that is still to run.
  stopQuitSave = onBeforeQuit(() => flushSave());

  document.addEventListener('visibilitychange', handleVisibilityChange);
  window.addEventListener('paste', handlePaste);
  window.addEventListener('copy', handleCopy);
  window.addEventListener('cut', handleCut);

  // Any write to the vault may be a write to the open board: Syn editing it,
  // a project linking it, the board pane beside a conversation. A save lands
  // as a rename, so it can arrive as either event. Checking costs one read.
  const onVaultChange = async (event?: { paths?: string[] }) => {
    const paths = event?.paths ?? [];
    // Our own save, reported back: the list and the live frames have nothing
    // new. The file is still read — one read and a comparison — because
    // someone else (Syn, a sync) may have written it in the same moments, and
    // skipping the report outright lost their change.
    if (paths.length && paths.every((p) => store.wroteRecently(p))) {
      if (isAppActive.value) await refreshFromDisk();
      return;
    }
    await store.loadBoards();
    if (isAppActive.value) await refreshFromDisk();
    // Something else in the vault changed: live frames may have new answers.
    // A board changing does not change what a frame asks about.
    if (isAppActive.value && (!paths.length || paths.some((p) => !p.endsWith('.whiteboard.json')))) refreshLiveSoon();
  };
  bus.on('vault:file-modified', onVaultChange);
  bus.on('vault:file-created-deleted', onVaultChange);

  // A pull rewrites the board's file on disk. Refreshing only the list left
  // the open board held in memory as it was before the pull, and the next
  // edit wrote that copy back over what had just arrived — the other device's
  // work, gone, with nothing on screen to say so. The pulled copy is merged
  // in; a change of ours still waiting to be saved is kept, and the save
  // merges again before it writes.
  bus.on('vault:sync-completed', (payload: any) => {
    const pulled = payload?.pulled_files as string[] | undefined;
    const openId = store.currentBoardId.value;
    store.loadBoards().then(() => {
      // Through the same door as any other change on disk: not redrawn under
      // a field being typed in (taken in when it is left), and merged by the
      // save if one is waiting.
      if (openId && pulled?.includes(openId)) void refreshFromDisk();
    });
  });
});

onUnmounted(() => {
  void flushSave();
  window.removeEventListener('resize', handleResize);
  window.removeEventListener('keydown', handleKeydown);
  window.removeEventListener('keydown', handleKeydownCapture, true);
  stopQuitSave?.();
  window.removeEventListener('paste', handlePaste);
  window.removeEventListener('copy', handleCopy);
  window.removeEventListener('cut', handleCut);
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
      :boards="visibleBoards"
      :currentBoardId="store.currentBoardId.value || ''"
      :currentBoardData="store.currentBoardData.value"
      :notes="whiteboardNotes"
      @switch-board="switchBoard"
      @create-board="templatePicker = { mode: 'new' }"
      @delete-board="deleteBoard"
      @add-note="addNoteFromSidebar"
      @note-drag-start="() => {}"
    />

    <!-- Main Canvas Area -->
    <div
      class="wb-canvas flex-1 relative transition-colors bg-base dark:bg-base-dark"
      :class="{ 'wb-presenting': presenting }"
      ref="canvasRef"
      :style="paperStyle"
    >
      <template v-if="store.currentBoardData.value">
        <!-- Title Bar -->
        <WhiteboardTitleBar
          v-if="!presenting"
          :boardData="store.currentBoardData.value"
          :isSaving="store.isSaving.value"
          :saveProblem="store.saveProblem.value"
          @update-title="handleUpdateTitle"
          @add-tag="handleAddTag"
          @remove-tag="handleRemoveTag"
          @open-sidebar="sidebarRef && (sidebarRef.sidebarOpen = true)"
          @present="presentation.start()"
          @history="showHistory = true"
        />

        <!-- Vue Flow Canvas -->
        <VueFlow
          id="whiteboard-flow"
          ref="vueFlowRef"
          :nodes="canvasNodes"
          @update:nodes="onCanvasNodes"
          v-model:edges="vfEdges"
          :class="[
            'flex-1',
            store.activeTool.value === 'pan' && 'wb-cursor-grab',
            store.activeTool.value === 'draw' && store.drawSubTool.value !== 'eraser' && 'cursor-crosshair',
            store.activeTool.value === 'draw' && store.drawSubTool.value === 'eraser' && 'wb-cursor-eraser',
            store.activeTool.value === 'eraser' && 'wb-cursor-eraser',
          ]"
          :default-viewport="initialViewport"
          :snap-to-grid="store.snapToGrid.value"
          :snap-grid="[10, 10]"
          :only-render-visible-elements="!isExporting"
          :min-zoom="0.1"
          :connection-mode="ConnectionMode.Loose"
          :delete-key-code="isAppActive && !presenting ? ['Delete', 'Backspace'] : null"
          :pan-on-drag="store.activeTool.value === 'pan' || (isMobile && store.activeTool.value === 'select') ? [0, 1, 2] : (store.activeTool.value === 'select' ? [1, 2] : false)"
          :selection-on-drag="!isMobile && store.activeTool.value === 'select'"
          :multi-selection-key-code="['Shift', 'Meta', 'Control']"
          :aria-label="$t('whiteboard.a11y.canvas')"
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
          @node-drag="handleNodeDrag"
          @node-drag-stop="handleNodeDragStop"
          @edge-click="handleEdgeClick"
          @node-context-menu="handleNodeContextMenu"
          @selection-context-menu="({ event }: any) => openContextMenu(event, true)"
          @pane-context-menu="(event: any) => openContextMenu(event, false)"
          @connect="handleConnect"
          @selection-end="handleSelectionEnd"
          @pane-ready="handlePaneReady"
          :edges-updatable="store.activeTool.value === 'select'"
          @edge-update="handleEdgeReconnect"
          @nodes-change="handleNodesChange"
          @edges-change="handleEdgesChange"
          @dragover.prevent="handleDragOver"
          @drop.prevent="handleDrop"
        >
          <template #edge-default="edgeProps"><WaypointEdge v-bind="(edgeProps as any)" edge-type="default" /></template>
          <template #edge-straight="edgeProps"><WaypointEdge v-bind="(edgeProps as any)" edge-type="straight" /></template>
          <template #edge-step="edgeProps"><WaypointEdge v-bind="(edgeProps as any)" edge-type="step" /></template>
          <template #node-shape="nodeProps"><ShapeNode v-bind="nodeProps" @update:data="(d: any) => handleNodeDataUpdate(nodeProps.id, d)" @rotate="(deg: number, final: boolean) => handleImageRotation(nodeProps.id, deg, final)" /></template>
          <template #node-stroke="nodeProps"><StrokeNode v-bind="nodeProps" /></template>
          <template #node-mindmap="nodeProps">
            <MindmapNode v-bind="nodeProps" @update:data="(d: any) => handleNodeDataUpdate(nodeProps.id, d)" @add-child="handleMindmapAddChild" @add-sibling="handleMindmapAddSibling" @remove-node="handleMindmapRemoveNode" @toggle-collapse="toggleMindmapCollapse" />
          </template>
          <template #node-frame="nodeProps"><FrameNode v-bind="nodeProps" @update:data="(d: any) => handleNodeDataUpdate(nodeProps.id, d)" @refresh="refreshOneLive" /></template>
          <template #node-sticky="nodeProps"><StickyNode v-bind="nodeProps" @update:data="(d: any) => handleNodeDataUpdate(nodeProps.id, d)" @rotate="(deg: number, final: boolean) => handleImageRotation(nodeProps.id, deg, final)" /></template>
          <template #node-text="nodeProps"><TextNode v-bind="nodeProps" @update:data="(d: any) => handleNodeDataUpdate(nodeProps.id, d)" @rotate="(deg: number, final: boolean) => handleImageRotation(nodeProps.id, deg, final)" /></template>
          <template #node-note="nodeProps"><NoteCardNode v-bind="nodeProps" @update:data="(d: any) => handleNodeDataUpdate(nodeProps.id, d)" @open-note="(n: any) => openInApp(n.id, 'note')" /></template>
          <template #node-comment="nodeProps"><CommentNode v-bind="nodeProps" @update:data="(d: any) => handleNodeDataUpdate(nodeProps.id, d)" /></template>
          <template #node-card="nodeProps"><VaultCardNode v-bind="nodeProps" @update:data="(d: any) => handleCardUpdate(nodeProps.id, d)" @open="openInApp" /></template>
          <template #node-image="nodeProps">
            <ImageNode
              v-bind="nodeProps"
              @update:box="(b: any, final: boolean) => handleNodeBoxUpdate(nodeProps.id, b, final)"
              @update:rotation="(deg: number, final: boolean) => handleImageRotation(nodeProps.id, deg, final)"
            />
          </template>

          <Teleport v-if="inkTarget" :to="inkTarget">
            <InkLayer
              :strokes="looseInk"
              :viewport="viewport"
              :size="dimensions"
              :everything="isExporting"
              :only="exportOnly"
              :interactive="store.activeTool.value === 'select'"
              :marquee="marquee"
              :selection-box="looseSelectionBox"
              @press="pressInk"
              @menu="inkMenu"
            />
          </Teleport>

          <Background v-if="store.backgroundPattern.value === 'dots'" variant="dots" :gap="20" :size="1" pattern-color="currentColor" class="text-[#a1a1aa] dark:text-[#52525b]" />
          <template v-if="store.backgroundPattern.value === 'lines'">
            <Background variant="lines" :gap="20" :size="1" pattern-color="currentColor" class="text-black/[0.03] dark:text-white/[0.03]" />
            <Background variant="lines" :gap="100" :size="1" pattern-color="currentColor" class="text-black/[0.08] dark:text-white/[0.08]" />
          </template>
          <!-- The canvas's own buttons are unnamed; these say what they do. -->
          <Controls position="bottom-right" :show-zoom="false" :show-fit-view="false" :show-interactive="false">
            <ControlButton :title="$t('whiteboard.a11y.zoom_in')" :aria-label="$t('whiteboard.a11y.zoom_in')" @click="zoomIn({ duration: motion(150) })"><Plus class="w-3.5 h-3.5" /></ControlButton>
            <ControlButton :title="$t('whiteboard.a11y.zoom_out')" :aria-label="$t('whiteboard.a11y.zoom_out')" @click="zoomOut({ duration: motion(150) })"><Minus class="w-3.5 h-3.5" /></ControlButton>
            <ControlButton :title="$t('whiteboard.a11y.fit')" :aria-label="$t('whiteboard.a11y.fit')" @click="fitAll(motion(300))"><Maximize class="w-3.5 h-3.5" /></ControlButton>
          </Controls>
          <EdgeMarkerDefs :uses="markerUses" />
        </VueFlow>

        <!-- Drawing overlay -->
        <svg
          v-if="store.activeTool.value === 'draw'"
          class="absolute inset-0 w-full h-full z-40 pointer-events-auto"
          @pointerdown="handleCanvasPointerDown"
          @pointermove="handleCanvasPointerMove"
          @pointerup="handleCanvasPointerUp"
          @pointercancel="handleCanvasPointerCancel"
          @pointerleave="handleCanvasPointerLeave"
          style="touch-action: none;"
        >
          <g :transform="`translate(${viewport.x}, ${viewport.y}) scale(${viewport.zoom})`">
            <path
              v-if="freeDrawing.previewPath.value"
              :d="freeDrawing.previewPath.value"
              :style="{ fill: paint(store.activeColor.value) }"
              :opacity="drawingOpacity"
            />
          </g>
        </svg>

        <!-- Alignment guides while dragging -->
        <svg v-if="guides.guides.value.length" class="absolute inset-0 w-full h-full z-40 pointer-events-none" aria-hidden="true">
          <g :transform="`translate(${viewport.x}, ${viewport.y}) scale(${viewport.zoom})`">
            <line
              v-for="(g, i) in guides.guides.value"
              :key="i"
              :x1="g.axis === 'x' ? g.at : g.from"
              :x2="g.axis === 'x' ? g.at : g.to"
              :y1="g.axis === 'x' ? g.from : g.at"
              :y2="g.axis === 'x' ? g.to : g.at"
              class="wb-guide"
              :stroke-width="1 / viewport.zoom"
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

        <GenerateDialog
          v-if="generateDialog"
          :selection-count="arrange.selectedIds().length"
          @submit="generateDiagram"
          @close="generateDialog = false"
        />
        <ShortcutsDialog v-if="showShortcuts" @close="showShortcuts = false" />
        <AltTextDialog
          v-if="altDialog"
          :alt="altDialog.alt"
          @save="(alt: string) => setAlt(altDialog!.id, alt)"
          @close="altDialog = null"
        />
        <LinkDialog
          v-if="linkDialog"
          :link="linkDialog.link"
          @save="(link: string | null) => setLink(linkDialog!.id, link)"
          @close="linkDialog = null"
        />
        <BoardHistory
          v-if="showHistory && store.currentBoardId.value"
          :vault-path="vaultPathRef"
          :path="store.currentBoardId.value"
          @restore="restoreVersion"
          @close="showHistory = false"
        />
        <BoardSearch
          v-if="showSearch && !presenting"
          :nodes="store.currentBoardData.value.nodes"
          :hidden="hiddenItems"
          @show="showFound"
          @close="showSearch = false"
        />
        <MiniMap
          v-if="store.showMinimap.value && !presenting"
          :boxes="minimapBoxes"
          :viewport="viewport"
          :size="dimensions"
          @go="(c: any) => setCenter(c.x, c.y, { zoom: viewport.zoom })"
        />

        <!-- Presenting: the board shown frame by frame, not editable -->
        <div
          v-if="presenting"
          class="wb-present-stage"
          @click="!presentation.laser.value && presentation.next()"
          @contextmenu.prevent
          @wheel.prevent
        >
          <LaserPointer v-if="presentation.laser.value" />
          <PresentationBar
            :index="presentation.index.value"
            :count="presentation.slides.value.length"
            :title="presentation.current.value?.title ?? ''"
            :laser="presentation.laser.value"
            @prev="presentation.prev()"
            @next="presentation.next()"
            @laser="presentation.laser.value = !presentation.laser.value"
            @exit="presentation.stop()"
          />
        </div>

        <!-- Floating Toolbar -->
        <WhiteboardToolbar
          v-if="!presenting"
          :active-tool="store.activeTool.value"
          :can-undo="store.undoStack.value.length > 0"
          :can-redo="store.redoStack.value.length > 0"
          :draw-sub-tool="store.drawSubTool.value"
          :draw-color="store.activeColor.value"
          :draw-size="store.activeStrokeSize.value"
          :background-pattern="store.backgroundPattern.value"
          :background-color="store.backgroundColor.value"
          :snap-to-grid="store.snapToGrid.value"
          :smart-guides="store.smartGuides.value"
          :minimap="store.showMinimap.value"
          @update:minimap="store.showMinimap.value = $event"
          @update:snap-to-grid="store.snapToGrid.value = $event"
          @update:smart-guides="store.smartGuides.value = $event"
          @update:active-tool="store.activeTool.value = $event"
          :libraries="libraries"
          @select-shape="store.activeShapeType.value = $event"
          @pick-icon="pickIcon"
          @pick-item="placeLibraryItem"
          @import-library="importLibrary"
          @remove-library="removeLibrary"
          @update:draw-sub-tool="store.drawSubTool.value = $event"
          @update:draw-color="store.activeColor.value = $event"
          @update:draw-size="store.activeStrokeSize.value = $event"
          @update:background-pattern="store.backgroundPattern.value = $event"
          @update:background-color="store.backgroundColor.value = $event"
          @undo="() => { store.undo(); syncToVueFlow(); scheduleSave(); }"
          @redo="() => { store.redo(); syncToVueFlow(); scheduleSave(); }"
          @export="exportBoard"
          @insert="(el: HTMLElement) => openMenuFrom(el, false)"
          @add-image="pickImages"
          @import="pickDrawing"
          @export-file="exportToApp"
        />

        <!-- Property Menus (Teleported) -->
        <Teleport to="body">
          <EdgeMenu v-if="isAppActive && selectedEdgeId && selectedEdgeData" :edge-id="selectedEdgeId" :edge-data="selectedEdgeData" @update="handleEdgeUpdate" @delete="handleEdgeDelete" @close="closeEdgeMenu" />
        </Teleport>
        <Teleport to="body">
          <ShapeMenu v-if="isAppActive && selectedShapeNodeId && selectedShapeData" :node-id="selectedShapeNodeId" :node-data="selectedShapeData" :can-paste-style="arrange.hasCopiedStyle()" @action="handleItemAction" @change-type="(id: string, type: string) => arrange.changeShapeType(id, type)" @update="handleShapeUpdate" @delete="handleShapeDelete" @close="closeShapeMenu" />
        </Teleport>
        <Teleport to="body">
          <TextMenu v-if="isAppActive && selectedTextNodeId && selectedTextData" :node-id="selectedTextNodeId" :node-data="selectedTextData" :can-paste-style="arrange.hasCopiedStyle()" @action="handleItemAction" @update="handleTextUpdate" @delete="handleTextDelete" @close="closeTextMenu" />
        </Teleport>
        <Teleport to="body">
          <ContextMenu
            v-if="isAppActive && contextMenu"
            :x="contextMenu.x"
            :y="contextMenu.y"
            :items="contextMenu.items"
            :label="$t('whiteboard.ctx.menu_label')"
            @select="handleContextAction"
            @close="contextMenu = null"
          />
        </Teleport>
        <Teleport to="body">
          <MultiSelectMenu v-if="isAppActive && showMultiSelectMenu" :selected-nodes="multiSelectedNodes" @group="handleMultiGroup" @ungroup="handleMultiUngroup" @delete="arrange.removeSelection()" @update-all="handleMultiUpdateAll" @action="handleArrangeAction" @close="closeMultiSelectMenu" />
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
          <button @click="templatePicker = { mode: 'new' }" class="btn-primary">
            {{ $t('whiteboard.new_board') }}
          </button>
        </div>
      </div>
    </div>

    <Teleport to="body">
      <VaultPicker v-if="isAppActive && vaultPickerAt" @pick="handleVaultPick" @close="vaultPickerAt = null" />
    </Teleport>
    <Teleport to="body">
      <LiveFrameDialog v-if="isAppActive && liveDialogAt" @create="handleLiveCreate" @close="liveDialogAt = null" />
    </Teleport>
    <Teleport to="body">
      <TemplatePicker
        v-if="isAppActive && templatePicker"
        :mode="templatePicker.mode"
        @pick="handleTemplatePick"
        @close="templatePicker = null"
      />
    </Teleport>

    <UndoToast
      :show="undoBoardDelete.show.value"
      :restart-key="undoBoardDelete.key.value"
      :message="undoBoardDelete.message.value"
      :hint="$t('common.in_trash_hint')"
      :undo-label="$t('common.undo')"
      :seconds="undoBoardDelete.seconds"
      @undo="undoBoardDelete.undo"
      @pause="undoBoardDelete.pause"
      @resume="undoBoardDelete.resume"
    />
    <UndoToast
      :show="undoCardMove.show.value"
      :restart-key="undoCardMove.key.value"
      :message="undoCardMove.message.value"
      :undo-label="$t('common.undo')"
      :seconds="undoCardMove.seconds"
      @undo="undoCardMove.undo"
      @pause="undoCardMove.pause"
      @resume="undoCardMove.resume"
    />
  </div>
</template>

<style scoped>
/* An item with a link carries a small mark in its corner; Alt-click or the
   menu follows it. */
:deep(.vue-flow__node.wb-has-link)::before {
  content: '';
  position: absolute;
  top: -8px;
  left: -8px;
  z-index: 2;
  width: 16px;
  height: 16px;
  border-radius: 999px;
  background: var(--color-accent) url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='white' stroke-width='2.5' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71'/%3E%3Cpath d='M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71'/%3E%3C/svg%3E") center / 10px no-repeat;
  pointer-events: none;
}
/* Presenting: the canvas over the whole of the app, as the app's other
   overlays are (see `.syn-pane-open` in App.vue), and nothing on it to edit. */
.wb-presenting {
  position: fixed;
  inset: 0;
  z-index: 60;
}
.wb-presenting :deep(.vue-flow__controls),
.wb-presenting :deep(.vue-flow__node-comment) {
  display: none;
}
.wb-present-stage {
  position: absolute;
  inset: 0;
  z-index: 50;
}
/* Override Vue Flow theme for our design system */
:deep(.vue-flow) {
  height: 100% !important;
  /* Selection chrome on the canvas — outlines, resize handles, the selected
     edge. The accent itself is too dark on the dark canvas (2.7:1), so the
     dark theme swaps in its paler twin. Nodes read this with the plain
     accent as fallback. */
  --wb-selection: var(--color-accent);
}
:global(.dark .vue-flow) {
  --wb-selection: var(--color-accent-dark);
}
/* The board's black: dark on the light canvas, light on the dark one (see
   ink.ts). Set on the canvas area rather than the canvas, so the stroke being
   drawn — on an overlay beside the canvas — is painted with it too. */
.wb-canvas {
  --wb-ink: #1e1e1e;
  /* Lines with no colour of their own, and what hollow line ends are filled
     with: the board's grey and its background. */
  --wb-edge: var(--color-muted, #8b8b8b);
  --wb-paper: var(--color-base, #fdfdfc);
}
:global(.dark .wb-canvas) {
  --wb-ink: #e4e4e7;
  --wb-edge: var(--color-muted-dark, #71717a);
  --wb-paper: var(--color-base-dark, #242424);
}
:deep(.vue-flow__pane) {
  cursor: default;
}
:deep(.vue-flow__edge-path) {
  stroke: var(--color-muted, #8b8b8b);
  stroke-width: 2;
}
:deep(.vue-flow__edge-path:is(.dark *)) {
  stroke: var(--color-muted-dark, #71717a);
}
:deep(.vue-flow__controls) {
  border-radius: 10px;
  overflow: hidden;
  border: 1px solid var(--color-border, #e6e6e6);
  box-shadow: 0 2px 8px rgba(0,0,0,0.05);
}
:deep(.vue-flow__controls:is(.dark *)) {
  border-color: var(--color-border-dark, #2c2c2c);
}
:deep(.vue-flow__controls-button) {
  background: var(--color-surface, #fff);
  border: none;
  color: var(--color-text-secondary, #52525b);
}
:deep(.vue-flow__controls-button:is(.dark *)) {
  background: var(--color-surface-dark, #1e1e1e);
  color: var(--color-text-secondary-dark, #a1a1aa);
}
:deep(.vue-flow__controls-button:hover) {
  background: var(--color-surface-hover, #f5f5f5);
}
:deep(.vue-flow__controls-button:hover:is(.dark *)) {
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
:deep(.vue-flow__node:focus) {
  box-shadow: none !important;
  outline: none !important;
  border: none !important;
}
/* Reached by the keyboard, an item says so: a ring the pointer never shows. */
:deep(.vue-flow__node:focus-visible) {
  outline: 2px solid var(--wb-selection) !important;
  outline-offset: 4px;
  border-radius: 4px;
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
.wb-guide {
  stroke: #f43f5e;
  stroke-dasharray: 4 3;
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
  stroke: var(--wb-selection) !important;
  filter: drop-shadow(0 0 3px color-mix(in oklab, var(--wb-selection) 40%, transparent));
}
:deep(.vue-flow__edge-interaction) {
  stroke-width: 20px;
}
:deep(.vue-flow__node-shape),
:deep(.vue-flow__node-stroke),
:deep(.vue-flow__node-frame) {
  pointer-events: none !important;
}
:deep(.vue-flow__node-shape .vue-flow__resize-control) {
  pointer-events: auto !important;
}
:deep(.vue-flow__resize-control.line) {
  display: none !important;
}
/* A locked item, when selected, says so: a small padlock at its corner. It
   cannot be dragged, so it does not offer the grab cursor either. */
:deep(.vue-flow__node.selected.wb-locked)::after {
  content: '';
  position: absolute;
  top: -10px;
  right: -10px;
  width: 18px;
  height: 18px;
  border-radius: 50%;
  background: var(--wb-selection) center / 11px 11px no-repeat
    url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' stroke='white' stroke-width='2.5' stroke-linecap='round' stroke-linejoin='round'%3E%3Crect x='4' y='11' width='16' height='10' rx='2'/%3E%3Cpath d='M8 11V7a4 4 0 0 1 8 0v4'/%3E%3C/svg%3E");
  pointer-events: none;
  z-index: 30;
}
:deep(.vue-flow__node.wb-locked),
:deep(.vue-flow__node.wb-locked *) {
  cursor: default !important;
}
</style>
