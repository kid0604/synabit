<script setup lang="ts">
/**
 * A board Syn drew, shown in the conversation as the board itself.
 *
 * # Why the app's own canvas, and not a picture of it
 *
 * The first version drew an SVG from the file: a rectangle per box, a line
 * from middle to middle, and — whenever the board had to shrink to fit 640 by
 * 360 — a grey bar where each label would have been. Every board came out as
 * the same purple lozenges joined by diagonals: no shapes, no routed lines, no
 * arrowheads, no table rows, no words. Nothing could be read without opening
 * the board somewhere else, which is the one thing a picture in the answer is
 * there to save.
 *
 * So this is the Whiteboard's canvas with everything that edits switched off:
 * the same nodes, the same lines and ends, the same lanes and tables. A
 * second renderer would only ever be a worse copy of the first.
 *
 * # Size
 *
 * Fitted to the width of the answer, but never smaller than a zoom at which
 * the words are still words (`fitPicture`). A board wider than that — a
 * swimlane of fifteen steps — keeps the readable size and can be dragged
 * sideways, rather than shrinking into smudges. It is built only once it is
 * near the screen, and its height is known before then, so a long
 * conversation neither builds every board at once nor jumps as they land.
 *
 * Nothing here is ever written: a board still waiting to be laid out or routed
 * is worked out on a copy, and saving is left to the pane and the app.
 */
import { computed, onBeforeUnmount, onMounted, provide, ref, shallowRef, toRef, watch } from 'vue';
import { VueFlow, type VueFlowStore } from '@vue-flow/core';
import { Maximize2, PenTool } from 'lucide-vue-next';
import ShapeNode from '../../whiteboard/nodes/ShapeNode.vue';
import FrameNode from '../../whiteboard/nodes/FrameNode.vue';
import StickyNode from '../../whiteboard/nodes/StickyNode.vue';
import TextNode from '../../whiteboard/nodes/TextNode.vue';
import MindmapNode from '../../whiteboard/nodes/MindmapNode.vue';
import VaultCardNode from '../../whiteboard/nodes/VaultCardNode.vue';
import NoteCardNode from '../../whiteboard/nodes/NoteCardNode.vue';
import ImageNode from '../../whiteboard/nodes/ImageNode.vue';
import StrokeNode from '../../whiteboard/nodes/StrokeNode.vue';
import WaypointEdge from '../../whiteboard/components/WaypointEdge.vue';
import EdgeMarkerDefs from '../../whiteboard/components/EdgeMarkerDefs.vue';
import { isMarker } from '../../whiteboard/edgeMarkers';
import { paint } from '../../whiteboard/ink';
import { hiddenByCollapse } from '../../whiteboard/mindmap';
import { EDGE_GREY } from '../../whiteboard/composables/useNodeOperations';
import { captionOf, outlineOf, routePending } from '../../whiteboard/autoRoute';
import { layOutPending } from '../../whiteboard/autoLayout';
import type { WBEdge, WBNode, WhiteboardData } from '../../whiteboard/boardFile';
import type { KeptBoard } from '../keepAsBoard';
import { edgeForCanvas, fitPicture } from '../boardCanvas';
import { forCanvas } from './BoardPane.vue';
import { logger } from '../../../utils/logger';

import '@vue-flow/core/dist/style.css';
import '@vue-flow/core/dist/theme-default.css';

const props = defineProps<{ board: KeptBoard; vaultPath: string }>();
const emit = defineEmits<{ arrange: []; open: [] }>();

provide('whiteboardVaultPath', toRef(props, 'vaultPath'));

const flowId = `chat-board-${Math.random().toString(36).slice(2)}`;

/** The board as it is drawn: the file's, or a copy laid out and routed. */
const drawn = shallowRef<{ nodes: WBNode[]; edges: WBEdge[] } | null>(null);

async function settle(data: WhiteboardData) {
  const waiting = data.nodes.some((n) => n.data?.layout === 'pending') || data.edges.some((e) => e.data?.routing === 'pending');
  if (!waiting) {
    drawn.value = { nodes: data.nodes, edges: data.edges };
    return;
  }
  const copy = JSON.parse(JSON.stringify({ nodes: data.nodes, edges: data.edges })) as { nodes: WBNode[]; edges: WBEdge[] };
  try {
    await layOutPending(copy);
    await routePending(copy);
  } catch (e) {
    logger.warn('[Syn] Could not lay out the board for its picture', e as string);
  }
  if (props.board.data !== data) return;
  drawn.value = copy;
}
// The pane beside the conversation writes moves into this same board, by
// replacing its lists: the picture follows when it saves.
watch(
  () => [props.board.data, props.board.data.nodes, props.board.data.edges] as const,
  () => void settle(props.board.data),
  { immediate: true },
);

const nodes = computed(() => {
  const d = drawn.value;
  if (!d) return [];
  const hidden = hiddenByCollapse(d.nodes, d.edges);
  return d.nodes
    .filter((n) => n.type !== 'comment')
    .map((n) => ({ ...forCanvas({ ...n, hidden: hidden.has(n.id) }), draggable: false, selectable: false, focusable: false }));
});
const edges = computed(() => {
  const d = drawn.value;
  if (!d) return [];
  const hidden = hiddenByCollapse(d.nodes, d.edges);
  return d.edges
    .filter((e) => !hidden.has(e.source) && !hidden.has(e.target))
    .map((e) => ({ ...edgeForCanvas(e), selectable: false, focusable: false, updatable: false }));
});
const markerUses = computed(() =>
  edges.value.flatMap((e: any) => {
    const color = paint(e.data?.color) || EDGE_GREY;
    return [e.data?.markerStart, e.data?.markerEnd].filter(isMarker).map((kind: string) => ({ kind, color }));
  }),
);

/** What the drawing covers, in board coordinates: boxes, words under figures, bends. */
const bounds = computed(() => {
  const d = drawn.value;
  if (!d) return null;
  let left = Infinity;
  let top = Infinity;
  let right = -Infinity;
  let bottom = -Infinity;
  const take = (x: number, y: number, w = 0, h = 0) => {
    left = Math.min(left, x);
    top = Math.min(top, y);
    right = Math.max(right, x + w);
    bottom = Math.max(bottom, y + h);
  };
  for (const n of nodes.value) {
    if (n.hidden) continue;
    const box = outlineOf(n) ?? { x: n.position.x, y: n.position.y, width: 160, height: 80 };
    take(box.x, box.y, box.width, box.height);
    const words = captionOf(n);
    if (words) take(words.x, words.y, words.width, words.height);
  }
  for (const e of d.edges) {
    for (const p of (e.data?.waypoints as { x: number; y: number }[] | undefined) ?? []) take(p.x, p.y);
  }
  if (!Number.isFinite(left)) return null;
  return { left, top, right, bottom, span: Math.max(right - left, 1), drop: Math.max(bottom - top, 1) };
});

// ── Fitting it to the answer ───────────────────────────────
const frame = ref<HTMLElement | null>(null);
const width = ref(0);
const near = ref(false);

const fit = computed(() => (bounds.value && width.value ? fitPicture(bounds.value, width.value) : null));

/**
 * No limit on where the view may sit, for a drawing that fits.
 *
 * The limit is only there to stop a dragged board being dragged away. Vue
 * Flow keeps the view inside it by putting the limit's corner at the frame's
 * corner, and for a drawing smaller than its frame that is the top-left:
 * every board that fitted was pushed against the left edge with the rest of
 * the answer's width empty beside it.
 */
const UNBOUNDED: [[number, number], [number, number]] = [
  [-Infinity, -Infinity],
  [Infinity, Infinity],
];

const flow = shallowRef<VueFlowStore | null>(null);
/**
 * The view again, whenever the answer's width changes: the pane beside the
 * conversation opening and closing, the window resized, the column widened.
 *
 * The limit first, then the view. Vue Flow keeps every new view inside the
 * limit it already has, and the prop that carries the limit reaches it only
 * after this runs: closing the pane — where the board had been too wide and
 * so was limited — centred it and had it pushed back to the left edge by the
 * limit it no longer had. `post`, so the frame has its new size by then.
 */
watch(
  fit,
  (now) => {
    if (!now || !flow.value) return;
    flow.value.setTranslateExtent(now.overflows ? now.extent : UNBOUNDED);
    void flow.value.setViewport(now.viewport);
  },
  { flush: 'post' },
);

let sizing: ResizeObserver | null = null;
let watching: IntersectionObserver | null = null;
onMounted(() => {
  if (!frame.value) return;
  width.value = frame.value.clientWidth;
  sizing = new ResizeObserver(([entry]) => {
    width.value = Math.round(entry.contentRect.width);
  });
  sizing.observe(frame.value);
  watching = new IntersectionObserver(
    ([entry]) => {
      if (!entry.isIntersecting) return;
      near.value = true;
      watching?.disconnect();
      watching = null;
    },
    { rootMargin: '600px 0px' },
  );
  watching.observe(frame.value);
});
onBeforeUnmount(() => {
  sizing?.disconnect();
  watching?.disconnect();
});
</script>

<template>
  <figure class="chat-board">
    <div
      ref="frame"
      class="chat-board-frame"
      :class="fit?.overflows && 'is-draggable'"
      :style="{ height: `${fit?.height ?? 240}px` }"
      @dblclick="emit('arrange')"
    >
      <VueFlow
        v-if="near && fit"
        :id="flowId"
        :nodes="nodes"
        :edges="edges"
        class="chat-board-flow"
        :default-viewport="fit.viewport"
        :min-zoom="0.1"
        :max-zoom="2"
        :translate-extent="fit.overflows ? fit.extent : UNBOUNDED"
        :nodes-draggable="false"
        :nodes-connectable="false"
        :elements-selectable="false"
        :nodes-focusable="false"
        :edges-focusable="false"
        :edges-updatable="false"
        :pan-on-drag="fit.overflows"
        :pan-on-scroll="false"
        :zoom-on-scroll="false"
        :zoom-on-pinch="false"
        :zoom-on-double-click="false"
        :prevent-scrolling="false"
        @init="(f: VueFlowStore) => (flow = f)"
      >
        <template #node-shape="p"><ShapeNode v-bind="(p as any)" /></template>
        <template #node-frame="p"><FrameNode v-bind="(p as any)" /></template>
        <template #node-sticky="p"><StickyNode v-bind="(p as any)" /></template>
        <template #node-text="p"><TextNode v-bind="(p as any)" /></template>
        <template #node-mindmap="p"><MindmapNode v-bind="(p as any)" /></template>
        <template #node-card="p"><VaultCardNode v-bind="(p as any)" /></template>
        <template #node-note="p"><NoteCardNode v-bind="(p as any)" /></template>
        <template #node-image="p"><ImageNode v-bind="(p as any)" /></template>
        <template #node-stroke="p"><StrokeNode v-bind="(p as any)" /></template>
        <template #edge-default="edgeProps">
          <WaypointEdge v-bind="(edgeProps as any)" edge-type="default" />
        </template>
        <template #edge-step="edgeProps">
          <WaypointEdge v-bind="(edgeProps as any)" edge-type="step" />
        </template>
        <template #edge-straight="edgeProps">
          <WaypointEdge v-bind="(edgeProps as any)" edge-type="straight" />
        </template>
        <EdgeMarkerDefs :uses="markerUses" />
      </VueFlow>
    </div>
    <figcaption class="chat-board-bar">
      <span class="chat-board-title">{{ board.title }}</span>
      <span v-if="fit?.overflows" class="chat-board-hint">{{ $t('syn.board_drag_to_see') }}</span>
      <button type="button" @click="emit('arrange')">
        <Maximize2 class="w-3.5 h-3.5" aria-hidden="true" />
        {{ $t('syn.board_arrange_here') }}
      </button>
      <button type="button" @click="emit('open')">
        <PenTool class="w-3.5 h-3.5" aria-hidden="true" />
        {{ $t('syn.board_open_in_app') }}
      </button>
    </figcaption>
  </figure>
</template>

<style>
/* The board's ink, paper and lines on the answer's own background, as the
   pane beside it paints them (see BoardPane.vue, whiteboard/ink.ts). */
.chat-board-flow {
  --wb-ink: #1e1e1e;
  --wb-edge: var(--color-muted, #8b8b8b);
  --wb-paper: #fff;
}
.dark .chat-board-flow {
  --wb-ink: #e4e4e7;
  --wb-edge: var(--color-muted-dark, #71717a);
  --wb-paper: #1e1f25;
}
.chat-board-flow .vue-flow__edge-path {
  stroke: var(--wb-edge);
  stroke-width: 1.5;
}
</style>

<style scoped>
.chat-board {
  margin: 0.75rem 0 0.25rem;
  border: 1px solid color-mix(in srgb, var(--color-accent) 15%, transparent);
  border-radius: 0.75rem;
  overflow: hidden;
}

.chat-board-frame {
  position: relative;
  width: 100%;
}

.chat-board-frame.is-draggable :deep(.vue-flow__pane) {
  cursor: grab;
}

.chat-board-frame.is-draggable :deep(.vue-flow__pane.dragging) {
  cursor: grabbing;
}

.chat-board-bar {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.4rem 0.5rem 0.4rem 0.75rem;
  border-top: 1px solid color-mix(in srgb, var(--color-accent) 10%, transparent);
  font-size: 12px;
  color: #6b7280;
}

.chat-board-title {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.chat-board-hint {
  flex-shrink: 0;
}

.chat-board-bar button {
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  min-height: 24px;
  padding: 0.2rem 0.55rem;
  border-radius: 0.5rem;
  font-size: 12px;
  color: var(--color-accent);
  cursor: pointer;
}

.chat-board-bar button:is(.dark *) {
  color: var(--color-accent-dark);
}

.chat-board-bar button:hover {
  background: color-mix(in srgb, var(--color-accent) 8%, transparent);
}
</style>
