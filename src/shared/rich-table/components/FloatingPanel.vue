<script setup lang="ts">
/**
 * A menu next to the thing that opened it. Positioned by hand from the
 * anchor's rectangle, the way the editor's other menus are: the Popover API is
 * only Baseline Newly available, and anchor positioning not Baseline at all,
 * so neither may carry behaviour a WebView on an older macOS lacks (CLAUDE.md).
 *
 * Teleported to `<body>` — outside the editor, so ProseMirror never sees its
 * keys, and outside the table's scroll box, so it is never clipped.
 */
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue';

const props = defineProps<{
  anchor: { left: number; top: number; bottom: number; right: number };
  /** Prefer the anchor's right edge rather than its left. */
  alignEnd?: boolean;
  width?: number;
  /** Leave Escape to whoever opened it — an editor that decides what Escape means. */
  ownEscape?: boolean;
}>();
const emit = defineEmits<{ close: [] }>();

const panel = ref<HTMLElement | null>(null);
// Transparent, not hidden, until placed: what is inside may want the focus
// as it mounts, and `visibility: hidden` refuses it.
const style = ref<Record<string, string>>({ opacity: '0' });

const GAP = 4;
const MARGIN = 8;

function place() {
  const el = panel.value;
  if (!el) return;
  const { innerWidth: vw, innerHeight: vh } = window;
  const w = el.offsetWidth;
  const h = el.offsetHeight;
  let left = props.alignEnd ? props.anchor.right - w : props.anchor.left;
  left = Math.max(MARGIN, Math.min(left, vw - w - MARGIN));
  let top = props.anchor.bottom + GAP;
  if (top + h > vh - MARGIN && props.anchor.top - GAP - h >= MARGIN) top = props.anchor.top - GAP - h;
  top = Math.max(MARGIN, Math.min(top, vh - h - MARGIN));
  style.value = { left: `${left}px`, top: `${top}px`, maxHeight: `${vh - 2 * MARGIN}px` };
}

function onPointerDown(e: PointerEvent) {
  if (!panel.value || panel.value.contains(e.target as Node)) return;
  emit('close');
  // A press on what opened the menu closes it — and the click that follows
  // must not open it again, or the button could never close its own menu.
  const a = props.anchor;
  if (e.clientX >= a.left && e.clientX <= a.right && e.clientY >= a.top && e.clientY <= a.bottom) {
    const swallow = (ev: MouseEvent) => {
      ev.stopPropagation();
      ev.preventDefault();
    };
    window.addEventListener('click', swallow, { capture: true, once: true });
    setTimeout(() => window.removeEventListener('click', swallow, true), 500);
  }
}

/**
 * Escape closes the menu wherever the focus is — clicking a header leaves it
 * on the editor, not in the menu. Caught before anything else sees it, so the
 * same Escape does not also leave the table.
 */
function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape' && !props.ownEscape) {
    e.stopPropagation();
    e.preventDefault();
    emit('close');
  }
}

/** Where the focus was before the menu: it goes back there when the menu closes from inside. */
let before: HTMLElement | null = null;

const items = () => [...(panel.value?.querySelectorAll<HTMLElement>('.rt-item:not(:disabled)') ?? [])];

/** ↑ ↓ Home End between a menu's items, as in any menu. */
function onPanelKey(e: KeyboardEvent) {
  if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(e.key)) return;
  const target = e.target as HTMLElement;
  // In a field, the arrows are the field's.
  if (target.matches('input, textarea, select')) return;
  const all = items();
  if (!all.length) return;
  e.preventDefault();
  const at = all.indexOf(target);
  const next = e.key === 'Home' ? 0
    : e.key === 'End' ? all.length - 1
      : e.key === 'ArrowDown' ? (at + 1) % all.length
        : (at - 1 + all.length) % all.length;
  all[next].focus();
}

onMounted(() => {
  before = document.activeElement as HTMLElement | null;
  nextTick(place);
  // A menu of items takes the focus, unless something in it already has —
  // a name field, a formula — or its opener keeps typing (`ownEscape`: a picker).
  if (!props.ownEscape) {
    setTimeout(() => {
      if (!panel.value || panel.value.contains(document.activeElement)) return;
      items()[0]?.focus({ preventScroll: true });
    }, 0);
  }
  window.addEventListener('pointerdown', onPointerDown, true);
  window.addEventListener('keydown', onKey, true);
  window.addEventListener('resize', place);
});
onBeforeUnmount(() => {
  if (panel.value?.contains(document.activeElement) && before?.isConnected) before.focus({ preventScroll: true });
  window.removeEventListener('pointerdown', onPointerDown, true);
  window.removeEventListener('keydown', onKey, true);
  window.removeEventListener('resize', place);
});
watch(() => props.anchor, () => nextTick(place));

defineExpose({ place });
</script>

<template>
  <Teleport to="body">
    <div
      ref="panel"
      class="rt-panel"
      role="dialog"
      :style="{ ...style, width: width ? `${width}px` : undefined }"
      @keydown="onPanelKey"
    >
      <slot />
    </div>
  </Teleport>
</template>
