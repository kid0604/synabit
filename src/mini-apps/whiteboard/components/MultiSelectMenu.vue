<script setup lang="ts">
import { computed } from 'vue';
import {
  Trash2, X, Group, Ungroup,
  AlignStartVertical, AlignCenterVertical, AlignEndVertical,
  AlignStartHorizontal, AlignCenterHorizontal, AlignEndHorizontal,
  AlignHorizontalDistributeCenter, AlignVerticalDistributeCenter,
  MoveHorizontal, MoveVertical, BringToFront, SendToBack, Lock, LockOpen, Copy, Ellipsis } from 'lucide-vue-next';
import CustomColorSwatch from './CustomColorSwatch.vue';

const props = defineProps<{
  selectedNodes: { id: string; type: string; data: any }[];
}>();

const emit = defineEmits<{
  (e: 'group'): void;
  (e: 'ungroup'): void;
  (e: 'delete'): void;
  (e: 'update-all', data: Record<string, any>): void;
  /** One of the arrange actions below, by id; the canvas does the work. */
  (e: 'action', id: string, el?: HTMLElement): void;
  (e: 'close'): void;
}>();

// Check if ALL selected nodes share the same groupId → they're already grouped together
const isGrouped = computed(() => {
  const nodes = props.selectedNodes;
  if (nodes.length < 2) return false;
  const firstGroup = nodes[0]?.data?.groupId;
  if (!firstGroup) return false;
  return nodes.every((n) => n.data?.groupId === firstGroup);
});

const COLORS = [
  { value: '#7c3aed', labelKey: 'whiteboard.colors.purple' },
  { value: '#3b82f6', labelKey: 'whiteboard.colors.blue' },
  { value: '#10b981', labelKey: 'whiteboard.colors.green' },
  { value: '#f59e0b', labelKey: 'whiteboard.colors.amber' },
  { value: '#ef4444', labelKey: 'whiteboard.colors.red' },
  { value: '#ec4899', labelKey: 'whiteboard.colors.pink' },
  { value: '#06b6d4', labelKey: 'whiteboard.colors.cyan' },
  { value: '#6b7280', labelKey: 'whiteboard.colors.gray' },
  { value: '#000000', labelKey: 'whiteboard.colors.black' },
];

const FILL_COLORS = [
  { value: '', labelKey: 'whiteboard.colors.none' },
  ...COLORS,
];

const allLocked = computed(() => props.selectedNodes.every((n) => n.data?.locked));

/** The arrange buttons. Distribute needs three things to space; the rest two. */
const ARRANGE = computed(() => [
  { id: 'align-left', icon: AlignStartVertical, labelKey: 'whiteboard.arrange.align_left' },
  { id: 'align-centerX', icon: AlignCenterVertical, labelKey: 'whiteboard.arrange.align_center_x' },
  { id: 'align-right', icon: AlignEndVertical, labelKey: 'whiteboard.arrange.align_right' },
  { id: 'align-top', icon: AlignStartHorizontal, labelKey: 'whiteboard.arrange.align_top' },
  { id: 'align-centerY', icon: AlignCenterHorizontal, labelKey: 'whiteboard.arrange.align_center_y' },
  { id: 'align-bottom', icon: AlignEndHorizontal, labelKey: 'whiteboard.arrange.align_bottom' },
  { id: 'distribute-x', icon: AlignHorizontalDistributeCenter, labelKey: 'whiteboard.arrange.distribute_x', disabled: props.selectedNodes.length < 3 },
  { id: 'distribute-y', icon: AlignVerticalDistributeCenter, labelKey: 'whiteboard.arrange.distribute_y', disabled: props.selectedNodes.length < 3 },
  { id: 'same-width', icon: MoveHorizontal, labelKey: 'whiteboard.arrange.same_width' },
  { id: 'same-height', icon: MoveVertical, labelKey: 'whiteboard.arrange.same_height' },
  { id: 'front', icon: BringToFront, labelKey: 'whiteboard.ctx.to_front' },
  { id: 'back', icon: SendToBack, labelKey: 'whiteboard.ctx.to_back' },
  { id: 'lock', icon: allLocked.value ? LockOpen : Lock, labelKey: allLocked.value ? 'whiteboard.ctx.unlock' : 'whiteboard.ctx.lock' },
  { id: 'duplicate', icon: Copy, labelKey: 'whiteboard.arrange.duplicate' },
]);

function doGroup() { emit('group'); }
function doUngroup() { emit('ungroup'); }
function doDelete() { emit('delete'); }
function doClose() { emit('close'); }
function setStrokeColor(c: string) { emit('update-all', { color: c }); }
function setFillColor(c: string) { emit('update-all', { fillColor: c }); }
</script>

<template>
  <div class="sp-panel" role="region" :aria-label="$t('whiteboard.panel_for', { what: $t('whiteboard.n_selected', { count: selectedNodes.length }) })" @mousedown.stop @click.stop @keydown.escape.stop="$emit('close')">
    <!-- Header -->
    <div class="sp-header">
      <span class="sp-title">{{ $t('whiteboard.n_selected', { count: selectedNodes.length }) }}</span>
      <div class="sp-header-actions">
        <button @click="doDelete" class="sp-icon-btn sp-delete-btn" :title="$t('whiteboard.delete_all')" :aria-label="$t('whiteboard.delete_all')">
          <Trash2 :size="14" />
        </button>
        <button
          class="sp-icon-btn"
          aria-haspopup="menu"
          :title="$t('whiteboard.ctx.more')"
          :aria-label="$t('whiteboard.ctx.more')"
          @click="(e: MouseEvent) => emit('action', 'more', e.currentTarget as HTMLElement)"
        >
          <Ellipsis :size="14" />
        </button>
        <button @click="doClose" class="sp-icon-btn" :title="$t('whiteboard.close')" :aria-label="$t('whiteboard.close')">
          <X :size="14" />
        </button>
      </div>
    </div>

    <div class="sp-body">
      <!-- Arrange -->
      <div class="sp-section">
        <span class="sp-label">{{ $t('whiteboard.arrange.title') }}</span>
        <div class="sp-tool-grid">
          <button
            v-for="a in ARRANGE"
            :key="a.id"
            class="sp-tool"
            :disabled="a.disabled"
            :title="$t(a.labelKey)"
            :aria-label="$t(a.labelKey)"
            @click="emit('action', a.id)"
          >
            <component :is="a.icon" :size="15" />
          </button>
        </div>
      </div>

      <!-- Group / Ungroup -->
      <div class="sp-section">
        <span class="sp-label">{{ $t('whiteboard.organize') }}</span>
        <button
          v-if="!isGrouped"
          class="sp-action-btn"
          @click="doGroup"
        >
          <Group :size="14" />
          <span>{{ $t('whiteboard.group') }}</span>
        </button>
        <button
          v-else
          class="sp-action-btn sp-action-ungroup"
          @click="doUngroup"
        >
          <Ungroup :size="14" />
          <span>{{ $t('whiteboard.ungroup') }}</span>
        </button>
      </div>

      <!-- Bulk Border Color -->
      <div class="sp-section">
        <span class="sp-label">{{ $t('whiteboard.border_color') }}</span>
        <div class="sp-color-grid">
          <button
            v-for="c in COLORS"
            :key="c.value"
            @click="setStrokeColor(c.value)"
            class="sp-swatch"
            :style="{ '--sw-color': c.value }"
            :title="$t(c.labelKey)" :aria-label="$t(c.labelKey)"
          />
          <CustomColorSwatch :presets="COLORS.map((c) => c.value)" :label="$t('whiteboard.custom_color')" @pick="setStrokeColor" />
        </div>
      </div>

      <!-- Bulk Fill Color -->
      <div class="sp-section">
        <span class="sp-label">{{ $t('whiteboard.fill_color') }}</span>
        <div class="sp-color-grid">
          <button
            v-for="c in FILL_COLORS"
            :key="c.value"
            @click="setFillColor(c.value)"
            :class="['sp-swatch', !c.value && 'sp-swatch-none']"
            :style="c.value ? { '--sw-color': c.value } : {}"
            :title="$t(c.labelKey)" :aria-label="$t(c.labelKey)"
          />
          <CustomColorSwatch :presets="FILL_COLORS.map((c) => c.value)" :label="$t('whiteboard.custom_color')" @pick="setFillColor" />
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.sp-tool-grid {
  display: grid;
  grid-template-columns: repeat(6, 1fr);
  gap: 4px;
}
.sp-tool {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 30px;
  border-radius: 6px;
  color: var(--color-text-secondary, #52525b);
  transition: background-color 0.12s, color 0.12s;
}
.sp-tool:hover:not(:disabled),
.sp-tool:focus-visible {
  background: var(--color-surface-hover, #f4f4f5);
  color: var(--color-accent);
  outline: none;
}
.dark .sp-tool {
  color: var(--color-text-secondary-dark, #a1a1aa);
}
.dark .sp-tool:hover:not(:disabled),
.dark .sp-tool:focus-visible {
  background: var(--color-surface-hover-dark, #2a2a2a);
  color: var(--color-accent-dark);
}
.sp-tool:disabled {
  opacity: 0.35;
  cursor: default;
}
/* Reuse the same panel design system as ShapeMenu / EdgeMenu */
.sp-panel {
  position: fixed;
  top: 48px;
  right: 0;
  width: 232px;
  bottom: 0;
  z-index: 100;
  display: flex;
  flex-direction: column;
  background: var(--color-surface, #ffffff);
  border-left: 1px solid var(--color-border, #e6e6e6);
  box-shadow: -2px 0 12px rgba(0, 0, 0, 0.06);
  animation: sp-slide-in 0.15s ease-out;
}
.dark .sp-panel {
  background: var(--color-surface-dark, #1a1a1a);
  border-left-color: var(--color-border-dark, #333);
  box-shadow: -2px 0 12px rgba(0, 0, 0, 0.3);
}
@keyframes sp-slide-in {
  from { transform: translateX(100%); opacity: 0; }
  to { transform: translateX(0); opacity: 1; }
}

.sp-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 10px;
  border-bottom: 1px solid var(--color-border, #e6e6e6);
}
.dark .sp-header {
  border-bottom-color: var(--color-border-dark, #333);
}
.sp-title {
  font-size: 12px;
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
.dark .sp-icon-btn { color: var(--color-text-secondary-dark, #a1a1aa); }
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
  background: rgba(239, 68, 68, 0.15) !important;
}

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
  font-size: 12px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.3px;
  color: var(--color-text-secondary, #71717a);
  margin-bottom: 5px;
}
.dark .sp-label {
  color: var(--color-text-secondary-dark, #a1a1aa);
}

/* ─── Action Button ──── */
.sp-action-btn {
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  height: 32px;
  border-radius: 6px;
  border: 1.5px solid var(--color-border, #e6e6e6);
  background: var(--color-surface-hover, #f5f5f5);
  cursor: pointer;
  font-size: 12px;
  font-weight: 600;
  color: var(--color-text, #18181b);
  transition: all 0.12s;
}
.dark .sp-action-btn {
  border-color: var(--color-border-dark, #444);
  background: var(--color-surface-hover-dark, #2a2a2a);
  color: var(--color-text-dark, #f4f4f5);
}
.sp-action-btn:hover {
  border-color: var(--color-accent);
  background: color-mix(in oklab, var(--color-accent) 8%, transparent);
  color: var(--color-accent);
}
.dark .sp-action-btn:hover {
  background: color-mix(in oklab, var(--color-accent) 15%, transparent);
  color: var(--color-accent-dark);
}
.sp-action-ungroup {
  border-color: #fbbf24;
  color: #b45309;
}
.dark .sp-action-ungroup {
  border-color: #92400e;
  color: #fbbf24;
}
.sp-action-ungroup:hover {
  border-color: #f59e0b !important;
  background: rgba(245, 158, 11, 0.08) !important;
  color: #d97706 !important;
}
.dark .sp-action-ungroup:hover {
  background: rgba(245, 158, 11, 0.15) !important;
  color: #fbbf24 !important;
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
.sp-swatch:hover {
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
/* The accent is 2.7:1 on the dark panel; borders, rings and text that carry
   it there take the paler dark accent. Fills under white text keep the accent. */
.dark .sp-action-btn:hover {
  border-color: var(--color-accent-dark);
}
</style>
