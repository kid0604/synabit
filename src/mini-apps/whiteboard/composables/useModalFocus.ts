import { onBeforeUnmount, onMounted, ref, type Ref } from 'vue';
import { useBackGuard } from '../../../composables/useBackGuard';

const FOCUSABLE = 'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/**
 * Focus for a dialog the board opens: kept inside it while it is open, and
 * given back to whatever had it when it closes. Without this, Tab walked out
 * of the dialog into the board behind it, and closing one left focus on the
 * page itself, so a keyboard user started again from the top.
 *
 * `dismiss`, when given, is what Android's Back does — the same as the
 * dialog's Escape. The board's dialogs are mounted only while open, so the
 * claim on Back lasts exactly as long as the component, and one opened from
 * inside another (the source picker in Generate) sits above it and closes
 * first.
 *
 * Returns the handler for the dialog's `keydown`.
 */
export function useModalFocus(box: Ref<HTMLElement | null>, dismiss?: () => void) {
  if (dismiss) useBackGuard(ref(true), dismiss);
  let before: HTMLElement | null = null;
  onMounted(() => { before = document.activeElement as HTMLElement | null; });
  onBeforeUnmount(() => {
    if (before?.isConnected) before.focus();
  });
  return function keepFocus(e: KeyboardEvent) {
    if (e.key !== 'Tab' || !box.value) return;
    const stops = [...box.value.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => el.offsetParent !== null || el === document.activeElement);
    if (!stops.length) return;
    const first = stops[0];
    const last = stops[stops.length - 1];
    if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
    else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
  };
}
