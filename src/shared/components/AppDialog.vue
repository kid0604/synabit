<script setup lang="ts">
/**
 * The shell every modal in the app sits in.
 *
 * It was `ConfirmModal`'s, taken out so the other sixty-odd overlays stop
 * drawing their own: one scrim, one shape, one z-order, and the same promises
 * to the keyboard everywhere —
 *
 * * **Escape closes**, handled on the dialog rather than on `window`, so a
 *   screen that already listens for Escape keeps deciding what it means there.
 * * **Focus moves in and comes back.** In it lands on `initialFocus` if the
 *   caller names one, else on the first `[autofocus]`, else on the first thing
 *   that can take it; on close it returns to wherever it was.
 * * **It is announced as a dialog**, labelled by `labelledby` (the id of the
 *   caller's own heading) or `ariaLabel`.
 *
 * * **Tab stays inside.** It wraps from the last control to the first and back,
 *   as `aria-modal` promises a screen reader it will.
 * * **Only a click that starts on the scrim closes it.** A click is sent to the
 *   nearest common ancestor of where the button went down and where it came
 *   up, so selecting text in a field and letting go outside the panel is a
 *   "click" on the scrim — which used to close the dialog and lose the typing.
 * * **Focus comes back even when the dialog is removed while open.** Most
 *   callers mount it with `v-if` and `:show="true"`, so it never sees `show`
 *   turn false; unmounting is the same moment for them.
 *
 * `unstyled` drops the panel's own look for a caller that draws its own
 * surface; the behaviour stays.
 */
import { nextTick, onBeforeUnmount, ref, watch } from 'vue';

const props = withDefaults(
  defineProps<{
    show: boolean;
    /** Id of the element that names the dialog, usually its heading. */
    labelledby?: string;
    /** The name, when there is no visible heading to point at. */
    ariaLabel?: string;
    describedby?: string;
    size?: 'sm' | 'md' | 'lg' | 'xl';
    /** Where focus should land when the dialog opens. */
    initialFocus?: () => HTMLElement | null | undefined;
    /** False for a dialog that must be answered: no Escape, no scrim click. */
    dismissible?: boolean;
    /** Above other dialogs — for a question asked from inside one. */
    elevated?: boolean;
    unstyled?: boolean;
    panelClass?: string;
  }>(),
  { size: 'md', dismissible: true },
);

const emit = defineEmits<{ (e: 'close'): void }>();

const panel = ref<HTMLElement | null>(null);

const WIDTH = { sm: 'max-w-sm', md: 'max-w-lg', lg: 'max-w-2xl', xl: 'max-w-4xl' } as const;

const FOCUSABLE =
  'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

let returnFocusTo: HTMLElement | null = null;

watch(
  () => props.show,
  async (open) => {
    if (open) {
      returnFocusTo = document.activeElement as HTMLElement | null;
      await nextTick();
      const el =
        props.initialFocus?.() ??
        panel.value?.querySelector<HTMLElement>('[autofocus]') ??
        panel.value?.querySelector<HTMLElement>(FOCUSABLE) ??
        panel.value;
      el?.focus();
      return;
    }
    returnFocusTo?.focus?.();
    returnFocusTo = null;
  },
  // Immediate, because some callers mount the dialog already open.
  { immediate: true },
);

onBeforeUnmount(() => {
  if (props.show) returnFocusTo?.focus?.();
});

function dismiss() {
  if (props.dismissible) emit('close');
}

/** Whether the press that ends in this click began on the scrim itself. */
let pressedOnScrim = false;

function onScrimClick(e: MouseEvent) {
  if (e.target === e.currentTarget && pressedOnScrim) dismiss();
  pressedOnScrim = false;
}

function trapTab(e: KeyboardEvent) {
  const items = Array.from(panel.value?.querySelectorAll<HTMLElement>(FOCUSABLE) ?? [])
    .filter(el => el.offsetParent !== null || el === document.activeElement);
  if (!items.length) {
    e.preventDefault();
    panel.value?.focus();
    return;
  }
  const first = items[0];
  const last = items[items.length - 1];
  const active = document.activeElement;
  if (e.shiftKey && (active === first || active === panel.value)) {
    e.preventDefault();
    last.focus();
  } else if (!e.shiftKey && active === last) {
    e.preventDefault();
    first.focus();
  }
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.stopPropagation();
    dismiss();
  } else if (e.key === 'Tab') {
    trapTab(e);
  }
}
</script>

<template>
  <Teleport to="body">
    <Transition name="app-dialog">
      <div
        v-if="show"
        class="app-dialog-scrim fixed inset-0 flex items-center justify-center p-4 sm:p-6"
        :class="elevated ? 'z-[10000]' : 'z-[1000]'"
        @mousedown="pressedOnScrim = $event.target === $event.currentTarget"
        @click="onScrimClick"
        @keydown="onKeydown"
      >
        <div
          ref="panel"
          role="dialog"
          aria-modal="true"
          tabindex="-1"
          :aria-labelledby="labelledby"
          :aria-label="labelledby ? undefined : ariaLabel"
          :aria-describedby="describedby"
          class="app-dialog-panel w-full outline-none"
          :class="[
            WIDTH[size],
            unstyled
              ? ''
              : 'bg-surface dark:bg-surface-dark text-text dark:text-text-dark rounded-2xl shadow-2xl border border-border dark:border-border-dark max-h-[90vh] overflow-y-auto',
            panelClass,
          ]"
        >
          <slot />
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
/* One scrim for every dialog: a neutral 32% black, no blur. */
.app-dialog-scrim {
  background-color: rgb(0 0 0 / 0.32);
}

.app-dialog-enter-active {
  transition: opacity 150ms linear;
}
.app-dialog-leave-active {
  transition: opacity 100ms linear;
}
.app-dialog-enter-from,
.app-dialog-leave-to {
  opacity: 0;
}
.app-dialog-enter-active .app-dialog-panel {
  transition: transform 250ms cubic-bezier(0.05, 0.7, 0.1, 1);
}
.app-dialog-leave-active .app-dialog-panel {
  transition: transform 150ms cubic-bezier(0.3, 0, 0.8, 0.15);
}
.app-dialog-enter-from .app-dialog-panel,
.app-dialog-leave-to .app-dialog-panel {
  transform: scale(0.95);
}

@media (prefers-reduced-motion: reduce) {
  .app-dialog-enter-active,
  .app-dialog-leave-active,
  .app-dialog-enter-active .app-dialog-panel,
  .app-dialog-leave-active .app-dialog-panel {
    transition-duration: 1ms;
  }
  .app-dialog-enter-from .app-dialog-panel,
  .app-dialog-leave-to .app-dialog-panel {
    transform: none;
  }
}
</style>
