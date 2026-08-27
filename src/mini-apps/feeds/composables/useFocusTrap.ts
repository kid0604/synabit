import { onMounted, onBeforeUnmount, type Ref } from 'vue';

const FOCUSABLE = [
  'a[href]',
  'button:not([disabled])',
  'input:not([disabled])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(',');

/**
 * Keep keyboard focus inside a dialog while it is open, and give it back
 * afterwards.
 *
 * Without this, Tab walks straight out of the dialog and into the page behind
 * it, which for a screen-reader or keyboard user means the dialog is modal in
 * appearance only. Focusing the container also makes `Escape` work at all:
 * these dialogs listen for the key on themselves, and an element nothing has
 * focused never receives it.
 */
export function useFocusTrap(container: Ref<HTMLElement | null>) {
  let previouslyFocused: HTMLElement | null = null;

  const focusable = (): HTMLElement[] =>
    Array.from(container.value?.querySelectorAll<HTMLElement>(FOCUSABLE) ?? []).filter(
      el => el.offsetParent !== null || el === document.activeElement,
    );

  const onKeydown = (e: KeyboardEvent) => {
    if (e.key !== 'Tab' || !container.value) return;

    const items = focusable();
    if (items.length === 0) {
      e.preventDefault();
      return;
    }

    const first = items[0];
    const last = items[items.length - 1];
    const active = document.activeElement as HTMLElement | null;

    // Wrap at both ends, and pull focus back in if it has escaped already.
    if (e.shiftKey && (active === first || !container.value.contains(active))) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && (active === last || !container.value.contains(active))) {
      e.preventDefault();
      first.focus();
    }
  };

  onMounted(() => {
    previouslyFocused = document.activeElement as HTMLElement | null;
    // An element with no autofocus still needs somewhere for focus to land,
    // which is the container itself.
    const target = focusable().find(el => el.hasAttribute('autofocus')) ?? container.value;
    target?.focus();
    document.addEventListener('keydown', onKeydown, true);
  });

  onBeforeUnmount(() => {
    document.removeEventListener('keydown', onKeydown, true);
    previouslyFocused?.focus();
  });
}
