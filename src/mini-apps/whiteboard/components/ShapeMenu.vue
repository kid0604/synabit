<script setup lang="ts">
import { ref, watch } from 'vue';
import { Trash2, X } from 'lucide-vue-next';

const props = defineProps<{
  nodeId: string;
  nodeData: {
    shapeType: string;
    label: string;
    color: string;
    fillColor?: string;
    borderWidth?: number;
    dashStyle?: string;
    opacity?: number;
    fontSize?: number;
  };
}>();

const emit = defineEmits<{
  (e: 'update', nodeId: string, data: Record<string, any>): void;
  (e: 'delete', nodeId: string): void;
  (e: 'close'): void;
}>();

// ─── Local State ────────────────────────────────────────
const strokeColor = ref(props.nodeData.color || '#7c3aed');
const fillColor = ref(props.nodeData.fillColor || '');
const borderWidth = ref(props.nodeData.borderWidth || 2);
const dashStyle = ref(props.nodeData.dashStyle || 'solid');
const opacity = ref(props.nodeData.opacity ?? 100);
const fontSize = ref(props.nodeData.fontSize || 13);
const nodeLabel = ref(props.nodeData.label || '');

// Sync on prop changes (node selection change)
watch(() => props.nodeId, () => {
  strokeColor.value = props.nodeData.color || '#7c3aed';
  fillColor.value = props.nodeData.fillColor || '';
  borderWidth.value = props.nodeData.borderWidth || 2;
  dashStyle.value = props.nodeData.dashStyle || 'solid';
  opacity.value = props.nodeData.opacity ?? 100;
  fontSize.value = props.nodeData.fontSize || 13;
  nodeLabel.value = props.nodeData.label || '';
});

const COLORS = [
  { value: '#7c3aed', label: 'Purple' },
  { value: '#3b82f6', label: 'Blue' },
  { value: '#10b981', label: 'Green' },
  { value: '#f59e0b', label: 'Amber' },
  { value: '#ef4444', label: 'Red' },
  { value: '#ec4899', label: 'Pink' },
  { value: '#06b6d4', label: 'Cyan' },
  { value: '#6b7280', label: 'Gray' },
  { value: '#000000', label: 'Black' },
];

const FILL_COLORS = [
  { value: '', label: 'None' },
  ...COLORS,
];

const WIDTHS = [1, 2, 3, 4, 5];

const DASH_STYLES = [
  { value: 'solid', label: 'Solid', dash: '0' },
  { value: 'dashed', label: 'Dashed', dash: '8 4' },
  { value: 'dotted', label: 'Dotted', dash: '2 4' },
];

const FONT_SIZES = [10, 12, 13, 14, 16, 18, 20, 24];

function emitUpdate() {
  emit('update', props.nodeId, {
    color: strokeColor.value,
    fillColor: fillColor.value,
    borderWidth: borderWidth.value,
    dashStyle: dashStyle.value,
    opacity: opacity.value,
    fontSize: fontSize.value,
    label: nodeLabel.value,
  });
}

function setStrokeColor(c: string) { strokeColor.value = c; emitUpdate(); }
function setFillColor(c: string) { fillColor.value = c; emitUpdate(); }
function setWidth(w: number) { borderWidth.value = w; emitUpdate(); }
function setDash(d: string) { dashStyle.value = d; emitUpdate(); }
function setFontSize(s: number) { fontSize.value = s; emitUpdate(); }
function updateLabel() { emitUpdate(); }

function handleDelete() {
  emit('delete', props.nodeId);
}
</script>

<template>
  <div class="sp-panel" @mousedown.stop @click.stop>
    <!-- Header -->
    <div class="sp-header">
      <span class="sp-title">Shape</span>
      <div class="sp-header-actions">
        <button @click="handleDelete" class="sp-icon-btn sp-delete-btn" title="Delete">
          <Trash2 :size="14" />
        </button>
        <button @click="$emit('close')" class="sp-icon-btn" title="Close">
          <X :size="14" />
        </button>
      </div>
    </div>

    <div class="sp-body">
      <!-- Border Color -->
      <div class="sp-section">
        <span class="sp-label">Border</span>
        <div class="sp-color-grid">
          <button
            v-for="c in COLORS"
            :key="c.value"
            @click="setStrokeColor(c.value)"
            :class="['sp-swatch', strokeColor === c.value && 'active']"
            :style="{ '--sw-color': c.value }"
            :title="c.label"
          />
        </div>
      </div>

      <!-- Fill Color -->
      <div class="sp-section">
        <span class="sp-label">Fill</span>
        <div class="sp-color-grid">
          <button
            v-for="c in FILL_COLORS"
            :key="c.value"
            @click="setFillColor(c.value)"
            :class="['sp-swatch', fillColor === c.value && 'active', !c.value && 'sp-swatch-none']"
            :style="c.value ? { '--sw-color': c.value } : {}"
            :title="c.label"
          />
        </div>
      </div>

      <!-- Border Width + Style (compact row) -->
      <div class="sp-section">
        <span class="sp-label">Stroke</span>
        <div class="sp-row">
          <div class="sp-width-group">
            <button
              v-for="w in WIDTHS"
              :key="w"
              @click="setWidth(w)"
              :class="['sp-chip', borderWidth === w && 'active']"
              :title="`${w}px`"
            >
              <div class="sp-width-bar" :style="{ height: w + 'px' }" />
            </button>
          </div>
        </div>
        <div class="sp-dash-group">
          <button
            v-for="ds in DASH_STYLES"
            :key="ds.value"
            @click="setDash(ds.value)"
            :class="['sp-dash-chip', dashStyle === ds.value && 'active']"
            :title="ds.label"
          >
            <svg viewBox="0 0 28 6" class="sp-dash-icon">
              <line x1="1" y1="3" x2="27" y2="3" stroke="currentColor" stroke-width="2"
                stroke-linecap="round" :stroke-dasharray="ds.dash" />
            </svg>
          </button>
        </div>
      </div>

      <!-- Opacity -->
      <div class="sp-section">
        <span class="sp-label">Opacity <span class="sp-value">{{ opacity }}%</span></span>
        <input
          type="range" min="10" max="100" step="5"
          v-model.number="opacity" @input="emitUpdate"
          class="sp-slider"
        />
      </div>

      <!-- Font Size -->
      <div class="sp-section">
        <span class="sp-label">Font</span>
        <div class="sp-font-row">
          <button
            v-for="s in FONT_SIZES"
            :key="s"
            @click="setFontSize(s)"
            :class="['sp-font-chip', fontSize === s && 'active']"
          >{{ s }}</button>
        </div>
      </div>

      <!-- Label -->
      <div class="sp-section">
        <span class="sp-label">Label</span>
        <input
          v-model="nodeLabel"
          @input="updateLabel"
          @keydown.enter="($event.target as HTMLInputElement).blur()"
          class="sp-input"
          placeholder="Type here…"
        />
      </div>
    </div>
  </div>
</template>

<style scoped>
.sp-panel {
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
  animation: spSlideIn 0.2s cubic-bezier(0.16, 1, 0.3, 1);
}
.dark .sp-panel {
  background: var(--color-surface-dark, #1e1e1e);
  border-color: var(--color-border-dark, #333);
  box-shadow: 0 4px 24px rgba(0,0,0,0.3);
}
@keyframes spSlideIn {
  from { opacity: 0; transform: translateX(24px); }
  to { opacity: 1; transform: translateX(0); }
}

/* ─── Header ──── */
.sp-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 10px 8px;
  border-bottom: 1px solid var(--color-border, #e6e6e6);
}
.dark .sp-header {
  border-bottom-color: var(--color-border-dark, #333);
}
.sp-title {
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.6px;
  color: var(--color-text, #18181b);
  opacity: 0.6;
}
.dark .sp-title {
  color: var(--color-text-dark, #f4f4f5);
}
.sp-header-actions {
  display: flex;
  gap: 2px;
}
.sp-icon-btn {
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
.sp-icon-btn:hover {
  background: var(--color-surface-hover, #f5f5f5);
}
.dark .sp-icon-btn:hover {
  background: var(--color-surface-hover-dark, #2a2a2a);
}
.sp-delete-btn:hover {
  background: #fee2e2 !important;
  color: #ef4444 !important;
}
.dark .sp-delete-btn:hover {
  background: rgba(239,68,68,0.15) !important;
}

/* ─── Body ──── */
.sp-body {
  flex: 1;
  overflow-y: auto;
  padding: 4px 0;
}
.sp-section {
  padding: 6px 10px;
}
.sp-label {
  display: block;
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.3px;
  color: var(--color-text-secondary, #71717a);
  margin-bottom: 5px;
}
.dark .sp-label {
  color: var(--color-text-secondary-dark, #a1a1aa);
}

/* ─── Color Swatches ──── */
.sp-color-grid {
  display: flex;
  gap: 4px;
  flex-wrap: wrap;
}
.sp-swatch {
  width: 20px;
  height: 20px;
  border-radius: 50%;
  border: 2px solid transparent;
  background: var(--sw-color);
  cursor: pointer;
  transition: all 0.12s;
  padding: 0;
}
.sp-swatch.active {
  border-color: var(--color-accent, #7c3aed);
  box-shadow: 0 0 0 2px rgba(124, 58, 237, 0.2);
}
.sp-swatch:hover:not(.active) {
  transform: scale(1.15);
}
.sp-swatch-none {
  background: var(--color-surface-hover, #f0f0f0);
  position: relative;
}
.sp-swatch-none::after {
  content: '';
  position: absolute;
  inset: 3px;
  border: 1.5px solid var(--color-text-secondary, #999);
  border-radius: 50%;
}
.sp-swatch-none::before {
  content: '';
  position: absolute;
  width: 1.5px;
  height: 70%;
  top: 15%;
  left: 50%;
  background: #ef4444;
  transform: translateX(-50%) rotate(45deg);
  border-radius: 1px;
}
.dark .sp-swatch-none {
  background: var(--color-surface-hover-dark, #2a2a2a);
}

/* ─── Stroke Width ──── */
.sp-row {
  display: flex;
  gap: 4px;
}
.sp-width-group {
  display: flex;
  gap: 3px;
  flex: 1;
}
.sp-chip {
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
.dark .sp-chip {
  background: var(--color-surface-hover-dark, #2a2a2a);
}
.sp-chip.active {
  border-color: var(--color-accent, #7c3aed);
  background: rgba(124, 58, 237, 0.08);
}
.dark .sp-chip.active {
  background: rgba(124, 58, 237, 0.15);
}
.sp-chip:hover:not(.active) {
  background: #ebebeb;
}
.dark .sp-chip:hover:not(.active) {
  background: #333;
}
.sp-width-bar {
  width: 60%;
  border-radius: 1px;
  background: var(--color-text, #18181b);
}
.dark .sp-width-bar {
  background: var(--color-text-dark, #f4f4f5);
}

/* ─── Dash Style ──── */
.sp-dash-group {
  display: flex;
  gap: 3px;
  margin-top: 5px;
}
.sp-dash-chip {
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
.dark .sp-dash-chip {
  background: var(--color-surface-hover-dark, #2a2a2a);
  color: var(--color-text-secondary-dark, #a1a1aa);
}
.sp-dash-chip.active {
  border-color: var(--color-accent, #7c3aed);
  background: rgba(124, 58, 237, 0.08);
  color: var(--color-accent, #7c3aed);
}
.dark .sp-dash-chip.active {
  background: rgba(124, 58, 237, 0.15);
  color: var(--color-accent-dark, #a78bfa);
}
.sp-dash-chip:hover:not(.active) {
  background: #ebebeb;
}
.dark .sp-dash-chip:hover:not(.active) {
  background: #333;
}
.sp-dash-icon {
  width: 28px;
  height: 6px;
}

/* ─── Opacity ──── */
.sp-value {
  float: right;
  font-weight: 500;
  color: var(--color-accent, #7c3aed);
}
.sp-slider {
  width: 100%;
  height: 4px;
  -webkit-appearance: none;
  appearance: none;
  background: var(--color-border, #e6e6e6);
  border-radius: 2px;
  outline: none;
  cursor: pointer;
}
.dark .sp-slider {
  background: var(--color-border-dark, #444);
}
.sp-slider::-webkit-slider-thumb {
  -webkit-appearance: none;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: var(--color-accent, #7c3aed);
  cursor: pointer;
  border: 2px solid white;
  box-shadow: 0 1px 3px rgba(0,0,0,0.15);
}

/* ─── Font Size ──── */
.sp-font-row {
  display: flex;
  gap: 3px;
  flex-wrap: wrap;
}
.sp-font-chip {
  min-width: 26px;
  height: 22px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 4px;
  border: 1.5px solid transparent;
  background: var(--color-surface-hover, #f5f5f5);
  cursor: pointer;
  font-size: 10px;
  font-weight: 600;
  color: var(--color-text-secondary, #71717a);
  transition: all 0.12s;
  padding: 0 2px;
}
.dark .sp-font-chip {
  background: var(--color-surface-hover-dark, #2a2a2a);
  color: var(--color-text-secondary-dark, #a1a1aa);
}
.sp-font-chip.active {
  border-color: var(--color-accent, #7c3aed);
  background: rgba(124, 58, 237, 0.08);
  color: var(--color-accent, #7c3aed);
}
.dark .sp-font-chip.active {
  background: rgba(124, 58, 237, 0.15);
  color: var(--color-accent-dark, #a78bfa);
}
.sp-font-chip:hover:not(.active) {
  background: #ebebeb;
}
.dark .sp-font-chip:hover:not(.active) {
  background: #333;
}

/* ─── Label Input ──── */
.sp-input {
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
.dark .sp-input {
  border-color: var(--color-border-dark, #444);
  color: var(--color-text-dark, #f4f4f5);
}
.sp-input:focus {
  border-color: var(--color-accent, #7c3aed);
}
</style>
