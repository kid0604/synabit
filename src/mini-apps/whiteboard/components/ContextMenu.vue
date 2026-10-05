<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref } from 'vue';

export interface ContextMenuItem {
  id: string;
  label: string;
  /** The key that does the same, shown at the right. */
  shortcut?: string;
  disabled?: boolean;
  danger?: boolean;
  /** A line above this item. */
  separated?: boolean;
}

const props = defineProps<{
  x: number;
  y: number;
  items: ContextMenuItem[];
  label: string;
}>();

const emit = defineEmits<{
  (e: 'select', id: string): void;
  (e: 'close'): void;
}>();

const menuRef = ref<HTMLElement | null>(null);
const position = ref({ left: props.x, top: props.y });

/** The enabled items, as elements, in order. */
const buttons = () =>
  Array.from(menuRef.value?.querySelectorAll<HTMLButtonElement>('button[role="menuitem"]:not([disabled])') ?? []);

/**
 * Open where the pointer is, kept on screen, with the first item focused so
 * the keyboard can take it from there.
 */
onMounted(async () => {
  await nextTick();
  const el = menuRef.value;
  if (!el) return;
  const rect = el.getBoundingClientRect();
  position.value = {
    left: Math.max(8, Math.min(props.x, window.innerWidth - rect.width - 8)),
    top: Math.max(8, Math.min(props.y, window.innerHeight - rect.height - 8)),
  };
  buttons()[0]?.focus();
  window.addEventListener('pointerdown', onPointerDown, true);
  window.addEventListener('blur', close);
});

onUnmounted(() => {
  window.removeEventListener('pointerdown', onPointerDown, true);
  window.removeEventListener('blur', close);
});

function close() {
  emit('close');
}

function onPointerDown(e: PointerEvent) {
  if (!menuRef.value?.contains(e.target as Node)) close();
}

function choose(item: ContextMenuItem) {
  if (item.disabled) return;
  emit('select', item.id);
  close();
}

/** Arrow keys move between items, Home and End go to the ends, Escape closes. */
function onKeydown(e: KeyboardEvent) {
  const list = buttons();
  const at = list.indexOf(document.activeElement as HTMLButtonElement);
  const go = (i: number) => list[(i + list.length) % list.length]?.focus();
  if (e.key === 'ArrowDown') go(at + 1);
  else if (e.key === 'ArrowUp') go(at - 1);
  else if (e.key === 'Home') go(0);
  else if (e.key === 'End') go(list.length - 1);
  else if (e.key === 'Escape' || e.key === 'Tab') close();
  else return;
  e.preventDefault();
  e.stopPropagation();
}
</script>

<template>
  <div
    ref="menuRef"
    class="wb-context-menu"
    role="menu"
    :aria-label="label"
    :style="{ left: position.left + 'px', top: position.top + 'px' }"
    @keydown="onKeydown"
    @contextmenu.prevent
  >
    <template v-for="item in items" :key="item.id">
      <div v-if="item.separated" class="wb-context-sep" role="separator" />
      <button
        role="menuitem"
        class="wb-context-item"
        :class="{ 'wb-context-item--danger': item.danger }"
        :disabled="item.disabled"
        @click="choose(item)"
      >
        <span>{{ item.label }}</span>
        <kbd v-if="item.shortcut" class="wb-context-kbd">{{ item.shortcut }}</kbd>
      </button>
    </template>
  </div>
</template>

<style scoped>
.wb-context-menu {
  position: fixed;
  z-index: 200;
  min-width: 220px;
  padding: 4px;
  background: var(--color-surface, #fff);
  border: 1px solid var(--color-border, #e6e6e6);
  border-radius: 10px;
  box-shadow: 0 8px 28px rgba(0, 0, 0, 0.14);
}
.dark .wb-context-menu {
  background: var(--color-surface-dark, #1e1e1e);
  border-color: var(--color-border-dark, #333);
  box-shadow: 0 8px 28px rgba(0, 0, 0, 0.45);
}
.wb-context-item {
  display: flex;
  width: 100%;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 6px 10px;
  border-radius: 6px;
  font-size: 13px;
  text-align: left;
  color: var(--color-text, #18181b);
}
.dark .wb-context-item {
  color: var(--color-text-dark, #e4e4e7);
}
.wb-context-item:hover:not(:disabled),
.wb-context-item:focus-visible {
  background: var(--color-surface-hover, #f4f4f5);
  outline: none;
}
.dark .wb-context-item:hover:not(:disabled),
.dark .wb-context-item:focus-visible {
  background: var(--color-surface-hover-dark, #2a2a2a);
}
.wb-context-item:disabled {
  opacity: 0.4;
  cursor: default;
}
.wb-context-item--danger {
  color: var(--color-danger, #dc2626);
}
.wb-context-kbd {
  font-family: inherit;
  font-size: 12px;
  color: var(--color-muted, #8b8b8b);
}
.wb-context-sep {
  height: 1px;
  margin: 4px 6px;
  background: var(--color-border, #e6e6e6);
}
.dark .wb-context-sep {
  background: var(--color-border-dark, #333);
}
</style>
