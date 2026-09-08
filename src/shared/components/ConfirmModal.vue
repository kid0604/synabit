<script setup lang="ts">
/**
 * The one question the app asks before it does something it cannot take back.
 *
 * # Why it looks like this
 *
 * Material 3's basic dialog, as literally as it goes into an app whose other
 * surfaces are not Material. The parts that carry the meaning are the ones
 * kept:
 *
 * * **A 28px container and no dividers.** M3 dialogs are a single soft shape.
 *   The old header and footer rules cut it into three strips, which made a
 *   sixty-word question look like a form.
 * * **Text buttons, not a filled one.** A filled red button is the loudest
 *   thing on screen, and it was on the side that destroys something. M3 gives
 *   both answers the same weight and lets the *words* carry the difference —
 *   which is the right way round when the safe answer is the one people should
 *   find easy.
 * * **No close ✕.** Three ways to say no (✕, Cancel, the scrim) is two ways to
 *   wonder whether they mean the same thing.
 * * **An icon, only when it is destructive.** M3's rule: with an icon, the icon
 *   and headline centre; without one, the headline starts at the left. So the
 *   shape of the dialog itself says which kind of question this is, before any
 *   of the words are read.
 *
 * # What it does that the old one did not
 *
 * Escape answers *no*, focus moves into the dialog and goes back where it came
 * from afterwards, and the whole thing is announced as a dialog. A modal that
 * cannot be dismissed from the keyboard is one a keyboard user is stuck inside.
 *
 * On a destructive question the focus lands on **Cancel**. Enter is what people
 * press to get a dialog out of the way, and it should not be the key that
 * deletes their work.
 *
 * This is not a focus trap — Tab can still leave. Adding one means owning every
 * corner of it, and the scrim plus Escape plus a landed focus already covers
 * what goes wrong in practice.
 */
import { nextTick, ref, watch } from 'vue';
import { AlertTriangle } from 'lucide-vue-next';

const props = defineProps<{
  show: boolean;
  title: string;
  message: string;
  confirmText?: string;
  cancelText?: string;
  /**
   * A third choice, shown only when given.
   *
   * For the decisions where "no" and "yes" are not the whole question —
   * deleting a task that has subtasks is either "this one" or "this one and
   * everything under it", and cancelling is neither.
   */
  secondaryText?: string;
  isDestructive?: boolean;
}>();

const emit = defineEmits<{
  (e: 'confirm'): void;
  (e: 'secondary'): void;
  (e: 'cancel'): void;
}>();

/** Labelling the dialog by its own headline, per element rather than per app. */
const uid = Math.random().toString(36).slice(2, 9);
const titleId = `confirm-title-${uid}`;
const bodyId = `confirm-body-${uid}`;

const cancelButton = ref<HTMLButtonElement | null>(null);
const confirmButton = ref<HTMLButtonElement | null>(null);

/** Where focus was before the dialog took it, so it can be given back. */
let returnFocusTo: HTMLElement | null = null;

watch(
  () => props.show,
  async (open) => {
    if (open) {
      returnFocusTo = document.activeElement as HTMLElement | null;
      await nextTick();
      // Cancel on a destructive question: Enter is the key people press to
      // make a dialog go away, and it must not be the key that deletes.
      (props.isDestructive ? cancelButton.value : confirmButton.value)?.focus();
      return;
    }
    returnFocusTo?.focus?.();
    returnFocusTo = null;
  },
  // Immediate, because not every caller flips `show` from false. `SettingsModal`
  // mounts the dialog with `v-if` and a hardcoded `:show="true"`, so a watcher
  // that only ran on a change would leave that one dialog with focus still on
  // whatever was behind it.
  { immediate: true }
);

/**
 * Escape answers no.
 *
 * On the element rather than on `window`, so a screen that already listens for
 * Escape keeps deciding what Escape means everywhere else on it.
 */
const onKeydown = (e: KeyboardEvent) => {
  if (e.key === 'Escape') {
    e.stopPropagation();
    emit('cancel');
  }
};
</script>

<template>
  <Teleport to="body">
    <Transition name="m3-dialog">
      <div
        v-if="show"
        class="fixed inset-0 z-[10000] flex items-center justify-center p-6 m3-scrim"
        @click.self="emit('cancel')"
        @keydown="onKeydown"
      >
        <div
          role="dialog"
          aria-modal="true"
          :aria-labelledby="titleId"
          :aria-describedby="bodyId"
          class="m3-surface bg-surface dark:bg-surface-dark w-full min-w-[280px] max-w-[560px] p-6 flex flex-col gap-4"
        >
          <!-- M3: an icon centres the headline under it; without one the
               headline starts at the left. -->
          <AlertTriangle
            v-if="isDestructive"
            class="w-6 h-6 self-center text-danger"
            aria-hidden="true"
          />

          <h2
            :id="titleId"
            class="text-2xl leading-8 font-normal text-text dark:text-text-dark"
            :class="isDestructive ? 'text-center' : 'text-left'"
          >
            {{ title }}
          </h2>

          <p
            :id="bodyId"
            class="text-sm leading-5 text-text-secondary dark:text-text-secondary-dark"
          >
            {{ message }}
          </p>

          <!-- Text buttons, trailing, confirm last. Same weight on both
               answers; the words carry the difference. -->
          <div class="flex flex-wrap justify-end items-center gap-2 pt-2">
            <button
              ref="cancelButton"
              @click="emit('cancel')"
              class="m3-text-button text-text dark:text-text-dark hover:bg-surface-hover dark:hover:bg-surface-hover-dark"
            >
              {{ cancelText || 'Cancel' }}
            </button>
            <button
              v-if="secondaryText"
              @click="emit('secondary')"
              class="m3-text-button text-text dark:text-text-dark hover:bg-surface-hover dark:hover:bg-surface-hover-dark"
            >
              {{ secondaryText }}
            </button>
            <button
              ref="confirmButton"
              @click="emit('confirm')"
              class="m3-text-button"
              :class="
                isDestructive
                  ? 'text-danger hover:bg-danger/10'
                  : 'text-accent dark:text-accent-dark hover:bg-accent/10'
              "
            >
              {{ confirmText || 'Confirm' }}
            </button>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
@reference "../../style.css";

/* M3's scrim: 32% of a neutral black, and no blur — the dialog is meant to
   sit on the screen behind it, not to dissolve it. */
.m3-scrim {
  background-color: rgb(0 0 0 / 0.32);
}

/* Corner 28px and elevation 3, which are the two shapes people recognise a
   Material dialog by before they read a word of it. */
.m3-surface {
  border-radius: 28px;
  box-shadow:
    0 1px 3px rgb(0 0 0 / 0.15),
    0 4px 8px 3px rgb(0 0 0 / 0.15);
}

/* Fully rounded, 40px tall, 12px of side padding — the M3 text button. */
.m3-text-button {
  @apply inline-flex items-center justify-center h-10 px-3 min-w-[64px] rounded-full text-sm font-medium cursor-pointer transition-colors;
}

.m3-text-button:focus-visible {
  @apply outline-2 outline-offset-2 outline-accent;
}

/* M3 motion: emphasised decelerate on the way in, accelerate on the way out,
   and the container scales rather than sliding. */
.m3-dialog-enter-active {
  transition: opacity 150ms linear;
}

.m3-dialog-leave-active {
  transition: opacity 100ms linear;
}

.m3-dialog-enter-from,
.m3-dialog-leave-to {
  opacity: 0;
}

.m3-dialog-enter-active .m3-surface {
  transition: transform 250ms cubic-bezier(0.05, 0.7, 0.1, 1);
}

.m3-dialog-leave-active .m3-surface {
  transition: transform 150ms cubic-bezier(0.3, 0, 0.8, 0.15);
}

.m3-dialog-enter-from .m3-surface,
.m3-dialog-leave-to .m3-surface {
  transform: scale(0.9);
}

@media (prefers-reduced-motion: reduce) {
  .m3-dialog-enter-active,
  .m3-dialog-leave-active,
  .m3-dialog-enter-active .m3-surface,
  .m3-dialog-leave-active .m3-surface {
    transition-duration: 1ms;
  }

  .m3-dialog-enter-from .m3-surface,
  .m3-dialog-leave-to .m3-surface {
    transform: none;
  }
}
</style>
