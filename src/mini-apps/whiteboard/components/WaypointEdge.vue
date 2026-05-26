<script setup lang="ts">
import { computed, ref, watch, inject } from 'vue';
import { BaseEdge, EdgeLabelRenderer, getBezierPath, getStraightPath, getSmoothStepPath, useVueFlow } from '@vue-flow/core';
import * as d3 from 'd3';

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
}>();

const { screenToFlowCoordinate } = useVueFlow();
const updateEdgeWaypoints = inject<(id: string, waypoints: any[]) => void>('updateEdgeWaypoints');

const isDragging = ref(false);
const activeDragIndex = ref<number | null>(null);
const localWaypoints = ref<any[]>([]);

watch(() => props.data?.waypoints, (newWp) => {
  if (!isDragging.value) {
    localWaypoints.value = JSON.parse(JSON.stringify(newWp || []));
  }
}, { immediate: true, deep: true });

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
  
  if (waypoints.length === 0) {
    if (type === 'straight') return getStraightPath(props as any);
    if (type === 'step') return getSmoothStepPath(props as any);
    return getBezierPath(props as any);
  }

  // Calculate label position (middle of the points)
  const allPoints = [
    { x: props.sourceX, y: props.sourceY },
    ...waypoints,
    { x: props.targetX, y: props.targetY }
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
    const points = allPoints.map(p => [p.x, p.y]);
    p = d3.line().curve(d3.curveStepBefore)(points as [number, number][]) || '';
  } else {
    const startTp = getTangentPoint(props.sourceX, props.sourceY, props.sourcePosition);
    const endTp = getTangentPoint(props.targetX, props.targetY, props.targetPosition);
    const points = [
      [props.sourceX, props.sourceY],
      [startTp.x, startTp.y],
      ...waypoints.map((w: any) => [w.x, w.y]),
      [endTp.x, endTp.y],
      [props.targetX, props.targetY]
    ];
    p = d3.line().curve(d3.curveCatmullRom.alpha(0.5))(points as [number, number][]) || '';
  }
  return [p, lx, ly];
});

const path = computed(() => edgePaths.value[0]);
const labelX = computed(() => edgePaths.value[1]);
const labelY = computed(() => edgePaths.value[2]);

function startDragWaypoint(index: number, e: MouseEvent) {
  activeDragIndex.value = index;
  isDragging.value = true;
  
  function onMouseMove(ev: MouseEvent) {
    if (activeDragIndex.value === null) return;
    const pos = screenToFlowCoordinate({ x: ev.clientX, y: ev.clientY });
    // Reassign array to guarantee reactivity triggers for the computed path
    const newWps = [...localWaypoints.value];
    newWps[activeDragIndex.value] = { x: pos.x, y: pos.y };
    localWaypoints.value = newWps;
  }
  
  function onMouseUp(ev: MouseEvent) {
    window.removeEventListener('mousemove', onMouseMove);
    window.removeEventListener('mouseup', onMouseUp);
    activeDragIndex.value = null;
    isDragging.value = false;
    
    if (updateEdgeWaypoints) {
      updateEdgeWaypoints(props.id, [...localWaypoints.value]);
    }
  }
  
  window.addEventListener('mousemove', onMouseMove);
  window.addEventListener('mouseup', onMouseUp);
}

function addWaypoint(e: MouseEvent) {
  const pos = screenToFlowCoordinate({ x: e.clientX, y: e.clientY });
  const waypoints = [...localWaypoints.value];
  
  const pts = [
    { x: props.sourceX, y: props.sourceY },
    ...waypoints,
    { x: props.targetX, y: props.targetY }
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
    :marker-end="markerEnd"
    :marker-start="markerStart"
  />
  
  <EdgeLabelRenderer v-if="label || data?.label">
    <div
      class="nodrag nopan absolute text-xs font-semibold px-2 py-1 bg-[--wb-bg] border border-[--wb-border] rounded shadow-sm text-[--wb-text]"
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
      fill="#fff"
      stroke="#7c3aed"
      stroke-width="2"
      style="pointer-events: none"
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
      @mousedown.stop.prevent="(e) => startDragWaypoint(index, e)"
      @dblclick.stop.prevent="removeWaypoint(index)"
    />
  </g>
</template>
