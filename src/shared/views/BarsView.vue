<script setup lang="ts">
/**
 * A heap of labels with a number against each, drawn so the numbers compare.
 *
 * The design is `docs/query-grammar-2026-09-20.md` §7. This is what `| stats`
 * produces, and it arrives now rather than earlier because a renderer built
 * before there was data to put in it would have been a guess about what the
 * data would look like.
 *
 * # Why a bar and not a column of digits
 *
 * `stats count by month` answers a question about *proportion* — was this
 * month busier than that one — and a column of digits makes the reader do the
 * comparing. The bar does it for them; the digit stays beside it, because the
 * bar is the comparison and the digit is the fact.
 *
 * Keeps the same contract as the other view primitives:
 *
 * 1. It is given a result; it does not fetch one.
 * 2. It never asks what type a row is in order to decide what to do.
 *
 * A row here has no node behind it — a heap of things is not a thing — so it
 * emits nothing on click and shows no pointer. Everything else about the
 * result is read positionally, from the two columns `stats` makes.
 */
import { computed } from 'vue';
import type { QueryResult } from './types';

const props = defineProps<{ result: QueryResult | null }>();

interface Bar {
  label: string;
  value: number;
  shown: string;
  /** How much of the track to fill, 0–100. */
  width: number;
}

/**
 * The bars, scaled against the largest.
 *
 * Against the largest rather than against the total: the question is which of
 * these is bigger, and a share-of-total scale flattens everything when there
 * are twenty of them. Zero is drawn as a hairline rather than as nothing, so a
 * row that is there but empty is still visibly a row.
 */
const bars = computed<Bar[]>(() => {
  if (!props.result) return [];
  const rows = props.result.rows.map(row => ({
    label: (row.cells[0] ?? '').trim(),
    shown: (row.cells[1] ?? '').trim(),
    value: Number((row.cells[1] ?? '').trim()) || 0,
  }));
  const most = rows.reduce((top, row) => Math.max(top, row.value), 0);
  return rows.map(row => ({
    ...row,
    width: most > 0 ? Math.max(1.5, (row.value / most) * 100) : 0,
  }));
});

/** What the number column is called, which is what was worked out. */
const measure = computed(() => props.result?.columns[1] ?? '');
</script>

<template>
  <div v-if="result" data-bars-view class="px-4 py-3">
    <p v-if="!bars.length" data-bars-empty class="py-2 text-[12px] text-gray-400">
      {{ $t('nexus.lens_nothing') }}
    </p>

    <div
      v-for="bar in bars"
      :key="bar.label"
      data-bar
      class="flex items-center gap-3 py-[3px]"
    >
      <span
        data-bar-label
        class="w-[92px] flex-shrink-0 truncate text-right text-[11px] tabular-nums text-gray-500 dark:text-gray-400"
        :title="bar.label"
      >
        {{ bar.label }}
      </span>

      <span class="h-[14px] min-w-0 flex-grow rounded-sm bg-gray-100 dark:bg-[#3a3a3c]">
        <span
          data-bar-fill
          class="block h-full rounded-sm bg-indigo-500 dark:bg-indigo-400"
          :style="{ width: `${bar.width}%` }"
          :aria-label="`${bar.label}: ${bar.shown} ${measure}`"
        />
      </span>

      <span
        data-bar-value
        class="w-[52px] flex-shrink-0 text-[11px] tabular-nums text-gray-700 dark:text-gray-200"
      >
        {{ bar.shown }}
      </span>
    </div>
  </div>
</template>
