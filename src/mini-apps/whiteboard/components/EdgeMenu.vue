<script setup lang="ts">
import { ref, watch } from 'vue';
import { Trash2, X } from 'lucide-vue-next';
import CustomColorSwatch from './CustomColorSwatch.vue';
import { MARKER_KINDS, type MarkerPart } from '../edgeMarkers';

const props = defineProps<{
  edgeId: string;
  edgeData: {
    type: string;
    color?: string;
    strokeWidth?: number;
    animated?: boolean;
    label?: string;
    markerEnd?: string;
    markerStart?: string;
    dashStyle?: string;
    sides?: string;
  };
}>();

const emit = defineEmits<{
  (e: 'update', edgeId: string, data: Record<string, any>): void;
  (e: 'delete', edgeId: string): void;
  (e: 'close'): void;
}>();

// ─── Local State ────────────────────────────────────────
const edgeType = ref(props.edgeData.type || 'default');
const edgeColor = ref(props.edgeData.color || '');
const strokeWidth = ref(props.edgeData.strokeWidth || 2);
const animated = ref(props.edgeData.animated || false);
/** Whether the line picks the sides that face each other (routing.ts), or keeps the handles it was drawn from. */
const autoSides = ref(props.edgeData.sides === 'auto');
const edgeLabel = ref(props.edgeData.label || '');
const markerEnd = ref(props.edgeData.markerEnd || 'none');
const markerStart = ref(props.edgeData.markerStart || 'none');
const dashStyle = ref(props.edgeData.dashStyle || 'solid');

// Follow the edge itself, not only which edge: an undo, or a change merged in
// from another copy of the board, has to show here before the next click
// sends this panel's copy back.
watch(() => props.edgeData, () => {
  edgeType.value = props.edgeData.type || 'default';
  edgeColor.value = props.edgeData.color || '';
  strokeWidth.value = props.edgeData.strokeWidth || 2;
  animated.value = props.edgeData.animated || false;
  autoSides.value = props.edgeData.sides === 'auto';
  edgeLabel.value = props.edgeData.label || '';
  markerEnd.value = props.edgeData.markerEnd || 'none';
  markerStart.value = props.edgeData.markerStart || 'none';
  dashStyle.value = props.edgeData.dashStyle || 'solid';
}, { deep: true });

const EDGE_TYPES = [
  { value: 'straight', labelKey: 'whiteboard.edge_type.straight' },
  { value: 'default', labelKey: 'whiteboard.edge_type.curve' },
  { value: 'step', labelKey: 'whiteboard.edge_type.step' },
];

const ARROW_MODES = [
  { value: 'none', labelKey: 'whiteboard.arrow_mode.none' },
  { value: 'forward', labelKey: 'whiteboard.arrow_mode.forward' },
  { value: 'backward', labelKey: 'whiteboard.arrow_mode.backward' },
  { value: 'both', labelKey: 'whiteboard.arrow_mode.both' },
];

const COLORS = [
  { value: '', labelKey: 'whiteboard.colors.default' },
  { value: '#7c3aed', labelKey: 'whiteboard.colors.purple' },
  { value: '#3b82f6', labelKey: 'whiteboard.colors.blue' },
  { value: '#10b981', labelKey: 'whiteboard.colors.green' },
  { value: '#f59e0b', labelKey: 'whiteboard.colors.amber' },
  { value: '#ef4444', labelKey: 'whiteboard.colors.red' },
  { value: '#ec4899', labelKey: 'whiteboard.colors.pink' },
  { value: '#6b7280', labelKey: 'whiteboard.colors.gray' },
  { value: '#000000', labelKey: 'whiteboard.colors.black' },
];

const WIDTHS = [1, 2, 3, 4, 5];

const DASH_STYLES = [
  { value: 'solid', labelKey: 'whiteboard.dash.solid', dash: '0' },
  { value: 'dashed', labelKey: 'whiteboard.dash.dashed', dash: '8 4' },
  { value: 'dotted', labelKey: 'whiteboard.dash.dotted', dash: '2 4' },
];

function getCurrentArrowMode(): string {
  const hasEnd = markerEnd.value === 'arrow';
  const hasStart = markerStart.value === 'arrow';
  if (hasEnd && hasStart) return 'both';
  if (hasEnd) return 'forward';
  if (hasStart) return 'backward';
  return 'none';
}

function emitUpdate() {
  emit('update', props.edgeId, {
    type: edgeType.value,
    color: edgeColor.value,
    strokeWidth: strokeWidth.value,
    animated: animated.value,
    label: edgeLabel.value,
    markerEnd: markerEnd.value,
    markerStart: markerStart.value,
    dashStyle: dashStyle.value,
    sides: autoSides.value ? 'auto' : undefined,
  });
}

function setType(type: string) { edgeType.value = type; emitUpdate(); }
function setColor(color: string) { edgeColor.value = color; emitUpdate(); }
function setWidth(w: number) { strokeWidth.value = w; emitUpdate(); }
function toggleAnimated() { animated.value = !animated.value; emitUpdate(); }
function toggleAutoSides() { autoSides.value = !autoSides.value; emitUpdate(); }
function updateLabel() { emitUpdate(); }
function setDashStyle(style: string) { dashStyle.value = style; emitUpdate(); }

function setArrowMode(mode: string) {
  switch (mode) {
    case 'forward':
      markerEnd.value = 'arrow'; markerStart.value = 'none'; break;
    case 'backward':
      markerEnd.value = 'none'; markerStart.value = 'arrow'; break;
    case 'both':
      markerEnd.value = 'arrow'; markerStart.value = 'arrow'; break;
    default:
      markerEnd.value = 'none'; markerStart.value = 'none';
  }
  emitUpdate();
}

/** The ends a line can have, `none` first. */
const MARKER_CHOICES = [{ id: 'none', labelKey: 'whiteboard.marker.none', parts: [] as MarkerPart[] }, ...MARKER_KINDS];

function setMarker(which: 'start' | 'end', kind: string) {
  if (which === 'start') markerStart.value = kind;
  else markerEnd.value = kind;
  emitUpdate();
}

/** A marker part drawn in the panel's own colours, for its icon. */
function iconPaint(part: MarkerPart) {
  if (part.paint === 'solid') return { fill: 'currentColor', stroke: 'currentColor' };
  if (part.paint === 'hollow') return { fill: 'var(--color-surface, #fff)', stroke: 'currentColor' };
  return { fill: 'none', stroke: 'currentColor' };
}

function handleDelete() { emit('delete', props.edgeId); }
</script>

<template>
  <div class="ep-panel" role="region" :aria-label="$t('whiteboard.panel_for', { what: $t('whiteboard.edge') })" @mousedown.stop @click.stop @keydown.escape.stop="$emit('close')">
    <!-- Header -->
    <div class="ep-header">
      <span class="ep-title">{{ $t('whiteboard.edge') }}</span>
      <div class="ep-header-actions">
        <button @click="handleDelete" class="ep-icon-btn ep-delete-btn" :title="$t('whiteboard.delete')" :aria-label="$t('whiteboard.delete')">
          <Trash2 :size="14" />
        </button>
        <button @click="$emit('close')" class="ep-icon-btn" :title="$t('whiteboard.close')" :aria-label="$t('whiteboard.close')">
          <X :size="14" />
        </button>
      </div>
    </div>

    <div class="ep-body">
      <!-- Edge Type -->
      <div class="ep-section">
        <span class="ep-label">{{ $t('whiteboard.style') }}</span>
        <div class="ep-type-row">
          <button
            v-for="t in EDGE_TYPES" :key="t.value"
            @click="setType(t.value)"
            :class="['ep-type-btn', edgeType === t.value && 'active']"
            :title="$t(t.labelKey)"
          >
            <svg viewBox="0 0 32 20" class="ep-type-svg">
              <line v-if="t.value === 'straight'" x1="2" y1="18" x2="30" y2="2" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
              <path v-if="t.value === 'default'" d="M2 18 C2 6, 30 14, 30 2" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
              <path v-if="t.value === 'step'" d="M2 18 L2 10 L30 10 L30 2" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
            </svg>
            <span class="ep-type-name">{{ $t(t.labelKey) }}</span>
          </button>
        </div>
      </div>

      <!-- Arrow Direction -->
      <div class="ep-section">
        <span class="ep-label">{{ $t('whiteboard.arrow') }}</span>
        <div class="ep-arrow-row">
          <button
            v-for="a in ARROW_MODES" :key="a.value"
            @click="setArrowMode(a.value)"
            :class="['ep-arrow-btn', getCurrentArrowMode() === a.value && 'active']"
            :title="$t(a.labelKey)"
          >
            <svg viewBox="0 0 36 16" class="ep-arrow-svg">
              <template v-if="a.value === 'none'">
                <line x1="4" y1="8" x2="32" y2="8" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
              </template>
              <template v-if="a.value === 'forward'">
                <line x1="4" y1="8" x2="28" y2="8" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
                <polyline points="24,3 32,8 24,13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
              </template>
              <template v-if="a.value === 'backward'">
                <line x1="8" y1="8" x2="32" y2="8" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
                <polyline points="12,3 4,8 12,13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
              </template>
              <template v-if="a.value === 'both'">
                <line x1="10" y1="8" x2="26" y2="8" stroke="currentColor" stroke-width="2" stroke-linecap="round" />
                <polyline points="14,3 6,8 14,13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
                <polyline points="22,3 30,8 22,13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />
              </template>
            </svg>
          </button>
        </div>
      </div>

      <!-- Line ends: arrows, UML and ER notation -->
      <div v-for="which in (['start', 'end'] as const)" :key="which" class="ep-section">
        <span class="ep-label">{{ $t('whiteboard.marker.' + which) }}</span>
        <div class="ep-marker-grid">
          <button
            v-for="k in MARKER_CHOICES"
            :key="k.id"
            :class="['ep-marker-btn', (which === 'start' ? markerStart : markerEnd) === k.id && 'active']"
            :title="$t(k.labelKey)"
            :aria-label="$t(k.labelKey)"
            :aria-pressed="(which === 'start' ? markerStart : markerEnd) === k.id"
            @click="setMarker(which, k.id)"
          >
            <svg viewBox="0 0 40 20" class="ep-marker-icon" :class="{ 'ep-marker-icon--start': which === 'start' }">
              <line x1="2" y1="10" :x2="k.parts.length ? 24 : 38" y2="10" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
              <g transform="translate(18, 0)">
                <template v-for="(part, i) in k.parts" :key="i">
                  <circle v-if="part.cx !== undefined" :cx="part.cx" cy="10" r="4" stroke-width="1.5" :style="iconPaint(part)" />
                  <path v-else :d="part.d" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" :style="iconPaint(part)" />
                </template>
              </g>
            </svg>
          </button>
        </div>
      </div>

      <!-- Color -->
      <div class="ep-section">
        <span class="ep-label">{{ $t('whiteboard.color') }}</span>
        <div class="ep-color-grid">
          <button
            v-for="c in COLORS" :key="c.value"
            @click="setColor(c.value)"
            :class="['ep-swatch', edgeColor === c.value && 'active', !c.value && 'ep-swatch-default']"
            :style="c.value ? { '--sw-color': c.value } : {}"
            :title="$t(c.labelKey)" :aria-label="$t(c.labelKey)"
          />
          <CustomColorSwatch :value="edgeColor" :presets="COLORS.map((c) => c.value)" :label="$t('whiteboard.custom_color')" @pick="setColor" />
        </div>
      </div>

      <!-- Stroke Width -->
      <div class="ep-section">
        <span class="ep-label">{{ $t('whiteboard.width') }}</span>
        <div class="ep-width-row">
          <button
            v-for="w in WIDTHS" :key="w"
            @click="setWidth(w)"
            :class="['ep-chip', strokeWidth === w && 'active']"
            :title="`${w}px`"
          >
            <div class="ep-width-bar" :style="{ height: w + 'px' }" />
          </button>
        </div>
      </div>

      <!-- Dash Style -->
      <div class="ep-section">
        <span class="ep-label">{{ $t('whiteboard.stroke') }}</span>
        <div class="ep-dash-row">
          <button
            v-for="ds in DASH_STYLES" :key="ds.value"
            @click="setDashStyle(ds.value)"
            :class="['ep-dash-chip', dashStyle === ds.value && 'active']"
            :title="$t(ds.labelKey)"
          >
            <svg viewBox="0 0 28 6" class="ep-dash-icon">
              <line x1="1" y1="3" x2="27" y2="3" stroke="currentColor" stroke-width="2"
                stroke-linecap="round" :stroke-dasharray="ds.dash" />
            </svg>
          </button>
        </div>
      </div>

      <!-- Animated Toggle -->
      <div class="ep-section ep-toggle-row">
        <span class="ep-label">{{ $t('whiteboard.animated') }}</span>
        <button @click="toggleAnimated" :class="['ep-toggle', animated && 'active']" :aria-label="$t('whiteboard.animated')" :aria-pressed="animated">
          <div class="ep-toggle-thumb" />
        </button>
      </div>

      <div class="ep-section ep-toggle-row">
        <span class="ep-label">{{ $t('whiteboard.auto_sides') }}</span>
        <button @click="toggleAutoSides" :class="['ep-toggle', autoSides && 'active']" :aria-label="$t('whiteboard.auto_sides')" :aria-pressed="autoSides" :title="$t('whiteboard.auto_sides_hint')">
          <div class="ep-toggle-thumb" />
        </button>
      </div>

      <!-- Label -->
      <div class="ep-section">
        <span class="ep-label">{{ $t('whiteboard.label') }}</span>
        <input
          v-model="edgeLabel"
          @input="updateLabel"
          @keydown.enter="($event.target as HTMLInputElement).blur()"
          class="ep-input"
          :placeholder="$t('whiteboard.type_here')"
        />
      </div>
    </div>
  </div>
</template>

<style scoped>
.ep-marker-grid {
  display: grid;
  grid-template-columns: repeat(5, 1fr);
  gap: 3px;
}
.ep-marker-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 26px;
  border-radius: 6px;
  color: var(--color-text-secondary, #52525b);
}
.ep-marker-btn:hover,
.ep-marker-btn:focus-visible,
.ep-marker-btn.active {
  background: var(--color-surface-hover, #f4f4f5);
  color: var(--color-accent);
  outline: none;
}
.dark .ep-marker-btn {
  color: var(--color-text-secondary-dark, #a1a1aa);
}
.dark .ep-marker-btn:hover,
.dark .ep-marker-btn:focus-visible,
.dark .ep-marker-btn.active {
  background: var(--color-surface-hover-dark, #2a2a2a);
  color: var(--color-accent-dark);
}
.ep-marker-icon {
  width: 34px;
  height: 17px;
  overflow: visible;
}
/* The start of a line points the other way. */
.ep-marker-icon--start {
  transform: scaleX(-1);
}
/* ─── Panel Shell (matches ShapeMenu) ──── */
.ep-panel {
  position: fixed;
  top: 60px;
  right: 12px;
  bottom: 12px;
  width: 232px;
  background: var(--color-surface, #fff);
  border: 1px solid var(--color-border, #e6e6e6);
  border-radius: 14px;
  box-shadow: 0 4px 24px rgba(0,0,0,0.08);
  z-index: 100;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  animation: epSlideIn 0.2s cubic-bezier(0.16, 1, 0.3, 1);
}
.dark .ep-panel {
  background: var(--color-surface-dark, #1e1e1e);
  border-color: var(--color-border-dark, #333);
  box-shadow: 0 4px 24px rgba(0,0,0,0.3);
}
@keyframes epSlideIn {
  from { opacity: 0; transform: translateX(24px); }
  to { opacity: 1; transform: translateX(0); }
}

/* ─── Header ──── */
.ep-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 10px 8px;
  border-bottom: 1px solid var(--color-border, #e6e6e6);
}
.dark .ep-header {
  border-bottom-color: var(--color-border-dark, #333);
}
.ep-title {
  font-size: 12px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.6px;
  color: var(--color-text, #18181b);
  opacity: 0.6;
}
.dark .ep-title {
  color: var(--color-text-dark, #f4f4f5);
}
.ep-header-actions {
  display: flex;
  gap: 2px;
}
.ep-icon-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  border-radius: 6px;
  border: none;
  background: transparent;
  cursor: pointer;
  color: var(--color-text-secondary, #71717a);
  transition: all 0.12s;
}
.dark .ep-icon-btn { color: var(--color-text-secondary-dark, #a1a1aa); }
.ep-icon-btn:hover {
  background: var(--color-surface-hover, #f5f5f5);
}
.dark .ep-icon-btn:hover {
  background: var(--color-surface-hover-dark, #2a2a2a);
}
.ep-delete-btn:hover {
  background: #fee2e2 !important;
  color: #ef4444 !important;
}
.dark .ep-delete-btn:hover {
  background: rgba(239,68,68,0.15) !important;
}

/* ─── Body ──── */
.ep-body {
  flex: 1;
  overflow-y: auto;
  padding: 4px 0;
}
.ep-section {
  padding: 6px 10px;
}
.ep-label {
  display: block;
  font-size: 12px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.3px;
  color: var(--color-text-secondary, #71717a);
  margin-bottom: 5px;
}
.dark .ep-label {
  color: var(--color-text-secondary-dark, #a1a1aa);
}

/* ─── Edge Type ──── */
.ep-type-row {
  display: flex;
  gap: 3px;
}
.ep-type-btn {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 3px;
  padding: 5px 2px;
  border-radius: 6px;
  border: 1.5px solid transparent;
  background: var(--color-surface-hover, #f5f5f5);
  cursor: pointer;
  transition: all 0.12s;
  color: var(--color-text-secondary, #71717a);
}
.dark .ep-type-btn {
  background: var(--color-surface-hover-dark, #2a2a2a);
  color: var(--color-text-secondary-dark, #a1a1aa);
}
.ep-type-btn.active {
  border-color: var(--color-accent);
  background: color-mix(in oklab, var(--color-accent) 8%, transparent);
  color: var(--color-accent);
}
.dark .ep-type-btn.active {
  background: color-mix(in oklab, var(--color-accent) 15%, transparent);
  color: var(--color-accent-dark);
}
.ep-type-btn:hover:not(.active) { background: #ebebeb; }
.dark .ep-type-btn:hover:not(.active) { background: #333; }
.ep-type-svg { width: 28px; height: 18px; }
.ep-type-name { font-size: 12px; font-weight: 600; line-height: 1.2; text-align: center; }

/* ─── Arrow Direction ──── */
.ep-arrow-row {
  display: flex;
  gap: 3px;
}
.ep-arrow-btn {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 5px 2px;
  border-radius: 6px;
  border: 1.5px solid transparent;
  background: var(--color-surface-hover, #f5f5f5);
  cursor: pointer;
  transition: all 0.12s;
  color: var(--color-text-secondary, #71717a);
}
.dark .ep-arrow-btn {
  background: var(--color-surface-hover-dark, #2a2a2a);
  color: var(--color-text-secondary-dark, #a1a1aa);
}
.ep-arrow-btn.active {
  border-color: var(--color-accent);
  background: color-mix(in oklab, var(--color-accent) 8%, transparent);
  color: var(--color-accent);
}
.dark .ep-arrow-btn.active {
  background: color-mix(in oklab, var(--color-accent) 15%, transparent);
  color: var(--color-accent-dark);
}
.ep-arrow-btn:hover:not(.active) { background: #ebebeb; }
.dark .ep-arrow-btn:hover:not(.active) { background: #333; }
.ep-arrow-svg { width: 32px; height: 14px; }

/* ─── Color Swatches ──── */
.ep-color-grid {
  display: flex;
  gap: 4px;
  flex-wrap: wrap;
}
.ep-swatch {
  width: 20px;
  height: 20px;
  border-radius: 50%;
  border: 2px solid transparent;
  background: var(--sw-color);
  cursor: pointer;
  transition: all 0.12s;
  padding: 0;
}
.ep-swatch.active {
  border-color: var(--color-accent);
  box-shadow: 0 0 0 2px color-mix(in oklab, var(--color-accent) 20%, transparent);
}
.ep-swatch:hover:not(.active) { transform: scale(1.15); }
.ep-swatch-default {
  background: var(--color-surface-hover, #eee);
  position: relative;
}
.ep-swatch-default::after {
  content: '—';
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 12px;
  font-weight: 700;
  color: var(--color-text-secondary, #999);
}
.dark .ep-swatch-default {
  background: var(--color-surface-hover-dark, #2a2a2a);
}

/* ─── Width ──── */
.ep-width-row {
  display: flex;
  gap: 3px;
}
.ep-chip {
  flex: 1;
  height: 26px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 5px;
  border: 1.5px solid transparent;
  background: var(--color-surface-hover, #f5f5f5);
  cursor: pointer;
  transition: all 0.12s;
  padding: 0;
}
.dark .ep-chip {
  background: var(--color-surface-hover-dark, #2a2a2a);
}
.ep-chip.active {
  border-color: var(--color-accent);
  background: color-mix(in oklab, var(--color-accent) 8%, transparent);
}
.dark .ep-chip.active {
  background: color-mix(in oklab, var(--color-accent) 15%, transparent);
}
.ep-chip:hover:not(.active) { background: #ebebeb; }
.dark .ep-chip:hover:not(.active) { background: #333; }
.ep-width-bar {
  width: 60%;
  border-radius: 1px;
  background: var(--color-text, #18181b);
}
.dark .ep-width-bar {
  background: var(--color-text-dark, #f4f4f5);
}

/* ─── Dash Style ──── */
.ep-dash-row {
  display: flex;
  gap: 3px;
}
.ep-dash-chip {
  flex: 1;
  height: 26px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 5px;
  border: 1.5px solid transparent;
  background: var(--color-surface-hover, #f5f5f5);
  cursor: pointer;
  transition: all 0.12s;
  color: var(--color-text-secondary, #71717a);
  padding: 0;
}
.dark .ep-dash-chip {
  background: var(--color-surface-hover-dark, #2a2a2a);
  color: var(--color-text-secondary-dark, #a1a1aa);
}
.ep-dash-chip.active {
  border-color: var(--color-accent);
  background: color-mix(in oklab, var(--color-accent) 8%, transparent);
  color: var(--color-accent);
}
.dark .ep-dash-chip.active {
  background: color-mix(in oklab, var(--color-accent) 15%, transparent);
  color: var(--color-accent-dark);
}
.ep-dash-chip:hover:not(.active) { background: #ebebeb; }
.dark .ep-dash-chip:hover:not(.active) { background: #333; }
.ep-dash-icon { width: 28px; height: 6px; }

/* ─── Animated Toggle ──── */
.ep-toggle-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.ep-toggle-row .ep-label { margin-bottom: 0; }
.ep-toggle {
  width: 34px;
  height: 20px;
  border-radius: 10px;
  background: var(--color-surface-hover, #d4d4d8);
  border: none;
  cursor: pointer;
  position: relative;
  transition: background 0.2s;
  padding: 0;
}
.dark .ep-toggle { background: #3f3f46; }
.ep-toggle.active { background: var(--color-accent); }
.ep-toggle-thumb {
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: white;
  position: absolute;
  top: 2px;
  left: 2px;
  transition: transform 0.2s;
  box-shadow: 0 1px 3px rgba(0,0,0,0.15);
}
.ep-toggle.active .ep-toggle-thumb {
  transform: translateX(14px);
}

/* ─── Label Input ──── */
.ep-input {
  width: 100%;
  padding: 5px 8px;
  border: 1px solid var(--color-border, #e6e6e6);
  border-radius: 6px;
  font-size: 12px;
  background: transparent;
  color: var(--color-text, #18181b);
  outline: none;
  transition: border-color 0.12s;
  box-sizing: border-box;
}
.dark .ep-input {
  border-color: var(--color-border-dark, #444);
  color: var(--color-text-dark, #f4f4f5);
}
.ep-input:focus {
  border-color: var(--color-accent);
}
/* The accent is 2.7:1 on the dark panel; borders, rings and text that carry
   it there take the paler dark accent. Fills under white text keep the accent. */
.dark .ep-type-btn.active,
.dark .ep-arrow-btn.active,
.dark .ep-swatch.active,
.dark .ep-chip.active,
.dark .ep-dash-chip.active,
.dark .ep-input:focus {
  border-color: var(--color-accent-dark);
}
.dark .ep-swatch.active {
  box-shadow: 0 0 0 2px color-mix(in oklab, var(--color-accent-dark) 25%, transparent);
}
</style>
