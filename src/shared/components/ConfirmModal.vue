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
 * The shell that does those things is `AppDialog`, which every other dialog
 * now shares; this file is only the question.
 */
import { ref } from 'vue';
import { AlertTriangle } from 'lucide-vue-next';
import { i18n } from '../../i18n';
import AppDialog from './AppDialog.vue';

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

/** The global instance, so the dialog also works where no i18n plugin is installed. */
const t = i18n.global.t;

/** Labelling the dialog by its own headline, per element rather than per app. */
const uid = Math.random().toString(36).slice(2, 9);
const titleId = `confirm-title-${uid}`;
const bodyId = `confirm-body-${uid}`;

const cancelButton = ref<HTMLButtonElement | null>(null);
const confirmButton = ref<HTMLButtonElement | null>(null);

/**
 * Cancel on a destructive question: Enter is the key people press to make a
 * dialog go away, and it must not be the key that deletes. Escape, the scrim
 * and giving focus back are `AppDialog`'s.
 */
const initialFocus = () => (props.isDestructive ? cancelButton.value : confirmButton.value);
</script>

<template>
  <AppDialog
    :show="show"
    :labelledby="titleId"
    :describedby="bodyId"
    :initial-focus="initialFocus"
    elevated
    unstyled
    panel-class="max-w-[560px]"
    @close="emit('cancel')"
  >
    <div class="m3-surface bg-surface dark:bg-surface-dark w-full min-w-[280px] p-6 flex flex-col gap-4">
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
          {{ cancelText || t('common.cancel') }}
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
          {{ confirmText || (isDestructive ? t('common.delete') : t('common.confirm')) }}
        </button>
      </div>
    </div>
  </AppDialog>
</template>

<style scoped>
@reference "../../style.css";

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
</style>
