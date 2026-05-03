<script setup lang="ts">
import { computed } from 'vue';
import { Trash2, X, Group, Ungroup } from 'lucide-vue-next';

const props = defineProps<{
  selectedNodes: { id: string; type: string; data: any }[];
}>();

const emit = defineEmits<{
  (e: 'group'): void;
  (e: 'ungroup'): void;
  (e: 'delete'): void;
  (e: 'update-all', data: Record<string, any>): void;
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

function doGroup() { emit('group'); }
function doUngroup() { emit('ungroup'); }
function doDelete() { emit('delete'); }
function doClose() { emit('close'); }
function setStrokeColor(c: string) { emit('update-all', { color: c }); }
function setFillColor(c: string) { emit('update-all', { fillColor: c }); }
</script>

<template>
  <div class="sp-panel" @mousedown.stop @click.stop>
    <!-- Header -->
    <div class="sp-header">
      <span class="sp-title">{{ selectedNodes.length }} Selected</span>
      <div class="sp-header-actions">
        <button @click="doDelete" class="sp-icon-btn sp-delete-btn" title="Delete All">
          <Trash2 :size="14" />
        </button>
        <button @click="doClose" class="sp-icon-btn" title="Close">
          <X :size="14" />
        </button>
      </div>
    </div>

    <div class="sp-body">
      <!-- Group / Ungroup -->
      <div class="sp-section">
        <span class="sp-label">Organize</span>
        <button
          v-if="!isGrouped"
          class="sp-action-btn"
          @click="doGroup"
        >
          <Group :size="14" />
          <span>Group</span>
        </button>
        <button
          v-else
          class="sp-action-btn sp-action-ungroup"
          @click="doUngroup"
        >
          <Ungroup :size="14" />
          <span>Ungroup</span>
        </button>
      </div>

      <!-- Bulk Border Color -->
      <div class="sp-section">
        <span class="sp-label">Border Color</span>
        <div class="sp-color-grid">
          <button
            v-for="c in COLORS"
            :key="c.value"
            @click="setStrokeColor(c.value)"
            class="sp-swatch"
            :style="{ '--sw-color': c.value }"
            :title="c.label"
          />
        </div>
      </div>

      <!-- Bulk Fill Color -->
      <div class="sp-section">
        <span class="sp-label">Fill Color</span>
        <div class="sp-color-grid">
          <button
            v-for="c in FILL_COLORS"
            :key="c.value"
            @click="setFillColor(c.value)"
            :class="['sp-swatch', !c.value && 'sp-swatch-none']"
            :style="c.value ? { '--sw-color': c.value } : {}"
            :title="c.label"
          />
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
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
  border-color: var(--color-accent, #7c3aed);
  background: rgba(124, 58, 237, 0.08);
  color: var(--color-accent, #7c3aed);
}
.dark .sp-action-btn:hover {
  background: rgba(124, 58, 237, 0.15);
  color: var(--color-accent-dark, #a78bfa);
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
</style>
