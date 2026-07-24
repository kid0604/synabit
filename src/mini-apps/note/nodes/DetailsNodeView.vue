<template>
  <node-view-wrapper class="details-node my-3" :class="{ 'is-open': isOpen }">
    <div class="details-wrapper" :class="{ 'is-open': isOpen }">
      <!-- Summary / Toggle Header -->
      <div 
        class="details-summary"
        contenteditable="false"
        @click="toggleOpen"
      >
        <button class="toggle-btn" :class="{ 'is-open': isOpen }" contenteditable="false" aria-label="Chevron Right Icon">
          <ChevronRightIcon class="toggle-icon" />
        </button>
        <span
          class="summary-text"
          :contenteditable="true"
          :suppress-content-editable-warning="true"
          @blur="onSummaryBlur"
          @keydown.enter.prevent="onSummaryEnter"
          @click.stop
          ref="summaryInput"
        >{{ node.attrs.summary }}</span>
      </div>

      <!-- Collapsible Content -->
      <div class="details-body" v-show="isOpen">
        <node-view-content class="details-content-inner" />
      </div>
    </div>
  </node-view-wrapper>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue';
import { NodeViewWrapper, NodeViewContent, nodeViewProps } from '@tiptap/vue-3';
import { ChevronRight as ChevronRightIcon } from 'lucide-vue-next';

const props = defineProps(nodeViewProps);

const summaryInput = ref<HTMLElement | null>(null);

const isOpen = computed(() => props.node.attrs.open);

const toggleOpen = () => {
  props.updateAttributes({ open: !isOpen.value });
};

const onSummaryBlur = (e: FocusEvent) => {
  const text = (e.target as HTMLElement)?.textContent?.trim() || 'Toggle';
  if (text !== props.node.attrs.summary) {
    props.updateAttributes({ summary: text });
  }
};

const onSummaryEnter = () => {
  // On Enter in summary, open the content area and focus it
  if (!isOpen.value) {
    props.updateAttributes({ open: true });
  }
  // Move focus into the content area
  const contentEl = props.editor.view.dom.querySelector('.details-content-inner');
  if (contentEl) {
    (contentEl as HTMLElement).focus();
  }
};
</script>

<style>
/* Details Node — Toggle Block */
.details-node {
  position: relative;
}

.details-wrapper {
  border: 1px solid #e5e7eb;
  border-radius: 8px;
  overflow: hidden;
  transition: border-color 0.2s ease, box-shadow 0.2s ease;
}

.details-wrapper:hover {
  border-color: #d1d5db;
}

.details-wrapper.is-open {
  border-color: #d1d5db;
}

/* Summary header */
.details-summary {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 12px;
  cursor: pointer;
  user-select: none;
  background: #f9fafb;
  transition: background 0.15s ease;
}

.details-summary:hover {
  background: #f3f4f6;
}

/* Toggle arrow button */
.toggle-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border: none;
  background: transparent;
  padding: 0;
  cursor: pointer;
  border-radius: 4px;
  color: #9ca3af;
  flex-shrink: 0;
  transition: transform 0.2s ease, color 0.15s ease;
}

.toggle-btn:hover {
  color: #6b7280;
  background: #e5e7eb;
}

.toggle-btn.is-open {
  transform: rotate(90deg);
}

.toggle-icon {
  width: 14px;
  height: 14px;
}

/* Summary text (editable) */
.summary-text {
  font-weight: 600;
  font-size: 0.9375rem;
  line-height: 1.4;
  color: #374151;
  outline: none;
  flex: 1;
  cursor: text;
  min-width: 0;
}

.summary-text:focus {
  color: #111827;
}

.summary-text:empty::before {
  content: 'Toggle heading...';
  color: #9ca3af;
}

/* Collapsible body */
.details-body {
  border-top: 1px solid #e5e7eb;
  animation: details-slide-down 0.15s ease-out;
}

.details-content-inner {
  padding: 10px 16px 10px 38px;
  min-height: 1.5em;
}

.details-content-inner > *:first-child {
  margin-top: 0;
}

.details-content-inner > *:last-child {
  margin-bottom: 0;
}

@keyframes details-slide-down {
  from {
    opacity: 0;
    transform: translateY(-4px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

/* ─── Dark Mode ─── */
.dark .details-wrapper {
  border-color: #374151;
}

.dark .details-wrapper:hover,
.dark .details-wrapper.is-open {
  border-color: #4b5563;
}

.dark .details-summary {
  background: #1f2937;
}

.dark .details-summary:hover {
  background: #263244;
}

.dark .toggle-btn {
  color: #6b7280;
}

.dark .toggle-btn:hover {
  color: #9ca3af;
  background: #374151;
}

.dark .summary-text {
  color: #e5e7eb;
}

.dark .summary-text:focus {
  color: #f9fafb;
}

.dark .summary-text:empty::before {
  color: #6b7280;
}

.dark .details-body {
  border-top-color: #374151;
}
</style>
