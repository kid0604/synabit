<script setup lang="ts">
/**
 * Which stretch of time the chart and the list below it are drawn over.
 *
 * One row, above what it scopes, stretches counted back from today first and
 * a stretch of one's own last — the order the dataviz guidance gives for a
 * date range, because "the last year" is what somebody reaches for and a
 * calendar is what they reach for second.
 *
 * It owns no state beyond the two half-typed days of a custom stretch; what
 * is chosen belongs to whoever draws the chart, so the chart and the list
 * cannot be scoped to two different windows.
 */
import { computed, ref, watch } from 'vue';
import { PRESETS, dayOf, type DayRange, type Preset, type WindowChoice } from './overTime';

const props = defineProps<{
  modelValue: WindowChoice;
  /** The stretch the choice covers today, for seeding a custom one. */
  range: DayRange | null;
  /** Dated rows the window leaves out. */
  hidden: number;
}>();

const emit = defineEmits<{ 'update:modelValue': [choice: WindowChoice] }>();

const custom = computed(() => typeof props.modelValue === 'object');
const picking = ref(false);
const from = ref('');
const to = ref('');

/** Opening the custom stretch starts from whatever is on screen now. */
const openCustom = () => {
  picking.value = true;
  from.value = props.range?.from ?? '';
  to.value = props.range?.to ?? '';
};

watch(
  () => props.modelValue,
  choice => {
    if (typeof choice === 'object') {
      from.value = choice.from;
      to.value = choice.to;
    } else {
      picking.value = false;
    }
  },
  { immediate: true },
);

/**
 * Applied as soon as both ends are days and the stretch runs forwards.
 *
 * Read through `dayOf`, not trusted: a WebView old enough to draw this as a
 * plain text box lets anything be typed, and `22/09/2026` compared as a
 * string would make a window that means nothing.
 */
const applyCustom = () => {
  if (dayOf(from.value) && dayOf(to.value) && from.value <= to.value) {
    emit('update:modelValue', { from: from.value, to: to.value });
  }
};

const pressed = (preset: Preset) => props.modelValue === preset;
const backwards = computed(() => !!from.value && !!to.value && from.value > to.value);

const CHIP = 'h-7 rounded-full px-3 text-[12px] font-semibold transition-colors';
const ON = 'bg-indigo-600 text-white dark:bg-indigo-500';
const OFF = 'text-gray-500 hover:bg-gray-100 hover:text-gray-800 dark:text-gray-400 dark:hover:bg-[#2c2c2e] dark:hover:text-gray-100';
</script>

<template>
  <div data-time-window class="flex flex-col gap-1.5">
    <div role="group" :aria-label="$t('nexus.window_label')" class="flex flex-wrap items-center gap-1">
      <button
        v-for="preset in PRESETS"
        :key="preset"
        type="button"
        data-window-preset
        :data-preset="preset"
        :aria-pressed="pressed(preset)"
        :class="[CHIP, pressed(preset) ? ON : OFF]"
        @click="emit('update:modelValue', preset)"
      >
        {{ $t(`nexus.window_${preset}`) }}
      </button>
      <button
        type="button"
        data-window-custom
        :aria-pressed="custom || picking"
        :class="[CHIP, custom ? ON : OFF]"
        @click="openCustom"
      >
        {{ $t('nexus.window_custom') }}
      </button>

      <!-- The custom stretch, only once asked for. -->
      <span v-if="picking || custom" class="ml-1 flex items-center gap-1.5 text-[12px] text-gray-500 dark:text-gray-400">
        <input
          v-model="from"
          data-window-from
          type="date"
          :aria-label="$t('nexus.window_from')"
          class="h-7 rounded-md border border-gray-200 bg-white px-1.5 text-[12px] text-gray-800 dark:border-[#3a3a3c] dark:bg-[#1e1e20] dark:text-gray-100"
          @change="applyCustom"
        />
        →
        <input
          v-model="to"
          data-window-to
          type="date"
          :aria-label="$t('nexus.window_to')"
          class="h-7 rounded-md border border-gray-200 bg-white px-1.5 text-[12px] text-gray-800 dark:border-[#3a3a3c] dark:bg-[#1e1e20] dark:text-gray-100"
          @change="applyCustom"
        />
        <span v-if="backwards" data-window-backwards class="text-amber-600 dark:text-amber-400">{{ $t('nexus.window_backwards') }}</span>
      </span>
    </div>

    <!-- What the window leaves out, said rather than hidden, with the way
         to see it one press away. -->
    <p v-if="hidden" data-window-hidden class="text-[11px] text-gray-400">
      {{ $t('nexus.window_hidden', { n: hidden }, hidden) }}
      <button
        type="button"
        data-window-all
        class="ml-1 font-semibold text-indigo-600 underline decoration-dotted underline-offset-2 dark:text-indigo-400"
        @click="emit('update:modelValue', 'all')"
      >{{ $t('nexus.window_show_all') }}</button>
    </p>
  </div>
</template>
