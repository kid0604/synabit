<script setup lang="ts">
import { ref, watch, nextTick } from 'vue';

/**
 * A modal that the browser holds, rather than one this app pretends to hold.
 *
 * Every dialog here used to be a fixed overlay `<div>`. It looked modal and
 * was not: `Tab` walked straight out of it into the calendar underneath,
 * which for anyone using a keyboard means being lost in an interface they
 * cannot see. The usual fix is a focus trap — a few dozen lines of event
 * tracking that has to know what is focusable, and be wrong about it as the
 * contents change.
 *
 * `showModal()` does all of it: the rest of the document goes inert, focus
 * cannot leave, Escape closes, and it renders in the top layer so nothing can
 * stack over it. `<dialog>` has been widely available since 2022, which is
 * inside the floor this app targets.
 */
const props = defineProps<{
    show: boolean;
    /** Id of the heading that names this dialog. */
    labelledBy?: string;
    /** Extra classes for the card inside. */
    cardClass?: string;
}>();

const emit = defineEmits<{ (e: 'close'): void }>();

const dialog = ref<HTMLDialogElement | null>(null);

watch(() => props.show, async (open) => {
    await nextTick();
    const el = dialog.value;
    if (!el) return;
    if (open && !el.open) el.showModal();
    else if (!open && el.open) el.close();
}, { immediate: true });

/**
 * Escape reaches the dialog before this app does. Left alone the browser
 * would close the element while `show` stayed true, and the dialog could
 * never be opened again — so the default is refused and the state is changed
 * instead, which closes it on the way back through.
 */
const onCancel = (e: Event) => {
    e.preventDefault();
    emit('close');
};
/*
 * `cancel` is the event the browser fires for a close request, and it is the
 * right hook. The `keydown` beside it is not a duplicate for the sake of it:
 * closing twice costs nothing, and this app runs inside whatever WebView the
 * operating system provides — on macOS that is pinned to the OS version, so
 * "the platform handles Escape" is a claim about a range of engines rather
 * than about one.
 */
</script>

<template>
    <!--
      No layout classes on the `<dialog>` itself. See the note in the style
      block below: a `display` utility here makes every dialog in the app
      permanently visible.

      The dim and the centring live on the sheet inside, which is also what a
      click outside the card lands on.
    -->
    <dialog ref="dialog"
            :aria-labelledby="labelledBy"
            class="w-full h-full max-w-none max-h-none m-0 p-0 bg-transparent overflow-hidden"
            @cancel="onCancel"
            @keydown.esc="onCancel">
        <div class="w-full h-full p-4 bg-black/40 backdrop-blur-sm flex items-center justify-center"
             @click.self="emit('close')">
            <div class="bg-white dark:bg-[#1e1e1e] w-full rounded-2xl shadow-2xl overflow-hidden border border-[#e6e6e6] dark:border-[#333] flex flex-col"
                 :class="cardClass ?? 'max-w-md max-h-[90vh]'">
                <slot />
            </div>
        </div>
    </dialog>
</template>

<style scoped>
/*
 * A closed dialog is not shown. This should go without saying — the browser's
 * own stylesheet says `dialog:not([open]) { display: none }` — and it does
 * not: a stylesheet written by the page beats the browser's whatever the
 * specificity, so a single `flex` class on the element made every dialog in
 * this app visible from the moment it loaded. Three full-screen overlays
 * stacked over the calendar, nothing beneath them clickable, and the app
 * looking broken on open.
 *
 * So it is said here, out loud, where it cannot be undone by a utility class.
 */
dialog:not([open]) {
    display: none;
}

/* The UA gives `<dialog>` a border, padding and auto margins; the sheet
   inside supplies its own. `::backdrop` stays transparent because that sheet
   is already the dim. */
dialog {
    border: 0;
    color: inherit;
}
dialog::backdrop {
    background: transparent;
}
</style>
