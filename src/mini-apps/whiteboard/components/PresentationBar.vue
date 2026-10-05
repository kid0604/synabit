<script setup lang="ts">
import { ChevronLeft, ChevronRight, X, Pointer } from 'lucide-vue-next';

/** The controls along the bottom of a presented board. */
defineProps<{ index: number; count: number; title: string; laser: boolean }>();
defineEmits<{
  (e: 'prev'): void;
  (e: 'next'): void;
  (e: 'laser'): void;
  (e: 'exit'): void;
}>();
</script>

<template>
  <div class="wb-present-bar" role="toolbar" :aria-label="$t('whiteboard.present.controls')" @pointerdown.stop @click.stop>
    <button class="wb-present-btn" :disabled="index === 0" :aria-label="$t('whiteboard.present.prev')" :title="$t('whiteboard.present.prev')" @click="$emit('prev')">
      <ChevronLeft class="w-4 h-4" />
    </button>
    <span class="wb-present-where" aria-live="polite">
      <span class="tabular-nums">{{ index + 1 }} / {{ count }}</span>
      <span v-if="title" class="wb-present-title">{{ title }}</span>
    </span>
    <button class="wb-present-btn" :disabled="index >= count - 1" :aria-label="$t('whiteboard.present.next')" :title="$t('whiteboard.present.next')" @click="$emit('next')">
      <ChevronRight class="w-4 h-4" />
    </button>
    <span class="wb-present-sep" />
    <button
      class="wb-present-btn"
      :class="{ 'wb-present-btn--on': laser }"
      :aria-pressed="laser"
      :aria-label="$t('whiteboard.present.laser')"
      :title="$t('whiteboard.present.laser') + ' (L)'"
      @click="$emit('laser')"
    >
      <Pointer class="w-4 h-4" />
    </button>
    <button class="wb-present-btn" :aria-label="$t('whiteboard.present.exit')" :title="$t('whiteboard.present.exit') + ' (Esc)'" @click="$emit('exit')">
      <X class="w-4 h-4" />
    </button>
  </div>
</template>

<style scoped>
/* Out of the way of the slide until it is wanted: faint until the pointer
   reaches it or a key moves into it. */
.wb-present-bar {
  position: absolute;
  left: 50%;
  bottom: 16px;
  transform: translateX(-50%);
  z-index: 20;
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 6px;
  max-width: calc(100% - 32px);
  border-radius: 12px;
  background: rgb(24 24 27 / 0.85);
  color: #fafafa;
  opacity: 0.4;
  transition: opacity 0.2s;
}
.wb-present-bar:hover,
.wb-present-bar:focus-within {
  opacity: 1;
}
.wb-present-btn {
  display: grid;
  place-items: center;
  width: 32px;
  height: 32px;
  border-radius: 8px;
}
.wb-present-btn:hover:not(:disabled) {
  background: rgb(255 255 255 / 0.12);
}
.wb-present-btn:disabled {
  opacity: 0.35;
}
.wb-present-btn--on {
  color: #f87171;
}
.wb-present-where {
  display: flex;
  align-items: baseline;
  gap: 8px;
  min-width: 0;
  padding: 0 6px;
  font-size: 13px;
}
.wb-present-title {
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
  max-width: 40ch;
  opacity: 0.8;
}
.wb-present-sep {
  width: 1px;
  height: 20px;
  margin: 0 4px;
  background: rgb(255 255 255 / 0.2);
}
@media (prefers-reduced-motion: reduce) {
  .wb-present-bar { transition: none; }
}
</style>
