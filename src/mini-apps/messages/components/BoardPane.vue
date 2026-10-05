<script lang="ts">
/**
 * The kinds of item this pane draws with the Whiteboard app's own pieces.
 * Syn writes frames around a subgraph, and a board opened here may have been
 * worked on in the app since, so it can hold anything the app can. A kind
 * not on this list — a newer app's, or a typo in a file — is drawn as a plain
 * labelled box rather than a warning and nothing.
 */
export const PANE_NODE_TYPES = ['shape', 'frame', 'sticky', 'text', 'mindmap', 'card', 'note', 'image', 'comment', 'stroke'] as const;

/** The size an item has when its file does not say, as the app gives it. */
const SIZED: Record<string, [number, number]> = {
  shape: [160, 80], image: [320, 240], sticky: [200, 200], frame: [480, 320], card: [260, 120], note: [280, 180],
};

/**
 * A node as the canvas needs it, which is not quite as the file holds it.
 *
 * Three fields the file has no business carrying, all of them about drawing:
 *
 * * `style`, because the resizer drags the canvas element itself — a shape
 *   that sized its own box would leave an element of no size around it. This
 *   is what the first version missed, and the pane came up showing lines and
 *   labels floating over nothing: every box was there, nought pixels wide.
 *   Only for the kinds the app sizes this way; text, a mind map's topics and
 *   ink size themselves.
 * * `zIndex` by area, so the small thing sits above the big one that contains
 *   it. A subgraph frame is the biggest box on the board and would otherwise
 *   be drawn over everything inside it.
 * * `data.locked`, which every item honours by offering no editing of its
 *   own — no renaming, no ticking a task off, no resizing. This pane moves
 *   boxes; a task card that could be ticked here would write to the vault
 *   from a pane that says it only moves things. Never written back: saving
 *   reads positions only (see `positionsNow`).
 *
 * The sizes are the Whiteboard app's own rules — see `useNodeOperations` —
 * kept in step by hand because this pane does not carry that app's store.
 */
export const forCanvas = (n: { id: string; type: string; position: { x: number; y: number }; data: any; hidden?: boolean }) => {
  const known = (PANE_NODE_TYPES as readonly string[]).includes(n.type);
  const size = SIZED[known ? n.type : 'shape'];
  const w = n.data?.width || size?.[0] || 160;
  const h = n.data?.height || size?.[1] || 80;
  // `editing` is a fresh item's ask to open its editor; there is none here.
  const data = { ...n.data, locked: true, editing: undefined };
  if (!known) data.label = n.data?.label || n.data?.title || '';
  return {
    id: n.id,
    type: known ? n.type : 'default',
    position: { ...n.position },
    data,
    // An unknown kind's box uses Vue Flow's own node, which reads `label`.
    ...(known ? {} : { label: data.label }),
    hidden: !!n.hidden,
    draggable: true,
    ...(size ? { style: { width: `${w}px`, height: `${h}px` } } : {}),
    zIndex: size ? Math.max(1, Math.round(10000 - (w * h) / 100)) : 10000,
  };
};
</script>

<script setup lang="ts">
/**
 * The diagram, beside the conversation, with the boxes loose.
 *
 * # What this is for, and what it deliberately is not
 *
 * It is for the one thing Mermaid cannot be told: *where things go*. Drag a
 * box, the position is saved, and the conversation is still on screen next to
 * it — which is the whole reason this is a pane and not a full screen, because
 * rearranging a system drawing is done while re-reading what it is supposed to
 * show.
 *
 * It is not a second whiteboard. No shapes, no colours, no freehand, no text —
 * that is the Whiteboard app, one button away, editing the same board this
 * pane is editing. Two editors of one document is a thing to keep small on
 * purpose: this one moves boxes, and everything else lives where it already
 * lived.
 *
 * # Why it takes room rather than covering it
 *
 * The first version was a `fixed` panel over the right-hand side, like the run
 * inspector. The run inspector is read instead of the conversation; this is
 * read *against* it, and covering half the answer while rearranging the
 * picture that answer describes is the one thing it must not do. So it sits in
 * the row, the conversation narrows, and the edge between them can be dragged.
 */
import { ref, computed, watch, onBeforeUnmount, provide, toRef } from 'vue';
import { VueFlow, type NodeDragEvent } from '@vue-flow/core';
import { Controls } from '@vue-flow/controls';
import { X, PenTool } from 'lucide-vue-next';
import ShapeNode from '../../whiteboard/nodes/ShapeNode.vue';
import FrameNode from '../../whiteboard/nodes/FrameNode.vue';
import StickyNode from '../../whiteboard/nodes/StickyNode.vue';
import TextNode from '../../whiteboard/nodes/TextNode.vue';
import MindmapNode from '../../whiteboard/nodes/MindmapNode.vue';
import VaultCardNode from '../../whiteboard/nodes/VaultCardNode.vue';
import NoteCardNode from '../../whiteboard/nodes/NoteCardNode.vue';
import ImageNode from '../../whiteboard/nodes/ImageNode.vue';
import CommentNode from '../../whiteboard/nodes/CommentNode.vue';
import StrokeNode from '../../whiteboard/nodes/StrokeNode.vue';
import { hiddenByCollapse } from '../../whiteboard/mindmap';
import WaypointEdge from '../../whiteboard/components/WaypointEdge.vue';
import { saveBoard, type KeptBoard } from '../keepAsBoard';
import { logger } from '../../../utils/logger';

import '@vue-flow/core/dist/style.css';
import '@vue-flow/core/dist/theme-default.css';

const props = defineProps<{ vaultPath: string; board: KeptBoard }>();
const emit = defineEmits<{ close: []; open: [] }>();

// Pictures find their files through the vault the canvas hands down.
provide('whiteboardVaultPath', toRef(props, 'vaultPath'));

const nodes = ref<any[]>([]);
const edges = ref<any[]>([]);
const saving = ref(false);

watch(
  () => props.board,
  board => {
    // A folded mind-map branch stays folded here, as it is in the app.
    const hidden = hiddenByCollapse(board.data.nodes, board.data.edges);
    nodes.value = board.data.nodes.map(n => forCanvas({ ...n, hidden: hidden.has(n.id) }));
    edges.value = board.data.edges.map(e => ({
      ...e,
      type: e.type || 'default',
      label: (e.data as { label?: string })?.label || '',
      zIndex: 10001,
    }));
  },
  { immediate: true },
);

// ── How much of the row this takes ──────────────────────────
const width = ref(Math.min(Math.max(Math.round(window.innerWidth * 0.45), 420), 900));
const dragging = ref(false);
const paneStyle = computed(() => ({ width: `${width.value}px` }));

const startResize = (e: PointerEvent) => {
  dragging.value = true;
  const from = e.clientX;
  const was = width.value;
  const move = (m: PointerEvent) => {
    width.value = Math.min(Math.max(was + (from - m.clientX), 360), window.innerWidth - 420);
  };
  const done = () => {
    dragging.value = false;
    window.removeEventListener('pointermove', move);
    window.removeEventListener('pointerup', done);
  };
  window.addEventListener('pointermove', move);
  window.addEventListener('pointerup', done);
};

/**
 * Saved a moment after the box is let go, not on every pixel of the drag.
 *
 * A drag is a hundred position changes and one intention.
 */
let pending: ReturnType<typeof setTimeout> | null = null;
/**
 * The board's boxes where they now are. Only a box that actually moved is
 * stamped as changed: the stamps are how a merge with another copy of the
 * board decides whose change to a box wins, and stamping every box made this
 * pane win for boxes it never touched.
 */
const positionsNow = () =>
  props.board.data.nodes.map(n => {
    const moved = nodes.value.find(v => v.id === n.id);
    if (!moved) return n;
    if (moved.position.x === n.position.x && moved.position.y === n.position.y) return n;
    return { ...n, position: { ...moved.position }, updated: Date.now() };
  });

const save = () => {
  if (pending) clearTimeout(pending);
  pending = setTimeout(async () => {
    pending = null;
    saving.value = true;
    try {
      props.board.data.nodes = positionsNow();
      await saveBoard(props.vaultPath, props.board.path, props.board.data);
    } catch (e) {
      logger.error('[Syn] Could not save the board', e as string);
    } finally {
      saving.value = false;
    }
  }, 400);
};

const onDragStop = (_: NodeDragEvent) => save();

// A pane closed mid-drag still owes the board its last move.
onBeforeUnmount(() => {
  if (!pending) return;
  clearTimeout(pending);
  pending = null;
  props.board.data.nodes = positionsNow();
  void saveBoard(props.vaultPath, props.board.path, props.board.data);
});
</script>

<template>
  <aside
    class="relative h-full shrink-0 flex flex-col bg-white dark:bg-[#15161a]
           border-l border-gray-200 dark:border-gray-800"
    :style="paneStyle"
  >
    <!-- The edge is the handle. Four pixels wide, the whole height, and it
         stops the pointer events reaching the canvas behind it. -->
    <div
      class="absolute left-0 top-0 bottom-0 w-1 -ml-0.5 z-10 cursor-col-resize
             hover:bg-accent/40 dark:hover:bg-accent-dark/40"
      :class="dragging && 'bg-accent/60 dark:bg-accent-dark/60'"
      @pointerdown.prevent="startResize"
    />

    <header class="flex items-center gap-2 px-4 py-3 border-b border-gray-100 dark:border-gray-800/60">
      <div class="min-w-0 flex-1">
        <p class="text-sm font-semibold truncate">{{ board.title }}</p>
        <p class="text-xs text-gray-500 dark:text-gray-400 truncate">
          {{ saving ? $t('syn.board_saving') : $t('syn.board_drag_hint') }}
        </p>
      </div>
      <button
        type="button"
        class="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs font-medium shrink-0
               text-accent dark:text-accent-dark hover:bg-accent/10 dark:hover:bg-accent-dark/10 cursor-pointer"
        @click="emit('open')"
      >
        <PenTool class="w-3.5 h-3.5" />
        {{ $t('syn.board_open_in_app') }}
      </button>
      <button
        type="button"
        class="p-1.5 rounded-lg text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800 cursor-pointer shrink-0"
        :aria-label="$t('whiteboard.close')"
        @click="emit('close')"
      >
        <X class="w-4 h-4" />
      </button>
    </header>

    <div class="flex-1 min-h-0">
      <!--
        No dotted background, and no zooming out past a quarter.

        A drawing of two data centres is five thousand pixels wide, and fitting
        that into a pane means a zoom of about a tenth: the boxes become
        smudges, the dot pattern lands at sub-pixel spacing, and every pan
        repaints a field of dots nobody can see. Opening at a readable zoom and
        letting the person pan is both quicker and more use — and with
        `onlyRenderVisibleElements`, what is off-screen is not in the page at
        all.
      -->
      <VueFlow
        v-model:nodes="nodes"
        v-model:edges="edges"
        class="w-full h-full"
        :fit-view-on-init="true"
        :fit-view-options="{ padding: 0.08, minZoom: 0.25, maxZoom: 1 }"
        :nodes-connectable="false"
        :only-render-visible-elements="true"
        :min-zoom="0.1"
        @node-drag-stop="onDragStop"
      >
        <Controls :show-interactive="false" />
        <!-- The app's own pieces, each locked (see `forCanvas`); nothing
             here listens for their edits, so none can reach the file. -->
        <template #node-shape="p"><ShapeNode v-bind="(p as any)" /></template>
        <template #node-frame="p"><FrameNode v-bind="(p as any)" /></template>
        <template #node-sticky="p"><StickyNode v-bind="(p as any)" /></template>
        <template #node-text="p"><TextNode v-bind="(p as any)" /></template>
        <template #node-mindmap="p"><MindmapNode v-bind="(p as any)" /></template>
        <template #node-card="p"><VaultCardNode v-bind="(p as any)" /></template>
        <template #node-note="p"><NoteCardNode v-bind="(p as any)" /></template>
        <template #node-image="p"><ImageNode v-bind="(p as any)" /></template>
        <template #node-comment="p"><CommentNode v-bind="(p as any)" /></template>
        <template #node-stroke="p"><StrokeNode v-bind="(p as any)" /></template>
        <template #edge-default="edgeProps">
          <WaypointEdge v-bind="(edgeProps as any)" edge-type="default" />
        </template>
      </VueFlow>
    </div>
  </aside>
</template>
