<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { ChevronDown, ChevronUp, X } from 'lucide-vue-next';
import { findOnBoard } from '../boardSearch';
import type { WBNode } from '../boardFile';

/**
 * Find words on the board (Ctrl+F): each match in turn is selected and
 * brought into view.
 */
const props = defineProps<{ nodes: WBNode[]; hidden: Set<string> }>();
const emit = defineEmits<{ (e: 'show', id: string): void; (e: 'close'): void }>();

const query = ref('');
const at = ref(0);
const input = ref<HTMLInputElement | null>(null);
const matches = computed(() => findOnBoard(props.nodes, query.value, props.hidden));

const root = ref<HTMLElement | null>(null);
/** The match last brought into view, if any — where focus lands on close. */
const shown = ref<string | null>(null);

/**
 * Where focus was before the bar opened. Closing it (Escape, or the ×) used
 * to unmount the input that had focus and leave it on the page itself, so a
 * keyboard user started again from the top. Now it goes to the match the bar
 * last showed — the thing they were looking for — or back where it came from.
 */
let before: HTMLElement | null = null;
onMounted(() => {
  before = document.activeElement as HTMLElement | null;
  nextTick(() => input.value?.focus());
});
onBeforeUnmount(() => {
  // Only if focus is still ours to give back: a click on the board that
  // closed the bar has already put it somewhere the user chose.
  const active = document.activeElement;
  if (active && active !== document.body && !root.value?.contains(active)) return;
  if (shown.value) {
    // Compared rather than put in a selector: an id is the file's to choose.
    const node = [...document.querySelectorAll<HTMLElement>('.vue-flow__node')].find((el) => el.dataset.id === shown.value);
    node?.focus({ preventScroll: true });
    if (node && document.activeElement === node) return;
  }
  if (before?.isConnected && before !== document.body) before.focus({ preventScroll: true });
});

function show(id: string) {
  shown.value = id;
  emit('show', id);
}

// The first match is shown once typing pauses: moving the board on every
// letter made it lurch from match to match while the word was half typed.
let pause: ReturnType<typeof setTimeout> | null = null;
watch(query, () => {
  at.value = 0;
  shown.value = null;
  if (pause) clearTimeout(pause);
  pause = setTimeout(() => { if (matches.value.length) show(matches.value[0].id); }, 150);
});
onBeforeUnmount(() => { if (pause) clearTimeout(pause); });

function step(by: number) {
  const n = matches.value.length;
  if (!n) return;
  at.value = (at.value + by + n) % n;
  show(matches.value[at.value].id);
}

function onKey(e: KeyboardEvent) {
  if (e.key === 'Enter') { e.preventDefault(); step(e.shiftKey ? -1 : 1); }
  else if (e.key === 'Escape') { e.preventDefault(); emit('close'); }
  e.stopPropagation();
}
</script>

<template>
  <div ref="root" class="wb-search" role="search" @pointerdown.stop>
    <input
      ref="input"
      v-model="query"
      class="wb-search__input"
      :placeholder="$t('whiteboard.search.placeholder')"
      :aria-label="$t('whiteboard.search.placeholder')"
      @keydown="onKey"
    />
    <span class="wb-search__count" aria-live="polite">
      {{ query.trim() ? (matches.length ? $t('whiteboard.search.count', { at: at + 1, of: matches.length }) : $t('whiteboard.search.none')) : '' }}
    </span>
    <button class="wb-search__btn" :disabled="!matches.length" :aria-label="$t('whiteboard.search.prev')" :title="$t('whiteboard.search.prev')" @click="step(-1)"><ChevronUp :size="15" /></button>
    <button class="wb-search__btn" :disabled="!matches.length" :aria-label="$t('whiteboard.search.next')" :title="$t('whiteboard.search.next')" @click="step(1)"><ChevronDown :size="15" /></button>
    <button class="wb-search__btn" :aria-label="$t('whiteboard.close')" :title="$t('whiteboard.close')" @click="emit('close')"><X :size="15" /></button>
  </div>
</template>

<style scoped>
.wb-search {
  position: absolute;
  top: 60px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 55;
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 4px 6px;
  max-width: calc(100% - 32px);
  border-radius: 12px;
  border: 1px solid var(--color-border, #e6e6e6);
  background: var(--color-surface, #fff);
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.1);
}
:global(.dark .wb-search) {
  border-color: var(--color-border-dark, #2c2c2c);
  background: var(--color-surface-dark, #1e1e1e);
}
.wb-search__input {
  width: 220px;
  min-width: 0;
  padding: 6px 8px;
  border: none;
  outline: none;
  background: transparent;
  font-size: 14px;
  color: inherit;
}
.wb-search__count {
  min-width: 4.5em;
  font-size: 12px;
  color: var(--color-text-secondary, #52525b);
  white-space: nowrap;
}
:global(.dark .wb-search__count) {
  color: var(--color-text-secondary-dark, #a1a1aa);
}
.wb-search__btn {
  display: grid;
  place-items: center;
  width: 30px;
  height: 30px;
  border-radius: 8px;
}
.wb-search__btn:hover:not(:disabled) {
  background: var(--color-surface-hover, #f4f4f5);
}
:global(.dark .wb-search__btn:hover:not(:disabled)) {
  background: var(--color-surface-hover-dark, #2a2a2a);
}
.wb-search__btn:disabled {
  opacity: 0.35;
}
</style>
