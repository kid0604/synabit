<script setup lang="ts">
/**
 * One section of Syn's settings, which folds.
 *
 * `<details>` rather than a button and a `v-if`: the browser does the opening,
 * the keyboard and the screen-reader state, and a folded section's fields stay
 * in the form — Save still sends them. It is Baseline Widely available, so the
 * oldest WebView this app runs in has it.
 *
 * Which sections are open is remembered on this device, per section. A person
 * who only ever touches the model should not have to fold the rest every time.
 * Local storage, because it is a convenience: when it is unavailable every
 * section simply starts as `defaultOpen` says.
 */
import { onMounted, ref } from 'vue';
import { ChevronRight } from 'lucide-vue-next';

const props = withDefaults(
  defineProps<{
    /** Stable, for remembering: `connection`, `telegram`… */
    id: string;
    title: string;
    /** One line shown beside the title, so a folded section still says what it holds. */
    summary?: string;
    /** A section whose controls act at once rather than waiting for Save. */
    savesItself?: boolean;
    savesItselfLabel?: string;
    defaultOpen?: boolean;
  }>(),
  { summary: '', savesItself: false, savesItselfLabel: '', defaultOpen: false },
);

const KEY = 'syn.settings.open';
const open = ref(props.defaultOpen);

const readAll = (): Record<string, boolean> => {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? '{}') ?? {};
  } catch {
    return {};
  }
};

onMounted(() => {
  const kept = readAll()[props.id];
  if (typeof kept === 'boolean') open.value = kept;
});

const onToggle = (event: Event) => {
  open.value = (event.target as HTMLDetailsElement).open;
  try {
    localStorage.setItem(KEY, JSON.stringify({ ...readAll(), [props.id]: open.value }));
  } catch {
    // Private window, blocked storage: it just is not remembered.
  }
};
</script>

<template>
  <details
    :open="open"
    class="group rounded-xl border border-gray-200 dark:border-gray-700/50"
    :data-section="id"
    @toggle="onToggle"
  >
    <summary
      class="flex items-center gap-2 px-4 py-3 cursor-pointer select-none list-none rounded-xl
             [&::-webkit-details-marker]:hidden
             hover:bg-gray-50 dark:hover:bg-white/5
             focus-visible:outline-2 focus-visible:outline-accent dark:focus-visible:outline-accent-dark"
    >
      <ChevronRight
        class="w-4 h-4 shrink-0 text-gray-500 dark:text-gray-400 transition-transform duration-150 group-open:rotate-90"
        aria-hidden="true"
      />
      <span class="text-xs font-bold uppercase tracking-wider text-gray-600 dark:text-gray-300">
        {{ title }}
      </span>
      <span
        v-if="savesItself"
        class="px-1.5 py-0.5 rounded-md text-xs font-medium bg-gray-100 dark:bg-white/10 text-gray-500 dark:text-gray-400"
      >
        {{ savesItselfLabel }}
      </span>
      <span v-if="summary" class="ml-auto min-w-0 truncate text-xs text-gray-500 dark:text-gray-400">
        {{ summary }}
      </span>
    </summary>
    <div class="px-4 pb-4 pt-1">
      <slot />
    </div>
  </details>
</template>
