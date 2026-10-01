<script setup lang="ts">
/**
 * A small "Ask Syn" button beside whatever text is selected inside `within`.
 *
 * For the screens where text is read rather than edited — a text file, a PDF.
 * Notes does not use this: its editor already draws a toolbar over a
 * selection, and a second floating thing beside the first would be two
 * buttons fighting for one spot. The editor puts the same action in its own
 * toolbar instead; see `EditorBubbleMenu.vue`.
 *
 * # What it must never do
 *
 * Get in the way of selecting. So it hides while a pointer is down — a button
 * that chased the end of a drag would sit under the cursor of the person
 * trying to extend the selection — and appears once the selection has settled.
 * It hides on scroll rather than following, because a button left floating
 * over a line it no longer belongs to is worse than none. It never takes focus
 * from where the person was when pressed; see `onPointerDown`.
 *
 * # Why it keeps the text rather than reading it on click
 *
 * On a phone, tapping outside the selected text can dismiss the selection
 * before the click event exists — so a button that read the selection when
 * pressed would, on exactly the devices it was built for, read nothing. The
 * text is read when the button appears and handed over as it was; see
 * `focusWithSelection`.
 */
import { ref, inject, watch, onMounted, onBeforeUnmount, nextTick } from 'vue';
import { useI18n } from 'vue-i18n';
import synAvatar from '../../assets/syn-avatar.jpg';
import { readSelection } from './focus';
import { askSynAbout, placeNearSelection, SYN_ASK, synAskFallback } from './selectionAsk';

const props = defineProps<{
  /** Only selections inside this element count. */
  within: HTMLElement | null | undefined;
  /**
   * Stand aside. The PDF viewer's highlight mode opens its own popup on a
   * selection, and two things appearing for one gesture is one too many.
   */
  disabled?: boolean;
}>();

const { t } = useI18n();
const { allowed, shortcut } = inject(SYN_ASK, synAskFallback, true);

const shown = ref(false);
const position = ref({ top: 0, left: 0 });
const buttonRef = ref<HTMLButtonElement | null>(null);
let text = '';
let pointerDown = false;
let frame = 0;

/**
 * An estimate of the button's size, for the first placement.
 *
 * The real size is only known once it is drawn, and it depends on the
 * language of its label. It is measured then, and the button moved if the
 * estimate was off — see `place`.
 */
let size = { width: 96, height: 36 };

const hide = () => {
  shown.value = false;
  text = '';
};

/** A touch screen's own selection menu sits above; see `placeNearSelection`. */
const coarse = () => window.matchMedia?.('(pointer: coarse)').matches ?? false;

const place = (rect: DOMRect) => {
  const at = placeNearSelection(
    rect,
    { width: window.innerWidth, height: window.innerHeight },
    size,
    coarse() ? 'below' : 'above',
  );
  if (!at) return false;
  position.value = at;
  return true;
};

const update = () => {
  frame = 0;
  if (!allowed.value || props.disabled || !props.within || pointerDown) return hide();

  const selection = window.getSelection();
  if (!selection || selection.isCollapsed || selection.rangeCount === 0) return hide();

  const range = selection.getRangeAt(0);
  if (!props.within.contains(range.commonAncestorContainer)) return hide();

  // The same rules the key applies — trimmed, whitespace is nothing, capped.
  const read = readSelection(null, selection.toString());
  if (!read) return hide();

  const rect = range.getBoundingClientRect();
  if (!place(rect)) return hide();
  text = read;
  shown.value = true;

  // Measure what was actually drawn, and correct the placement once.
  void nextTick(() => {
    const el = buttonRef.value;
    if (!el || !shown.value) return;
    const drawn = { width: el.offsetWidth, height: el.offsetHeight };
    if (drawn.width !== size.width || drawn.height !== size.height) {
      size = drawn;
      place(rect);
    }
  });
};

/** Once per frame at most: `selectionchange` fires on every character a
 *  keyboard selection grows by. */
const schedule = () => {
  if (!frame) frame = requestAnimationFrame(update);
};

const onPointerDownAnywhere = (e: PointerEvent) => {
  if (buttonRef.value?.contains(e.target as Node)) return;
  pointerDown = true;
  hide();
};

const onPointerUpAnywhere = () => {
  if (!pointerDown) return;
  pointerDown = false;
  schedule();
};

const onKeydown = (e: KeyboardEvent) => {
  if (e.key === 'Escape' && shown.value) hide();
};

/**
 * Keep the selection where it is.
 *
 * Pressing a button moves focus to it, and in an editable area that can
 * collapse the selection this button is about — the same trap `focus.ts`
 * describes for the bar. The text is already kept, but the person's
 * highlight disappearing under their finger reads as "that did not work".
 * This only stops the *press* from moving focus: it is still a real button,
 * in the tab order and named for a screen reader. Somebody selecting from the
 * keyboard has a shorter way than tabbing to it, though — the key that does
 * the same thing, which is what the tooltip says.
 */
const onPointerDown = (e: PointerEvent) => {
  e.preventDefault();
};

const ask = () => {
  const selected = text;
  hide();
  if (selected) askSynAbout(selected);
};

onMounted(() => {
  document.addEventListener('selectionchange', schedule);
  document.addEventListener('pointerdown', onPointerDownAnywhere, true);
  document.addEventListener('pointerup', onPointerUpAnywhere, true);
  document.addEventListener('pointercancel', onPointerUpAnywhere, true);
  // Capture, because `scroll` does not bubble and the thing scrolling is
  // usually a panel inside the app rather than the window.
  document.addEventListener('scroll', hide, { capture: true, passive: true });
  window.addEventListener('resize', hide);
  window.addEventListener('keydown', onKeydown);
});

onBeforeUnmount(() => {
  if (frame) cancelAnimationFrame(frame);
  document.removeEventListener('selectionchange', schedule);
  document.removeEventListener('pointerdown', onPointerDownAnywhere, true);
  document.removeEventListener('pointerup', onPointerUpAnywhere, true);
  document.removeEventListener('pointercancel', onPointerUpAnywhere, true);
  document.removeEventListener('scroll', hide, { capture: true });
  window.removeEventListener('resize', hide);
  window.removeEventListener('keydown', onKeydown);
});

// Syn switched off, a lock coming down, the viewer going into highlight mode:
// the button goes at once rather than at the next selection.
watch(
  () => [allowed.value, props.disabled] as const,
  ([ok, off]) => {
    if (!ok || off) hide();
  },
);
</script>

<template>
  <Teleport to="body">
    <button
      v-if="shown && allowed"
      ref="buttonRef"
      type="button"
      class="syn-selection-ask fixed z-[65] inline-flex items-center gap-1.5 rounded-full border border-black/10 dark:border-white/10 bg-white dark:bg-[#1c1c1e] pl-1 pr-3 text-[12px] font-medium text-gray-700 dark:text-gray-200 shadow-lg hover:bg-gray-50 dark:hover:bg-[#2c2c2e] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent dark:focus-visible:outline-accent-dark cursor-pointer select-none"
      :style="{ top: position.top + 'px', left: position.left + 'px' }"
      :title="shortcut ? t('syn.ask_about_selection_hint', { shortcut }) : undefined"
      :aria-label="t('syn.ask_about_selection')"
      @pointerdown="onPointerDown"
      @click="ask"
    >
      <img :src="synAvatar" alt="" class="w-6 h-6 rounded-full object-cover" />
      {{ t('syn.ask_about_selection') }}
    </button>
  </Teleport>
</template>

<style scoped>
/*
  36px on a pointer that can aim, 44px under a finger — the size both Apple and
  Google give as the smallest target a thumb can hit without a second try.
  `pointer: coarse` has been in every engine since long before this app.
*/
.syn-selection-ask {
  height: 36px;
}
@media (pointer: coarse) {
  .syn-selection-ask {
    height: 44px;
    padding-right: 1rem;
  }
}
</style>
