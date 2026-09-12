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
 */
import { ref, watch, onBeforeUnmount } from 'vue';
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

const nodes = ref<any[]>([]);
const edges = ref<any[]>([]);
const saving = ref(false);

watch(
  () => props.board,
  board => {
    nodes.value = board.data.nodes.map(n => ({ ...n }));
    edges.value = board.data.edges.map(e => ({ ...e }));
  },
  { immediate: true },
);

/**
 * Saved a moment after the box is let go, not on every pixel of the drag.
 *
 * A drag is a hundred position changes and one intention.
 */
let pending: ReturnType<typeof setTimeout> | null = null;
const save = () => {
  if (pending) clearTimeout(pending);
  pending = setTimeout(async () => {
    pending = null;
    saving.value = true;
    try {
      props.board.data.nodes = nodes.value.map(n => ({
        ...n,
        position: { ...n.position },
        updated: Date.now(),
      }));
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
  props.board.data.nodes = nodes.value.map(n => ({ ...n, position: { ...n.position } }));
  void saveBoard(props.vaultPath, props.board.path, props.board.data);
});
</script>

<template>
  <!-- No backdrop, unlike the run inspector: the conversation beside this is
       what the arranging is being done against, and dimming it would be
       dimming the reason the pane is open. -->
  <aside
    class="fixed right-0 top-0 bottom-0 z-[999] w-[680px] max-w-[70vw] flex flex-col
           bg-white dark:bg-[#15161a] border-l border-gray-200 dark:border-gray-800
           shadow-[-8px_0_24px_rgba(0,0,0,0.08)]"
  >
    <header class="flex items-center gap-3 px-4 py-3 border-b border-gray-100 dark:border-gray-800/60">
      <div class="min-w-0 flex-1">
        <p class="text-sm font-semibold truncate">{{ board.title }}</p>
        <p class="text-xs text-gray-500 dark:text-gray-400">
          {{ saving ? $t('syn.board_saving') : $t('syn.board_drag_hint') }}
        </p>
      </div>
      <button
        type="button"
        class="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs font-medium
               text-violet-600 dark:text-violet-400 hover:bg-violet-50 dark:hover:bg-violet-500/10 cursor-pointer"
        @click="emit('open')"
      >
        <PenTool class="w-3.5 h-3.5" />
        {{ $t('syn.board_open_in_app') }}
      </button>
      <button
        type="button"
        class="p-1.5 rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-800 cursor-pointer"
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
        :elements-selectable="true"
        :min-zoom="0.1"
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
