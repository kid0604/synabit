<script setup lang="ts">
/**
 * A chart of a Rich Table: handed a `ChartModel`, never the table.
 *
 * Drawn by the dataviz method this repository follows: thin marks with a
 * rounded data end and a 2px surface gap, hairline solid grid, a legend for
 * two series or more and selective direct labels, text in text colours never
 * in the series colour, a hover layer on every form, and a table of the same
 * numbers one click away — the chart enhances the numbers, never gates them.
 * Colours are the validated categorical palette (`richTable.css`, `--rt-s*`).
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import {
  scaleBand, scaleLinear, scalePoint, area as d3area, arc as d3arc, line as d3line, pie as d3pie,
  curveMonotoneX, Delaunay, min, max,
} from 'd3';
import { Table2, ArrowUp, ArrowDown, ImageDown } from 'lucide-vue-next';
import { toPng } from 'html-to-image';
import { save } from '@tauri-apps/plugin-dialog';
import { writeFile } from '@tauri-apps/plugin-fs';
import { BLANK_KEY } from '../aggregate';
import { OTHER, type ChartModel } from '../chartData';
import { formatNumber } from '../model';

const props = defineProps<{
  model: ChartModel;
  locale: string;
  height?: number;
  /** What a saved picture is called, before `.png`. */
  title?: string;
}>();
const emit = defineEmits<{ pick: [key: string] }>();
const { t } = useI18n();

const host = ref<HTMLElement | null>(null);
const width = ref(640);
let observer: ResizeObserver | null = null;
onMounted(() => {
  if (!host.value) return;
  width.value = host.value.clientWidth || 640;
  if (typeof ResizeObserver !== 'undefined') {
    observer = new ResizeObserver(([entry]) => { width.value = Math.max(240, entry.contentRect.width); });
    observer.observe(host.value);
  }
});
onBeforeUnmount(() => observer?.disconnect());

const showTable = ref(false);
const exporting = ref(false);
const exportError = ref('');

/**
 * The chart as a PNG, saved where the person chooses. Drawn at twice the
 * size, on the chart's own background, without the buttons above it.
 */
async function exportPng() {
  const el = host.value;
  if (!el || exporting.value) return;
  exporting.value = true;
  exportError.value = '';
  try {
    const background = getComputedStyle(el).backgroundColor;
    const url = await toPng(el, {
      pixelRatio: 2,
      backgroundColor: background,
      filter: (n) => !(n instanceof HTMLElement && n.classList.contains('rt-chart-actions')),
    });
    const fallback = t('rich_table.layouts.chart');
    const name = (props.title || fallback).replace(/[\\/:*?"<>|]/g, ' ').trim() || fallback;
    const path = await save({ defaultPath: `${name}.png`, filters: [{ name: 'PNG', extensions: ['png'] }] });
    if (!path) return;
    const bytes = Uint8Array.from(atob(url.split(',')[1]), (c) => c.charCodeAt(0));
    await writeFile(path, bytes);
  } catch (e) {
    exportError.value = t('rich_table.export.failed', { reason: String((e as { message?: string })?.message ?? e) });
  } finally {
    exporting.value = false;
  }
}

// ─── Words and numbers ──────────────────────────────────────────

const name = (key: string, label: string) =>
  key === BLANK_KEY ? t('rich_table.chart.blank') : key === OTHER ? t('rich_table.chart.other') : label;

const fmt = (n: number | null | undefined, format = props.model.format) =>
  n === null || n === undefined ? '—' : formatNumber(n, props.locale, format);

const compact = computed(() => {
  const f = props.model.format;
  const currency = f === 'vnd' ? 'VND' : f === 'usd' ? 'USD' : f === 'eur' ? 'EUR' : null;
  return new Intl.NumberFormat(props.locale, {
    notation: 'compact',
    maximumFractionDigits: 1,
    ...(currency ? { style: 'currency', currency } : f === 'percent' ? { style: 'percent' } : {}),
  });
});
const tick = (n: number) => compact.value.format(n);

const color = (slot: number | 'other') => (slot === 'other' ? 'var(--rt-s-other)' : `var(--rt-s${slot + 1})`);

// ─── Geometry ───────────────────────────────────────────────────

const kind = computed(() => props.model.kind);
const H = computed(() => props.height ?? 260);
const horizontal = computed(() => kind.value === 'hbar');
const multi = computed(() => props.model.series.length > 1);

/** Per category, the bottom and top of each series' segment: stacked or side by side. */
const extents = computed(() => {
  const { values, stack } = props.model;
  const cats = props.model.categories.length;
  let lo = 0;
  let hi = 0;
  for (let c = 0; c < cats; c++) {
    let pos = 0;
    let neg = 0;
    for (const vs of values) {
      const v = vs[c] ?? 0;
      if (stack) {
        if (v >= 0) pos += v; else neg += v;
      } else {
        hi = Math.max(hi, v);
        lo = Math.min(lo, v);
      }
    }
    if (stack) {
      hi = Math.max(hi, pos);
      lo = Math.min(lo, neg);
    }
  }
  return { lo, hi: hi === lo ? lo + 1 : hi };
});

const valueScale = computed(() => {
  const range = horizontal.value ? [0, plotW.value] : [plotH.value, 0];
  return scaleLinear().domain([extents.value.lo, extents.value.hi]).nice(5).range(range);
});

const ticks = computed(() => valueScale.value.ticks(5));

const labelWidth = (s: string) => Math.min(160, s.length * 6.6 + 8);
const margin = computed(() => {
  if (horizontal.value) {
    const longest = Math.max(40, ...props.model.categories.map((c) => labelWidth(name(c.key, c.label))));
    return { top: 8, right: 56, bottom: 24, left: longest + 8 };
  }
  const longest = Math.max(...ticks.value.map((v) => labelWidth(tick(v))), 32);
  return { top: 12, right: 20, bottom: 28, left: longest + 4 };
});
const plotW = computed(() => Math.max(60, width.value - margin.value.left - margin.value.right));
const plotH = computed(() => {
  if (horizontal.value) return Math.max(60, props.model.categories.length * 30);
  return H.value - 12 - 28;
});
const svgH = computed(() => plotH.value + margin.value.top + margin.value.bottom);

const band = computed(() => scaleBand<string>()
  .domain(props.model.categories.map((c) => c.key))
  .range(horizontal.value ? [0, plotH.value] : [0, plotW.value])
  .paddingInner(0.3)
  .paddingOuter(0.15));

const point = computed(() => scalePoint<string>()
  .domain(props.model.categories.map((c) => c.key))
  .range([0, plotW.value])
  .padding(0.5));

/** Category labels along the x axis: every one when they fit, every nth when not. */
const xLabels = computed(() => {
  const cats = props.model.categories;
  if (!cats.length || horizontal.value) return [];
  const longest = Math.max(...cats.map((c) => labelWidth(name(c.key, c.label))));
  const step = Math.max(1, Math.ceil((cats.length * longest) / plotW.value));
  const scale = kind.value === 'line' || kind.value === 'area' ? (k: string) => point.value(k)! : (k: string) => band.value(k)! + band.value.bandwidth() / 2;
  return cats
    .map((c, i) => ({ i, x: scale(c.key), text: name(c.key, c.label) }))
    .filter(({ i }) => i % step === 0 || i === cats.length - 1)
    .filter((l, k, all) => k === all.length - 1 || all[k + 1].x - l.x >= longest * 0.9 || step === 1);
});

/** A bar with a 4px rounded data end and a square base. */
function barPath(x: number, y: number, w: number, h: number, end: 'top' | 'bottom' | 'right' | 'left'): string {
  const r = Math.min(4, w / 2, Math.abs(h));
  if (h <= 0 || w <= 0) return '';
  switch (end) {
    case 'top':
      return `M${x},${y + h}V${y + r}Q${x},${y} ${x + r},${y}H${x + w - r}Q${x + w},${y} ${x + w},${y + r}V${y + h}Z`;
    case 'bottom':
      return `M${x},${y}V${y + h - r}Q${x},${y + h} ${x + r},${y + h}H${x + w - r}Q${x + w},${y + h} ${x + w},${y + h - r}V${y}Z`;
    case 'right':
      return `M${x},${y}H${x + w - r}Q${x + w},${y} ${x + w},${y + r}V${y + h - r}Q${x + w},${y + h} ${x + w - r},${y + h}H${x}Z`;
    default:
      return `M${x + w},${y}H${x + r}Q${x},${y} ${x},${y + r}V${y + h - r}Q${x},${y + h} ${x + r},${y + h}H${x + w}Z`;
  }
}

interface Bar { key: string; d: string; fill: string; c: number; s: number; tip?: { x: number; y: number; text: string; anchor: 'start' | 'middle' } }

const bars = computed<Bar[]>(() => {
  if (kind.value !== 'bar' && kind.value !== 'hbar') return [];
  const { values, series, stack, categories } = props.model;
  const v = valueScale.value;
  const b = band.value;
  const GAP = 2;
  const out: Bar[] = [];
  const slots = stack ? 1 : series.length;
  const thick = Math.min(24, (b.bandwidth() - GAP * (slots - 1)) / slots);
  const labelled = series.length === 1 && categories.length <= 16;
  categories.forEach((cat, c) => {
    const start = b(cat.key)! + (b.bandwidth() - (thick * slots + GAP * (slots - 1))) / 2;
    let pos = 0;
    let neg = 0;
    const tops: number[] = [];
    values.forEach((vs, s) => {
      const value = vs[c];
      if (value === null || value === 0) return;
      let from: number;
      let to: number;
      if (stack) {
        from = value >= 0 ? pos : neg;
        to = from + value;
        if (value >= 0) pos = to; else neg = to;
      } else {
        from = 0;
        to = value;
      }
      tops.push(s);
      const offset = stack ? 0 : s * (thick + GAP);
      const a = v(from);
      const z = v(to);
      // Stacked segments are parted by a 2px gap in the surface colour.
      const gap = stack && from !== 0 ? GAP : 0;
      if (!horizontal.value) {
        const y = Math.min(a, z);
        const h = Math.abs(a - z) - gap;
        const isTop = !stack || s === values.length - 1 || values.slice(s + 1).every((w) => !w[c]);
        const d = isTop ? barPath(start + offset, value >= 0 ? y : y + gap, thick, h, value >= 0 ? 'top' : 'bottom')
          : `M${start + offset},${value >= 0 ? y : y + gap}h${thick}v${Math.max(0, h)}h${-thick}Z`;
        out.push({
          key: `${c}:${s}`, d, fill: color(series[s].slot), c, s,
          tip: labelled ? { x: start + offset + thick / 2, y: value >= 0 ? y - 6 : y + h + 14, text: tick(value), anchor: 'middle' } : undefined,
        });
      } else {
        const x = Math.min(a, z);
        const w = Math.abs(a - z) - gap;
        const isTop = !stack || s === values.length - 1 || values.slice(s + 1).every((ww) => !ww[c]);
        const d = isTop ? barPath(value >= 0 ? x + gap : x, start + offset, w, thick, value >= 0 ? 'right' : 'left')
          : `M${value >= 0 ? x + gap : x},${start + offset}h${Math.max(0, w)}v${thick}h${-Math.max(0, w)}Z`;
        out.push({
          key: `${c}:${s}`, d, fill: color(series[s].slot), c, s,
          tip: labelled ? { x: value >= 0 ? x + w + 6 : x - 6, y: start + offset + thick / 2 + 4, text: tick(value), anchor: 'start' } : undefined,
        });
      }
    });
  });
  return out;
});

/** Lines and areas: one path per series; stacked areas sit on the ones before. */
const paths = computed(() => {
  if (kind.value !== 'line' && kind.value !== 'area') return [];
  const { values, series, categories, stack } = props.model;
  const v = valueScale.value;
  const x = (c: number) => point.value(categories[c].key)!;
  const base = categories.map(() => 0);
  return values.map((vs, s) => {
    const lows = [...base];
    const highs = vs.map((value, c) => (stack ? base[c] + (value ?? 0) : value));
    if (stack) highs.forEach((h, c) => { base[c] = h ?? base[c]; });
    const pts = highs.map((h, c) => ({ c, h }));
    const line = d3line<{ c: number; h: number | null }>()
      .defined((p) => p.h !== null)
      .x((p) => x(p.c))
      .y((p) => v(p.h!))
      .curve(curveMonotoneX);
    const fill = kind.value === 'area'
      ? d3area<{ c: number; h: number | null }>()
        .defined((p) => p.h !== null)
        .x((p) => x(p.c))
        .y0((p) => v(stack ? lows[p.c] : 0))
        .y1((p) => v(p.h!))
        .curve(curveMonotoneX)(pts)
      : null;
    const last = [...pts].reverse().find((p) => p.h !== null);
    return {
      key: series[s].key,
      color: color(series[s].slot),
      line: line(pts) ?? '',
      fill,
      dots: categories.length <= 24 ? pts.filter((p) => p.h !== null).map((p) => ({ x: x(p.c), y: v(p.h!) })) : [],
      end: last && series.length === 1 ? { x: x(last.c), y: v(last.h!), text: tick(vs[last.c] ?? 0) } : null,
    };
  });
});

const donut = computed(() => {
  if (kind.value !== 'donut') return null;
  const size = Math.min(H.value, width.value * 0.55);
  const r = size / 2 - 4;
  const arcs = d3pie<number>().sort(null).padAngle(2 / r)(props.model.values[0]?.map((v) => v ?? 0) ?? []);
  const gen = d3arc<{ startAngle: number; endAngle: number; padAngle: number }>().innerRadius(r * 0.62).outerRadius(r).cornerRadius(2);
  return {
    size,
    slices: arcs.map((a, i) => ({ d: gen(a) ?? '', fill: color(props.model.series[i].slot), i })),
  };
});

const scatterGeo = computed(() => {
  if (kind.value !== 'scatter') return null;
  const pts = props.model.points;
  const xs = pts.map((p) => p.x);
  const ys = pts.map((p) => p.y);
  const x = scaleLinear().domain([Math.min(0, min(xs) ?? 0), max(xs) ?? 0]).nice(5).range([0, plotW.value]);
  const y = scaleLinear().domain([Math.min(0, min(ys) ?? 0), max(ys) ?? 0]).nice(5).range([plotH.value, 0]);
  const placed = pts.map((p) => ({ ...p, px: x(p.x), py: y(p.y) }));
  return { x, y, placed, finder: Delaunay.from(placed, (p) => p.px, (p) => p.py) };
});

// ─── Hover ──────────────────────────────────────────────────────

interface Tip { left: number; top: number; title: string; rows: { label: string; value: string; color: string }[] }
const tip = ref<Tip | null>(null);
const crosshair = ref<number | null>(null);

function tipFor(c: number, x: number, y: number): Tip {
  const cat = props.model.categories[c];
  return {
    left: x + margin.value.left,
    top: y + margin.value.top,
    title: name(cat.key, cat.label),
    rows: props.model.series.map((s, i) => ({
      label: props.model.series.length > 1 ? name(s.key, s.label) : '',
      value: fmt(props.model.values[i][c]),
      color: color(s.slot),
    })),
  };
}

function onBandMove(e: PointerEvent) {
  const svg = (e.currentTarget as SVGElement).getBoundingClientRect();
  const px = e.clientX - svg.left - margin.value.left;
  const py = e.clientY - svg.top - margin.value.top;
  const cats = props.model.categories;
  if (!cats.length) return;
  if (kind.value === 'line' || kind.value === 'area') {
    const step = cats.length > 1 ? point.value.step() : plotW.value;
    const c = Math.max(0, Math.min(cats.length - 1, Math.round((px - point.value(cats[0].key)!) / step)));
    crosshair.value = point.value(cats[c].key)!;
    tip.value = tipFor(c, crosshair.value, Math.max(0, py));
    return;
  }
  const along = horizontal.value ? py : px;
  const c = cats.findIndex((cat) => {
    const start = band.value(cat.key)! - (band.value.step() - band.value.bandwidth()) / 2;
    return along >= start && along < start + band.value.step();
  });
  if (c === -1) {
    tip.value = null;
    return;
  }
  const mid = band.value(cats[c].key)! + band.value.bandwidth() / 2;
  tip.value = horizontal.value ? tipFor(c, Math.max(0, px), mid) : tipFor(c, mid, Math.max(0, py));
  hovered.value = c;
}

const hovered = ref<number | null>(null);

function onScatterMove(e: PointerEvent) {
  const geo = scatterGeo.value;
  if (!geo) return;
  const svg = (e.currentTarget as SVGElement).getBoundingClientRect();
  const px = e.clientX - svg.left - margin.value.left;
  const py = e.clientY - svg.top - margin.value.top;
  const i = geo.finder.find(px, py);
  const p = geo.placed[i];
  if (!p || Math.hypot(p.px - px, p.py - py) > 24) {
    tip.value = null;
    return;
  }
  const s = props.model.series[p.series];
  tip.value = {
    left: p.px + margin.value.left,
    top: p.py + margin.value.top,
    title: props.model.series.length > 1 ? name(s.key, s.label) : '',
    rows: [
      { label: 'x', value: fmt(p.x, props.model.xFormat), color: color(s.slot) },
      { label: 'y', value: fmt(p.y), color: color(s.slot) },
    ],
  };
  hovered.value = i;
}

function onSliceMove(i: number, e: PointerEvent) {
  const box = host.value!.getBoundingClientRect();
  const s = props.model.series[i];
  const v = props.model.values[0][i];
  tip.value = {
    left: e.clientX - box.left,
    top: e.clientY - box.top - 8,
    title: name(s.key, s.label),
    rows: [{ label: percentOf(v), value: fmt(v), color: color(s.slot) }],
  };
  hovered.value = i;
}

function leave() {
  tip.value = null;
  crosshair.value = null;
  hovered.value = null;
}

// A new chart is a new picture: nothing hovered in the old one carries over.
watch(() => props.model, leave);

function pick(c: number | null) {
  if (c === null) return;
  const key = props.model.categories[c]?.key;
  if (key !== undefined && key !== OTHER) emit('pick', key);
}

const percentOf = (v: number | null) =>
  props.model.total ? formatNumber((v ?? 0) / props.model.total, props.locale, 'percent') : '';

const legend = computed(() => {
  if (kind.value === 'donut') return [];
  return multi.value ? props.model.series.map((s) => ({ label: name(s.key, s.label), color: color(s.slot) })) : [];
});

const deltaRatio = computed(() => {
  const d = props.model.delta;
  if (!d || d.current === null || !d.previous) return null;
  return (d.current - d.previous) / Math.abs(d.previous);
});

const problemText = computed(() => (props.model.problem ? t(`rich_table.chart.problems.${props.model.problem}`) : ''));
</script>

<template>
  <div ref="host" class="rt-chart" @pointerleave="leave">
    <div v-if="problemText" class="rt-chart-empty">{{ problemText }}</div>

    <!-- A number, as a hero figure. -->
    <div v-else-if="kind === 'number'" class="rt-chart-figure">
      <div class="rt-chart-hero">{{ fmt(model.total) }}</div>
      <div v-if="model.delta" class="rt-chart-delta">
        <component :is="(deltaRatio ?? 0) >= 0 ? ArrowUp : ArrowDown" :size="14" />
        <span>{{ fmt(model.delta.current) }} · {{ model.delta.label }}</span>
        <span class="rt-muted">
          {{ deltaRatio === null ? '' : formatNumber(deltaRatio, locale, 'percent') }}
          {{ t('rich_table.chart.versus', { period: model.delta.previousLabel }) }}
        </span>
      </div>
    </div>

    <template v-else>
      <div class="rt-chart-top">
        <div v-if="legend.length" class="rt-legend" role="list">
          <span v-for="item in legend" :key="item.label" class="rt-legend-item" role="listitem">
            <span :class="kind === 'line' ? 'rt-key-line' : 'rt-key-box'" :style="{ background: item.color }" />{{ item.label }}
          </span>
        </div>
        <span v-if="model.folded" class="rt-muted rt-chart-note">{{ t('rich_table.chart.folded', model.folded) }}</span>
        <span class="rt-bar-spacer" />
        <span v-if="exportError" class="rt-chart-note rt-error-text" role="alert">{{ exportError }}</span>
        <span class="rt-chart-actions">
          <button type="button" class="rt-bar-btn" :disabled="exporting" :title="t('rich_table.chart.export_png')" @click="exportPng">
            <ImageDown :size="13" /><span>PNG</span>
          </button>
          <button type="button" class="rt-bar-btn" :class="{ 'is-on': showTable }" :title="t('rich_table.chart.table')" @click="showTable = !showTable">
            <Table2 :size="13" /><span>{{ t('rich_table.chart.table') }}</span>
          </button>
        </span>
      </div>

      <!-- Donut: the slices, and beside them their names and numbers. -->
      <div v-if="donut" class="rt-donut">
        <svg :width="donut.size" :height="donut.size" role="img" :aria-label="t('rich_table.chart.aria')">
          <g :transform="`translate(${donut.size / 2},${donut.size / 2})`">
            <path
              v-for="slice in donut.slices"
              :key="slice.i"
              :d="slice.d"
              :fill="slice.fill"
              class="rt-mark"
              :class="{ 'is-dim': hovered !== null && hovered !== slice.i }"
              @pointermove="onSliceMove(slice.i, $event)"
              @click="pick(slice.i)"
            />
            <text class="rt-chart-center" text-anchor="middle" dy="0.35em">{{ tick(model.total ?? 0) }}</text>
          </g>
        </svg>
        <ul class="rt-donut-legend">
          <li v-for="(s, i) in model.series" :key="s.key">
            <span class="rt-key-box" :style="{ background: color(s.slot) }" />
            <span class="rt-donut-name">{{ name(s.key, s.label) }}</span>
            <b>{{ fmt(model.values[0][i]) }}</b>
            <span class="rt-muted">{{ percentOf(model.values[0][i]) }}</span>
          </li>
        </ul>
      </div>

      <svg
        v-else
        :width="width"
        :height="svgH"
        class="rt-chart-svg"
        role="img"
        :aria-label="t('rich_table.chart.aria')"
        @pointermove="kind === 'scatter' ? onScatterMove($event) : onBandMove($event)"
        @click="pick(hovered)"
      >
        <g :transform="`translate(${margin.left},${margin.top})`">
          <!-- Value grid and its ticks: hairlines, solid, recessive. -->
          <template v-if="kind !== 'scatter'">
            <g v-for="v in ticks" :key="v">
              <line
                v-if="!horizontal"
                :x1="0" :x2="plotW" :y1="valueScale(v)" :y2="valueScale(v)"
                class="rt-grid-line" :class="{ 'is-zero': v === 0 }"
              />
              <line
                v-else
                :y1="0" :y2="plotH" :x1="valueScale(v)" :x2="valueScale(v)"
                class="rt-grid-line" :class="{ 'is-zero': v === 0 }"
              />
              <text
                v-if="!horizontal"
                :x="-8" :y="valueScale(v)" dy="0.32em" text-anchor="end" class="rt-axis-text"
              >{{ tick(v) }}</text>
              <text v-else :x="valueScale(v)" :y="plotH + 16" text-anchor="middle" class="rt-axis-text">{{ tick(v) }}</text>
            </g>
          </template>

          <!-- Scatter axes. -->
          <template v-if="scatterGeo">
            <g v-for="v in scatterGeo.y.ticks(5)" :key="`y${v}`">
              <line :x1="0" :x2="plotW" :y1="scatterGeo.y(v)" :y2="scatterGeo.y(v)" class="rt-grid-line" />
              <text :x="-8" :y="scatterGeo.y(v)" dy="0.32em" text-anchor="end" class="rt-axis-text">{{ tick(v) }}</text>
            </g>
            <text
              v-for="v in scatterGeo.x.ticks(5)"
              :key="`x${v}`"
              :x="scatterGeo.x(v)" :y="plotH + 18" text-anchor="middle" class="rt-axis-text"
            >{{ formatNumber(v, locale, model.xFormat) }}</text>
            <circle
              v-for="(p, i) in scatterGeo.placed"
              :key="i"
              :cx="p.px" :cy="p.py" r="4"
              :fill="color(model.series[p.series].slot)"
              class="rt-dot" :class="{ 'is-hover': hovered === i }"
            />
          </template>

          <!-- Category labels. -->
          <text
            v-for="l in xLabels"
            :key="l.i"
            :x="l.x" :y="plotH + 18" text-anchor="middle" class="rt-axis-text"
          >{{ l.text }}</text>
          <template v-if="horizontal">
            <text
              v-for="cat in model.categories"
              :key="cat.key"
              :x="-8" :y="band(cat.key)! + band.bandwidth() / 2" dy="0.32em" text-anchor="end" class="rt-axis-text"
            >{{ name(cat.key, cat.label).length > 22 ? `${name(cat.key, cat.label).slice(0, 21)}…` : name(cat.key, cat.label) }}</text>
          </template>

          <!-- Bars. -->
          <path
            v-for="b in bars"
            :key="b.key"
            :d="b.d"
            :fill="b.fill"
            class="rt-mark"
            :class="{ 'is-dim': hovered !== null && hovered !== b.c }"
          />
          <text
            v-for="b in bars.filter((x) => x.tip)"
            :key="`t${b.key}`"
            :x="b.tip!.x" :y="b.tip!.y" :text-anchor="b.tip!.anchor" class="rt-value-text"
          >{{ b.tip!.text }}</text>

          <!-- Lines and areas. -->
          <g v-for="p in paths" :key="p.key">
            <path v-if="p.fill" :d="p.fill" :fill="p.color" class="rt-area" />
            <path :d="p.line" :stroke="p.color" class="rt-line" />
            <circle v-for="(d, i) in p.dots" :key="i" :cx="d.x" :cy="d.y" r="4" :fill="p.color" class="rt-dot" />
            <text v-if="p.end" :x="p.end.x + 8" :y="p.end.y" dy="0.32em" class="rt-value-text">{{ p.end.text }}</text>
          </g>
          <line v-if="crosshair !== null" :x1="crosshair" :x2="crosshair" :y1="0" :y2="plotH" class="rt-crosshair" />

          <!-- The whole plot answers the pointer: no pinpoint targets. -->
          <rect :width="plotW" :height="plotH" fill="transparent" />
        </g>
      </svg>

      <div v-if="tip" class="rt-tip" :style="{ left: `${tip.left}px`, top: `${tip.top}px` }" role="status">
        <div v-if="tip.title" class="rt-tip-title">{{ tip.title }}</div>
        <div v-for="(row, i) in tip.rows" :key="i" class="rt-tip-row">
          <span class="rt-key-line" :style="{ background: row.color }" />
          <b>{{ row.value }}</b>
          <span v-if="row.label" class="rt-muted">{{ row.label }}</span>
        </div>
      </div>

      <!-- The same numbers, as a table: the chart never gates a value. -->
      <table v-if="showTable" class="rt-chart-table">
        <thead>
          <tr>
            <th />
            <th v-for="s in (kind === 'donut' || kind === 'scatter' ? [] : model.series)" :key="s.key">{{ model.series.length > 1 ? name(s.key, s.label) : '' }}</th>
            <th v-if="kind === 'donut'" />
          </tr>
        </thead>
        <tbody v-if="kind === 'scatter'">
          <tr v-for="(p, i) in model.points" :key="i">
            <td>{{ name(model.series[p.series].key, model.series[p.series].label) }}</td>
            <td>{{ fmt(p.x, model.xFormat) }}</td>
            <td>{{ fmt(p.y) }}</td>
          </tr>
        </tbody>
        <tbody v-else-if="kind === 'donut'">
          <tr v-for="(s, i) in model.series" :key="s.key">
            <td>{{ name(s.key, s.label) }}</td>
            <td>{{ fmt(model.values[0][i]) }}</td>
          </tr>
        </tbody>
        <tbody v-else>
          <tr v-for="(cat, c) in model.categories" :key="cat.key">
            <td>{{ name(cat.key, cat.label) }}</td>
            <td v-for="(s, i) in model.series" :key="s.key">{{ fmt(model.values[i][c]) }}</td>
          </tr>
        </tbody>
      </table>
    </template>
  </div>
</template>
