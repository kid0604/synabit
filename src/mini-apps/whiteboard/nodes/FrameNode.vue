<script setup lang="ts">
import { nextTick, ref } from 'vue';
import { NodeResizer } from '@vue-flow/node-resizer';
import { Handle, Position } from '@vue-flow/core';
import { paint } from '../ink';
import { RefreshCw } from 'lucide-vue-next';
import { useI18n } from 'vue-i18n';

const props = defineProps<{
  id: string;
  selected?: boolean;
  data: {
    label?: string;
    color?: string;
    width?: number;
    height?: number;
    locked?: boolean;
    /** A live frame's question to the vault; see `useLiveFrames`. */
    query?: string;
    /** How a live frame lays its cards out, and what it draws for it (liveLayouts.ts). */
    layout?: 'grid' | 'kanban' | 'timeline';
    groupBy?: string;
    lanes?: { value: string; x: number; width: number; count: number }[];
    ticks?: { day: string; x: number }[];
    undatedY?: number | null;
  };
}>();

const { t, te, locale } = useI18n();

/** A column's heading: a task status in words, any other value as it is. */
function laneName(value: string): string {
  if (!value) return t('whiteboard.live.none');
  const key = `whiteboard.live.status.${value}`;
  return (props.data.groupBy ?? 'status') === 'status' && te(key) ? t(key) : value;
}

/** A day on the line, short: "9 Oct". */
function dayName(day: string): string {
  return new Intl.DateTimeFormat(String(locale.value), { day: 'numeric', month: 'short', timeZone: 'UTC' }).format(new Date(`${day}T00:00:00Z`));
}

const emit = defineEmits<{ (e: 'update:data', data: any): void; (e: 'refresh', id: string): void }>();

const isEditing = ref(false);
const editText = ref('');
const inputRef = ref<HTMLInputElement | null>(null);

function startEdit() {
  if (props.data.locked) return;
  isEditing.value = true;
  editText.value = props.data.label ?? '';
  nextTick(() => {
    inputRef.value?.focus();
    inputRef.value?.select();
  });
}

function finishEdit() {
  if (!isEditing.value) return;
  isEditing.value = false;
  emit('update:data', { label: editText.value.trim() });
}

function onResizeEnd(event: any) {
  emit('update:data', { width: Math.round(event.params.width), height: Math.round(event.params.height) });
}
</script>

<template>
  <!--
    A frame is the area behind other items. Only its title and its border take
    the pointer: the inside belongs to what is in it, and to the canvas — a drag
    that starts in an empty part of a frame draws a selection, as it would
    anywhere else.
  -->
  <div class="wb-frame" :class="{ 'is-selected': selected }" :style="{ '--frame-color': paint(data.color) || '#94a3b8' }">
    <NodeResizer
      :is-visible="!!selected && !data.locked"
      :min-width="120"
      :min-height="80"
      color="var(--wb-selection, var(--color-accent))"
      @resize-end="onResizeEnd"
    />

    <div class="wb-frame__title" @dblclick.stop="startEdit">
      <input
        v-if="isEditing"
        ref="inputRef"
        v-model="editText"
        class="wb-frame__input nodrag nopan"
        :aria-label="$t('whiteboard.frame_title')"
        @blur="finishEdit"
        @keydown.enter="finishEdit"
        @keydown.escape.stop="isEditing = false"
      />
      <span v-else>{{ data.label || $t('whiteboard.frame') }}</span>
      <template v-if="data.query">
        <span class="wb-frame__live" :title="data.query">{{ $t('whiteboard.live.badge') }}</span>
        <button
          class="wb-frame__refresh nodrag"
          :title="$t('whiteboard.live.refresh')"
          :aria-label="$t('whiteboard.live.refresh')"
          @click.stop="emit('refresh', id)"
        >
          <RefreshCw :size="12" />
        </button>
      </template>
    </div>

    <!-- A kanban frame's columns: a heading each, and the count beside it. -->
    <template v-if="data.layout === 'kanban' && data.lanes">
      <div v-for="lane in data.lanes" :key="lane.value" class="wb-frame__lane" :style="{ left: lane.x + 'px', width: lane.width + 'px' }">
        <span class="wb-frame__lane-name">{{ laneName(lane.value) }}</span>
        <span class="wb-frame__lane-count">{{ lane.count }}</span>
      </div>
    </template>
    <!-- A timeline frame's days along the top, and where the undated wait. -->
    <template v-if="data.layout === 'timeline' && data.ticks">
      <div v-for="tick in data.ticks" :key="tick.day" class="wb-frame__tick" :style="{ left: tick.x + 'px' }">
        <span>{{ dayName(tick.day) }}</span>
      </div>
      <div v-if="data.undatedY != null" class="wb-frame__undated" :style="{ top: data.undatedY + 'px' }">
        {{ $t('whiteboard.live.undated') }}
      </div>
    </template>

    <div class="wb-frame__edge wb-frame__edge--top" />
    <div class="wb-frame__edge wb-frame__edge--right" />
    <div class="wb-frame__edge wb-frame__edge--bottom" />
    <div class="wb-frame__edge wb-frame__edge--left" />

    <Handle id="top" type="source" :position="Position.Top" class="wb-frame__handle" />
    <Handle id="right" type="source" :position="Position.Right" class="wb-frame__handle" />
    <Handle id="bottom" type="source" :position="Position.Bottom" class="wb-frame__handle" />
    <Handle id="left" type="source" :position="Position.Left" class="wb-frame__handle" />
  </div>
</template>

<style scoped>
.wb-frame {
  position: relative;
  width: 100%;
  height: 100%;
  border-radius: 10px;
  border: 1.5px solid color-mix(in srgb, var(--frame-color) 70%, transparent);
  background: color-mix(in srgb, var(--frame-color) 7%, transparent);
  pointer-events: none;
}
.wb-frame.is-selected {
  border-color: var(--wb-selection, var(--color-accent));
}
.wb-frame__title {
  position: absolute;
  left: 0;
  bottom: calc(100% + 4px);
  max-width: 100%;
  padding: 0 2px;
  font-size: 13px;
  font-weight: 600;
  color: color-mix(in srgb, var(--frame-color) 85%, currentColor);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  pointer-events: auto;
  cursor: grab;
}
.wb-frame__title {
  display: flex;
  align-items: center;
  gap: 6px;
}
.wb-frame__live {
  padding: 0 6px;
  border-radius: 999px;
  font-size: 12px;
  font-weight: 600;
  color: #fff;
  background: var(--color-accent);
}
.wb-frame__refresh {
  display: flex;
  padding: 2px;
  border-radius: 4px;
  opacity: 0.7;
}
.wb-frame__refresh:hover,
.wb-frame__refresh:focus-visible {
  opacity: 1;
  background: color-mix(in srgb, currentColor 12%, transparent);
  outline: none;
}
.wb-frame__input {
  font: inherit;
  color: inherit;
  background: transparent;
  border: none;
  border-bottom: 1px solid var(--color-accent);
  outline: none;
  min-width: 120px;
}
/* Thin strips along the border that take the pointer, so a frame can be
   grabbed by its edge as well as its title. */
.wb-frame__edge {
  position: absolute;
  pointer-events: auto;
  cursor: grab;
}
.wb-frame__edge--top { top: -4px; left: 0; right: 0; height: 8px; }
.wb-frame__edge--bottom { bottom: -4px; left: 0; right: 0; height: 8px; }
.wb-frame__edge--left { left: -4px; top: 0; bottom: 0; width: 8px; }
.wb-frame__edge--right { right: -4px; top: 0; bottom: 0; width: 8px; }
.wb-frame__lane {
  position: absolute;
  top: 12px;
  bottom: 12px;
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  padding: 8px 12px;
  border-radius: 8px;
  background: color-mix(in srgb, var(--frame-color) 8%, transparent);
  font-size: 13px;
  font-weight: 600;
  color: color-mix(in srgb, var(--frame-color) 60%, currentColor);
}
.wb-frame__lane-count {
  font-weight: 500;
  opacity: 0.8;
}
.wb-frame__tick {
  position: absolute;
  top: 16px;
  bottom: 12px;
  border-left: 1px dashed color-mix(in srgb, var(--frame-color) 45%, transparent);
  padding-left: 6px;
  font-size: 12px;
  color: color-mix(in srgb, var(--frame-color) 60%, currentColor);
}
.wb-frame__undated {
  position: absolute;
  left: 24px;
  right: 24px;
  padding-top: 6px;
  border-top: 1px solid color-mix(in srgb, var(--frame-color) 35%, transparent);
  font-size: 12px;
  font-weight: 600;
  color: color-mix(in srgb, var(--frame-color) 60%, currentColor);
}
.wb-frame__handle {
  width: 10px !important;
  height: 10px !important;
  background: var(--color-accent) !important;
  border: 2px solid white !important;
  opacity: 0;
  pointer-events: auto !important;
}
.wb-frame:hover .wb-frame__handle,
.wb-frame.is-selected .wb-frame__handle {
  opacity: 1;
}
</style>
