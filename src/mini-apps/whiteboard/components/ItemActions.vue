<script setup lang="ts">
import { BringToFront, SendToBack, Lock, LockOpen, Copy, Paintbrush, Pipette, Ellipsis } from 'lucide-vue-next';

/**
 * The row of things a property panel can do to its item besides styling it:
 * stack it, lock it, duplicate it, carry its look to another item. The same
 * actions as the right-click menu and the keyboard, for whoever finds them here.
 */
defineProps<{ locked?: boolean; canPasteStyle?: boolean }>();
const emit = defineEmits<{ (e: 'action', id: string, el?: HTMLElement): void }>();
</script>

<template>
  <div class="wb-item-actions">
    <button class="wb-item-action" :title="$t('whiteboard.ctx.to_front')" :aria-label="$t('whiteboard.ctx.to_front')" @click="emit('action', 'front')">
      <BringToFront :size="15" />
    </button>
    <button class="wb-item-action" :title="$t('whiteboard.ctx.to_back')" :aria-label="$t('whiteboard.ctx.to_back')" @click="emit('action', 'back')">
      <SendToBack :size="15" />
    </button>
    <button
      class="wb-item-action"
      :aria-pressed="!!locked"
      :title="$t(locked ? 'whiteboard.ctx.unlock' : 'whiteboard.ctx.lock')"
      :aria-label="$t(locked ? 'whiteboard.ctx.unlock' : 'whiteboard.ctx.lock')"
      @click="emit('action', 'lock')"
    >
      <LockOpen v-if="locked" :size="15" />
      <Lock v-else :size="15" />
    </button>
    <button class="wb-item-action" :title="$t('whiteboard.arrange.duplicate')" :aria-label="$t('whiteboard.arrange.duplicate')" @click="emit('action', 'duplicate')">
      <Copy :size="15" />
    </button>
    <button class="wb-item-action" :title="$t('whiteboard.ctx.copy_style')" :aria-label="$t('whiteboard.ctx.copy_style')" @click="emit('action', 'copy-style')">
      <Pipette :size="15" />
    </button>
    <button
      class="wb-item-action"
      :disabled="!canPasteStyle"
      :title="$t('whiteboard.ctx.paste_style')"
      :aria-label="$t('whiteboard.ctx.paste_style')"
      @click="emit('action', 'paste-style')"
    >
      <Paintbrush :size="15" />
    </button>
    <!-- Everything else the right-click menu has: Syn, export, frame… -->
    <button
      class="wb-item-action"
      aria-haspopup="menu"
      :title="$t('whiteboard.ctx.more')"
      :aria-label="$t('whiteboard.ctx.more')"
      @click="(e: MouseEvent) => emit('action', 'more', e.currentTarget as HTMLElement)"
    >
      <Ellipsis :size="15" />
    </button>
  </div>
</template>

<style scoped>
.wb-item-actions {
  display: grid;
  grid-template-columns: repeat(7, 1fr);
  gap: 4px;
}
.wb-item-action {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 30px;
  border-radius: 6px;
  color: var(--color-text-secondary, #52525b);
  transition: background-color 0.12s, color 0.12s;
}
.wb-item-action:hover:not(:disabled),
.wb-item-action:focus-visible,
.wb-item-action[aria-pressed='true'] {
  background: var(--color-surface-hover, #f4f4f5);
  color: var(--color-accent);
  outline: none;
}
.dark .wb-item-action {
  color: var(--color-text-secondary-dark, #a1a1aa);
}
.dark .wb-item-action:hover:not(:disabled),
.dark .wb-item-action:focus-visible,
.dark .wb-item-action[aria-pressed='true'] {
  background: var(--color-surface-hover-dark, #2a2a2a);
  color: var(--color-accent-dark);
}
.wb-item-action:disabled {
  opacity: 0.35;
  cursor: default;
}
</style>
