<script lang="ts">
import type { ShapeDef } from '../../whiteboard/shapes';

/**
 * Where a shape's path is drawn, as an SVG transform: the same mapping the
 * board makes. Shape paths are written on a 0–100 square with their outline
 * from 2 to 98, and the board draws them in `viewBox="2 2 96 96"` stretched
 * to the item — so the outline touches the item's edges. Scaling 0–100 here
 * left every embedded shape about 4% small and nudged inward.
 */
export function shapeTransform(x: number, y: number, w: number, h: number): string {
  return `translate(${x}, ${y}) scale(${w / 96}, ${h / 96}) translate(-2, -2)`;
}

/**
 * The middle of where a shape's words go, as the board places them: inside
 * its `labelBox` (insets top, right, bottom, left, in percent of the item)
 * when it has one — a UML class's name in its top compartment — and the
 * middle of the item otherwise.
 */
export function shapeLabelCenter(
  def: Pick<ShapeDef, 'labelBox'> | undefined,
  x: number, y: number, w: number, h: number,
): { x: number; y: number } {
  const [top, right, bottom, left] = def?.labelBox ?? [0, 0, 0, 0];
  return {
    x: x + (w * (left + (100 - left - right) / 2)) / 100,
    y: y + (h * (top + (100 - top - bottom) / 2)) / 100,
  };
}
</script>

<script setup lang="ts">
import { ref, shallowRef, computed, defineAsyncComponent, onMounted, onUnmounted, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { NodeViewWrapper } from '@tiptap/vue-3';
import {
  PenTool, ExternalLink, Trash2,
  AlignLeft, AlignCenter, AlignRight, Pencil,
} from 'lucide-vue-next';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { SHAPES_MAP } from '../../whiteboard/shapes';
import { cleanGlyph, GLYPH_SHAPE } from '../../whiteboard/glyph';
import { readBoardFile } from '../../whiteboard/boardFile';
import { paint } from '../../whiteboard/ink';
import { stickyColor } from '../../whiteboard/sticky';
import { isMarker } from '../../whiteboard/edgeMarkers';
import { hiddenByCollapse } from '../../whiteboard/mindmap';
import { renderRichText } from '../../whiteboard/richText';
import { assetUrl, rotatedOverhang } from '../../whiteboard/imageAssets';
import type { WBNode, WhiteboardData } from '../../whiteboard/boardFile';
import { logger } from '../../../utils/logger';

// The editing canvas loads only when somebody edits a board here.
const InlineBoardEditor = defineAsyncComponent(() => import('../../whiteboard/components/InlineBoardEditor.vue'));

const props = defineProps<{
  node: any;
  updateAttributes: (attrs: Record<string, any>) => void;
  deleteNode: () => void;
  getPos: () => number;
  editor: any;
  selected: boolean;
}>();

const { t } = useI18n();

// Inject vaultPath from editor storage (set by TiptapEditor)
const vaultPath = computed(() => props.editor?.storage?.whiteboard?.vaultPath || '');

// --- Board data ---
// The same reader the whiteboard app uses. This preview had its own copy of
// the file's shape, which had already drifted — it did not know about note
// cards — and its own `JSON.parse`, which meant a board from a newer build
// rendered here as whichever parts happened to still be recognisable.
type BoardData = WhiteboardData;

// Shallow: the board is replaced whole on every reload and only read here,
// and a deep ref turned every point of every stroke into a proxy.
const boardData = shallowRef<BoardData | null>(null);
/** Whether the board is being edited here, in the note, rather than shown. */
const editingHere = ref(false);

/**
 * What the board shows: everything but what a folded mind-map branch hides,
 * drawn in stacking order (a frame under what it holds, an item brought to
 * the front over the rest), as the board itself draws it.
 */
const visibleNodes = computed<WBNode[]>(() => {
  const data = boardData.value;
  if (!data) return [];
  const hidden = hiddenByCollapse(data.nodes, data.edges);
  const z = (n: WBNode) => (typeof n.data?.z === 'number' ? n.data.z : n.type === 'frame' ? -100_000 : 0);
  // Comments are for the people working on the board, not for its readers.
  return data.nodes.filter((n) => !hidden.has(n.id) && n.type !== 'comment').sort((a, b) => z(a) - z(b));
});

/** An item's turn, about its own middle — as the board turns it. */
function turn(node: WBNode, w: number, h: number): string | undefined {
  const deg = node.data?.rotation;
  return deg ? `rotate(${deg}, ${node.position.x + w / 2}, ${node.position.y + h / 2})` : undefined;
}
const loading = ref(true);
const error = ref('');

const blockWidth = computed(() => props.node.attrs.width || '100%');
const blockHeight = computed(() => props.node.attrs.height || '240px');
const blockAlign = computed(() => props.node.attrs.align || 'center');

const alignStyle = computed(() => {
  switch (blockAlign.value) {
    case 'left': return { marginRight: 'auto', marginLeft: '0' };
    case 'right': return { marginLeft: 'auto', marginRight: '0' };
    default: return { marginLeft: 'auto', marginRight: 'auto' };
  }
});

// --- Load whiteboard data ---
/** `quiet`: a reload of a board already shown, without the spinner in between. */
const loadBoard = async (quiet = false) => {
  const path = props.node.attrs.boardPath;
  const vp = vaultPath.value;
  if (!path || !vp) {
    error.value = t('note.editor.whiteboard.load_failed');
    loading.value = false;
    return;
  }
  try {
    if (!quiet || !boardData.value) loading.value = true;
    error.value = '';
    const raw = await invoke<string>('read_whiteboard', { vaultPath: vp, path });
    const read = readBoardFile(raw);
    if (!read.ok) {
      boardData.value = null;
      error.value =
        read.reason === 'too-new'
          ? t('note.editor.whiteboard.too_new')
          : t('note.editor.whiteboard.load_failed');
      return;
    }
    boardData.value = read.data;
  } catch (e: any) {
    error.value = t('note.editor.whiteboard.load_failed');
    logger.error('WhiteboardNodeView: load failed', e);
  } finally {
    loading.value = false;
  }
};

onMounted(() => loadBoard());

// Reload when boardPath changes
watch(() => props.node.attrs.boardPath, () => loadBoard());

// A burst of saves (a drag in the app writes every two seconds) is one reload.
// While the board is edited here, not at all: the editor is what is on
// screen, its own saves are among the burst, and reloading would rebuild it
// under the pointer. It is read again when editing ends.
let reloadTimer: ReturnType<typeof setTimeout> | null = null;
function reloadSoon() {
  if (editingHere.value) return;
  if (reloadTimer) clearTimeout(reloadTimer);
  reloadTimer = setTimeout(() => { reloadTimer = null; void loadBoard(true); }, 300);
}
watch(editingHere, (editing) => { if (!editing) void loadBoard(true); });

// Auto-reload when whiteboard is updated in the Whiteboard app
let unlistenWbUpdate: (() => void) | null = null;

onMounted(async () => {
  unlistenWbUpdate = await listen<{ path: string; id: string }>('whiteboard-updated', (event) => {
    const boardPath = props.node.attrs.boardPath;
    const boardId = props.node.attrs.boardId;
    if (event.payload.path === boardPath || event.payload.id === boardId) {
      reloadSoon();
    }
  });
});

onUnmounted(() => {
  if (unlistenWbUpdate) unlistenWbUpdate();
  if (reloadTimer) clearTimeout(reloadTimer);
});

// --- Mindmap node dimensions ---
function getMindmapWidth(node: WBNode): number {
  const label = node.data.label || t('whiteboard.idea');
  const fontSize = node.data.level === 0 ? 15 : 13;
  const minW = node.data.level === 0 ? 140 : 100;
  // Approximate text width: ~0.6 * fontSize per character + padding
  const textW = label.length * fontSize * 0.6 + 32;
  return Math.max(minW, textW);
}

function getMindmapHeight(_node: WBNode): number {
  return 40;
}

// --- Text node helpers ---
function getTextLines(node: WBNode): string[] {
  const label = node.data.label || '';
  const fontSize = node.data.fontSize || 16;
  const nodeWidth = node.data.width || 200;
  // Available text width inside the node (account for padding 8px each side)
  const availW = nodeWidth - 16;
  // Approximate char width: ~0.55 * fontSize for proportional fonts
  const charW = fontSize * 0.55;
  const maxCharsPerLine = Math.max(1, Math.floor(availW / charW));

  const result: string[] = [];
  // First split by actual newlines
  const paragraphs = label.split('\n');
  for (const para of paragraphs) {
    if (para.length <= maxCharsPerLine) {
      result.push(para || ' ');
      continue;
    }
    // Word-wrap within the paragraph
    const words = para.split(' ');
    let currentLine = '';
    for (const word of words) {
      const testLine = currentLine ? currentLine + ' ' + word : word;
      if (testLine.length > maxCharsPerLine && currentLine) {
        result.push(currentLine);
        currentLine = word;
      } else {
        currentLine = testLine;
      }
    }
    if (currentLine) result.push(currentLine);
  }
  return result.length > 0 ? result : [' '];
}

function getTextNodeWidth(node: WBNode): number {
  return node.data.width || 200;
}

function getTextNodeHeight(node: WBNode): number {
  // Estimate height for foreignObject: calculate how many lines the text wraps to
  const label = node.data.label || '';
  const fontSize = node.data.fontSize || 16;
  const nodeWidth = node.data.width || 200;
  const availW = nodeWidth - 24; // padding 12px each side
  const charW = fontSize * 0.5;
  const charsPerLine = Math.max(1, Math.floor(availW / charW));
  // Count explicit newlines + word-wrapped lines
  const paragraphs = label.split('\n');
  let totalLines = 0;
  for (const para of paragraphs) {
    totalLines += Math.max(1, Math.ceil(para.length / charsPerLine));
  }
  const lineHeight = fontSize * 1.4;
  return Math.max(32, totalLines * lineHeight + 20);
}

// --- Get accurate node bounds (position + size) ---
function getNodeBounds(node: WBNode): { x: number; y: number; w: number; h: number } {
  const x = node.position.x;
  const y = node.position.y;
  if (node.type === 'mindmap') {
    return { x, y, w: getMindmapWidth(node), h: getMindmapHeight(node) };
  }
  if (node.type === 'shape') {
    const def = SHAPES_MAP[node.data.shapeType] || SHAPES_MAP['rectangle'];
    return {
      x, y,
      w: node.data.width || def?.defaultWidth || 160,
      h: node.data.height || def?.defaultHeight || 80,
    };
  }
  if (node.type === 'text') {
    return { x, y, w: getTextNodeWidth(node), h: getTextNodeHeight(node) };
  }
  if (node.type === 'image' || node.type === 'note') {
    const w = node.data.width || 320;
    const h = node.data.height || 240;
    // A turned picture reaches past its box, and the preview is cropped to
    // these bounds.
    const over = node.type === 'image' ? rotatedOverhang(w, h, node.data.rotation || 0) : 0;
    return { x: x - over, y: y - over, w: w + over * 2, h: h + over * 2 };
  }
  if (node.type === 'sticky') {
    return { x, y, w: node.data.width || 200, h: node.data.height || 200 };
  }
  if (node.type === 'card') {
    return { x, y, w: node.data.width || 260, h: node.data.height || 120 };
  }
  if (node.type === 'frame') {
    // Its title sits above it.
    return { x, y: y - 24, w: node.data.width || 480, h: (node.data.height || 320) + 24 };
  }
  // A stroke carries its box; one drawn before that is sized by its points.
  if (node.data.width && node.data.height) return { x, y, w: node.data.width, h: node.data.height };
  return { x, y, w: 100, h: 100 };
}

// --- SVG preview computation ---
const svgViewBox = computed(() => {
  if (!boardData.value || boardData.value.nodes.length === 0) return '0 0 400 240';

  let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;

  for (const node of boardData.value.nodes) {
    const b = getNodeBounds(node);
    minX = Math.min(minX, b.x);
    minY = Math.min(minY, b.y);
    maxX = Math.max(maxX, b.x + b.w);
    maxY = Math.max(maxY, b.y + b.h);
  }

  // Add padding
  const pad = 40;
  minX -= pad;
  minY -= pad;
  maxX += pad;
  maxY += pad;

  return `${minX} ${minY} ${maxX - minX} ${maxY - minY}`;
});

// --- Computed edges with bezier paths ---
interface ComputedEdge {
  id: string;
  path: string;
  color: string;
  strokeWidth: number;
  dashArray: string;
  animated: boolean;
  markerEnd: boolean;
  markerStart: boolean;
}

const computedEdges = computed<ComputedEdge[]>(() => {
  if (!boardData.value) return [];
  // Only between items that are showing: a folded branch takes its lines with it.
  const nodeMap = new Map<string, WBNode>();
  for (const n of visibleNodes.value) nodeMap.set(n.id, n);

  return boardData.value.edges.map(edge => {
    const src = nodeMap.get(edge.source);
    const tgt = nodeMap.get(edge.target);
    if (!src || !tgt) return null;

    const sb = getNodeBounds(src);
    const tb = getNodeBounds(tgt);

    // Determine anchor points based on sourceHandle / targetHandle
    const srcHandle = edge.sourceHandle || '';
    const tgtHandle = edge.targetHandle || '';

    let sx: number, sy: number, tx: number, ty: number;

    // Source anchor
    if (srcHandle.startsWith('left')) {
      sx = sb.x;
      sy = sb.y + sb.h / 2;
    } else if (srcHandle.startsWith('right')) {
      sx = sb.x + sb.w;
      sy = sb.y + sb.h / 2;
    } else {
      // Auto: connect from closest side
      const srcCx = sb.x + sb.w / 2;
      const tgtCx = tb.x + tb.w / 2;
      if (tgtCx > srcCx) {
        sx = sb.x + sb.w;
        sy = sb.y + sb.h / 2;
      } else {
        sx = sb.x;
        sy = sb.y + sb.h / 2;
      }
    }

    // Target anchor
    if (tgtHandle.startsWith('left')) {
      tx = tb.x;
      ty = tb.y + tb.h / 2;
    } else if (tgtHandle.startsWith('right')) {
      tx = tb.x + tb.w;
      ty = tb.y + tb.h / 2;
    } else {
      const srcCx = sb.x + sb.w / 2;
      const tgtCx = tb.x + tb.w / 2;
      if (srcCx > tgtCx) {
        tx = tb.x + tb.w;
        ty = tb.y + tb.h / 2;
      } else {
        tx = tb.x;
        ty = tb.y + tb.h / 2;
      }
    }

    // Bezier control point offset (matches VueFlow default edge)
    const dx = Math.abs(tx - sx);
    const cpOffset = Math.max(dx * 0.5, 50);

    // Determine control point direction based on which side the anchor is on
    const srcIsLeft = (srcHandle.startsWith('left') || (!srcHandle && sx === sb.x));
    const tgtIsLeft = (tgtHandle.startsWith('left') || (!tgtHandle && tx === tb.x));

    const cpx1 = srcIsLeft ? sx - cpOffset : sx + cpOffset;
    const cpy1 = sy;
    const cpx2 = tgtIsLeft ? tx - cpOffset : tx + cpOffset;
    const cpy2 = ty;

    const path = `M ${sx} ${sy} C ${cpx1} ${cpy1}, ${cpx2} ${cpy2}, ${tx} ${ty}`;

    const d = edge.data || {};
    const dashStyle = d.dashStyle || 'solid';
    let dashArray = 'none';
    if (d.animated) dashArray = '8 4';
    else if (dashStyle === 'dashed') dashArray = '8 4';
    else if (dashStyle === 'dotted') dashArray = '2 4';

    return {
      id: edge.id,
      path,
      color: paint(d.color) || '#94a3b8',
      strokeWidth: d.strokeWidth || 2,
      dashArray,
      animated: !!d.animated,
      // Any end the board draws shows here as an arrow: at this size the
      // difference between a diamond and a crow's foot is not readable.
      markerEnd: isMarker(d.markerEnd),
      markerStart: isMarker(d.markerStart),
    };
  }).filter(Boolean) as ComputedEdge[];
});

// --- Node count ---
const nodeCount = computed(() => boardData.value?.nodes.length || 0);

// --- Alignment ---
const setAlign = (align: 'left' | 'center' | 'right') => {
  props.updateAttributes({ align });
};

// --- Open in whiteboard app ---
const openInApp = () => {
  // Emit event through the editor to navigate to whiteboard
  const boardId = props.node.attrs.boardId;
  if (props.editor) {
    props.editor.commands.focus();
    // Use the same event pattern as synabit:// links
    const event = new CustomEvent('open-whiteboard-embed', {
      detail: { id: boardId, type: 'whiteboard' },
      bubbles: true,
    });
    props.editor.view.dom.dispatchEvent(event);
  }
};

// --- Select node ---
const selectNode = () => {
  const pos = props.getPos();
  if (pos != null && props.editor) {
    props.editor.commands.setNodeSelection(pos);
  }
};

// --- Resize ---
const resizing = ref(false);
const blockRef = ref<HTMLElement | null>(null);

const onResizeWidth = (e: MouseEvent, side: 'left' | 'right') => {
  e.preventDefault();
  e.stopPropagation();
  resizing.value = true;

  const startX = e.clientX;
  const container = blockRef.value;
  if (!container) return;

  const parentWidth = container.parentElement?.clientWidth || container.clientWidth;
  const startW = container.clientWidth;

  const onMove = (ev: MouseEvent) => {
    const dx = side === 'right' ? ev.clientX - startX : startX - ev.clientX;
    const factor = blockAlign.value === 'center' ? 2 : 1;
    const newW = Math.max(200, Math.min(parentWidth, startW + dx * factor));
    const pct = Math.round((newW / parentWidth) * 100);
    props.updateAttributes({ width: `${pct}%` });
  };

  const onUp = () => {
    resizing.value = false;
    document.removeEventListener('mousemove', onMove);
    document.removeEventListener('mouseup', onUp);
  };

  document.addEventListener('mousemove', onMove);
  document.addEventListener('mouseup', onUp);
};

const onResizeHeight = (e: MouseEvent) => {
  e.preventDefault();
  e.stopPropagation();
  resizing.value = true;

  const startY = e.clientY;
  const previewEl = blockRef.value?.querySelector('.wb-embed-preview') as HTMLElement;
  if (!previewEl) return;
  const startH = previewEl.clientHeight;

  const onMove = (ev: MouseEvent) => {
    const dy = ev.clientY - startY;
    const newH = Math.max(120, Math.min(600, startH + dy));
    props.updateAttributes({ height: `${newH}px` });
  };

  const onUp = () => {
    resizing.value = false;
    document.removeEventListener('mousemove', onMove);
    document.removeEventListener('mouseup', onUp);
  };

  document.addEventListener('mousemove', onMove);
  document.addEventListener('mouseup', onUp);
};

/** An icon's drawing, checked; null for any other shape. */
function glyphOf(node: WBNode) {
  return node.data.shapeType === GLYPH_SHAPE ? cleanGlyph(node.data.glyph) : null;
}
/** Figures and icons have their words under them. */
function labelUnder(node: WBNode): boolean {
  return node.data.shapeType === GLYPH_SHAPE || !!SHAPES_MAP[node.data.shapeType]?.labelBelow;
}

// Render SVG shape path scaled to actual node position/size
function getShapeTransform(node: WBNode): string {
  const def = SHAPES_MAP[node.data.shapeType] || SHAPES_MAP['rectangle'];
  const w = node.data.width || def?.defaultWidth || 160;
  const h = node.data.height || def?.defaultHeight || 80;
  return shapeTransform(node.position.x, node.position.y, w, h);
}

/** Where a shape's label sits: under a figure or icon, else in its label box. */
function shapeLabelAt(node: WBNode): { x: number; y: number } {
  const def = SHAPES_MAP[node.data.shapeType];
  const under = labelUnder(node);
  const w = node.data.width || def?.defaultWidth || 160;
  const h = node.data.height || def?.defaultHeight || (under ? 64 : 80);
  if (under) return { x: node.position.x + w / 2, y: node.position.y + h + 6 };
  return shapeLabelCenter(def, node.position.x, node.position.y, w, h);
}

</script>

<template>
  <NodeViewWrapper
    class="wb-embed-wrapper"
    :class="[
      { 'is-selected': selected, 'is-resizing': resizing },
      `wb-align-${blockAlign}`
    ]"
  >
    <div
      ref="blockRef"
      class="wb-embed-container"
      :style="{ width: blockWidth, ...alignStyle }"
    >
      <!-- Resize handle LEFT -->
      <div
        v-if="selected"
        class="wb-resize-handle wb-resize-left"
        @mousedown="(e: MouseEvent) => onResizeWidth(e, 'left')"
      >
        <div class="wb-resize-bar" />
      </div>

      <!-- Resize handle RIGHT -->
      <div
        v-if="selected"
        class="wb-resize-handle wb-resize-right"
        @mousedown="(e: MouseEvent) => onResizeWidth(e, 'right')"
      >
        <div class="wb-resize-bar" />
      </div>

      <!-- Resize handle BOTTOM -->
      <div
        v-if="selected"
        class="wb-resize-handle wb-resize-bottom"
        @mousedown="onResizeHeight"
      >
        <div class="wb-resize-bar-h" />
      </div>

      <!-- SVG Preview -->
      <div class="wb-embed-preview" :style="{ height: blockHeight }" @click="selectNode">
        <!-- Loading -->
        <div v-if="loading" class="wb-embed-loading">
          <div class="wb-loading-spinner" />
          <span>{{ $t('note.editor.whiteboard.loading') }}</span>
        </div>

        <!-- Error -->
        <div v-else-if="error" class="wb-embed-error">
          <PenTool class="w-6 h-6 opacity-40" />
          <span>{{ error }}</span>
        </div>

        <!-- Empty board -->
        <div v-else-if="!boardData || boardData.nodes.length === 0" class="wb-embed-empty">
          <PenTool class="w-8 h-8 opacity-30" />
          <span>{{ $t('note.editor.whiteboard.empty') }}</span>
        </div>

        <!-- Edited where it is: see InlineBoardEditor -->
        <InlineBoardEditor
          v-else-if="editingHere && vaultPath"
          :vault-path="vaultPath"
          :path="node.attrs.boardPath"
          :board="boardData"
          @done="editingHere = false"
        />

        <!-- SVG Render -->
        <svg
          v-else
          class="wb-embed-svg"
          :viewBox="svgViewBox"
          preserveAspectRatio="xMidYMid meet"
        >
          <!-- Arrow marker definitions -->
          <defs>
            <marker
              v-for="edge in computedEdges.filter(e => e.markerEnd || e.markerStart)"
              :key="'marker-' + edge.id"
              :id="'arrow-' + edge.id"
              markerWidth="12"
              markerHeight="12"
              refX="10"
              refY="6"
              orient="auto"
              markerUnits="userSpaceOnUse"
            >
              <path d="M 0 0 L 12 6 L 0 12 Z" :style="{ fill: edge.color }" />
            </marker>
          </defs>

          <!-- Frames, behind everything they hold -->
          <template v-for="node in visibleNodes.filter(n => n.type === 'frame')" :key="node.id">
            <rect
              :x="node.position.x"
              :y="node.position.y"
              :width="node.data.width || 480"
              :height="node.data.height || 320"
              rx="10"
              class="wb-embed-frame"
            />
            <text
              :x="node.position.x + 2"
              :y="node.position.y - 8"
              font-size="13"
              font-weight="600"
              font-family="Inter, system-ui, sans-serif"
              fill="currentColor"
              class="wb-svg-text"
            >{{ node.data.label || $t('whiteboard.frame') }}</text>
          </template>

          <!-- Edges (bezier curves with dash/animation/arrow support) -->
          <path
            v-for="edge in computedEdges"
            :key="edge.id"
            :d="edge.path"
            fill="none"
            :style="{ stroke: edge.color }"
            :stroke-width="edge.strokeWidth"
            stroke-linecap="round"
            :stroke-dasharray="edge.dashArray"
            :marker-end="edge.markerEnd ? `url(#arrow-${edge.id})` : undefined"
            :marker-start="edge.markerStart ? `url(#arrow-${edge.id})` : undefined"
            :class="{ 'wb-edge-animated': edge.animated }"
          />

          <!-- Shape Nodes -->
          <template v-for="node in visibleNodes.filter(n => n.type === 'shape')" :key="node.id">
            <g :transform="turn(node, node.data.width || SHAPES_MAP[node.data.shapeType]?.defaultWidth || 160, node.data.height || SHAPES_MAP[node.data.shapeType]?.defaultHeight || 80)">
            <svg
              v-if="glyphOf(node)"
              :x="node.position.x" :y="node.position.y"
              :width="node.data.width || 64" :height="node.data.height || 64"
              :viewBox="glyphOf(node)!.viewBox.join(' ')"
              fill="none"
              :style="{ stroke: paint(node.data.color) || '#7c3aed' }"
              :stroke-width="node.data.borderWidth || 2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <component :is="part[0]" v-for="(part, pi) in glyphOf(node)!.parts" :key="pi" v-bind="part[1]" />
            </svg>
            <g v-else :transform="getShapeTransform(node)">
              <path
                :d="(SHAPES_MAP[node.data.shapeType] || SHAPES_MAP['rectangle']).path"
                :style="{ fill: paint(node.data.fillColor) || 'none', stroke: paint(node.data.color) || '#7c3aed' }"
                :stroke-width="node.data.borderWidth || 2"
                vector-effect="non-scaling-stroke"
                stroke-linejoin="round"
                fill-rule="evenodd"
              />
              <!-- Decoration paths -->
              <path
                v-for="(deco, di) in ((SHAPES_MAP[node.data.shapeType] || SHAPES_MAP['rectangle']).deco || [])"
                :key="di"
                :d="deco"
                fill="none"
                :style="{ stroke: paint(node.data.color) || '#7c3aed' }"
                :stroke-width="node.data.borderWidth || 2"
                vector-effect="non-scaling-stroke"
                stroke-linejoin="round"
              />
            </g>
            <!-- Label -->
            <text
              v-if="node.data.label"
              :x="shapeLabelAt(node).x"
              :y="shapeLabelAt(node).y"
              text-anchor="middle"
              :dominant-baseline="labelUnder(node) ? 'hanging' : 'central'"
              :font-size="node.data.fontSize || 13"
              font-family="Inter, system-ui, sans-serif"
              fill="currentColor"
              class="wb-svg-text"
            >{{ node.data.label }}</text>
            </g>
          </template>

          <!-- Note cards: the note's title on a card -->
          <template v-for="node in visibleNodes.filter(n => n.type === 'note' || n.type === 'card')" :key="node.id">
            <rect
              :x="node.position.x"
              :y="node.position.y"
              :width="node.data.width || 280"
              :height="node.data.height || 180"
              rx="12"
              class="wb-embed-note"
            />
            <text
              :x="node.position.x + 16"
              :y="node.position.y + 28"
              font-size="14"
              font-weight="600"
              font-family="Inter, system-ui, sans-serif"
              fill="currentColor"
              class="wb-svg-text"
            >{{ node.data.noteTitle || node.data.title || '' }}</text>
          </template>

          <!-- Image Nodes -->
          <template v-for="node in visibleNodes.filter(n => n.type === 'image')" :key="node.id">
            <image
              v-if="node.data.assetPath && vaultPath"
              :x="node.position.x"
              :y="node.position.y"
              :width="node.data.width || 320"
              :height="node.data.height || 240"
              :href="assetUrl(vaultPath, node.data.assetPath)"
              :transform="node.data.rotation
                ? `rotate(${node.data.rotation}, ${node.position.x + (node.data.width || 320) / 2}, ${node.position.y + (node.data.height || 240) / 2})`
                : undefined"
              preserveAspectRatio="xMidYMid meet"
            />
          </template>

          <!-- Stroke Nodes (freehand drawings) -->
          <template v-for="node in visibleNodes.filter(n => n.type === 'stroke')" :key="node.id">
            <path
              v-if="node.data.svgPath"
              :d="node.data.svgPath"
              :style="{ fill: paint(node.data.color) || 'var(--wb-ink)' }"
              :opacity="node.data.opacity ?? 0.85"
              :transform="`translate(${node.position.x}, ${node.position.y})`"
            />
          </template>

          <!-- Sticky notes: their paper, their words -->
          <template v-for="node in visibleNodes.filter(n => n.type === 'sticky')" :key="node.id">
            <g :transform="turn(node, node.data.width || 200, node.data.height || 200)">
            <rect
              :x="node.position.x"
              :y="node.position.y"
              :width="node.data.width || 200"
              :height="node.data.height || 200"
              rx="3"
              :fill="stickyColor(node.data.color).fill"
            />
            <foreignObject :x="node.position.x" :y="node.position.y" :width="node.data.width || 200" :height="node.data.height || 200">
              <div
                xmlns="http://www.w3.org/1999/xhtml"
                class="wb-embed-sticky"
              >{{ node.data.label || '' }}</div>
            </foreignObject>
            </g>
          </template>

          <!-- Text Nodes (foreignObject for native CSS word-wrap) -->
          <template v-for="node in visibleNodes.filter(n => n.type === 'text')" :key="node.id">
            <foreignObject
              :transform="turn(node, getTextNodeWidth(node), getTextNodeHeight(node))"
              :x="node.position.x"
              :y="node.position.y"
              :width="getTextNodeWidth(node)"
              :height="getTextNodeHeight(node)"
            >
              <div
                xmlns="http://www.w3.org/1999/xhtml"
                :style="{
                  width: '100%',
                  height: '100%',
                  padding: '8px 12px',
                  borderRadius: '8px',
                  backgroundColor: node.data.backgroundColor || 'transparent',
                  // A percentage, as the board stores it.
                  opacity: (node.data.opacity ?? 100) / 100,
                  fontSize: (node.data.fontSize || 16) + 'px',
                  fontWeight: node.data.fontWeight || 'normal',
                  fontStyle: node.data.fontStyle || 'normal',
                  color: paint(node.data.color) || 'inherit',
                  fontFamily: 'Inter, system-ui, sans-serif',
                  whiteSpace: 'pre-wrap',
                  wordBreak: 'break-word',
                  overflowWrap: 'break-word',
                  boxSizing: 'border-box',
                  lineHeight: '1.4',
                }"
                class="wb-embed-rich"
                v-html="renderRichText(node.data.label || '')"
              />
            </foreignObject>
          </template>

          <!-- Mindmap Nodes (pill-shaped with border + light fill) -->
          <template v-for="node in visibleNodes.filter(n => n.type === 'mindmap')" :key="node.id">
            <rect
              :x="node.position.x"
              :y="node.position.y"
              :width="getMindmapWidth(node)"
              :height="getMindmapHeight(node)"
              :rx="getMindmapHeight(node) / 2"
              :ry="getMindmapHeight(node) / 2"
              :fill="(node.data.color || '#7c3aed') + '12'"
              :stroke="node.data.color || '#7c3aed'"
              stroke-width="2"
            />
            <text
              :x="node.position.x + getMindmapWidth(node) / 2"
              :y="node.position.y + getMindmapHeight(node) / 2"
              text-anchor="middle"
              dominant-baseline="central"
              :font-size="node.data.level === 0 ? 15 : 13"
              font-weight="600"
              font-family="Inter, system-ui, sans-serif"
              fill="currentColor"
              class="wb-svg-text"
            >{{ node.data.label || t('whiteboard.idea') }}</text>
          </template>
        </svg>
      </div>

      <!-- Bubble toolbar -->
      <Transition name="wb-bubble">
        <div v-if="selected" class="wb-embed-bubble" @mousedown.prevent>
          <!-- Alignment -->
          <button @click="setAlign('left')" :title="$t('note.editor.align_left')" class="wb-bubble-btn" :class="{ 'wb-bubble-active': blockAlign === 'left' }">
            <AlignLeft class="w-3.5 h-3.5" />
          </button>
          <button @click="setAlign('center')" :title="$t('note.editor.align_center')" class="wb-bubble-btn" :class="{ 'wb-bubble-active': blockAlign === 'center' }">
            <AlignCenter class="w-3.5 h-3.5" />
          </button>
          <button @click="setAlign('right')" :title="$t('note.editor.align_right')" class="wb-bubble-btn" :class="{ 'wb-bubble-active': blockAlign === 'right' }">
            <AlignRight class="w-3.5 h-3.5" />
          </button>
          <div class="wb-bubble-sep" />
          <button
            v-if="boardData && boardData.nodes.length && !editingHere"
            @click="editingHere = true"
            :title="$t('whiteboard.inline.edit')"
            :aria-label="$t('whiteboard.inline.edit')"
            class="wb-bubble-btn"
          >
            <Pencil class="w-3.5 h-3.5" />
          </button>
          <button @click="openInApp" :title="$t('note.editor.whiteboard.open')" class="wb-bubble-btn">
            <ExternalLink class="w-3.5 h-3.5" />
          </button>
          <div class="wb-bubble-sep" />
          <button @click="deleteNode" :title="$t('note.editor.remove')" class="wb-bubble-btn wb-bubble-danger">
            <Trash2 class="w-3.5 h-3.5" />
          </button>
        </div>
      </Transition>

      <!-- Info bar -->
      <div class="wb-embed-info">
        <div class="wb-embed-label">
          <PenTool class="w-3.5 h-3.5 text-violet-500 flex-shrink-0" />
          <span class="wb-embed-name">{{ node.attrs.title || $t('note.editor.whiteboard.untitled') }}</span>
        </div>
        <div class="wb-embed-meta">
          <span v-if="nodeCount > 0" class="wb-embed-count">{{ $t('note.editor.whiteboard.node_count', { count: nodeCount }, nodeCount) }}</span>
          <span class="wb-embed-badge">{{ $t('note.editor.whiteboard.badge') }}</span>
        </div>
      </div>
    </div>
  </NodeViewWrapper>
</template>

<style>
/* ═══ Wrapper ═══ */
.wb-embed-wrapper {
  margin: 12px 0;
}

.wb-embed-container {
  border-radius: 12px;
  overflow: visible;
  border: 1px solid #e5e7eb;
  background: #fafbfc;
  transition: box-shadow 0.2s, border-color 0.2s;
  position: relative;
}

.wb-embed-wrapper.is-selected .wb-embed-container {
  border-color: var(--color-accent);
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-accent) 15%, transparent);
}

.dark .wb-embed-container {
  border-color: #333;
  background: #1a1a1e;
}

.wb-embed-note {
  fill: var(--color-surface, #fff);
  stroke: rgba(148, 163, 184, 0.8);
  stroke-width: 1;
}
.dark .wb-embed-note {
  fill: var(--color-surface-dark, #1e1e1e);
}
.wb-embed-rich p { margin: 0; }
.wb-embed-rich ul { list-style: disc; padding-left: 1.2em; margin: 0.2em 0; }
.wb-embed-rich ol { list-style: decimal; padding-left: 1.4em; margin: 0.2em 0; }
.wb-embed-rich h1 { font-size: 1.6em; font-weight: 700; margin: 0.1em 0; }
.wb-embed-rich h2 { font-size: 1.35em; font-weight: 700; margin: 0.1em 0; }
.wb-embed-rich h3 { font-size: 1.15em; font-weight: 600; margin: 0.1em 0; }
.wb-embed-frame {
  fill: rgba(148, 163, 184, 0.08);
  stroke: rgba(148, 163, 184, 0.8);
  stroke-width: 1.5;
}
.wb-embed-sticky {
  width: 100%;
  height: 100%;
  padding: 14px;
  display: flex;
  align-items: center;
  justify-content: center;
  text-align: center;
  color: #1f2937;
  font: 500 18px/1.25 Inter, system-ui, sans-serif;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  overflow: hidden;
  box-sizing: border-box;
}

/* The board's black, as the Whiteboard app paints it (see whiteboard/ink.ts):
   dark on the light theme, light on the dark one. */
.wb-embed-container {
  --wb-ink: #1e1e1e;
}
.dark .wb-embed-container {
  --wb-ink: #e4e4e7;
}

.dark .wb-embed-wrapper.is-selected .wb-embed-container {
  border-color: var(--color-accent-dark);
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-accent-dark) 15%, transparent);
}

/* Disable user-select during resize */
.wb-embed-wrapper.is-resizing * {
  user-select: none !important;
  pointer-events: none !important;
}
.wb-embed-wrapper.is-resizing .wb-resize-handle {
  pointer-events: auto !important;
}

/* ═══ Preview ═══ */
.wb-embed-preview {
  position: relative;
  border-radius: 12px 12px 0 0;
  overflow: hidden;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 120px;
  background:
    radial-gradient(circle, #e2e8f0 1px, transparent 1px);
  background-size: 20px 20px;
}

.dark .wb-embed-preview {
  background:
    radial-gradient(circle, #2a2a2e 1px, transparent 1px);
  background-size: 20px 20px;
}

.wb-embed-svg {
  width: 100%;
  height: 100%;
  display: block;
}

.wb-svg-text {
  pointer-events: none;
}

.dark .wb-svg-text {
  fill: #e4e4e7;
}

/* Animated edge (flowing dashes) */
.wb-edge-animated {
  animation: wb-dash-flow 0.5s linear infinite;
}

@keyframes wb-dash-flow {
  to {
    stroke-dashoffset: -12;
  }
}

/* ═══ States ═══ */
.wb-embed-loading,
.wb-embed-error,
.wb-embed-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  color: #9ca3af;
  font-size: 12px;
  padding: 24px;
  user-select: none;
}

.dark .wb-embed-loading,
.dark .wb-embed-error,
.dark .wb-embed-empty {
  color: #6b7280;
}

.wb-loading-spinner {
  width: 20px;
  height: 20px;
  border: 2px solid #e5e7eb;
  border-top-color: var(--color-accent);
  border-radius: 50%;
  animation: wb-spin 0.8s linear infinite;
}

.dark .wb-loading-spinner {
  border-color: #333;
  border-top-color: var(--color-accent-dark);
}

@keyframes wb-spin {
  to { transform: rotate(360deg); }
}

/* ═══ Resize Handles ═══ */
.wb-resize-handle {
  position: absolute;
  z-index: 60;
  display: flex;
  align-items: center;
  justify-content: center;
  opacity: 0;
  transition: opacity 0.15s;
}

.wb-embed-wrapper.is-selected .wb-resize-handle {
  opacity: 1;
}

.wb-resize-left,
.wb-resize-right {
  top: 0;
  bottom: 0;
  width: 16px;
  cursor: col-resize;
}

.wb-resize-left { left: -8px; }
.wb-resize-right { right: -8px; }

.wb-resize-bar {
  width: 4px;
  height: 40px;
  max-height: 40%;
  border-radius: 2px;
  background: var(--color-accent);
  opacity: 0.5;
  transition: opacity 0.15s, height 0.15s;
}

.wb-resize-handle:hover .wb-resize-bar {
  opacity: 1;
  height: 48px;
}

.wb-resize-bottom {
  left: 0;
  right: 0;
  bottom: 28px;
  height: 16px;
  cursor: row-resize;
}

.wb-resize-bar-h {
  width: 40px;
  max-width: 30%;
  height: 4px;
  border-radius: 2px;
  background: var(--color-accent);
  opacity: 0.5;
  transition: opacity 0.15s, width 0.15s;
}

.wb-resize-handle:hover .wb-resize-bar-h {
  opacity: 1;
  width: 56px;
}

.dark .wb-resize-bar,
.dark .wb-resize-bar-h {
  background: var(--color-accent-dark);
}

/* ═══ Bubble Toolbar ═══ */
.wb-embed-bubble {
  position: absolute;
  top: 8px;
  left: 0;
  right: 0;
  width: fit-content;
  margin: 0 auto;
  z-index: 50;
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 4px;
  background: #fff;
  border: 1px solid #e5e7eb;
  border-radius: 10px;
  box-shadow: 0 4px 12px rgba(0,0,0,0.1), 0 1px 3px rgba(0,0,0,0.06);
  white-space: nowrap;
}

.dark .wb-embed-bubble {
  background: #1e1e1e;
  border-color: #333;
  box-shadow: 0 4px 12px rgba(0,0,0,0.4);
}

.wb-bubble-btn {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 5px 7px;
  border: none;
  background: transparent;
  border-radius: 6px;
  cursor: pointer;
  color: #6b7280;
  font-size: 12px;
  font-weight: 500;
  transition: all 0.12s;
  white-space: nowrap;
}

.wb-bubble-btn:hover {
  background: #f3f4f6;
  color: #111;
}

.wb-bubble-btn.wb-bubble-active {
  background: #111;
  color: #fff;
}

.dark .wb-bubble-btn {
  color: #a1a1aa;
}

.dark .wb-bubble-btn:hover {
  background: #2a2a2a;
  color: #f4f4f5;
}

.dark .wb-bubble-btn.wb-bubble-active {
  background: #f4f4f5;
  color: #111;
}

.wb-bubble-danger:hover {
  background: #fee2e2 !important;
  color: #dc2626 !important;
}

.dark .wb-bubble-danger:hover {
  background: #450a0a !important;
  color: #f87171 !important;
}

.wb-bubble-sep {
  width: 1px;
  height: 18px;
  background: #e5e7eb;
  margin: 0 2px;
}

.dark .wb-bubble-sep {
  background: #3a3a3a;
}

/* Bubble transition */
.wb-bubble-enter-active { transition: opacity 0.15s ease, transform 0.15s ease; }
.wb-bubble-leave-active { transition: opacity 0.1s ease; }
.wb-bubble-enter-from { opacity: 0; transform: translateY(-4px); }
.wb-bubble-leave-to { opacity: 0; }

/* ═══ Info Bar ═══ */
.wb-embed-info {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  border-top: 1px solid #f3f4f6;
  gap: 8px;
}

.dark .wb-embed-info { border-top-color: #2a2a2a; }

.wb-embed-label {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  flex: 1;
}

.wb-embed-name {
  font-size: 13px;
  font-weight: 500;
  color: #374151;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.dark .wb-embed-name { color: #d4d4d8; }

.wb-embed-meta {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

.wb-embed-count {
  font-size: 12px;
  color: #6b7280;
}

.dark .wb-embed-count { color: #9ca3af; }

.wb-embed-badge {
  font-size: 12px;
  font-weight: 600;
  padding: 2px 6px;
  border-radius: 4px;
  background: #f3f4f6;
  color: #9ca3af;
  text-transform: uppercase;
  letter-spacing: 0.03em;
}

.dark .wb-embed-badge {
  background: #2a2a2a;
  color: #71717a;
}
</style>
