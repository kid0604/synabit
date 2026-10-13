<script setup lang="ts">
/**
 * The frame every card Syn puts in front of the person shares: a permission, a
 * choice, a plan of sends, changes held after reading.
 *
 * One frame, so the same kind of question looks the same in Messages and in
 * the AskBar, and the person learns one shape: what it is, why it is here,
 * what it would do, the answers at the bottom right.
 *
 * The colour is the one thing that differs, on purpose. A card asking for a
 * permission and a card asking which note was meant mean opposite things, and
 * two cards that look alike and mean opposite things is how somebody learns to
 * answer both with the same reflex. So the tone says what kind of question it
 * is, and each kind keeps its colour:
 *
 * - `ask` — a permission was not given. Amber.
 * - `change` — changes to the vault waiting for an OK. Amber too: it is the
 *   same kind of yes.
 * - `send` — something would leave this device. Violet.
 * - `choose` — the work is fine; Syn will not guess one detail. The accent.
 */
import type { Component } from 'vue';

export type Tone = 'ask' | 'change' | 'send' | 'choose';

defineProps<{
  tone: Tone;
  icon: Component;
  title: string;
  /** Why it is here, in a sentence under the title. */
  lead?: string;
  /** Why the last answer did not go through. */
  error?: string | null;
  /** A last line, small: what answering means. */
  note?: string;
}>();

const TONES: Record<Tone, { box: string; mark: string }> = {
  ask: {
    box: 'border-amber-300 dark:border-amber-900/70 bg-amber-50/60 dark:bg-amber-950/25',
    mark: 'text-amber-700 dark:text-amber-400',
  },
  change: {
    box: 'border-amber-300 dark:border-amber-900/70 bg-amber-50/60 dark:bg-amber-950/25',
    mark: 'text-amber-700 dark:text-amber-400',
  },
  send: {
    box: 'border-violet-300/60 dark:border-violet-500/30 bg-violet-50/40 dark:bg-violet-500/5',
    mark: 'text-violet-600 dark:text-violet-300',
  },
  choose: {
    box: 'border-accent/30 dark:border-accent-dark/30 bg-accent/5 dark:bg-accent-dark/5',
    mark: 'text-accent dark:text-accent-dark',
  },
};
</script>

<template>
  <section class="rounded-xl border p-3.5 space-y-3" :class="TONES[tone].box" :data-tone="tone">
    <header class="flex items-start gap-2">
      <component :is="icon" class="w-4 h-4 mt-0.5 shrink-0" :class="TONES[tone].mark" aria-hidden="true" />
      <div class="min-w-0 space-y-1">
        <p class="text-sm font-medium text-text dark:text-text-dark">{{ title }}</p>
        <p v-if="lead" class="text-xs text-gray-600 dark:text-gray-300">{{ lead }}</p>
        <slot name="lead" />
      </div>
    </header>

    <slot />

    <p v-if="error" role="alert" class="text-xs text-red-600 dark:text-red-400" data-card-error>{{ error }}</p>
    <div v-if="$slots.actions" class="flex flex-wrap items-center justify-end gap-2">
      <slot name="actions" />
    </div>
    <p v-if="note" class="text-xs text-gray-500 dark:text-gray-400">{{ note }}</p>
  </section>
</template>
