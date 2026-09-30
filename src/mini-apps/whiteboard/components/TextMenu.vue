<script setup lang="ts">
import { ref, watch } from 'vue';
import { Trash2, X, Bold, Italic, AlignLeft, AlignCenter, AlignRight } from 'lucide-vue-next';

const props = defineProps<{
  nodeId: string;
  nodeData: {
    label: string;
    fontSize?: number;
    fontWeight?: string;
    fontStyle?: string;
    textAlign?: string;
    color?: string;
    backgroundColor?: string;
    opacity?: number;
    width?: number;
  };
}>();

const emit = defineEmits<{
  (e: 'update', nodeId: string, data: Record<string, any>): void;
  (e: 'delete', nodeId: string): void;
  (e: 'close'): void;
}>();

// ─── Local State ────────────────────────────────────────
const fontSize = ref(props.nodeData.fontSize || 14);
const fontWeight = ref(props.nodeData.fontWeight || 'normal');
const fontStyle = ref(props.nodeData.fontStyle || 'normal');
const textAlign = ref(props.nodeData.textAlign || 'left');
const textColor = ref(props.nodeData.color || '#1e1e1e');
const bgColor = ref(props.nodeData.backgroundColor || '');
const opacity = ref(props.nodeData.opacity ?? 100);
const nodeWidth = ref(props.nodeData.width || 240);

// Sync on node selection change
watch(() => props.nodeId, () => {
  fontSize.value = props.nodeData.fontSize || 14;
  fontWeight.value = props.nodeData.fontWeight || 'normal';
  fontStyle.value = props.nodeData.fontStyle || 'normal';
  textAlign.value = props.nodeData.textAlign || 'left';
  textColor.value = props.nodeData.color || '#1e1e1e';
  bgColor.value = props.nodeData.backgroundColor || '';
  opacity.value = props.nodeData.opacity ?? 100;
  nodeWidth.value = props.nodeData.width || 240;
});

const COLORS = [
  { value: '#1e1e1e', labelKey: 'whiteboard.colors.black' },
  { value: '#7c3aed', labelKey: 'whiteboard.colors.purple' },
  { value: '#3b82f6', labelKey: 'whiteboard.colors.blue' },
  { value: '#10b981', labelKey: 'whiteboard.colors.green' },
  { value: '#f59e0b', labelKey: 'whiteboard.colors.amber' },
  { value: '#ef4444', labelKey: 'whiteboard.colors.red' },
  { value: '#ec4899', labelKey: 'whiteboard.colors.pink' },
  { value: '#06b6d4', labelKey: 'whiteboard.colors.cyan' },
  { value: '#6b7280', labelKey: 'whiteboard.colors.gray' },
];

const BG_COLORS = [
  { value: '', labelKey: 'whiteboard.colors.none' },
  { value: '#fef3c7', labelKey: 'whiteboard.colors.yellow' },
  { value: '#dbeafe', labelKey: 'whiteboard.colors.blue' },
  { value: '#d1fae5', labelKey: 'whiteboard.colors.green' },
  { value: '#fce7f3', labelKey: 'whiteboard.colors.pink' },
  { value: '#ede9fe', labelKey: 'whiteboard.colors.purple' },
  { value: '#e0f2fe', labelKey: 'whiteboard.colors.cyan' },
  { value: '#f3f4f6', labelKey: 'whiteboard.colors.gray' },
  { value: '#fef2f2', labelKey: 'whiteboard.colors.red' },
];

const FONT_SIZES = [10, 12, 14, 16, 18, 20, 24, 28, 32, 40, 48];

function emitUpdate() {
  emit('update', props.nodeId, {
    fontSize: fontSize.value,
    fontWeight: fontWeight.value,
    fontStyle: fontStyle.value,
    textAlign: textAlign.value,
    color: textColor.value,
    backgroundColor: bgColor.value,
    opacity: opacity.value,
    width: nodeWidth.value,
  });
}

function setColor(c: string) { textColor.value = c; emitUpdate(); }
function setBgColor(c: string) { bgColor.value = c; emitUpdate(); }
function setFontSize(s: number) { fontSize.value = s; emitUpdate(); }

function toggleBold() {
  fontWeight.value = fontWeight.value === 'bold' ? 'normal' : 'bold';
  emitUpdate();
}
function toggleItalic() {
  fontStyle.value = fontStyle.value === 'italic' ? 'normal' : 'italic';
  emitUpdate();
}
function setAlign(a: string) {
  textAlign.value = a;
  emitUpdate();
}
</script>

<template>
  <div class="sp-panel" @mousedown.stop @click.stop>
    <!-- Header -->
    <div class="sp-header">
      <span class="sp-title">{{ $t('whiteboard.text') }}</span>
      <div class="sp-header-actions">
        <button @click="$emit('delete', nodeId)" class="sp-icon-btn sp-delete-btn" :title="$t('whiteboard.delete')" :aria-label="$t('whiteboard.delete')">
          <Trash2 :size="14" />
        </button>
        <button @click="$emit('close')" class="sp-icon-btn" :title="$t('whiteboard.close')" :aria-label="$t('whiteboard.close')">
          <X :size="14" />
        </button>
      </div>
    </div>

    <div class="sp-body">
      <!-- Font Size -->
      <div class="sp-section">
        <span class="sp-label">{{ $t('whiteboard.size') }}</span>
        <div class="sp-font-row">
          <button
            v-for="s in FONT_SIZES"
            :key="s"
            @click="setFontSize(s)"
            :class="['sp-font-chip', fontSize === s && 'active']"
          >{{ s }}</button>
        </div>
      </div>

      <!-- Style: Bold / Italic -->
      <div class="sp-section">
        <span class="sp-label">{{ $t('whiteboard.style') }}</span>
        <div class="sp-row" style="gap: 4px">
          <button
            @click="toggleBold"
            :class="['sp-style-btn', fontWeight === 'bold' && 'active']"
            :title="$t('whiteboard.bold')"
          >
            <Bold :size="14" />
          </button>
          <button
            @click="toggleItalic"
            :class="['sp-style-btn', fontStyle === 'italic' && 'active']"
            :title="$t('whiteboard.italic')"
          >
            <Italic :size="14" />
          </button>
          <div class="sp-style-divider" />
          <button
            @click="setAlign('left')"
            :class="['sp-style-btn', textAlign === 'left' && 'active']"
            :title="$t('whiteboard.align_left')"
          >
            <AlignLeft :size="14" />
          </button>
          <button
            @click="setAlign('center')"
            :class="['sp-style-btn', textAlign === 'center' && 'active']"
            :title="$t('whiteboard.align_center')"
          >
            <AlignCenter :size="14" />
          </button>
          <button
            @click="setAlign('right')"
            :class="['sp-style-btn', textAlign === 'right' && 'active']"
            :title="$t('whiteboard.align_right')"
          >
            <AlignRight :size="14" />
          </button>
        </div>
      </div>

      <!-- Text Color -->
      <div class="sp-section">
        <span class="sp-label">{{ $t('whiteboard.color') }}</span>
        <div class="sp-color-grid">
          <button
            v-for="c in COLORS"
            :key="c.value"
            @click="setColor(c.value)"
            :class="['sp-swatch', textColor === c.value && 'active']"
            :style="{ '--sw-color': c.value }"
            :title="$t(c.labelKey)"
          />
        </div>
      </div>

      <!-- Background Color -->
      <div class="sp-section">
        <span class="sp-label">{{ $t('whiteboard.background') }}</span>
        <div class="sp-color-grid">
          <button
            v-for="c in BG_COLORS"
            :key="c.value"
            @click="setBgColor(c.value)"
            :class="['sp-swatch', bgColor === c.value && 'active', !c.value && 'sp-swatch-none']"
            :style="c.value ? { '--sw-color': c.value } : {}"
            :title="$t(c.labelKey)"
          />
        </div>
      </div>

      <!-- Opacity -->
      <div class="sp-section">
        <span class="sp-label">{{ $t('whiteboard.opacity') }} <span class="sp-value">{{ opacity }}%</span></span>
        <input
          type="range" min="10" max="100" step="5"
          v-model.number="opacity" @input="emitUpdate"
          :aria-label="$t('whiteboard.opacity')"
          class="sp-slider"
        />
      </div>

      <!-- Width -->
      <div class="sp-section">
        <span class="sp-label">{{ $t('whiteboard.width') }} <span class="sp-value">{{ nodeWidth }}px</span></span>
        <input
          type="range" min="80" max="600" step="10"
          v-model.number="nodeWidth" @input="emitUpdate"
          :aria-label="$t('whiteboard.width')"
          class="sp-slider"
        />
      </div>
    </div>
  </div>
</template>

<style scoped>
/* Reuse the sp-panel design system from ShapeMenu */
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
.sp-header {
  display: flex; align-items: center; justify-content: space-between;
  padding: 10px 10px 8px;
  border-bottom: 1px solid var(--color-border, #e6e6e6);
}
.dark .sp-header { border-bottom-color: var(--color-border-dark, #333); }
.sp-title { font-size: 12px; font-weight: 700; letter-spacing: 0.02em; color: var(--color-text, #18181b); }
.dark .sp-title { color: var(--color-text-dark, #fafafa); }
.sp-header-actions { display: flex; gap: 4px; }
.sp-icon-btn {
  width: 26px; height: 26px; display: flex; align-items: center; justify-content: center;
  border-radius: 7px; border: none; background: transparent;
  color: var(--color-text-secondary, #71717a); cursor: pointer; transition: all 0.15s;
}
.dark .sp-icon-btn { color: var(--color-text-secondary-dark, #a1a1aa); }
.sp-icon-btn:hover { background: var(--color-surface-hover, #f4f4f5); }
.dark .sp-icon-btn:hover { background: var(--color-surface-hover-dark, #27272a); }
.sp-delete-btn:hover { background: #fef2f2; color: #ef4444; }
.dark .sp-delete-btn:hover { background: #451a1a; color: #f87171; }

.sp-body {
  flex: 1; overflow-y: auto; padding: 6px 10px 10px;
  scrollbar-width: thin; scrollbar-color: transparent transparent;
}
.sp-body:hover { scrollbar-color: var(--color-border, #d4d4d8) transparent; }
.sp-section { margin-bottom: 10px; }
.sp-label {
  display: block; font-size: 12px; font-weight: 600; text-transform: uppercase;
  letter-spacing: 0.05em; color: var(--color-text-secondary, #71717a);
  margin-bottom: 4px; padding-left: 1px;
}
.dark .sp-label { color: var(--color-text-secondary-dark, #a1a1aa); }
.sp-value { float: right; text-transform: none; letter-spacing: 0; }

/* ─── Color Swatches ────── */
.sp-color-grid { display: flex; flex-wrap: wrap; gap: 4px; }
.sp-swatch {
  width: 22px; height: 22px; border-radius: 6px; border: 2px solid transparent;
  background: var(--sw-color); cursor: pointer; transition: all 0.15s; position: relative;
}
.sp-swatch:hover { transform: scale(1.15); }
.sp-swatch.active { border-color: var(--color-accent, #7c3aed); box-shadow: 0 0 0 2px color-mix(in oklab, var(--color-accent) 25%, transparent); }
.sp-swatch-none {
  background: var(--color-surface-hover, #f4f4f5) !important;
}
.sp-swatch-none::after {
  content: ''; position: absolute; inset: 3px;
  border-bottom: 2px solid #ef4444; transform: rotate(-45deg);
}
.dark .sp-swatch-none { background: var(--color-surface-hover-dark, #27272a) !important; }

/* ─── Font Size Chips ────── */
.sp-font-row { display: flex; flex-wrap: wrap; gap: 3px; }
.sp-font-chip {
  min-width: 28px; height: 24px; padding: 0 4px;
  display: flex; align-items: center; justify-content: center;
  border-radius: 6px; border: 1.5px solid var(--color-border, #e4e4e7);
  background: transparent; font-size: 12px; font-weight: 600;
  color: var(--color-text-secondary, #71717a); cursor: pointer; transition: all 0.15s;
}
.dark .sp-font-chip { border-color: var(--color-border-dark, #3f3f46); color: var(--color-text-secondary-dark, #a1a1aa); }
.sp-font-chip:hover { border-color: var(--color-accent, #7c3aed); }
.sp-font-chip.active {
  background: var(--color-accent, #7c3aed); color: white; border-color: transparent;
}

/* ─── Style Buttons ────── */
.sp-row { display: flex; align-items: center; }
.sp-style-btn {
  width: 30px; height: 28px; display: flex; align-items: center; justify-content: center;
  border-radius: 7px; border: 1.5px solid var(--color-border, #e4e4e7);
  background: transparent; color: var(--color-text-secondary, #71717a);
  cursor: pointer; transition: all 0.15s;
}
.dark .sp-style-btn { border-color: var(--color-border-dark, #3f3f46); color: var(--color-text-secondary-dark, #a1a1aa); }
.sp-style-btn:hover { border-color: var(--color-accent, #7c3aed); }
.sp-style-btn.active {
  background: var(--color-accent, #7c3aed); color: white; border-color: transparent;
}
.sp-style-divider {
  width: 1px; height: 20px; margin: 0 4px;
  background: var(--color-border, #e4e4e7);
}
.dark .sp-style-divider { background: var(--color-border-dark, #3f3f46); }

/* ─── Slider ────── */
.sp-slider {
  width: 100%; -webkit-appearance: none; appearance: none;
  height: 4px; border-radius: 2px; background: var(--color-border, #e4e4e7);
  outline: none; cursor: pointer;
}
.dark .sp-slider { background: var(--color-border-dark, #3f3f46); }
.sp-slider::-webkit-slider-thumb {
  -webkit-appearance: none; width: 14px; height: 14px; border-radius: 50%;
  background: var(--color-accent, #7c3aed); cursor: pointer;
}
</style>
