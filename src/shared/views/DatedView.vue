<script setup lang="ts">
/**
 * An answer that has days in it, drawn down the days.
 *
 * The design is `docs/nexus-lenses-2026-09-19.md`, §7. A table hides the one
 * thing a question about time is asking: how the rows sit against each other
 * in time. Three things on one day and then nothing for five months is an
 * answer, and in a table it is four rows.
 *
 * Keeps the same contract as `TableView` and `ListView`:
 *
 * 1. It is given a result; it does not fetch one.
 * 2. It never asks what type a row is in order to decide what to do.
 *
 * Which column holds the day comes from `shapeFor`, so this and the chooser
 * cannot disagree about it.
 */
import { computed } from 'vue';
import type { QueryResult, QueryRow } from './types';
import { dateColumn } from './shapeFor';

const props = defineProps<{
  result: QueryResult | null;
  selectedId?: string | null;
  /** Shown when a row has no title of its own. */
  untitledLabel?: string;
  /**
   * Whether a row may be put away — asked of the caller rather than assumed,
   * because a refusal is a decision about the timeline and this view also
   * draws ordinary tables of notes. See `shared/putAway`.
   */
  offerPutAway?: boolean;
}>();

const emit = defineEmits<{ open: [row: QueryRow]; putAway: [row: QueryRow] }>();

const at = computed(() => (props.result ? dateColumn(props.result) : -1));

/** The columns worth showing beside the title: everything but title and day. */
const rest = computed(() => {
  if (!props.result) return [];
  return props.result.columns
    .map((name, index) => ({ name, index }))
    .filter(c => c.index !== at.value && c.name !== 'title');
});

interface Day {
  day: string;
  rows: QueryRow[];
}

/**
 * Rows gathered under the day they happened, in the order they arrived.
 *
 * The query decided the order — `sort:` is part of the question — so this
 * groups without reordering. Sorting here would quietly overrule `sort:when`.
 */
const days = computed<Day[]>(() => {
  if (!props.result) return [];
  const out: Day[] = [];
  for (const row of props.result.rows) {
    const day = (at.value >= 0 ? row.cells[at.value] : '')?.trim() || '';
    const last = out[out.length - 1];
    if (last && last.day === day) last.rows.push(row);
    else out.push({ day, rows: [row] });
  }
  return out;
});

const hasMore = computed(
  () => !!props.result && props.result.total > props.result.rows.length,
);

const cells = (row: QueryRow) =>
  rest.value.map(c => (row.cells[c.index] ?? '').trim()).filter(Boolean);
</script>

<template>
  <div v-if="result" data-dated-view class="px-4 py-3">
    <p v-if="!result.rows.length" data-dated-empty class="py-2 text-[12px] text-gray-400">
      {{ $t('nexus.lens_nothing') }}
    </p>

    <div v-for="group in days" :key="group.day" data-day class="flex gap-4 py-1.5">
      <span
        data-day-label
        class="w-[86px] flex-shrink-0 pt-[3px] text-[11px] tabular-nums text-gray-400"
      >
        {{ group.day || '—' }}
      </span>

      <div class="flex min-w-0 flex-grow flex-col gap-1">
        <span v-for="row in group.rows" :key="row.id" class="group flex items-center gap-1">
        <button
          type="button"
          data-dated-row
          class="flex items-baseline gap-2 rounded-md px-2 py-1 text-left transition-colors hover:bg-gray-100 dark:hover:bg-[#3a3a3c]"
          :class="row.id === selectedId ? 'bg-gray-100 dark:bg-[#3a3a3c]' : ''"
          @click="emit('open', row)"
        >
          <span class="min-w-0 flex-grow truncate text-[13px] text-gray-900 dark:text-gray-100">
            {{ row.title || untitledLabel || $t('nexus.lens_untitled') }}
          </span>
          <span
            v-for="cell in cells(row)"
            :key="cell"
            class="flex-shrink-0 truncate text-[11px] text-gray-500 dark:text-gray-400"
          >
            {{ cell }}
          </span>
        </button>
        <button
          v-if="offerPutAway"
          type="button"
          data-put-away
          :aria-label="$t('nexus.put_away_moment')"
          :title="$t('nexus.put_away_moment')"
          class="flex-shrink-0 px-1 text-[13px] leading-none text-gray-300 opacity-0 transition-opacity focus-visible:opacity-100 group-hover:opacity-100 hover:text-gray-600 dark:hover:text-gray-200"
          @click="emit('putAway', row)"
        >
          ×
        </button>
        </span>
      </div>
    </div>

    <p v-if="hasMore" data-dated-more class="pt-2 text-[11px] text-gray-400">
      {{ $t('nexus.lens_more', { count: result.total - result.rows.length }) }}
    </p>
  </div>
</template>
