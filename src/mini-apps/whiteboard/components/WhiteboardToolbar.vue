<script setup lang="ts">
import { ref, computed, nextTick, watch } from 'vue';
import { onClickOutside } from '@vueuse/core';
import { MousePointer2, Hand, Pencil, Shapes, Type, Network, Undo2, Redo2, Download, Highlighter, Eraser, Grip, Grid3X3, Square, Image as ImageIcon, StickyNote, Frame, SquarePlus } from 'lucide-vue-next';
import ShapePicker from './ShapePicker.vue';
import type { Glyph } from '../glyph';
import type { LibraryItem, ShapeLibrary } from '../shapeLibraries';
import { paint, PAPERS } from '../ink';
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
  snapToGrid: boolean;
  smartGuides: boolean;
  minimap: boolean;
  libraries: ShapeLibrary[];
}>();

const emit = defineEmits<{
  (e: 'update:activeTool', tool: ToolMode): void;
  (e: 'select-shape', shape: string): void;
  (e: 'pick-icon', glyph: Glyph): void;
  (e: 'pick-item', item: LibraryItem): void;
  (e: 'import-library'): void;
  (e: 'remove-library', path: string): void;
  (e: 'update:drawSubTool', sub: DrawSubTool): void;
  (e: 'update:drawColor', color: string): void;
  (e: 'update:drawSize', size: number): void;
  (e: 'undo'): void;
  (e: 'redo'): void;
  (e: 'export', options: { format: 'png' | 'svg' | 'pdf' | 'html'; transparent?: boolean }): void;
  (e: 'insert', el: HTMLElement): void;
  (e: 'add-image'): void;
  (e: 'import'): void;
  (e: 'export-file', format: 'drawio' | 'excalidraw'): void;
  (e: 'update:backgroundPattern', pattern: 'dots' | 'lines' | 'none'): void;
  (e: 'update:backgroundColor', color: string): void;
  (e: 'update:snapToGrid', on: boolean): void;
  (e: 'update:smartGuides', on: boolean): void;
  (e: 'update:minimap', on: boolean): void;
}>();

const showShapeMenu = ref(false);
const showDrawMenu = ref(false);
const shapeMenuRef = ref<HTMLElement | null>(null);
const drawMenuRef = ref<HTMLElement | null>(null);

const showBgMenu = ref(false);
const bgMenuRef = ref<HTMLElement | null>(null);

const showExportMenu = ref(false);

/**
 * On a wide window, open a picker beside its button and inside the window.
 *
 * The toolbar scrolls when the window is too short for it, and a box that
 * scrolls clips what is positioned inside it — so the pickers are fixed to
 * the window and placed here, from where their button is on screen. A picker
 * opened from a low button (Export, the last one) goes up as far as it must
 * to fit, and scrolls if the window is shorter than it is.
 */
function placePicker(container: HTMLElement | null) {
  if (!container || !window.matchMedia('(min-width: 768px)').matches) return;
  const button = container.querySelector<HTMLElement>(':scope > button');
  const picker = container.querySelector<HTMLElement>('.wb-draw-picker, .wb-shape-picker');
  if (!button || !picker) return;
  const r = button.getBoundingClientRect();
  const room = window.innerHeight - 16;
  picker.style.position = 'fixed';
  picker.style.maxHeight = `${room}px`;
  if (!picker.classList.contains('wb-shape-picker')) picker.style.overflowY = 'auto';
  const h = Math.min(picker.offsetHeight, room);
  picker.style.left = `${Math.round(r.right + 8)}px`;
  picker.style.top = `${Math.round(Math.min(Math.max(8, r.top - 8), window.innerHeight - h - 8))}px`;
  picker.style.bottom = 'auto';
}
watch(showDrawMenu, (on) => { if (on) nextTick(() => placePicker(drawMenuRef.value)); });
watch(showShapeMenu, (on) => { if (on) nextTick(() => placePicker(shapeMenuRef.value)); });
watch(showBgMenu, (on) => { if (on) nextTick(() => placePicker(bgMenuRef.value)); });
watch(showExportMenu, (on) => { if (on) nextTick(() => placePicker(exportMenuRef.value)); });
const exportMenuRef = ref<HTMLElement | null>(null);

function exportAs(format: 'png' | 'svg' | 'pdf' | 'html', transparent = false) {
  showExportMenu.value = false;
  emit('export', { format, transparent });
}

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

onClickOutside(exportMenuRef, () => {
  showExportMenu.value = false;
});

onClickOutside(bgMenuRef, () => {
  if (showBgMenu.value) {
    showBgMenu.value = false;
  }
});

const drawColors = [
  { value: '#1e1e1e', labelKey: 'whiteboard.colors.ink' },
  { value: '#ef4444', labelKey: 'whiteboard.colors.red' },
  { value: '#f59e0b', labelKey: 'whiteboard.colors.amber' },
  { value: '#10b981', labelKey: 'whiteboard.colors.green' },
  { value: '#3b82f6', labelKey: 'whiteboard.colors.blue' },
  { value: '#7c3aed', labelKey: 'whiteboard.colors.purple' },
  { value: '#ec4899', labelKey: 'whiteboard.colors.pink' },
  { value: '#06b6d4', labelKey: 'whiteboard.colors.cyan' },
  { value: '#84cc16', labelKey: 'whiteboard.colors.lime' },
  { value: '#f97316', labelKey: 'whiteboard.colors.orange' },
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

/** Escape: whichever picker is open closes. */
function closePopups(e: KeyboardEvent) {
  if (!(showDrawMenu.value || showShapeMenu.value || showBgMenu.value || showExportMenu.value)) return;
  e.stopPropagation();
  showDrawMenu.value = showShapeMenu.value = showBgMenu.value = showExportMenu.value = false;
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

const isMac = /Mac|iPhone|iPad/.test(navigator.platform);
</script>

<template>
  <div class="wb-toolbar" @keydown.escape="closePopups">
    <!-- Tools -->
    <button
      @click="selectTool('select')"
      :class="['wb-toolbar-btn', activeTool === 'select' && 'wb-toolbar-btn--active']"
      :aria-pressed="activeTool === 'select'"
      :title="$t('whiteboard.select_tool')"
      :aria-label="$t('whiteboard.select_tool')"
    >
      <MousePointer2 class="w-4 h-4" />
    </button>
    <button
      @click="selectTool('pan')"
      :class="['wb-toolbar-btn', activeTool === 'pan' && 'wb-toolbar-btn--active']"
      :aria-pressed="activeTool === 'pan'"
      :title="$t('whiteboard.pan_tool')"
      :aria-label="$t('whiteboard.pan_tool')"
    >
      <Hand class="w-4 h-4" />
    </button>

    <!-- Draw tool with sub-menu -->
    <div class="relative" ref="drawMenuRef">
      <button
        @click="selectTool('draw')"
        :class="['wb-toolbar-btn', activeTool === 'draw' && 'wb-toolbar-btn--active']"
        aria-haspopup="true"
        :aria-expanded="showDrawMenu"
      :aria-pressed="activeTool === 'draw'"
        :title="$t('whiteboard.draw_tool')"
        :aria-label="$t('whiteboard.draw_tool')"
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
            :aria-pressed="drawSubTool === 'pen'"
            :title="$t('whiteboard.pen')"
          >
            <Pencil class="w-4 h-4" />
            <span class="text-xs mt-0.5">{{ $t('whiteboard.pen') }}</span>
          </button>
          <button
            @click.stop="selectDrawSub('highlighter')"
            :class="['wb-draw-sub-btn', drawSubTool === 'highlighter' && 'wb-draw-sub-btn--active']"
            :aria-pressed="drawSubTool === 'highlighter'"
            :title="$t('whiteboard.highlighter')"
          >
            <Highlighter class="w-4 h-4" />
            <span class="text-xs mt-0.5">{{ $t('whiteboard.highlight') }}</span>
          </button>
          <button
            @click.stop="selectDrawSub('eraser')"
            :class="['wb-draw-sub-btn', drawSubTool === 'eraser' && 'wb-draw-sub-btn--active']"
            :aria-pressed="drawSubTool === 'eraser'"
            :title="$t('whiteboard.eraser_tool')"
          >
            <Eraser class="w-4 h-4" />
            <span class="text-xs mt-0.5">{{ $t('whiteboard.eraser') }}</span>
          </button>
        </div>

        <!-- Size slider (all sub-tools) -->
        <div class="wb-draw-cat-label mt-2">{{ drawSubTool === 'eraser' ? $t('whiteboard.eraser_size') : $t('whiteboard.size') }}</div>
        <div class="flex items-center gap-2 px-1">
          <input
            type="range"
            min="1"
            :max="drawSubTool === 'eraser' ? 50 : 20"
            :value="drawSize"
            @input="$emit('update:drawSize', Number(($event.target as HTMLInputElement).value))"
            class="wb-draw-slider flex-1"
            :aria-label="drawSubTool === 'eraser' ? $t('whiteboard.eraser_size') : $t('whiteboard.size')"
          />
          <span class="wb-draw-size-label">{{ drawSize }}px</span>
        </div>

        <!-- Color palette (not for eraser) -->
        <template v-if="drawSubTool !== 'eraser'">
          <div class="wb-draw-cat-label mt-2">{{ $t('whiteboard.color') }}</div>
          <div class="wb-draw-colors">
            <button
              v-for="c in drawColors"
              :key="c.value"
              @click.stop="$emit('update:drawColor', c.value)"
              :class="['wb-draw-color-btn', drawColor === c.value && 'wb-draw-color-btn--active']"
              :style="{ background: paint(c.value) }"
              :aria-pressed="drawColor === c.value"
              :aria-label="$t(c.labelKey)"
              :title="$t(c.labelKey)"
            ></button>
          </div>
        </template>
      </div>
    </div>

    <!-- Shape tool -->
    <div class="relative" ref="shapeMenuRef">
      <button
        @click="selectTool('shape')"
        :class="['wb-toolbar-btn', activeTool === 'shape' && 'wb-toolbar-btn--active']"
        aria-haspopup="true"
        :aria-expanded="showShapeMenu"
      :aria-pressed="activeTool === 'shape'"
        :title="$t('whiteboard.shapes_tool')"
        :aria-label="$t('whiteboard.shapes_tool')"
      >
        <Shapes class="w-4 h-4" />
      </button>
      <!-- Shape picker popup -->
      <div v-if="showShapeMenu" class="wb-shape-picker">
        <ShapePicker
          :libraries="libraries"
          @pick-shape="selectShape"
          @pick-icon="(g: Glyph) => { emit('pick-icon', g); showShapeMenu = false; }"
          @pick-item="(i: LibraryItem) => { emit('pick-item', i); showShapeMenu = false; }"
          @import-library="emit('import-library')"
          @remove-library="(p: string) => emit('remove-library', p)"
        />
      </div>
    </div>

    <button
      @click="selectTool('mindmap')"
      :class="['wb-toolbar-btn', activeTool === 'mindmap' && 'wb-toolbar-btn--active']"
      :aria-pressed="activeTool === 'mindmap'"
      :title="$t('whiteboard.mindmap_tool')"
      :aria-label="$t('whiteboard.mindmap_tool')"
    >
      <Network class="w-4 h-4" />
    </button>
    <button
      @click="selectTool('text')"
      :class="['wb-toolbar-btn', activeTool === 'text' && 'wb-toolbar-btn--active']"
      :aria-pressed="activeTool === 'text'"
      :title="$t('whiteboard.text_tool')"
      :aria-label="$t('whiteboard.text_tool')"
    >
      <Type class="w-4 h-4" />
    </button>
    <button
      @click="selectTool('sticky')"
      :class="['wb-toolbar-btn', activeTool === 'sticky' && 'wb-toolbar-btn--active']"
      :title="$t('whiteboard.sticky_tool')"
      :aria-label="$t('whiteboard.sticky_tool')"
      :aria-pressed="activeTool === 'sticky'"
    >
      <StickyNote class="w-4 h-4" />
    </button>
    <button
      @click="selectTool('frame')"
      :class="['wb-toolbar-btn', activeTool === 'frame' && 'wb-toolbar-btn--active']"
      :title="$t('whiteboard.frame_tool')"
      :aria-label="$t('whiteboard.frame_tool')"
      :aria-pressed="activeTool === 'frame'"
    >
      <Frame class="w-4 h-4" />
    </button>

    <button
      @click="$emit('add-image')"
      class="wb-toolbar-btn"
      :title="$t('whiteboard.image_tool')"
      :aria-label="$t('whiteboard.image_tool')"
    >
      <ImageIcon class="w-4 h-4" />
    </button>

    <div class="wb-toolbar-divider" />

    <!-- Actions -->
    <button
      @click="$emit('undo')"
      :disabled="!canUndo"
      class="wb-toolbar-btn"
      :title="`${$t('whiteboard.undo')} (${isMac ? '⌘Z' : 'Ctrl+Z'})`"
      :aria-label="$t('whiteboard.undo')"
    >
      <Undo2 class="w-4 h-4" />
    </button>
    <button
      @click="$emit('redo')"
      :disabled="!canRedo"
      class="wb-toolbar-btn"
      :title="`${$t('whiteboard.redo')} (${isMac ? '⇧⌘Z' : 'Ctrl+Y'})`"
      :aria-label="$t('whiteboard.redo')"
    >
      <Redo2 class="w-4 h-4" />
    </button>

    <div class="wb-toolbar-divider" />

    <div class="relative" ref="bgMenuRef">
      <button
        @click="showBgMenu = !showBgMenu"
        aria-haspopup="true"
        :aria-expanded="showBgMenu"
        :class="['wb-toolbar-btn', showBgMenu && 'wb-toolbar-btn--active']"
        :title="$t('whiteboard.background_style')"
        :aria-label="$t('whiteboard.background_style')"
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
            :aria-pressed="backgroundPattern === 'none'"
            :class="['wb-draw-sub-btn', backgroundPattern === 'none' && 'wb-draw-sub-btn--active']"
            :title="$t('whiteboard.blank')"
          >
            <Square class="w-4 h-4" />
            <span class="text-xs mt-0.5">{{ $t('whiteboard.blank') }}</span>
          </button>
          <button
            @click.stop="$emit('update:backgroundPattern', 'dots')"
            :aria-pressed="backgroundPattern === 'dots'"
            :class="['wb-draw-sub-btn', backgroundPattern === 'dots' && 'wb-draw-sub-btn--active']"
            :title="$t('whiteboard.dots')"
          >
            <Grip class="w-4 h-4" />
            <span class="text-xs mt-0.5">{{ $t('whiteboard.dots') }}</span>
          </button>
          <button
            @click.stop="$emit('update:backgroundPattern', 'lines')"
            :aria-pressed="backgroundPattern === 'lines'"
            :class="['wb-draw-sub-btn', backgroundPattern === 'lines' && 'wb-draw-sub-btn--active']"
            :title="$t('whiteboard.lines')"
          >
            <Grid3X3 class="w-4 h-4" />
            <span class="text-xs mt-0.5">{{ $t('whiteboard.lines') }}</span>
          </button>
        </div>

        <label class="wb-toggle-row mt-2">
          <input type="checkbox" :checked="snapToGrid" @change="$emit('update:snapToGrid', ($event.target as HTMLInputElement).checked)" />
          <span>{{ $t('whiteboard.snap_to_grid') }}</span>
        </label>
        <label class="wb-toggle-row">
          <input type="checkbox" :checked="smartGuides" @change="$emit('update:smartGuides', ($event.target as HTMLInputElement).checked)" />
          <span>{{ $t('whiteboard.smart_guides') }}</span>
        </label>
        <label class="wb-toggle-row">
          <input type="checkbox" :checked="minimap" @change="$emit('update:minimap', ($event.target as HTMLInputElement).checked)" />
          <span>{{ $t('whiteboard.minimap') }}</span>
        </label>

        <div class="wb-draw-cat-label mt-2">{{ $t('whiteboard.background_color') }}</div>
        <div class="wb-draw-colors">
          <button
            v-for="p in PAPERS"
            :key="p.value"
            class="wb-draw-color-btn relative overflow-hidden"
            :class="{ 'wb-draw-color-btn--active': backgroundColor === p.value, 'wb-paper-default': p.value === 'transparent' }"
            :style="p.value === 'transparent' ? {} : { backgroundColor: p.value }"
            :aria-pressed="backgroundColor === p.value"
            :title="$t(p.labelKey)"
            :aria-label="$t(p.labelKey)"
            @click.stop="$emit('update:backgroundColor', p.value)"
          >
            <div v-if="p.value === 'transparent'" class="absolute inset-0 flex items-center justify-center opacity-30">
              <div class="w-[150%] h-[1.5px] bg-current -rotate-45"></div>
            </div>
          </button>
        </div>
      </div>
    </div>

    <!-- Templates, vault cards, live frames: the board's own menu, also
         reachable here and not only by right-clicking empty canvas. -->
    <button
      class="wb-toolbar-btn"
      aria-haspopup="menu"
      :title="$t('whiteboard.insert')"
      :aria-label="$t('whiteboard.insert')"
      @click="(e: MouseEvent) => $emit('insert', e.currentTarget as HTMLElement)"
    >
      <SquarePlus class="w-4 h-4" />
    </button>

    <div class="relative" ref="exportMenuRef">
      <button
        @click="showExportMenu = !showExportMenu"
        :class="['wb-toolbar-btn', showExportMenu && 'wb-toolbar-btn--active']"
        :title="$t('whiteboard.export_title')"
        :aria-label="$t('whiteboard.export_title')"
        aria-haspopup="menu"
        :aria-expanded="showExportMenu"
      >
        <Download class="w-4 h-4" />
      </button>

      <div v-if="showExportMenu" class="wb-draw-picker wb-export-menu" @pointerdown.stop>
        <div class="wb-draw-cat-label">{{ $t('whiteboard.export_png') }}</div>
        <button class="wb-export-option" @click.stop="exportAs('png')">
          {{ $t('whiteboard.export_as_shown') }}
        </button>
        <button class="wb-export-option" @click.stop="exportAs('png', true)">
          {{ $t('whiteboard.export_transparent') }}
        </button>
        <div class="wb-draw-cat-label mt-2">{{ $t('whiteboard.export_title') }}</div>
        <button class="wb-export-option" @click.stop="exportAs('svg')">
          {{ $t('whiteboard.export_svg') }}
        </button>
        <button class="wb-export-option" @click.stop="exportAs('pdf')">
          {{ $t('whiteboard.export_pdf') }}
        </button>
        <div class="wb-draw-cat-label mt-2">{{ $t('whiteboard.share.title') }}</div>
        <button class="wb-export-option" :title="$t('whiteboard.share.hint')" @click.stop="exportAs('html')">
          {{ $t('whiteboard.share.web_page') }}
        </button>
        <div class="wb-draw-cat-label mt-2">{{ $t('whiteboard.export_other.title') }}</div>
        <button class="wb-export-option" :title="$t('whiteboard.export_other.hint')" @click.stop="showExportMenu = false; $emit('export-file', 'drawio')">
          {{ $t('whiteboard.export_other.drawio') }}
        </button>
        <button class="wb-export-option" :title="$t('whiteboard.export_other.hint')" @click.stop="showExportMenu = false; $emit('export-file', 'excalidraw')">
          {{ $t('whiteboard.export_other.excalidraw') }}
        </button>
        <div class="wb-export-sep" />
        <button class="wb-export-option" @click.stop="showExportMenu = false; $emit('import')">
          {{ $t('whiteboard.import_file') }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* On a narrow window the toolbar scrolls sideways, and a box that scrolls
   clips whatever is positioned inside it — the pen, shape, background and
   export pickers opened and were cut off. So there the pickers are fixed to
   the window rather than to their button, and the toolbar is centred without
   a transform: a transform would make it the box the fixed pickers are
   placed and clipped in. */
.wb-toolbar {
  position: absolute;
  left: 16px;
  right: 16px;
  bottom: 16px;
  top: auto;
  width: max-content;
  margin-inline: auto;
  z-index: 50;
  display: flex;
  flex-direction: row;
  gap: 4px;
  padding: 8px;
  background: var(--color-surface, #fff);
  border: 1px solid var(--color-border, #e6e6e6);
  border-radius: 14px;
  box-shadow: 0 4px 20px rgba(0,0,0,0.08);
  max-width: calc(100vw - 32px);
  overflow-x: auto;
}
/* Centred down the side without a transform (a transform would make the
   toolbar the box its fixed pickers are placed in), and scrolling when the
   window is shorter than it is: its buttons used to go off the top and the
   bottom of a short window, out of reach. */
@media (min-width: 768px) {
  .wb-toolbar {
    left: 16px;
    right: auto;
    top: 16px;
    bottom: 16px;
    height: max-content;
    max-height: calc(100% - 32px);
    margin-block: auto;
    margin-inline: 0;
    flex-direction: column;
    overflow-x: hidden;
    overflow-y: auto;
    scrollbar-width: none;
  }
  /* Scrolled, not squeezed: buttons keep their size. */
  .wb-toolbar > * {
    flex-shrink: 0;
  }
}
/* The theme's own canvas, shown as it is on each theme. */
.wb-paper-default {
  background: var(--color-base, #fdfdfc);
}
.dark .wb-paper-default {
  background: var(--color-base-dark, #242424);
  color: #fafafa;
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
  background: var(--color-accent) !important;
  color: white !important;
}
.wb-toolbar-btn:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}
.wb-toolbar-divider {
  width: 1px;
  height: 24px;
  margin: auto 4px;
  background: var(--color-border, #e6e6e6);
}
@media (min-width: 768px) {
  .wb-toolbar-divider {
    width: 24px;
    height: 1px;
    margin: 4px auto;
  }
}
.dark .wb-toolbar-divider {
  background: var(--color-border-dark, #2c2c2c);
}

/* ─── Draw Picker Popup ─────────────────────────── */
.wb-draw-picker {
  position: fixed;
  left: 50%;
  bottom: 76px;
  top: auto;
  transform: translateX(-50%);
  max-height: calc(100% - 140px);
  overflow-y: auto;
  width: 200px;
  padding: 10px;
  background: var(--color-surface, #fff);
  border: 1px solid var(--color-border, #e6e6e6);
  border-radius: 12px;
  box-shadow: 0 8px 32px rgba(0,0,0,0.12);
  z-index: 100;
}
@media (min-width: 768px) {
  .wb-draw-picker {
    position: absolute;
    left: 48px;
    top: -8px;
    bottom: auto;
    transform: none;
    max-height: none;
    overflow-y: visible;
  }
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
.wb-toggle-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 2px;
  font-size: 13px;
  cursor: pointer;
  color: var(--color-text, #18181b);
}
.dark .wb-toggle-row {
  color: var(--color-text-dark, #e4e4e7);
}
.wb-toggle-row input {
  accent-color: var(--color-accent);
}
.wb-export-sep {
  height: 1px;
  margin: 6px 2px;
  background: var(--color-border, #e6e6e6);
}
.dark .wb-export-sep {
  background: var(--color-border-dark, #333);
}
.wb-export-option {
  display: block;
  width: 100%;
  padding: 6px 8px;
  border-radius: 8px;
  font-size: 13px;
  text-align: left;
  color: var(--color-text, #18181b);
  transition: background-color 0.15s;
}
.wb-export-option:hover,
.wb-export-option:focus-visible {
  background: var(--color-surface-hover, #f5f5f5);
}
.dark .wb-export-option {
  color: var(--color-text-dark, #e4e4e7);
}
.dark .wb-export-option:hover,
.dark .wb-export-option:focus-visible {
  background: var(--color-surface-hover-dark, #2a2a2a);
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
  border-color: var(--color-accent);
}
.wb-draw-sub-btn--active {
  border-color: var(--color-accent);
  background: color-mix(in oklab, var(--color-accent) 10%, transparent);
  color: var(--color-accent);
}
.dark .wb-draw-sub-btn--active {
  background: color-mix(in oklab, var(--color-accent) 20%, transparent);
  color: var(--color-accent-dark);
}
.wb-draw-cat-label {
  font-size: 12px;
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
  background: var(--color-accent);
  cursor: pointer;
}
.wb-draw-size-label {
  font-size: 12px;
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
  border-color: var(--color-accent);
  box-shadow: 0 0 0 2px color-mix(in oklab, var(--color-accent) 30%, transparent);
}

/* ─── Shape Picker Popup ─────────────────────────── */
.wb-shape-picker {
  position: fixed;
  left: 50%;
  bottom: 76px;
  top: auto;
  transform: translateX(-50%);
  max-height: calc(100% - 140px);
  overflow-y: auto;
  width: 360px;
  max-width: calc(100vw - 32px);
  max-height: 460px;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  padding: 10px;
  background: var(--color-surface, #fff);
  border: 1px solid var(--color-border, #e6e6e6);
  border-radius: 12px;
  box-shadow: 0 8px 32px rgba(0,0,0,0.12);
  z-index: 100;
}
@media (min-width: 768px) {
  .wb-shape-picker {
    position: absolute;
    left: 48px;
    top: -8px;
    bottom: auto;
    transform: none;
    max-height: 600px;
  }
}
.dark .wb-shape-picker {
  background: var(--color-surface-dark, #1e1e1e);
  border-color: var(--color-border-dark, #2c2c2c);
  box-shadow: 0 8px 32px rgba(0,0,0,0.4);
}
/* The accent is 2.7:1 on the dark panel; borders, rings and text that carry
   it there take the paler dark accent. Fills under white text keep the accent. */
.dark .wb-draw-sub-btn:hover,
.dark .wb-draw-sub-btn--active,
.dark .wb-draw-color-btn--active {
  border-color: var(--color-accent-dark);
}
.dark .wb-draw-color-btn--active {
  box-shadow: 0 0 0 2px color-mix(in oklab, var(--color-accent-dark) 30%, transparent);
}
</style>
