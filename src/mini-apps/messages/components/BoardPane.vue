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
import { ref, computed, watch, onBeforeUnmount } from 'vue';
import { VueFlow, type NodeDragEvent } from '@vue-flow/core';
import { Background } from '@vue-flow/background';
import { Controls } from '@vue-flow/controls';
import { X, PenTool } from 'lucide-vue-next';
import ShapeNode from '../../whiteboard/nodes/ShapeNode.vue';
import WaypointEdge from '../../whiteboard/components/WaypointEdge.vue';
import { saveBoard, type KeptBoard } from '../keepAsBoard';
import { logger } from '../../../utils/logger';

import '@vue-flow/core/dist/style.css';
import '@vue-flow/core/dist/theme-default.css';

const props = defineProps<{ vaultPath: string; board: KeptBoard }>();
const emit = defineEmits<{ close: []; open: [] }>();

/**
 * A node as the canvas needs it, which is not quite as the file holds it.
 *
 * Two fields the file has no business carrying, both of them about drawing:
 *
 * * `style`, because the resizer drags the canvas element itself — a shape
 *   that sized its own box would leave an element of no size around it. This
 *   is what the first version missed, and the pane came up showing lines and
 *   labels floating over nothing: every box was there, nought pixels wide.
 * * `zIndex` by area, so the small thing sits above the big one that contains
 *   it. A subgraph frame is the biggest box on the board and would otherwise
 *   be drawn over everything inside it.
 *
 * Both are the Whiteboard app's own rules — see `useNodeOperations` — kept in
 * step by hand because this pane does not carry that app's store with it.
 */
const forCanvas = (n: { id: string; type: string; position: { x: number; y: number }; data: any }) => {
  const w = n.data?.width || 160;
  const h = n.data?.height || 80;
  return {
    id: n.id,
    type: n.type,
    position: { ...n.position },
    data: { ...n.data },
    draggable: true,
    style: { width: `${w}px`, height: `${h}px` },
    zIndex: Math.max(1, Math.round(10000 - (w * h) / 100)),
  };
};

const nodes = ref<any[]>([]);
const edges = ref<any[]>([]);
const saving = ref(false);

watch(
  () => props.board,
  board => {
    nodes.value = board.data.nodes.map(forCanvas);
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
const width = ref(620);
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
const positionsNow = () =>
  props.board.data.nodes.map(n => {
    const moved = nodes.value.find(v => v.id === n.id);
    return moved ? { ...n, position: { ...moved.position }, updated: Date.now() } : n;
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
             hover:bg-violet-400/40"
      :class="dragging && 'bg-violet-400/60'"
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
               text-violet-600 dark:text-violet-400 hover:bg-violet-50 dark:hover:bg-violet-500/10 cursor-pointer"
        @click="emit('open')"
      >
        <PenTool class="w-3.5 h-3.5" />
        {{ $t('syn.board_open_in_app') }}
      </button>
      <button
        type="button"
        class="p-1.5 rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-800 cursor-pointer shrink-0"
        :aria-label="$t('whiteboard.close')"
        @click="emit('close')"
      >
        <X class="w-4 h-4" />
      </button>
    </header>

    <div class="flex-1 min-h-0">
      <VueFlow
        v-model:nodes="nodes"
        v-model:edges="edges"
        class="w-full h-full"
        :fit-view-on-init="true"
        :nodes-connectable="false"
        :only-render-visible-elements="true"
        :min-zoom="0.05"
        @node-drag-stop="onDragStop"
      >
        <Background :gap="16" />
        <Controls :show-interactive="false" />
        <template #node-shape="nodeProps"><ShapeNode v-bind="(nodeProps as any)" /></template>
        <template #edge-default="edgeProps">
          <WaypointEdge v-bind="(edgeProps as any)" edge-type="default" />
        </template>
      </VueFlow>
    </div>
  </aside>
</template>
