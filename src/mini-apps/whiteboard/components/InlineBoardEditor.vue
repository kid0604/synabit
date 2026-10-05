<script setup lang="ts">
import { onBeforeUnmount, provide, ref, toRef } from 'vue';
import { VueFlow } from '@vue-flow/core';
import '@vue-flow/core/dist/style.css';
import '@vue-flow/core/dist/theme-default.css';
import '@vue-flow/node-resizer/dist/style.css';
import ShapeNode from '../nodes/ShapeNode.vue';
import StickyNode from '../nodes/StickyNode.vue';
import TextNode from '../nodes/TextNode.vue';
import FrameNode from '../nodes/FrameNode.vue';
import VaultCardNode from '../nodes/VaultCardNode.vue';
import MindmapNode from '../nodes/MindmapNode.vue';
import NoteCardNode from '../nodes/NoteCardNode.vue';
import ImageNode from '../nodes/ImageNode.vue';
import StrokeNode from '../nodes/StrokeNode.vue';
import WaypointEdge from './WaypointEdge.vue';
import EdgeMarkerDefs from './EdgeMarkerDefs.vue';
import { isMarker } from '../edgeMarkers';
import { paint } from '../ink';
import { hiddenByCollapse } from '../mindmap';
import { writeItemChanges, type ItemChange } from '../boardWrites';
import type { WBEdge, WBNode, WhiteboardData } from '../boardFile';
import { logger } from '../../../utils/logger';

/**
 * A board edited where it is shown — inside a note.
 *
 * Moving things and changing their words, the edits that come up while
 * reading: a box in the wrong place, a typo, a task to tick off. Adding,
 * deleting and connecting stay in the Whiteboard app, one click away. What is
 * changed here is written back item by item (`writeItemChanges`), so the
 * Whiteboard app, Syn or another device editing the same board keep theirs.
 */
const props = defineProps<{ vaultPath: string; path: string; board: WhiteboardData }>();
const emit = defineEmits<{ (e: 'done'): void }>();

provide('whiteboardVaultPath', toRef(props, 'vaultPath'));

const flowId = `inline-board-${Math.random().toString(36).slice(2)}`;
const SIZED: Record<string, [number, number]> = {
  shape: [160, 80], image: [320, 240], sticky: [200, 200], frame: [480, 320], card: [260, 120], note: [280, 180],
};

/** The working copy: the board as it was when editing started, changed here. */
const working = new Map<string, WBNode>(props.board.nodes.map((n) => [n.id, JSON.parse(JSON.stringify(n))]));
/** What has been changed here and not yet written: per item, only the fields touched. */
const changed = new Map<string, ItemChange>();
const saveFailed = ref(false);

const hidden = hiddenByCollapse(props.board.nodes, props.board.edges);
const forCanvas = (n: WBNode): any => {
  const size = SIZED[n.type];
  const node: any = { id: n.id, type: n.type, position: { ...n.position }, data: { ...n.data }, hidden: hidden.has(n.id) };
  if (size) {
    node.style = { width: `${n.data.width || size[0]}px`, height: `${n.data.height || size[1]}px` };
  }
  node.zIndex = typeof n.data.z === 'number' ? n.data.z : n.type === 'frame' ? -100_000 : 0;
  node.draggable = !n.data.locked;
  return node;
};
// Comments are left to the Whiteboard app; a board in a note is read.
const nodes = ref<any[]>([...working.values()].filter((n) => n.type !== 'comment').map(forCanvas));
const edges = ref<any[]>(
  props.board.edges
    .filter((e: WBEdge) => !hidden.has(e.source) && !hidden.has(e.target))
    .map((e: WBEdge) => ({ id: e.id, source: e.source, target: e.target, sourceHandle: e.sourceHandle, targetHandle: e.targetHandle, type: e.type || 'default', data: e.data ?? {}, label: e.data?.label, style: { stroke: paint(e.data?.color) || undefined }, zIndex: 1_000_000 })),
);
const markerUses = props.board.edges.flatMap((e) =>
  [e.data?.markerStart, e.data?.markerEnd].filter(isMarker).map((kind) => ({ kind, color: paint(e.data?.color) || 'var(--wb-edge, #8b8b8b)' })),
);

let timer: ReturnType<typeof setTimeout> | null = null;
function scheduleSave() {
  if (timer) clearTimeout(timer);
  timer = setTimeout(() => void flush(), 500);
}
async function flush() {
  if (timer) clearTimeout(timer);
  timer = null;
  if (!changed.size) return;
  const pending = [...changed.values()];
  changed.clear();
  const ok = await writeItemChanges(props.vaultPath, props.path, pending);
  saveFailed.value = !ok;
  if (!ok) {
    // Kept, to go with the next attempt, unless something newer replaced it.
    for (const c of pending) if (!changed.has(c.id)) changed.set(c.id, c);
    logger.error('Could not save the board from the note');
  }
}
onBeforeUnmount(() => void flush());

function touch(id: string, part: { position?: { x: number; y: number }; data?: Record<string, unknown> }) {
  const n = working.get(id);
  if (!n) return;
  if (part.position) n.position = { ...part.position };
  if (part.data) n.data = { ...n.data, ...part.data };
  n.updated = Date.now();
  const was = changed.get(id);
  changed.set(id, {
    id,
    position: part.position ?? was?.position,
    data: part.data || was?.data ? { ...(was?.data ?? {}), ...(part.data ?? {}) } : undefined,
    updated: n.updated,
  });
  scheduleSave();
}

function onData(id: string, data: Record<string, any>) {
  touch(id, { data });
  const vf = nodes.value.find((x) => x.id === id);
  if (vf) vf.data = { ...vf.data, ...data };
}

function onRotate(id: string, degrees: number, final: boolean) {
  const vf = nodes.value.find((x) => x.id === id);
  if (vf) vf.data = { ...vf.data, rotation: degrees };
  if (final) touch(id, { data: { rotation: degrees } });
}

function onDragStop(event: any) {
  for (const moved of event.nodes ?? [event.node]) {
    touch(moved.id, { position: { x: moved.position.x, y: moved.position.y } });
  }
}

async function done() {
  await flush();
  emit('done');
}
</script>

<template>
  <div
    class="wb-inline wb-canvas nodrag"
    contenteditable="false"
    role="region"
    :aria-label="$t('whiteboard.inline.label')"
    @keydown.stop="(e: KeyboardEvent) => { if (e.key === 'Escape') done(); }"
    @paste.stop
    @copy.stop
    @cut.stop
    @mousedown.stop
  >
    <VueFlow
      :id="flowId"
      v-model:nodes="nodes"
      v-model:edges="edges"
      fit-view-on-init
      :fit-view-options="{ padding: 0.08, maxZoom: 1 }"
      :nodes-connectable="false"
      :delete-key-code="null"
      :min-zoom="0.1"
      @node-drag-stop="onDragStop"
    >
      <template #node-shape="p"><ShapeNode v-bind="p" @update:data="(d: any) => onData(p.id, d)" @rotate="(deg: number, f: boolean) => onRotate(p.id, deg, f)" /></template>
      <template #node-sticky="p"><StickyNode v-bind="p" @update:data="(d: any) => onData(p.id, d)" @rotate="(deg: number, f: boolean) => onRotate(p.id, deg, f)" /></template>
      <template #node-text="p"><TextNode v-bind="p" @update:data="(d: any) => onData(p.id, d)" @rotate="(deg: number, f: boolean) => onRotate(p.id, deg, f)" /></template>
      <template #node-frame="p"><FrameNode v-bind="p" @update:data="(d: any) => onData(p.id, d)" /></template>
      <template #node-card="p"><VaultCardNode v-bind="p" @update:data="(d: any) => onData(p.id, d)" /></template>
      <template #node-mindmap="p"><MindmapNode v-bind="p" @update:data="(d: any) => onData(p.id, d)" /></template>
      <template #node-note="p"><NoteCardNode v-bind="p" /></template>
      <template #node-image="p"><ImageNode v-bind="p" /></template>
      <template #node-stroke="p"><StrokeNode v-bind="p" /></template>
      <template #edge-default="p"><WaypointEdge v-bind="(p as any)" edge-type="default" /></template>
      <template #edge-straight="p"><WaypointEdge v-bind="(p as any)" edge-type="straight" /></template>
      <template #edge-step="p"><WaypointEdge v-bind="(p as any)" edge-type="step" /></template>
      <EdgeMarkerDefs :uses="markerUses" />
    </VueFlow>
    <p v-if="saveFailed" class="wb-inline__problem" role="alert">{{ $t('whiteboard.inline.save_failed') }}</p>
    <button class="wb-inline__done" @click.stop="done">{{ $t('whiteboard.inline.done') }}</button>
  </div>
</template>

<style scoped>
.wb-inline {
  position: relative;
  width: 100%;
  height: 100%;
  --wb-ink: #1e1e1e;
  --wb-edge: var(--color-muted, #8b8b8b);
  --wb-paper: var(--color-base, #fdfdfc);
  --wb-selection: var(--color-accent);
}
:global(.dark .wb-inline) {
  --wb-ink: #e4e4e7;
  --wb-edge: var(--color-muted-dark, #71717a);
  --wb-paper: var(--color-base-dark, #242424);
  --wb-selection: var(--color-accent-dark);
}
.wb-inline :deep(.vue-flow__node-shape),
.wb-inline :deep(.vue-flow__node-stroke),
.wb-inline :deep(.vue-flow__node-frame) {
  pointer-events: none !important;
}
.wb-inline :deep(.vue-flow__node) {
  border: none !important;
  box-shadow: none !important;
}
.wb-inline :deep(.vue-flow__edge-path) {
  stroke: var(--wb-edge);
  stroke-width: 2;
}
.wb-inline__problem {
  position: absolute;
  top: 8px;
  left: 8px;
  z-index: 10;
  padding: 4px 10px;
  border-radius: 8px;
  font-size: 13px;
  color: #fff;
  background: var(--color-danger, #dc2626);
}
.wb-inline__done {
  position: absolute;
  top: 8px;
  right: 8px;
  z-index: 10;
  padding: 4px 12px;
  border-radius: 8px;
  font-size: 13px;
  font-weight: 600;
  color: #fff;
  background: var(--color-accent);
}
</style>
