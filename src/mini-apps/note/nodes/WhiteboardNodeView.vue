<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from 'vue';
import { NodeViewWrapper } from '@tiptap/vue-3';
import {
  PenTool, ExternalLink, Trash2,
  AlignLeft, AlignCenter, AlignRight
} from 'lucide-vue-next';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { SHAPES_MAP } from '../../whiteboard/shapes';
import { logger } from '../../../utils/logger';

const props = defineProps<{
  node: any;
  updateAttributes: (attrs: Record<string, any>) => void;
  deleteNode: () => void;
  getPos: () => number;
  editor: any;
  selected: boolean;
}>();

// Inject vaultPath from editor storage (set by TiptapEditor)
const vaultPath = computed(() => props.editor?.storage?.whiteboard?.vaultPath || '');

// --- Board data ---
interface WBNode {
  id: string;
  type: 'shape' | 'stroke' | 'mindmap' | 'text';
  position: { x: number; y: number };
  data: Record<string, any>;
}

interface WBEdge {
  id: string;
  source: string;
  sourceHandle?: string;
  target: string;
  targetHandle?: string;
  type: string;
  data?: Record<string, any>;
}

interface BoardData {
  title: string;
  tags: string[];
  nodes: WBNode[];
  edges: WBEdge[];
  viewport: { x: number; y: number; zoom: number };
}

const boardData = ref<BoardData | null>(null);
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
const loadBoard = async () => {
  const path = props.node.attrs.boardPath;
  const vp = vaultPath.value;
  if (!path || !vp) {
    error.value = 'No vault path';
    loading.value = false;
    return;
  }
  try {
    loading.value = true;
    error.value = '';
    const raw = await invoke<string>('read_whiteboard', { vaultPath: vp, path });
    boardData.value = JSON.parse(raw);
  } catch (e: any) {
    error.value = 'Failed to load whiteboard';
    logger.error('WhiteboardNodeView: load failed', e);
  } finally {
    loading.value = false;
  }
};

onMounted(loadBoard);

// Reload when boardPath changes
watch(() => props.node.attrs.boardPath, loadBoard);

// Auto-reload when whiteboard is updated in the Whiteboard app
let unlistenWbUpdate: (() => void) | null = null;

onMounted(async () => {
  unlistenWbUpdate = await listen<{ path: string; id: string }>('whiteboard-updated', (event) => {
    const boardPath = props.node.attrs.boardPath;
    const boardId = props.node.attrs.boardId;
    if (event.payload.path === boardPath || event.payload.id === boardId) {
      loadBoard();
    }
  });
});

onUnmounted(() => {
  if (unlistenWbUpdate) unlistenWbUpdate();
});

// --- Mindmap node dimensions ---
function getMindmapWidth(node: WBNode): number {
  const label = node.data.label || 'Idea';
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
  // stroke
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
  const nodeMap = new Map<string, WBNode>();
  for (const n of boardData.value.nodes) nodeMap.set(n.id, n);

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
      color: d.color || '#94a3b8',
      strokeWidth: d.strokeWidth || 2,
      dashArray,
      animated: !!d.animated,
      markerEnd: d.markerEnd === 'arrow',
      markerStart: d.markerStart === 'arrow',
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

// Render SVG shape path scaled to actual node position/size
function getShapeTransform(node: WBNode): string {
  const def = SHAPES_MAP[node.data.shapeType] || SHAPES_MAP['rectangle'];
  const w = node.data.width || def?.defaultWidth || 160;
  const h = node.data.height || def?.defaultHeight || 80;
  return `translate(${node.position.x}, ${node.position.y}) scale(${w / 100}, ${h / 100})`;
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
          <span>Loading whiteboard…</span>
        </div>

        <!-- Error -->
        <div v-else-if="error" class="wb-embed-error">
          <PenTool class="w-6 h-6 opacity-40" />
          <span>{{ error }}</span>
        </div>

        <!-- Empty board -->
        <div v-else-if="!boardData || boardData.nodes.length === 0" class="wb-embed-empty">
          <PenTool class="w-8 h-8 opacity-30" />
          <span>Empty whiteboard</span>
        </div>

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
              <path d="M 0 0 L 12 6 L 0 12 Z" :fill="edge.color" />
            </marker>
          </defs>

          <!-- Edges (bezier curves with dash/animation/arrow support) -->
          <path
            v-for="edge in computedEdges"
            :key="edge.id"
            :d="edge.path"
            fill="none"
            :stroke="edge.color"
            :stroke-width="edge.strokeWidth"
            stroke-linecap="round"
            :stroke-dasharray="edge.dashArray"
            :marker-end="edge.markerEnd ? `url(#arrow-${edge.id})` : undefined"
            :marker-start="edge.markerStart ? `url(#arrow-${edge.id})` : undefined"
            :class="{ 'wb-edge-animated': edge.animated }"
          />

          <!-- Shape Nodes -->
          <template v-for="node in boardData.nodes.filter(n => n.type === 'shape')" :key="node.id">
            <g :transform="getShapeTransform(node)">
              <path
                :d="(SHAPES_MAP[node.data.shapeType] || SHAPES_MAP['rectangle']).path"
                :fill="node.data.fillColor || 'none'"
                :stroke="node.data.color || '#7c3aed'"
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
                :stroke="node.data.color || '#7c3aed'"
                :stroke-width="node.data.borderWidth || 2"
                vector-effect="non-scaling-stroke"
                stroke-linejoin="round"
              />
            </g>
            <!-- Label -->
            <text
              v-if="node.data.label"
              :x="node.position.x + (node.data.width || (SHAPES_MAP[node.data.shapeType]?.defaultWidth || 160)) / 2"
              :y="node.position.y + (node.data.height || (SHAPES_MAP[node.data.shapeType]?.defaultHeight || 80)) / 2"
              text-anchor="middle"
              dominant-baseline="central"
              :font-size="node.data.fontSize || 13"
              font-family="Inter, system-ui, sans-serif"
              fill="currentColor"
              class="wb-svg-text"
            >{{ node.data.label }}</text>
          </template>

          <!-- Stroke Nodes (freehand drawings) -->
          <template v-for="node in boardData.nodes.filter(n => n.type === 'stroke')" :key="node.id">
            <path
              v-if="node.data.svgPath"
              :d="node.data.svgPath"
              :fill="node.data.color || '#000'"
              :opacity="node.data.opacity ?? 0.85"
              :transform="`translate(${node.position.x}, ${node.position.y})`"
            />
          </template>

          <!-- Text Nodes (foreignObject for native CSS word-wrap) -->
          <template v-for="node in boardData.nodes.filter(n => n.type === 'text')" :key="node.id">
            <foreignObject
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
                  opacity: node.data.opacity || 1,
                  fontSize: (node.data.fontSize || 16) + 'px',
                  fontWeight: node.data.fontWeight || 'normal',
                  fontStyle: node.data.fontStyle || 'normal',
                  color: node.data.color || 'inherit',
                  fontFamily: 'Inter, system-ui, sans-serif',
                  whiteSpace: 'pre-wrap',
                  wordBreak: 'break-word',
                  overflowWrap: 'break-word',
                  boxSizing: 'border-box',
                  lineHeight: '1.4',
                }"
              >{{ node.data.label || '' }}</div>
            </foreignObject>
          </template>

          <!-- Mindmap Nodes (pill-shaped with border + light fill) -->
          <template v-for="node in boardData.nodes.filter(n => n.type === 'mindmap')" :key="node.id">
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
            >{{ node.data.label || 'Idea' }}</text>
          </template>
        </svg>
      </div>

      <!-- Bubble toolbar -->
      <Transition name="wb-bubble">
        <div v-if="selected" class="wb-embed-bubble" @mousedown.prevent>
          <!-- Alignment -->
          <button @click="setAlign('left')" title="Align left" class="wb-bubble-btn" :class="{ 'wb-bubble-active': blockAlign === 'left' }">
            <AlignLeft class="w-3.5 h-3.5" />
          </button>
          <button @click="setAlign('center')" title="Align center" class="wb-bubble-btn" :class="{ 'wb-bubble-active': blockAlign === 'center' }">
            <AlignCenter class="w-3.5 h-3.5" />
          </button>
          <button @click="setAlign('right')" title="Align right" class="wb-bubble-btn" :class="{ 'wb-bubble-active': blockAlign === 'right' }">
            <AlignRight class="w-3.5 h-3.5" />
          </button>
          <div class="wb-bubble-sep" />
          <button @click="openInApp" title="Open in Whiteboard" class="wb-bubble-btn">
            <ExternalLink class="w-3.5 h-3.5" />
          </button>
          <div class="wb-bubble-sep" />
          <button @click="deleteNode" title="Remove" class="wb-bubble-btn wb-bubble-danger">
            <Trash2 class="w-3.5 h-3.5" />
          </button>
        </div>
      </Transition>

      <!-- Info bar -->
      <div class="wb-embed-info">
        <div class="wb-embed-label">
          <PenTool class="w-3.5 h-3.5 text-violet-500 flex-shrink-0" />
          <span class="wb-embed-name">{{ node.attrs.title || 'Untitled Board' }}</span>
        </div>
        <div class="wb-embed-meta">
          <span v-if="nodeCount > 0" class="wb-embed-count">{{ nodeCount }} nodes</span>
          <span class="wb-embed-badge">Whiteboard</span>
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
  border-color: #7c3aed;
  box-shadow: 0 0 0 2px rgba(124, 58, 237, 0.15);
}

.dark .wb-embed-container {
  border-color: #333;
  background: #1a1a1e;
}

.dark .wb-embed-wrapper.is-selected .wb-embed-container {
  border-color: #a78bfa;
  box-shadow: 0 0 0 2px rgba(167, 139, 250, 0.15);
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
  border-top-color: #7c3aed;
  border-radius: 50%;
  animation: wb-spin 0.8s linear infinite;
}

.dark .wb-loading-spinner {
  border-color: #333;
  border-top-color: #a78bfa;
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
  background: #7c3aed;
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
  background: #7c3aed;
  opacity: 0.5;
  transition: opacity 0.15s, width 0.15s;
}

.wb-resize-handle:hover .wb-resize-bar-h {
  opacity: 1;
  width: 56px;
}

.dark .wb-resize-bar,
.dark .wb-resize-bar-h {
  background: #a78bfa;
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
  font-size: 11px;
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
  font-size: 10px;
  color: #9ca3af;
}

.dark .wb-embed-count { color: #71717a; }

.wb-embed-badge {
  font-size: 10px;
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
