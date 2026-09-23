<script setup lang="ts">
/**
 * An answer with days in it, drawn across time.
 *
 * Two layers on one axis. The columns count how many rows fall in each
 * stretch of time, which is what a question about a life usually wants first:
 * *when was this busy, when did it go quiet*. The dots under them are the rows
 * themselves, one each, so a quiet stretch with one important thing in it is
 * still visible as that one thing. Drag across the columns, or press one, to
 * narrow the list beside it to that stretch.
 *
 * The same contract as the other views: it is given a result and never
 * fetches one, and it never asks what type a row is. The list beside it is
 * its table view — every number here can also be read there without hovering.
 *
 * Colours: one series, so one hue and no legend; `indigo-600` on the light
 * surface and `indigo-500` on the dark one, both run through the dataviz
 * validator against Nexus's own surfaces (`indigo-400` fails the lightness
 * band on `#1a1a1c`).
 */
import { computed, onUnmounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { scaleLinear, scaleTime, timeDay } from 'd3';
import type { QueryResult, QueryRow } from './types';
import {
  bucketsOf, datedRows, dayOf, grainFor, inRange, intervalOf, isoOf, overlaps, rangeOf,
  type Bucket, type Dated, type DayRange,
} from './overTime';

const props = defineProps<{
  result: QueryResult | null;
  /** The stretch of time the list is narrowed to, or null. */
  modelValue: DayRange | null;
  /**
   * The stretch the axis covers, when a window is set. Drawn whole, so the
   * quiet months at either end of "the last year" are still on it.
   */
  span?: DayRange | null;
}>();

const emit = defineEmits<{
  'update:modelValue': [range: DayRange | null];
  open: [row: QueryRow];
}>();

const { t, locale } = useI18n();

// ── size ────────────────────────────────────────────────────────────────
// Measured, because the chart fills whatever it is put in: the whole pane
// beside a search, or a band above a list while browsing.
const box = ref<HTMLElement | null>(null);
const width = ref(640);
const height = ref(240);
let observer: ResizeObserver | null = null;

const measure = () => {
  if (!box.value) return;
  if (box.value.clientWidth > 0) width.value = box.value.clientWidth;
  if (box.value.clientHeight > 0) height.value = box.value.clientHeight;
};

// Watched rather than read on mount: the plot is only there once there is
// something to plot, and an answer usually arrives after the chart does.
watch(box, el => {
  observer?.disconnect();
  observer = null;
  if (!el) return;
  measure();
  if (typeof ResizeObserver !== 'undefined') {
    observer = new ResizeObserver(measure);
    observer.observe(el);
  }
});
onUnmounted(() => observer?.disconnect());

// ── layout ──────────────────────────────────────────────────────────────
const LEFT = 36;
const RIGHT = 12;
const TOP = 8;
/** Room under the plot for the axis labels, so they are never cropped. */
const AXIS = 22;
const DOTS = 44;
const GAP = 10;
/** The lane stretches sit in, above the columns. One row is this tall. */
const SPAN_ROW = 12;
const SPAN_ROWS = 3;
/** One dot and the space around it; dots on the same spot stack by this. */
const STEP = 9;
const LEVELS = 4;

const plotRight = computed(() => Math.max(LEFT + 40, width.value - RIGHT));
/** How much of the top the stretches take, which is none when there are none. */
const spanLane = computed(() => (spans.value.length ? Math.min(SPAN_ROWS, spans.value.length) * SPAN_ROW + 4 : 0));
const barsTop = computed(() => TOP + spanLane.value);
const barsBottom = computed(() => Math.max(barsTop.value + 60, height.value - AXIS - DOTS - GAP));
const dotsBase = computed(() => barsBottom.value + GAP + DOTS - 6);
const axisY = computed(() => height.value - AXIS + 15);

// ── data ────────────────────────────────────────────────────────────────
const placed = computed(() => datedRows(props.result));
const dated = computed(() => placed.value.dated);

/**
 * Stretches — a job, years at a school, a trip of five days — and the rest.
 *
 * Counted apart, and drawn apart. A stretch adds one to every column it
 * crosses if it is counted like a day, so four years at university would put
 * a 1 on forty-eight months and the columns would stop meaning anything.
 * Above the columns it says what it is: a line, with when it began and ended.
 */
const spans = computed(() => dated.value.filter(item => item.last > item.iso));
const points = computed(() => dated.value.filter(item => item.last === item.iso));

/** The window as days, if there is one. */
const spanDays = computed(() => {
  const from = props.span && dayOf(props.span.from);
  const to = props.span && dayOf(props.span.to);
  return from && to ? { from, to } : null;
});

const grain = computed(() => {
  if (spanDays.value) return grainFor(spanDays.value.from, spanDays.value.to);
  const all = dated.value;
  if (!all.length) return 'day' as const;
  let first = all[0].day;
  let last = all[0].day;
  for (const item of all) {
    if (item.day < first) first = item.day;
    // A stretch is on the axis until it ends, or the chart stops before it does.
    const ends = dayOf(item.last) ?? item.day;
    if (ends > last) last = ends;
  }
  return grainFor(first, last);
});

const buckets = computed(() => {
  const span = spanDays.value ?? undefined;
  if (span || points.value.length) return bucketsOf(points.value, grain.value, span);
  // Stretches and nothing else: the axis is theirs.
  const ends = dated.value.map(item => ({ ...item, day: dayOf(item.last) ?? item.day }));
  return bucketsOf([...dated.value, ...ends], grain.value, span);
});

/**
 * Columns only when there is something to compare. One busy bucket is a
 * one-bar chart, and a one-bar chart is a number pretending to be a picture:
 * the sentence in the header says it better.
 */
const busy = computed(() => buckets.value.filter(b => b.count > 0).length);
// Inside a window one busy column still says something — *where* in the
// year it was — because the axis is the window, not the data.
const drawBars = computed(() => busy.value > 1 || (!!spanDays.value && busy.value > 0));

const x = computed(() => {
  const all = buckets.value;
  const domain = all.length ? [all[0].start, all[all.length - 1].end] : [new Date(), new Date()];
  return scaleTime().domain(domain).range([LEFT, plotRight.value]);
});

const y = computed(() => {
  const peak = buckets.value.reduce((most, b) => Math.max(most, b.count), 1);
  return scaleLinear().domain([0, peak]).nice(3).range([barsBottom.value, barsTop.value]);
});

/** Whole numbers only: there is no such thing as half an event. */
const yTicks = computed(() => y.value.ticks(3).filter(Number.isInteger));

/** A column with a 4px rounded top and a square foot on the baseline. */
const roundedTop = (left: number, top: number, w: number, h: number) => {
  const r = Math.min(4, w / 2, h);
  return `M${left},${top + h}V${top + r}Q${left},${top} ${left + r},${top}H${left + w - r}Q${left + w},${top} ${left + w},${top + r}V${top + h}Z`;
};

const bars = computed(() =>
  buckets.value.map(bucket => {
    const x0 = x.value(bucket.start);
    const band = x.value(bucket.end) - x0;
    // Capped at 24px, and never more than 60% of its slot: a column that
    // fills its slot reads as a wall. Looked at on a narrow pane, `band − 2`
    // left 2px between eighteen columns and the chart was one indigo block.
    const w = Math.max(1, Math.min(24, band * 0.6, band - 2));
    const left = x0 + (band - w) / 2;
    const top = y.value(bucket.count);
    const h = barsBottom.value - top;
    return { bucket, left, w, top, path: bucket.count ? roundedTop(left, top, w, h) : '' };
  }),
);

/** One line per stretch, in the lane above the columns. */
const lanes = computed(() => {
  const first = buckets.value[0]?.start;
  const last = buckets.value[buckets.value.length - 1]?.end;
  if (!first || !last) return [];
  return spans.value.slice(0, SPAN_ROWS * 2).map((item, n) => {
    const ends = dayOf(item.last) ?? item.day;
    // Cut to the chart, and said to run on where it does.
    const from = Math.max(LEFT, x.value(item.day < first ? first : item.day));
    const to = Math.min(plotRight.value, x.value(ends > last ? last : ends));
    return {
      item,
      left: from,
      width: Math.max(3, to - from),
      y: TOP + (n % SPAN_ROWS) * SPAN_ROW,
      label: `${item.iso} – ${item.last}`,
      runsOn: ends > last,
    };
  });
});

/** One dot per row that happened on a day, stacked where several share a spot. */
const dots = computed(() => {
  const used = new Map<number, number>();
  return points.value.map(item => {
    const cx = (x.value(item.day) + x.value(timeDay.offset(item.day, 1))) / 2;
    const column = Math.round(cx / STEP);
    const level = used.get(column) ?? 0;
    used.set(column, level + 1);
    return { item, cx, cy: dotsBase.value - Math.min(level, LEVELS - 1) * STEP };
  });
});

/**
 * The axis labels: one per bucket, under the middle of it, thinned to fit.
 *
 * Not `x.ticks()`. d3 picks ticks for the *domain*, so an answer that is one
 * day long got ticks every three hours — nine labels all reading «Jul 8» —
 * because the scale knows nothing of the grain the columns were counted in.
 * Taking the labels from the buckets means the axis can never divide time
 * finer than the chart does, and each label sits under its own column.
 */
const xTicks = computed(() => {
  const all = buckets.value;
  const room = Math.max(1, Math.floor((plotRight.value - LEFT) / 80));
  const every = Math.max(1, Math.ceil(all.length / room));
  return all
    .filter((_, i) => i % every === 0)
    .map(bucket => ({
      key: bucket.start.getTime(),
      at: (x.value(bucket.start) + x.value(bucket.end)) / 2,
      label: tickFormat.value.format(bucket.start),
    }));
});

// ── words ───────────────────────────────────────────────────────────────
const tickFormat = computed(() =>
  new Intl.DateTimeFormat(
    locale.value,
    grain.value === 'year'
      ? { year: 'numeric' }
      : grain.value === 'month'
        ? { month: 'short', year: 'numeric' }
        : { day: 'numeric', month: 'short' },
  ),
);

const dayFormat = computed(
  () => new Intl.DateTimeFormat(locale.value, { day: 'numeric', month: 'short', year: 'numeric' }),
);

/** What stretch a bucket is, in words. */
const bucketLabel = (bucket: Bucket) => {
  if (grain.value === 'year') return String(bucket.start.getFullYear());
  if (grain.value === 'month') {
    return new Intl.DateTimeFormat(locale.value, { month: 'long', year: 'numeric' }).format(bucket.start);
  }
  if (grain.value === 'day') return dayFormat.value.format(bucket.start);
  return `${dayFormat.value.format(bucket.start)} – ${dayFormat.value.format(timeDay.offset(bucket.end, -1))}`;
};

const count = (n: number) => t('nexus.over_time_count', { n }, n);

const selectedCount = computed(() => {
  const range = props.modelValue;
  return range ? dated.value.filter(item => overlaps(item.iso, item.last, range)).length : 0;
});

/** The chart draws what it was given. When that is a page, it says so. */
const shown = computed(() => props.result?.rows.length ?? 0);
const partial = computed(() => !!props.result && props.result.total > shown.value);

// ── hover, focus and selecting a stretch ────────────────────────────────
type Hover = { kind: 'bar'; bucket: Bucket; at: number } | { kind: 'dot'; item: Dated; cx: number; cy: number };
const hover = ref<Hover | null>(null);
const focused = ref<number | null>(null);

const svg = ref<SVGSVGElement | null>(null);
const localX = (event: PointerEvent) => event.clientX - (svg.value?.getBoundingClientRect().left ?? 0);
const clampX = (px: number) => Math.min(plotRight.value, Math.max(LEFT, px));

const bucketAt = (px: number): number => {
  const all = buckets.value;
  for (let i = 0; i < all.length; i += 1) {
    if (px >= x.value(all[i].start) && px < x.value(all[i].end)) return i;
  }
  return -1;
};

const showBucket = (i: number) => {
  const bucket = buckets.value[i];
  hover.value = bucket ? { kind: 'bar', bucket, at: bars.value[i].left + bars.value[i].w / 2 } : null;
};

const pick = (range: DayRange | null) => emit('update:modelValue', range);

const toggleBucket = (i: number) => {
  const bucket = buckets.value[i];
  if (!bucket || !bucket.count) return;
  const range = rangeOf(bucket);
  const same = props.modelValue?.from === range.from && props.modelValue?.to === range.to;
  pick(same ? null : range);
};

/**
 * Dragging across the columns selects the stretch under the drag, snapped
 * out to whole buckets; a press without a drag selects the one bucket, and
 * pressing it again lets go. The capture is what keeps a drag that wanders
 * off the chart from being lost halfway.
 */
let dragFrom: number | null = null;
const dragTo = ref<number | null>(null);
const dragging = ref(false);

const onDown = (event: PointerEvent) => {
  (event.currentTarget as Element).setPointerCapture?.(event.pointerId);
  dragFrom = localX(event);
  dragTo.value = dragFrom;
  dragging.value = false;
};

const onMove = (event: PointerEvent) => {
  const px = localX(event);
  const i = bucketAt(px);
  if (i >= 0) showBucket(i);
  else hover.value = null;
  if (dragFrom !== null) {
    dragTo.value = px;
    if (Math.abs(px - dragFrom) > 4) dragging.value = true;
  }
};

const onUp = (event: PointerEvent) => {
  if (dragFrom === null) return;
  const px = localX(event);
  if (!dragging.value) {
    const i = bucketAt(px);
    if (i >= 0) toggleBucket(i);
  } else {
    const interval = intervalOf(grain.value);
    const lo = interval.floor(x.value.invert(clampX(Math.min(dragFrom, px))));
    const hi = interval.ceil(x.value.invert(clampX(Math.max(dragFrom, px))));
    pick({ from: isoOf(lo), to: isoOf(timeDay.offset(hi, -1)) });
  }
  dragFrom = null;
  dragTo.value = null;
  dragging.value = false;
};

const onLeave = () => {
  if (dragFrom === null) hover.value = null;
};

/** The same, from the keyboard: arrows move between stretches, Enter picks. */
const onKey = (event: KeyboardEvent) => {
  const last = buckets.value.length - 1;
  if (last < 0) return;
  if (event.key === 'ArrowRight' || event.key === 'ArrowLeft') {
    event.preventDefault();
    const step = event.key === 'ArrowRight' ? 1 : -1;
    const from = focused.value ?? (step > 0 ? -1 : last + 1);
    focused.value = Math.min(last, Math.max(0, from + step));
    showBucket(focused.value);
  } else if ((event.key === 'Enter' || event.key === ' ') && focused.value !== null) {
    event.preventDefault();
    toggleBucket(focused.value);
  } else if (event.key === 'Escape') {
    pick(null);
  }
};

const onBlur = () => {
  focused.value = null;
  hover.value = null;
};

/** Where the stretch is drawn: the one being dragged, or the one picked. */
const selection = computed(() => {
  if (dragging.value && dragFrom !== null && dragTo.value !== null) {
    const a = clampX(Math.min(dragFrom, dragTo.value));
    const b = clampX(Math.max(dragFrom, dragTo.value));
    return { left: a, width: Math.max(2, b - a) };
  }
  const range = props.modelValue;
  const from = range && dayOf(range.from);
  const to = range && dayOf(range.to);
  if (!from || !to) return null;
  const a = clampX(x.value(from));
  const b = clampX(x.value(timeDay.offset(to, 1)));
  return { left: a, width: Math.max(2, b - a) };
});

const tooltip = computed(() => {
  const h = hover.value;
  if (!h) return null;
  const at = h.kind === 'bar' ? h.at : h.cx;
  const top = h.kind === 'bar' ? barsTop.value : h.cy - 10;
  return {
    left: Math.min(width.value - 90, Math.max(90, at)),
    top,
    value: h.kind === 'bar' ? count(h.bucket.count) : h.item.row.title,
    label: h.kind === 'bar' ? bucketLabel(h.bucket) : dayFormat.value.format(h.item.day),
  };
});
</script>

<template>
  <div data-over-time class="flex h-full w-full flex-col gap-2">
    <!-- One series: the title names it, so there is no legend box. -->
    <div class="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1 px-1">
      <h3 class="text-sm font-bold text-gray-700 dark:text-gray-200">
        {{ $t('nexus.over_time_title', { grain: $t(`nexus.over_time_grain_${grain}`) }) }}
      </h3>
      <div class="flex items-center gap-3 text-[12px] text-gray-500 dark:text-gray-400">
        <template v-if="modelValue">
          <span data-over-time-selected class="tabular-nums">
            {{ $t('nexus.over_time_selected', { from: modelValue.from, to: modelValue.to, count: count(selectedCount) }) }}
          </span>
          <button
            type="button"
            data-over-time-clear
            class="font-semibold text-indigo-600 underline decoration-dotted underline-offset-2 dark:text-indigo-400"
            @click="pick(null)"
          >{{ $t('nexus.over_time_clear') }}</button>
        </template>
        <span v-else-if="drawBars">{{ $t('nexus.over_time_hint') }}</span>
      </div>
    </div>

    <!-- What the picture could not include, said rather than left out. -->
    <p v-if="partial || placed.undated" data-over-time-note class="px-1 text-[11px] text-gray-400">
      <template v-if="partial">{{ $t('nexus.over_time_partial', { shown, total: result?.total ?? 0 }) }}</template>
      <template v-if="partial && placed.undated"> · </template>
      <template v-if="placed.undated">{{ $t('nexus.over_time_undated', { n: placed.undated }, placed.undated) }}</template>
    </p>

    <p v-if="!dated.length" data-over-time-empty class="px-1 py-6 text-[12px] text-gray-400">
      {{ $t('nexus.over_time_none') }}
    </p>

    <template v-else>
      <p v-if="!drawBars" data-over-time-one class="px-1 text-[13px] text-gray-700 dark:text-gray-200">
        {{ $t('nexus.over_time_one', { count: count(dated.length), when: bucketLabel(buckets.find(b => b.count)!) }) }}
      </p>

      <!-- Grows with its container, to a point: a column chart stretched to
           the full height of a pane is mostly column, and the shape of a year
           reads just as well at a third of that. -->
      <div ref="box" class="relative min-h-[160px] max-h-[320px] flex-1">
        <svg
          ref="svg"
          class="absolute inset-0 h-full w-full select-none overflow-visible"
          :viewBox="`0 0 ${width} ${height}`"
          role="img"
          :aria-label="$t('nexus.over_time_title', { grain: $t(`nexus.over_time_grain_${grain}`) })"
        >
          <!-- Recessive: hairline, solid, one step off the surface. -->
          <g v-if="drawBars" aria-hidden="true">
            <template v-for="tick in yTicks" :key="`y${tick}`">
              <line
                :x1="LEFT" :x2="plotRight" :y1="y(tick)" :y2="y(tick)"
                class="stroke-gray-200 dark:stroke-[#2c2c2e]" stroke-width="1" shape-rendering="crispEdges"
              />
              <text
                :x="LEFT - 8" :y="y(tick)" dy="0.32em" text-anchor="end"
                class="fill-gray-400 text-[10px] tabular-nums"
              >{{ tick }}</text>
            </template>
          </g>

          <rect
            v-if="selection"
            data-over-time-band
            :x="selection.left" :y="barsTop" :width="selection.width" :height="dotsBase + 6 - barsTop"
            class="fill-indigo-600/10 dark:fill-indigo-500/15"
          />

          <g v-if="drawBars" aria-hidden="true">
            <path
              v-for="(bar, i) in bars"
              v-show="bar.path"
              :key="bar.bucket.start.getTime()"
              data-over-time-bar
              :data-count="bar.bucket.count"
              :d="bar.path"
              class="fill-indigo-600 transition-opacity dark:fill-indigo-500"
              :class="
                (hover?.kind === 'bar' && hover.bucket === bar.bucket) || focused === i
                  ? 'opacity-75'
                  : modelValue && !inRange(isoOf(bar.bucket.start), modelValue) && !inRange(isoOf(timeDay.offset(bar.bucket.end, -1)), modelValue)
                    ? 'opacity-30'
                    : ''
              "
            />
          </g>

          <line
            :x1="LEFT" :x2="plotRight" :y1="barsBottom" :y2="barsBottom"
            class="stroke-gray-300 dark:stroke-[#3a3a3c]" stroke-width="1" shape-rendering="crispEdges"
            aria-hidden="true"
          />

          <!-- The columns' hit area: the whole band, not the painted pixels,
               and a focus stop so the keyboard can do what the pointer does. -->
          <rect
            v-if="drawBars"
            data-over-time-hit
            :x="LEFT" :y="barsTop" :width="plotRight - LEFT" :height="barsBottom - barsTop"
            fill="transparent"
            class="cursor-crosshair touch-none outline-none focus-visible:stroke-indigo-500"
            tabindex="0"
            :aria-label="$t('nexus.over_time_keys')"
            @pointerdown="onDown"
            @pointermove="onMove"
            @pointerup="onUp"
            @pointercancel="onUp"
            @pointerleave="onLeave"
            @keydown="onKey"
            @blur="onBlur"
          />

          <!-- Stretches, above the columns and not counted in them: a bar
               from when it began to when it ended, named. -->
          <g v-if="lanes.length">
            <g
              v-for="lane in lanes"
              :key="`span-${lane.item.row.id}`"
              data-over-time-span
              class="cursor-pointer"
              @pointerenter="hover = { kind: 'dot', item: lane.item, cx: lane.left + lane.width / 2, cy: lane.y + 8 }"
              @pointerleave="hover = null"
              @click="emit('open', lane.item.row)"
            >
              <rect
                :x="lane.left" :y="lane.y" :width="lane.width" height="4" rx="2"
                class="fill-indigo-400 dark:fill-indigo-500"
                :class="modelValue && !overlaps(lane.item.iso, lane.item.last, modelValue) ? 'opacity-30' : ''"
              />
              <text
                :x="lane.left + 4" :y="lane.y + SPAN_ROW - 1"
                class="fill-gray-500 text-[9px] dark:fill-gray-400"
              >{{ lane.item.row.title }}<tspan v-if="lane.runsOn"> →</tspan></text>
            </g>
          </g>

          <!-- One dot per row, with a ring in the surface colour so the ones
               that overlap stay apart. Not in the tab order: a thousand stops
               is no way to reach anything, and the list beside it is. -->
          <g>
            <g
              v-for="dot in dots"
              :key="dot.item.row.id"
              data-over-time-dot
              class="cursor-pointer"
              @pointerenter="hover = { kind: 'dot', item: dot.item, cx: dot.cx, cy: dot.cy }"
              @pointerleave="hover = null"
              @click="emit('open', dot.item.row)"
            >
              <circle :cx="dot.cx" :cy="dot.cy" r="12" fill="transparent" />
              <circle
                :cx="dot.cx" :cy="dot.cy" r="4"
                class="fill-indigo-600 stroke-[#fdfdfc] dark:fill-indigo-500 dark:stroke-[#1a1a1c]"
                stroke-width="2"
                :class="modelValue && !overlaps(dot.item.iso, dot.item.last, modelValue) ? 'opacity-30' : ''"
              />
            </g>
          </g>

          <g aria-hidden="true">
            <text
              v-for="tick in xTicks"
              :key="`x${tick.key}`"
              data-over-time-tick
              :x="tick.at" :y="axisY" text-anchor="middle"
              class="fill-gray-400 text-[10px] tabular-nums"
            >{{ tick.label }}</text>
          </g>
        </svg>

        <!-- Values lead, labels follow. Interpolated, so a title is always
             text and never markup. -->
        <div
          v-if="tooltip"
          data-over-time-tooltip
          class="pointer-events-none absolute z-10 -translate-x-1/2 -translate-y-full rounded-lg border border-gray-200 bg-white px-2.5 py-1.5 shadow-lg dark:border-[#3a3a3c] dark:bg-[#242426]"
          :style="{ left: `${tooltip.left}px`, top: `${tooltip.top}px` }"
        >
          <p class="max-w-[240px] truncate text-[12px] font-semibold text-gray-900 dark:text-gray-100">{{ tooltip.value }}</p>
          <p class="text-[11px] text-gray-500 dark:text-gray-400">{{ tooltip.label }}</p>
        </div>
      </div>
    </template>
  </div>
</template>
