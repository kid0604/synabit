<script setup lang="ts">
import { computed, ref, watch, inject, type ComputedRef } from 'vue';
import { BaseEdge, EdgeLabelRenderer, getBezierPath, getStraightPath, getSmoothStepPath, useVueFlow } from '@vue-flow/core';
import * as d3 from 'd3';
import { markerUrl } from '../edgeMarkers';
import { paint } from '../ink';
import { EDGE_GREY } from '../composables/useNodeOperations';
import { anchor, anchorAt, lineEndOf, orthogonalRoute, roundedPath, sidesFacing, squareThrough, type Side } from '../routing';
import type { Box } from '../inkLayer';

const props = defineProps<{
  id: string;
  sourceX: number;
  sourceY: number;
  targetX: number;
  targetY: number;
  sourcePosition: string;
  targetPosition: string;
  data?: any;
  markerEnd?: string;
  markerStart?: string;
  selected?: boolean;
  style?: Record<string, any>;
  edgeType?: string;
  label?: string;
  sourceNode?: any;
  targetNode?: any;
}>();

const { screenToFlowCoordinate, getNodes } = useVueFlow();

/** A canvas item's box as drawn, or null before it has been measured. */
function boxOf(n: any): Box | null {
  if (!n) return null;
  const width = n.dimensions?.width || Number(n.data?.width) || 0;
  const height = n.dimensions?.height || Number(n.data?.height) || 0;
  if (!width || !height) return null;
  return { x: n.computedPosition?.x ?? n.position.x, y: n.computedPosition?.y ?? n.position.y, width, height };
}

/**
 * A line the router drew, while a box at one end of it is being dragged: it
 * is drawn as a stepped line that finds its own way, so it follows the box,
 * and is routed again when the box is let go (`linesToFollow`, autoRoute.ts).
 * Its bends were a way around where the box was; held while the box moved,
 * the line ran through the box it was attached to.
 */
const following = computed(() => !!props.data?.routed && !!(props.sourceNode?.dragging || props.targetNode?.dragging));

/**
 * Where each end of the line is: the place along a side the line says
 * (`sourceAt`/`targetAt`, see `lineEndOf`), or else its handle — the middle of
 * the side it was drawn from.
 */
const sourceAt = computed(() => (following.value ? null : lineEndOf(props.data?.sourceAt)));
const targetAt = computed(() => (following.value ? null : lineEndOf(props.data?.targetAt)));
const fixedEnd = (which: 'source' | 'target'): { x: number; y: number; side: Side } => {
  const at = which === 'source' ? sourceAt.value : targetAt.value;
  const box = boxOf(which === 'source' ? props.sourceNode : props.targetNode);
  if (at && box) return { ...anchorAt(box, at.side, at.t), side: at.side };
  return which === 'source'
    ? { x: props.sourceX, y: props.sourceY, side: props.sourcePosition as Side }
    : { x: props.targetX, y: props.targetY, side: props.targetPosition as Side };
};

/**
 * Ends that choose their own sides (see routing.ts): for a line set to, while
 * nobody has bent it by hand. A bent line keeps the sides it was bent from.
 * An end that says where it is keeps that place; the other still chooses.
 */
const autoEnds = computed(() => {
  if (!following.value && (props.data?.sides !== 'auto' || localWaypoints.value.length)) return null;
  const a = boxOf(props.sourceNode);
  const b = boxOf(props.targetNode);
  if (!a || !b) return null;
  const facing = sidesFacing(a, b);
  const from = sourceAt.value?.side ?? facing.from;
  const to = targetAt.value?.side ?? facing.to;
  const s = sourceAt.value ? anchorAt(a, from, sourceAt.value.t) : anchor(a, from);
  const t = targetAt.value ? anchorAt(b, to, targetAt.value.t) : anchor(b, to);
  return { a, b, from, to, s, t };
});

/**
 * What a stepped line goes around: the items on the board, frames aside — a
 * line inside a frame is meant to be in it. One list for every line, worked
 * out once by the board (`wbObstacles`); a board that offers none (the one in
 * a note) has each line work it out.
 */
const shared = inject<ComputedRef<Box[]> | null>('wbObstacles', null);
const obstacles = computed(() => {
  if (!autoEnds.value || (props.edgeType || props.data?.type) !== 'step') return [];
  if (shared) return shared.value;
  return getNodes.value
    .filter((n) => n.type !== 'frame' && n.type !== 'stroke')
    .map(boxOf)
    .filter((b): b is Box => !!b);
});

// The line's ends, from what it says it has, in its own colour.
const lineColor = computed(() => paint(props.data?.color) || EDGE_GREY);
const endMarker = computed(() => markerUrl(props.data?.markerEnd, lineColor.value));
const startMarker = computed(() => markerUrl(props.data?.markerStart, lineColor.value));
/** How round a stepped line's corners are, in board pixels. */
const CORNER = 8;
const updateEdgeWaypoints = inject<(id: string, waypoints: any[]) => void>('updateEdgeWaypoints');

const isDragging = ref(false);
const activeDragIndex = ref<number | null>(null);
const localWaypoints = ref<any[]>([]);

watch(() => props.data?.waypoints, (newWp) => {
  if (!isDragging.value) {
    localWaypoints.value = JSON.parse(JSON.stringify(newWp || []));
  }
}, { immediate: true, deep: true });

/**
 * The point halfway along a path, by its length.
 *
 * Where a label goes. The middle *bend* is not the middle of the line: on a
 * line with one long run and two short ones the label sat at the end of the
 * long run, next to the wrong box.
 */
let measurer: SVGPathElement | null = null;
function halfwayAlong(d: string): { x: number; y: number } | null {
  return along(d, labelAt.value);
}

/**
 * Where the label sits, as a share of the way along the line: the middle,
 * unless the line says otherwise (`labelAt`). A share rather than a point, so
 * the label stays on its line when a box is moved — a layout that put the
 * label of a "no" branch near its question writes 0.2, and it stays near it.
 */
const labelAt = computed(() => {
  const at = props.data?.labelAt;
  return typeof at === 'number' && Number.isFinite(at) ? Math.min(1, Math.max(0, at)) : 0.5;
});

function along(d: string, share: number): { x: number; y: number } | null {
  try {
    measurer ??= document.createElementNS('http://www.w3.org/2000/svg', 'path');
    measurer.setAttribute('d', d);
    const length = measurer.getTotalLength();
    if (!length) return null;
    const p = measurer.getPointAtLength(length * share);
    return { x: p.x, y: p.y };
  } catch {
    return null;
  }
}

/** A path and its label's place, the place moved if the line asks for one. */
function withLabel(drawn: [string, number, number, ...unknown[]]): [string, number, number] {
  const [d, x, y] = drawn;
  if (labelAt.value === 0.5) return [d, x, y];
  const p = along(d, labelAt.value);
  return p ? [d, p.x, p.y] : [d, x, y];
}

function getTangentPoint(x: number, y: number, pos: string) {
  const DIST = 40;
  switch (pos) {
    case 'left': return { x: x - DIST, y };
    case 'right': return { x: x + DIST, y };
    case 'top': return { x, y: y - DIST };
    case 'bottom': return { x, y: y + DIST };
    default: return { x, y };
  }
}

const edgePaths = computed(() => {
  const type = props.edgeType || props.data?.type || 'default';
  const waypoints = localWaypoints.value;
  
  const ends = autoEnds.value;
  if (ends) {
    const at = { sourceX: ends.s.x, sourceY: ends.s.y, sourcePosition: ends.from, targetX: ends.t.x, targetY: ends.t.y, targetPosition: ends.to };
    if (type === 'straight') return withLabel(getStraightPath(at as any));
    if (type === 'step') {
      const route = orthogonalRoute(ends.s, ends.from, ends.t, ends.to, obstacles.value);
      const d = roundedPath(route, CORNER);
      const half = halfwayAlong(d) ?? route[Math.floor(route.length / 2)];
      return [d, half.x, half.y];
    }
    return withLabel(getBezierPath(at as any));
  }

  const source = fixedEnd('source');
  const target = fixedEnd('target');
  if (waypoints.length === 0) {
    // Two ends facing each other a pixel or two out of line are one straight
    // run, not a step with a sliver in the middle.
    if (type === 'step') {
      const square = squareThrough(
        { ...source, box: boxOf(props.sourceNode) },
        [],
        { ...target, box: boxOf(props.targetNode) },
      );
      if (square.length === 2 && (square[0].x === square[1].x || square[0].y === square[1].y)) {
        return withLabel(getStraightPath({ sourceX: square[0].x, sourceY: square[0].y, targetX: square[1].x, targetY: square[1].y } as any));
      }
    }
    const at = { sourceX: source.x, sourceY: source.y, sourcePosition: source.side, targetX: target.x, targetY: target.y, targetPosition: target.side };
    if (type === 'straight') return withLabel(getStraightPath(at as any));
    if (type === 'step') return withLabel(getSmoothStepPath({ ...at, borderRadius: CORNER } as any));
    return withLabel(getBezierPath(at as any));
  }

  // Calculate label position (middle of the points)
  const allPoints = [
    { x: source.x, y: source.y },
    ...waypoints,
    { x: target.x, y: target.y }
  ];
  const midIndex = Math.floor((allPoints.length - 1) / 2);
  let lx, ly;
  if (allPoints.length % 2 === 0) {
    lx = (allPoints[midIndex].x + allPoints[midIndex + 1].x) / 2;
    ly = (allPoints[midIndex].y + allPoints[midIndex + 1].y) / 2;
  } else {
    lx = allPoints[midIndex].x;
    ly = allPoints[midIndex].y;
  }

  let p = '';
  if (type === 'straight') {
    const points = allPoints.map(p => [p.x, p.y]);
    p = d3.line()(points as [number, number][]) || '';
  } else if (type === 'step') {
    // Upright and level runs through the bends, square at both ends (see
    // `squareThrough`), with the corners rounded.
    p = roundedPath(
      squareThrough({ ...source, box: boxOf(props.sourceNode) }, waypoints, { ...target, box: boxOf(props.targetNode) }),
      CORNER,
    );
  } else {
    const startTp = getTangentPoint(source.x, source.y, source.side);
    const endTp = getTangentPoint(target.x, target.y, target.side);
    const points = [
      [source.x, source.y],
      [startTp.x, startTp.y],
      ...waypoints.map((w: any) => [w.x, w.y]),
      [endTp.x, endTp.y],
      [target.x, target.y]
    ];
    p = d3.line().curve(d3.curveCatmullRom.alpha(0.5))(points as [number, number][]) || '';
  }
  const half = halfwayAlong(p);
  if (half) {
    lx = half.x;
    ly = half.y;
  }
  return [p, lx, ly];
});

const path = computed(() => edgePaths.value[0]);
const labelX = computed(() => edgePaths.value[1]);
const labelY = computed(() => edgePaths.value[2]);

/** Drag a bend — by mouse, pen or finger. */
function startDragWaypoint(index: number, e: PointerEvent) {
  activeDragIndex.value = index;
  isDragging.value = true;
  (e.currentTarget as Element | null)?.setPointerCapture?.(e.pointerId);
  
  function onMouseMove(ev: PointerEvent) {
    if (activeDragIndex.value === null) return;
    const pos = screenToFlowCoordinate({ x: ev.clientX, y: ev.clientY });
    // Reassign array to guarantee reactivity triggers for the computed path
    const newWps = [...localWaypoints.value];
    newWps[activeDragIndex.value] = { x: pos.x, y: pos.y };
    localWaypoints.value = newWps;
  }
  
  function onMouseUp() {
    window.removeEventListener('pointermove', onMouseMove);
    window.removeEventListener('pointerup', onMouseUp);
    window.removeEventListener('pointercancel', onMouseUp);
    activeDragIndex.value = null;
    isDragging.value = false;
    
    if (updateEdgeWaypoints) {
      updateEdgeWaypoints(props.id, [...localWaypoints.value]);
    }
  }
  
  window.addEventListener('pointermove', onMouseMove);
  window.addEventListener('pointerup', onMouseUp);
  window.addEventListener('pointercancel', onMouseUp);
}

function addWaypoint(e: MouseEvent) {
  const pos = screenToFlowCoordinate({ x: e.clientX, y: e.clientY });
  const waypoints = [...localWaypoints.value];
  
  const source = fixedEnd('source');
  const target = fixedEnd('target');
  const pts = [
    { x: source.x, y: source.y },
    ...waypoints,
    { x: target.x, y: target.y }
  ];
  
  let bestIndex = 0;
  let minDiff = Infinity;
  for (let i = 0; i < pts.length - 1; i++) {
    const p1 = pts[i];
    const p2 = pts[i+1];
    const dist = (a: any, b: any) => Math.hypot(a.x - b.x, a.y - b.y);
    const d1 = dist(p1, pos);
    const d2 = dist(pos, p2);
    const dLine = dist(p1, p2);
    const diff = d1 + d2 - dLine;
    if (diff < minDiff) {
      minDiff = diff;
      bestIndex = i;
    }
  }
  
  waypoints.splice(bestIndex, 0, { x: pos.x, y: pos.y });
  localWaypoints.value = waypoints;
  
  if (updateEdgeWaypoints) {
    updateEdgeWaypoints(props.id, waypoints);
  }
}

function removeWaypoint(index: number) {
  const waypoints = [...localWaypoints.value];
  waypoints.splice(index, 1);
  localWaypoints.value = waypoints;
  
  if (updateEdgeWaypoints) {
    updateEdgeWaypoints(props.id, waypoints);
  }
}
</script>

<template>
  <BaseEdge
    :id="id"
    :path="path"
    :style="style"
    :marker-end="endMarker"
    :marker-start="startMarker"
  />
  
  <EdgeLabelRenderer v-if="label || data?.label">
    <div
      class="nodrag nopan absolute wb-edge-label"
      :data-edge-label="id"
      :style="{
        transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)`,
        pointerEvents: 'all'
      }"
    >
      {{ label || data?.label }}
    </div>
  </EdgeLabelRenderer>

  <g v-if="selected">
    <!-- Invisible thick path to allow double clicking to add waypoints -->
    <path
      :d="path"
      fill="none"
      stroke="transparent"
      stroke-width="15"
      class="cursor-pointer"
      @dblclick.stop.prevent="addWaypoint"
    />
    <!-- Waypoint visible circles -->
    <circle
      v-for="(wp, index) in localWaypoints"
      :key="index"
      :cx="wp.x"
      :cy="wp.y"
      r="4"
      stroke-width="2"
      style="pointer-events: none; fill: var(--wb-paper, #fff); stroke: var(--wb-selection, var(--color-accent))"
    />
    <!-- Transparent larger circles for easier grabbing -->
    <circle
      v-for="(wp, index) in localWaypoints"
      :key="`grab-${index}`"
      :cx="wp.x"
      :cy="wp.y"
      r="16"
      fill="rgba(0,0,0,0)"
      class="cursor-grab"
      style="pointer-events: all;"
      @pointerdown.stop.prevent="(e: PointerEvent) => startDragWaypoint(index, e)"
      @dblclick.stop.prevent="removeWaypoint(index)"
    />
  </g>
</template>

<style>
/*
 * A line's words, on the board's own paper so the line stops at them, and no
 * box of their own. They were drawn on `--wb-bg` with a `--wb-border`, which
 * nothing defined: no background, so the line ran through the words, and a
 * near-black outline and a shadow round every label.
 */
.wb-edge-label {
  /* Over the lines, which the Whiteboard app stacks above every node
     (EDGE_Z, useNodeOperations.ts): under them, a line ran through the
     words whatever was behind them. */
  z-index: 1000001;
  padding: 1px 5px;
  border-radius: 4px;
  font-size: 12px;
  font-weight: 500;
  line-height: 16px;
  white-space: nowrap;
  color: var(--wb-ink, currentColor);
  background: var(--wb-paper, #fdfdfc);
}
</style>
