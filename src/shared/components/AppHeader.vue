<script setup lang="ts">
/**
 * The top of every app, in one place.
 *
 * Before this each app drew its own: a 30px heading in Tasks, 24px with an
 * icon in Calendar, a 14px bar in People, an 11px capital heading in Things,
 * none in Nexus — and "add" in a different colour and corner each time. The
 * reader had to find the title and the main action afresh in every app.
 *
 * Left to right, always: back/forward, an optional menu button for the
 * sidebar on phones, the title, then whatever the app needs (`#search`,
 * `#actions`), and last the one primary action with its words on it.
 *
 * On a phone the search and the app's own controls drop to a second row that
 * scrolls sideways if it must, so the title always keeps the width it needs —
 * on one row, Calendar's controls alone were wider than the screen and the
 * month name was squeezed to nothing. The primary button keeps its icon and
 * its accessible name and drops the words.
 */
import type { Component } from 'vue';
import { Menu } from 'lucide-vue-next';
import NavButtons from './NavButtons.vue';
import { usePlatform } from '../../composables/usePlatform';

withDefaults(
  defineProps<{
    title: string;
    subtitle?: string;
    /** The icon beside the title, when the app has one. */
    icon?: Component;
    /** Words on the primary button. No button without them. */
    primaryLabel?: string;
    primaryIcon?: Component;
    /** Show the back/forward buttons. */
    nav?: boolean;
    /** Label for a phone-only button that opens the app's sidebar. */
    sidebarLabel?: string;
  }>(),
  { nav: true },
);

const emit = defineEmits<{
  (e: 'primary'): void;
  (e: 'open-sidebar'): void;
}>();

/**
 * Whether the app's sidebar is off screen, so the button to open it is needed.
 *
 * The layout in use rather than a width breakpoint: an Android tablet is wider
 * than `md` and still gets the mobile layout, where no sidebar is drawn — with
 * `md:hidden` it had no way to open one at all.
 */
const { useMobileLayout } = usePlatform();
</script>

<template>
  <header
    class="flex flex-wrap sm:flex-nowrap items-center gap-x-3 gap-y-2 min-h-14 px-4 md:px-6 py-2 shrink-0 border-b border-border dark:border-border-dark bg-base dark:bg-base-dark"
  >
    <NavButtons v-if="nav" />
    <button
      v-if="sidebarLabel && useMobileLayout"
      type="button"
      class="btn-icon -ml-1"
      :aria-label="sidebarLabel"
      :title="sidebarLabel"
      @click="emit('open-sidebar')"
    >
      <Menu class="w-5 h-5" aria-hidden="true" />
    </button>

    <div class="flex items-center gap-2 min-w-0 flex-1 sm:flex-none sm:max-w-[45%]">
      <component :is="icon" v-if="icon" class="w-5 h-5 shrink-0 text-accent dark:text-accent-dark" aria-hidden="true" />
      <div class="min-w-0">
        <h1 class="text-xl font-semibold tracking-tight text-text dark:text-text-dark truncate">
          <slot name="title">{{ title }}</slot>
        </h1>
        <p v-if="subtitle" class="hidden md:block text-sm text-muted dark:text-muted-dark truncate">
          {{ subtitle }}
        </p>
      </div>
    </div>

    <div
      v-if="$slots.search || $slots.actions"
      class="order-last sm:order-none basis-full sm:basis-auto sm:flex-1 min-w-0 flex items-center justify-start sm:justify-end gap-2 overflow-x-auto sm:overflow-visible"
    >
      <slot name="search" />
      <slot name="actions" />
    </div>

    <button
      v-if="primaryLabel"
      type="button"
      class="btn-primary shrink-0"
      :aria-label="primaryLabel"
      @click="emit('primary')"
    >
      <component :is="primaryIcon" v-if="primaryIcon" class="w-4 h-4" aria-hidden="true" />
      <span :class="primaryIcon ? 'hidden sm:inline' : ''">{{ primaryLabel }}</span>
    </button>
  </header>
</template>
