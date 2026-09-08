<script setup lang="ts">
/**
 * One line under an answer saying what it was standing on.
 *
 * # Why a mark and not a hedge in the prose
 *
 * Syn hedges in words today — *"có thể là…"*, *"theo tôi thấy…"* — and words
 * have two problems. Nothing but a person can read them, so no screen can
 * colour them and no count can say how often Syn was guessing this week. And a
 * model that hedges every sentence has spent the hedge: once everything is
 * qualified, the qualification stops meaning anything.
 *
 * The state behind this is decided in Rust, by arithmetic on the run's own
 * transcript — see `syn::footing`. Nothing here asks the model how sure it
 * feels, which is the question it is worst at answering.
 *
 * # Why all three show, and only one is coloured
 *
 * Showing only the bad ones would make the absence of a mark ambiguous: an
 * answer written before this existed and a well-grounded one would look
 * identical. So `grounded` says so too, quietly — grey, small, the size of a
 * timestamp.
 *
 * Only `guessing` is coloured, because it is the only one that should change
 * what the reader does next. `inferred` is grey on purpose even though it is
 * the subtler danger: colouring two of three states is how a warning becomes
 * wallpaper.
 */
import { Anchor, CircleDashed, CircleAlert } from 'lucide-vue-next';
import { computed } from 'vue';
import type { Footing } from '../types';

const props = defineProps<{ footing: Footing }>();

/** Icon and colour per state. The words live in i18n, keyed by the state. */
const look = computed(() => {
  switch (props.footing) {
    case 'grounded':
      return { icon: Anchor, tone: 'text-gray-400 dark:text-gray-500' };
    case 'inferred':
      return { icon: CircleDashed, tone: 'text-gray-400 dark:text-gray-500' };
    default:
      return { icon: CircleAlert, tone: 'text-amber-600 dark:text-amber-500' };
  }
});
</script>

<template>
  <!-- The longer sentence is a title rather than more text on screen: it is
       the explanation somebody wants once, not on every answer they read. -->
  <div
    class="inline-flex items-center gap-1 text-[11px] leading-none"
    :class="look.tone"
    :title="$t(`syn.footing_${footing}_why`)"
  >
    <component :is="look.icon" class="w-3 h-3 shrink-0" aria-hidden="true" />
    <span>{{ $t(`syn.footing_${footing}`) }}</span>
  </div>
</template>
