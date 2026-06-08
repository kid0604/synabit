<script setup lang="ts">
import { ref, computed } from 'vue';
import { onClickOutside } from '@vueuse/core';
import { MousePointer2, Hand, Pencil, Shapes, Type, Network, Undo2, Redo2, Download, Highlighter, Eraser, Grip, Grid3X3, Square } from 'lucide-vue-next';
import { SHAPES } from '../shapes';
import type { ToolMode, DrawSubTool } from '../composables/useWhiteboardStore';

const props = defineProps<{
  activeTool: ToolMode;
  canUndo: boolean;
  canRedo: boolean;
  drawSubTool: DrawSubTool;
  drawColor: string;
  drawSize: number;
  backgroundPattern: 'dots' | 'lines' | 'none';
  backgroundColor: string;
}>();

const emit = defineEmits<{
  (e: 'update:activeTool', tool: ToolMode): void;
  (e: 'select-shape', shape: string): void;
  (e: 'update:drawSubTool', sub: DrawSubTool): void;
  (e: 'update:drawColor', color: string): void;
  (e: 'update:drawSize', size: number): void;
  (e: 'undo'): void;
  (e: 'redo'): void;
  (e: 'export'): void;
  (e: 'update:backgroundPattern', pattern: 'dots' | 'lines' | 'none'): void;
  (e: 'update:backgroundColor', color: string): void;
}>();

const showShapeMenu = ref(false);
const showDrawMenu = ref(false);
const shapeMenuRef = ref<HTMLElement | null>(null);
const drawMenuRef = ref<HTMLElement | null>(null);

const showBgMenu = ref(false);
const bgMenuRef = ref<HTMLElement | null>(null);

onClickOutside(shapeMenuRef, () => {
  if (showShapeMenu.value) {
    showShapeMenu.value = false;
    if (props.activeTool === 'shape') emit('update:activeTool', 'select');
  }
});

onClickOutside(drawMenuRef, () => {
  if (showDrawMenu.value) {
    showDrawMenu.value = false;
  }
});

onClickOutside(bgMenuRef, () => {
  if (showBgMenu.value) {
    showBgMenu.value = false;
  }
});

const categories = [
  { key: 'basic', label: 'Basic' },
  { key: 'flowchart', label: 'Flowchart' },
  { key: 'arrow', label: 'Block Arrows' },
  { key: 'uml', label: 'UML' },
  { key: 'er', label: 'Entity Relationship' },
  { key: 'network', label: 'Network / Cloud' },
  { key: 'bpmn', label: 'BPMN' },
  { key: 'wireframe', label: 'Wireframe / UI' },
  { key: 'callout', label: 'Callouts' },
];

const drawColors = [
  '#1e1e1e', '#ef4444', '#f59e0b', '#10b981', '#3b82f6',
  '#7c3aed', '#ec4899', '#06b6d4', '#84cc16', '#f97316',
];

const drawSubIcon = computed(() => {
  if (props.drawSubTool === 'highlighter') return 'highlighter';
  if (props.drawSubTool === 'eraser') return 'eraser';
  return 'pen';
});

function selectTool(tool: ToolMode) {
  if (tool === 'shape') {
    showShapeMenu.value = !showShapeMenu.value;
    showDrawMenu.value = false;
  } else if (tool === 'draw') {
    showDrawMenu.value = !showDrawMenu.value;
    showShapeMenu.value = false;
  } else {
    showShapeMenu.value = false;
    showDrawMenu.value = false;
  }
  emit('update:activeTool', tool);
}

function selectShape(shapeId: string) {
  emit('select-shape', shapeId);
  showShapeMenu.value = false;
}

function selectDrawSub(sub: DrawSubTool) {
  emit('update:drawSubTool', sub);
  // Auto-switch to draw tool when selecting sub-tool
  if (props.activeTool !== 'draw') {
    emit('update:activeTool', 'draw');
  }
}
</script>

<template>
  <div class="wb-toolbar">
    <!-- Tools -->
    <button
      @click="selectTool('select')"
      :class="['wb-toolbar-btn', activeTool === 'select' && 'wb-toolbar-btn--active']"
      :title="$t('whiteboard.select_tool')"
    >
      <MousePointer2 class="w-4 h-4" />
    </button>
    <button
      @click="selectTool('pan')"
      :class="['wb-toolbar-btn', activeTool === 'pan' && 'wb-toolbar-btn--active']"
      :title="$t('whiteboard.pan_tool')"
    >
      <Hand class="w-4 h-4" />
    </button>

    <!-- Draw tool with sub-menu -->
    <div class="relative" ref="drawMenuRef">
      <button
        @click="selectTool('draw')"
        :class="['wb-toolbar-btn', activeTool === 'draw' && 'wb-toolbar-btn--active']"
        :title="$t('whiteboard.draw_tool')"
      >
        <Pencil v-if="drawSubIcon === 'pen'" class="w-4 h-4" />
        <Highlighter v-else-if="drawSubIcon === 'highlighter'" class="w-4 h-4" />
        <Eraser v-else class="w-4 h-4" />
      </button>
      <!-- Draw options popup -->
      <div v-if="showDrawMenu" class="wb-draw-picker" @pointerdown.stop>
        <div class="wb-draw-cat-label">{{ $t('whiteboard.tool') }}</div>
        <div class="wb-draw-sub-tools">
          <button
            @click.stop="selectDrawSub('pen')"
            :class="['wb-draw-sub-btn', drawSubTool === 'pen' && 'wb-draw-sub-btn--active']"
            title="Pen"
          >
            <Pencil class="w-4 h-4" />
            <span class="text-[10px] mt-0.5">Pen</span>
          </button>
          <button
            @click.stop="selectDrawSub('highlighter')"
            :class="['wb-draw-sub-btn', drawSubTool === 'highlighter' && 'wb-draw-sub-btn--active']"
            :title="$t('whiteboard.highlighter')"
          >
            <Highlighter class="w-4 h-4" />
            <span class="text-[10px] mt-0.5">Highlight</span>
          </button>
          <button
            @click.stop="selectDrawSub('eraser')"
            :class="['wb-draw-sub-btn', drawSubTool === 'eraser' && 'wb-draw-sub-btn--active']"
            :title="$t('whiteboard.eraser_tool')"
          >
            <Eraser class="w-4 h-4" />
            <span class="text-[10px] mt-0.5">Eraser</span>
          </button>
        </div>

        <!-- Size slider (all sub-tools) -->
        <div class="wb-draw-cat-label mt-2">{{ drawSubTool === 'eraser' ? 'Eraser Size' : 'Size' }}</div>
        <div class="flex items-center gap-2 px-1">
          <input
            type="range"
            min="1"
            :max="drawSubTool === 'eraser' ? 50 : 20"
            :value="drawSize"
            @input="$emit('update:drawSize', Number(($event.target as HTMLInputElement).value))"
            class="wb-draw-slider flex-1"
          />
          <span class="wb-draw-size-label">{{ drawSize }}px</span>
        </div>

        <!-- Color palette (not for eraser) -->
        <template v-if="drawSubTool !== 'eraser'">
          <div class="wb-draw-cat-label mt-2">{{ $t('whiteboard.color') }}</div>
          <div class="wb-draw-colors">
            <button
              v-for="c in drawColors"
              :key="c"
              @click.stop="$emit('update:drawColor', c)"
              :class="['wb-draw-color-btn', drawColor === c && 'wb-draw-color-btn--active']"
              :style="{ background: c }"
            />
          </div>
        </template>
      </div>
    </div>

    <!-- Shape tool -->
    <div class="relative" ref="shapeMenuRef">
      <button
        @click="selectTool('shape')"
        :class="['wb-toolbar-btn', activeTool === 'shape' && 'wb-toolbar-btn--active']"
        :title="$t('whiteboard.shapes_tool')"
      >
        <Shapes class="w-4 h-4" />
      </button>
      <!-- Shape picker popup -->
      <div v-if="showShapeMenu" class="wb-shape-picker">
        <div v-for="cat in categories" :key="cat.key" class="wb-shape-category">
          <div class="wb-shape-cat-label">{{ cat.label }}</div>
          <div class="wb-shape-grid">
            <button
              v-for="shape in SHAPES.filter(s => s.category === cat.key)"
              :key="shape.id"
              @click.stop="selectShape(shape.id)"
              class="wb-shape-grid-btn"
              :title="shape.label"
            >
              <svg viewBox="0 0 100 100" preserveAspectRatio="xMidYMid meet" class="w-6 h-6">
                <path
                  :d="shape.path"
                  fill="currentColor"
                  fill-opacity="0.08"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linejoin="round"
                  vector-effect="non-scaling-stroke"
                  fill-rule="evenodd"
                />
                <path
                  v-for="(deco, i) in (shape.deco || [])"
                  :key="i"
                  :d="deco"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  vector-effect="non-scaling-stroke"
                  stroke-linejoin="round"
                />
              </svg>
            </button>
          </div>
        </div>
      </div>
    </div>

    <button
      @click="selectTool('mindmap')"
      :class="['wb-toolbar-btn', activeTool === 'mindmap' && 'wb-toolbar-btn--active']"
      :title="$t('whiteboard.mindmap_tool')"
    >
      <Network class="w-4 h-4" />
    </button>
    <button
      @click="selectTool('text')"
      :class="['wb-toolbar-btn', activeTool === 'text' && 'wb-toolbar-btn--active']"
      :title="$t('whiteboard.text_tool')"
    >
      <Type class="w-4 h-4" />
    </button>

    <div class="wb-toolbar-divider" />

    <!-- Actions -->
    <button
      @click="$emit('undo')"
      :disabled="!canUndo"
      class="wb-toolbar-btn"
      :title="$t('whiteboard.undo')"
    >
      <Undo2 class="w-4 h-4" />
    </button>
    <button
      @click="$emit('redo')"
      :disabled="!canRedo"
      class="wb-toolbar-btn"
      :title="$t('whiteboard.redo')"
    >
      <Redo2 class="w-4 h-4" />
    </button>

    <div class="wb-toolbar-divider" />

    <div class="relative" ref="bgMenuRef">
      <button
        @click="showBgMenu = !showBgMenu"
        :class="['wb-toolbar-btn', showBgMenu && 'wb-toolbar-btn--active']"
        :title="$t('whiteboard.background_style')"
      >
        <Grip v-if="backgroundPattern === 'dots'" class="w-4 h-4" />
        <Grid3X3 v-else-if="backgroundPattern === 'lines'" class="w-4 h-4" />
        <Square v-else class="w-4 h-4" />
      </button>

      <!-- Background options popup -->
      <div v-if="showBgMenu" class="wb-draw-picker" @pointerdown.stop>
        <div class="wb-draw-cat-label">{{ $t('whiteboard.pattern') }}</div>
        <div class="wb-draw-sub-tools">
          <button
            @click.stop="$emit('update:backgroundPattern', 'none')"
            :class="['wb-draw-sub-btn', backgroundPattern === 'none' && 'wb-draw-sub-btn--active']"
            title="None"
          >
            <Square class="w-4 h-4" />
            <span class="text-[10px] mt-0.5">Blank</span>
          </button>
          <button
            @click.stop="$emit('update:backgroundPattern', 'dots')"
            :class="['wb-draw-sub-btn', backgroundPattern === 'dots' && 'wb-draw-sub-btn--active']"
            title="Dots"
          >
            <Grip class="w-4 h-4" />
            <span class="text-[10px] mt-0.5">Dots</span>
          </button>
          <button
            @click.stop="$emit('update:backgroundPattern', 'lines')"
            :class="['wb-draw-sub-btn', backgroundPattern === 'lines' && 'wb-draw-sub-btn--active']"
            title="Lines"
          >
            <Grid3X3 class="w-4 h-4" />
            <span class="text-[10px] mt-0.5">Lines</span>
          </button>
        </div>

        <div class="wb-draw-cat-label mt-2">{{ $t('whiteboard.background_color') }}</div>
        <div class="wb-draw-colors">
          <button
            v-for="color in ['transparent', ...drawColors]"
            :key="color"
            class="wb-draw-color-btn relative overflow-hidden"
            :class="{ 'wb-draw-color-btn--active': backgroundColor === color }"
            :style="{ backgroundColor: color === 'transparent' ? '#ffffff' : color }"
            :title="color === 'transparent' ? 'Default/Transparent' : color"
            @click.stop="$emit('update:backgroundColor', color)"
          >
            <div v-if="color === 'transparent'" class="absolute inset-0 flex items-center justify-center opacity-30">
              <div class="w-[150%] h-[1.5px] bg-black -rotate-45"></div>
            </div>
          </button>
        </div>
      </div>
    </div>

    <button
      @click="$emit('export')"
      class="wb-toolbar-btn"
      :title="$t('whiteboard.export_png')"
    >
      <Download class="w-4 h-4" />
    </button>
  </div>
</template>

<style scoped>
.wb-toolbar {
  position: absolute;
  left: 16px;
  top: 50%;
  transform: translateY(-50%);
  z-index: 50;
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 8px;
  background: var(--color-surface, #fff);
  border: 1px solid var(--color-border, #e6e6e6);
  border-radius: 14px;
  box-shadow: 0 4px 20px rgba(0,0,0,0.08);
}
.dark .wb-toolbar {
  background: var(--color-surface-dark, #1e1e1e);
  border-color: var(--color-border-dark, #2c2c2c);
  box-shadow: 0 4px 20px rgba(0,0,0,0.3);
}
.wb-toolbar-btn {
  width: 36px;
  height: 36px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 10px;
  border: none;
  background: transparent;
  color: var(--color-text-secondary, #52525b);
  cursor: pointer;
  transition: all 0.15s;
}
.dark .wb-toolbar-btn {
  color: var(--color-text-secondary-dark, #a1a1aa);
}
.wb-toolbar-btn:hover {
  background: var(--color-surface-hover, #f5f5f5);
}
.dark .wb-toolbar-btn:hover {
  background: var(--color-surface-hover-dark, #2a2a2a);
}
.wb-toolbar-btn--active {
  background: var(--color-accent, #7c3aed) !important;
  color: white !important;
}
.wb-toolbar-btn:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}
.wb-toolbar-divider {
  width: 24px;
  height: 1px;
  margin: 4px auto;
  background: var(--color-border, #e6e6e6);
}
.dark .wb-toolbar-divider {
  background: var(--color-border-dark, #2c2c2c);
}

/* ─── Draw Picker Popup ─────────────────────────── */
.wb-draw-picker {
  position: absolute;
  left: 48px;
  top: -8px;
  width: 200px;
  padding: 10px;
  background: var(--color-surface, #fff);
  border: 1px solid var(--color-border, #e6e6e6);
  border-radius: 12px;
  box-shadow: 0 8px 32px rgba(0,0,0,0.12);
  z-index: 100;
}
.dark .wb-draw-picker {
  background: var(--color-surface-dark, #1e1e1e);
  border-color: var(--color-border-dark, #2c2c2c);
  box-shadow: 0 8px 32px rgba(0,0,0,0.4);
}
.wb-draw-sub-tools {
  display: flex;
  gap: 4px;
}
.wb-draw-sub-btn {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 2px;
  padding: 6px 4px;
  border-radius: 8px;
  border: 1.5px solid transparent;
  background: var(--color-surface-hover, #f5f5f5);
  color: var(--color-text-secondary, #52525b);
  cursor: pointer;
  transition: all 0.15s;
}
.dark .wb-draw-sub-btn {
  background: var(--color-surface-hover-dark, #2a2a2a);
  color: var(--color-text-secondary-dark, #a1a1aa);
}
.wb-draw-sub-btn:hover {
  border-color: var(--color-accent, #7c3aed);
}
.wb-draw-sub-btn--active {
  border-color: var(--color-accent, #7c3aed);
  background: rgba(124, 58, 237, 0.1);
  color: var(--color-accent, #7c3aed);
}
.dark .wb-draw-sub-btn--active {
  background: rgba(124, 58, 237, 0.2);
  color: #a78bfa;
}
.wb-draw-cat-label {
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--color-text-secondary, #52525b);
  margin-bottom: 4px;
  padding-left: 2px;
}
.dark .wb-draw-cat-label {
  color: var(--color-text-secondary-dark, #a1a1aa);
}
.wb-draw-slider {
  -webkit-appearance: none;
  appearance: none;
  height: 4px;
  border-radius: 2px;
  background: var(--color-border, #e6e6e6);
  outline: none;
  cursor: pointer;
}
.dark .wb-draw-slider {
  background: var(--color-border-dark, #444);
}
.wb-draw-slider::-webkit-slider-thumb {
  -webkit-appearance: none;
  appearance: none;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: var(--color-accent, #7c3aed);
  cursor: pointer;
}
.wb-draw-size-label {
  font-size: 11px;
  min-width: 32px;
  text-align: right;
  color: var(--color-text-secondary, #52525b);
}
.dark .wb-draw-size-label {
  color: var(--color-text-secondary-dark, #a1a1aa);
}
.wb-draw-colors {
  display: flex;
  flex-wrap: wrap;
  gap: 5px;
}
.wb-draw-color-btn {
  width: 20px;
  height: 20px;
  border-radius: 50%;
  border: 2px solid transparent;
  cursor: pointer;
  transition: all 0.15s;
}
.wb-draw-color-btn:hover {
  transform: scale(1.2);
}
.wb-draw-color-btn--active {
  border-color: var(--color-accent, #7c3aed);
  box-shadow: 0 0 0 2px rgba(124, 58, 237, 0.3);
}

/* ─── Shape Picker Popup ─────────────────────────── */
.wb-shape-picker {
  position: absolute;
  left: 48px;
  top: -8px;
  width: 320px;
  max-height: 600px;
  overflow-y: auto;
  padding: 10px;
  background: var(--color-surface, #fff);
  border: 1px solid var(--color-border, #e6e6e6);
  border-radius: 12px;
  box-shadow: 0 8px 32px rgba(0,0,0,0.12);
  z-index: 100;
}
.dark .wb-shape-picker {
  background: var(--color-surface-dark, #1e1e1e);
  border-color: var(--color-border-dark, #2c2c2c);
  box-shadow: 0 8px 32px rgba(0,0,0,0.4);
}
.wb-shape-category + .wb-shape-category {
  margin-top: 8px;
  padding-top: 8px;
  border-top: 1px solid var(--color-border, #e6e6e6);
}
.dark .wb-shape-category + .wb-shape-category {
  border-color: var(--color-border-dark, #2c2c2c);
}
.wb-shape-cat-label {
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--color-text-secondary, #52525b);
  margin-bottom: 6px;
  padding-left: 2px;
}
.dark .wb-shape-cat-label {
  color: var(--color-text-secondary-dark, #a1a1aa);
}
.wb-shape-grid {
  display: grid;
  grid-template-columns: repeat(6, 1fr);
  gap: 4px;
}
.wb-shape-grid-btn {
  width: 38px;
  height: 38px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 8px;
  border: none;
  background: transparent;
  color: var(--color-text-secondary, #52525b);
  cursor: pointer;
  transition: all 0.15s;
}
.dark .wb-shape-grid-btn {
  color: var(--color-text-secondary-dark, #a1a1aa);
}
.wb-shape-grid-btn:hover {
  background: var(--color-accent, #7c3aed);
  color: white;
}
</style>
