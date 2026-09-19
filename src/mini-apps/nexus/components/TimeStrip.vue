<script setup lang="ts">
import { todayIso } from '../../../shared/localDay';
import { ref, computed, watch, onMounted, onUnmounted } from 'vue';
import { useI18n } from 'vue-i18n';
import { Play, Pause, X, EyeOff } from 'lucide-vue-next';
import type { TimeFrame } from '../timeFrame';

/**
 * The strip under the graph for looking back.
 *
 * One mark per month, from the first thing in the vault that has a date to
 * this month, with the height of each mark the number of things that
 * happened then. Dragging, the arrow keys (Shift for a year) or Play move
 * through it; the graph redraws what was already there by that day.
 *
 * The value is a day, not a month: the last day of the chosen month, and
 * never later than today, because this month has not finished happening.
 *
 * Sealed periods are drawn hatched and hold nothing. "Show for now" lifts
 * that for this look only; it is never written down, and leaving the strip
 * seals them again. See `src-tauri/src/timeline/seal.rs`.
 */
const props = defineProps<{
    frame: TimeFrame | null;
    /** The day being looked at, `YYYY-MM-DD`. */
    modelValue: string | null;
    /** Whether sealed periods are shown for this look. */
    revealed?: boolean;
}>();

const emit = defineEmits<{
    (e: 'update:modelValue', day: string): void;
    (e: 'update:revealed', revealed: boolean): void;
    (e: 'seal-period', from: string, to: string): void;
    (e: 'remove-seal', id: string): void;
    (e: 'close'): void;
}>();

const { locale } = useI18n();

const pad = (n: number) => String(n).padStart(2, '0');
/** Every month from the first dated thing to this one, oldest first. */
const months = computed<string[]>(() => {
    const now = todayIso().slice(0, 7);
    const first = props.frame?.earliest && props.frame.earliest <= now ? props.frame.earliest : now;
    let [year, month] = first.split('-').map(Number);
    const [lastYear, lastMonth] = now.split('-').map(Number);
    const out: string[] = [];
    while (year < lastYear || (year === lastYear && month <= lastMonth)) {
        out.push(`${year}-${pad(month)}`);
        month += 1;
        if (month > 12) { month = 1; year += 1; }
    }
    return out;
});

const counts = computed(() => {
    // Size, not tally: a month holding one wedding stands taller than a month
    // of errands. Falls back to the tally for a frame from an older build.
    const byMonth = new Map((props.frame?.density ?? []).map(d => [d.month, d.weight ?? d.count]));
    return months.value.map(m => byMonth.get(m) ?? 0);
});
const peak = computed(() => counts.value.reduce((max, n) => Math.max(max, n), 1));

const index = ref(0);

const dayFor = (month: string) => {
    const [year, m] = month.split('-').map(Number);
    const last = `${month}-${pad(new Date(year, m, 0).getDate())}`;
    const today = todayIso();
    return last < today ? last : today;
};

const indexFor = (day: string | null) => {
    const i = day ? months.value.indexOf(day.slice(0, 7)) : -1;
    return i >= 0 ? i : months.value.length - 1;
};

watch(() => [props.modelValue, months.value.length] as const, () => {
    index.value = indexFor(props.modelValue);
}, { immediate: true });

const setIndex = (i: number) => {
    const next = Math.max(0, Math.min(months.value.length - 1, i));
    index.value = next;
    const day = dayFor(months.value[next]);
    if (day !== props.modelValue) emit('update:modelValue', day);
};

onMounted(() => {
    if (!props.modelValue) setIndex(months.value.length - 1);
});

const label = computed(() => {
    const [year, month] = months.value[index.value].split('-').map(Number);
    return new Intl.DateTimeFormat(locale.value, { month: 'long', year: 'numeric' })
        .format(new Date(year, month - 1, 1));
});

const pct = (i: number) =>
    months.value.length <= 1 ? '100%' : `${(i / (months.value.length - 1)) * 100}%`;

/** A year label at each January, thinned to about ten so they never collide. */
const ticks = computed(() => {
    const januaries = months.value
        .map((month, i) => ({ month, i }))
        .filter(({ month }) => month.endsWith('-01'));
    if (januaries.length === 0) {
        return [{ year: months.value[0].slice(0, 4), left: '0%' }];
    }
    const step = Math.max(1, Math.ceil(januaries.length / 10));
    return januaries
        .filter((_, k) => k % step === 0)
        .map(({ month, i }) => ({ year: month.slice(0, 4), left: pct(i) }));
});

// ── Sealed periods ──────────────────────────────────────────
const sealed = computed(() => props.frame?.sealed ?? []);

/** Where each sealed period sits over the marks, in the marks' own coordinates. */
const sealBands = computed(() => {
    const n = months.value.length;
    const first = months.value[0];
    const last = months.value[n - 1];
    return sealed.value.flatMap(period => {
        const from = period.from.slice(0, 7);
        const to = period.to.slice(0, 7);
        if (to < first || from > last) return [];
        const start = from < first ? 0 : months.value.indexOf(from);
        const end = to > last ? n - 1 : months.value.indexOf(to);
        return [{
            id: period.id,
            left: `${(start / n) * 100}%`,
            width: `${((end - start + 1) / n) * 100}%`,
        }];
    });
});

const sealPanelOpen = ref(false);
const sealFrom = ref('');
const sealTo = ref('');

/** A period is sealed by the month or the year; a single day is not a period. */
const PERIOD_END = /^\d{4}(-(0[1-9]|1[0-2]))?$/;
const sealValid = computed(() => {
    const from = sealFrom.value.trim();
    const to = sealTo.value.trim();
    if (!PERIOD_END.test(from) || !PERIOD_END.test(to)) return false;
    const n = Math.min(from.length, to.length);
    return to.slice(0, n) >= from.slice(0, n);
});

const submitSeal = () => {
    if (!sealValid.value) return;
    emit('seal-period', sealFrom.value.trim(), sealTo.value.trim());
    sealFrom.value = '';
    sealTo.value = '';
};

// ── Moving through it ───────────────────────────────────────
const track = ref<HTMLElement | null>(null);
let dragging = false;

const indexAt = (clientX: number) => {
    const rect = track.value?.getBoundingClientRect();
    if (!rect || rect.width === 0) return index.value;
    const x = Math.min(Math.max(clientX - rect.left, 0), rect.width);
    return Math.round((x / rect.width) * (months.value.length - 1));
};

const playing = ref(false);
let timer: ReturnType<typeof setInterval> | undefined;

const stop = () => {
    playing.value = false;
    clearInterval(timer);
    timer = undefined;
};

const play = () => {
    if (playing.value) { stop(); return; }
    if (index.value >= months.value.length - 1) setIndex(0);
    playing.value = true;
    // A month every quarter of a second: a decade in thirty seconds, slow
    // enough to watch people arrive.
    timer = setInterval(() => {
        if (index.value >= months.value.length - 1) { stop(); return; }
        setIndex(index.value + 1);
    }, 250);
};

onUnmounted(stop);

const onPointerDown = (e: PointerEvent) => {
    stop();
    dragging = true;
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
    setIndex(indexAt(e.clientX));
};
const onPointerMove = (e: PointerEvent) => {
    if (dragging) setIndex(indexAt(e.clientX));
};
const onPointerUp = () => { dragging = false; };

const onKey = (e: KeyboardEvent) => {
    const step = e.shiftKey ? 12 : 1;
    if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') setIndex(index.value - step);
    else if (e.key === 'ArrowRight' || e.key === 'ArrowUp') setIndex(index.value + step);
    else if (e.key === 'Home') setIndex(0);
    else if (e.key === 'End') setIndex(months.value.length - 1);
    else return;
    e.preventDefault();
    stop();
};

const close = () => {
    stop();
    sealPanelOpen.value = false;
    emit('close');
};
</script>

<template>
    <div
        class="pointer-events-auto mx-4 mb-3 flex flex-wrap items-center gap-x-4 gap-y-2 rounded-2xl border border-gray-200 bg-white/85 px-4 py-3 shadow-lg backdrop-blur-md dark:border-[#3a3a3c] dark:bg-[#242426]/85"
        @click.stop
    >
        <template v-if="frame && frame.earliest">
            <button
                type="button"
                class="flex h-9 w-9 flex-shrink-0 items-center justify-center rounded-full bg-indigo-600 text-white transition-colors hover:bg-indigo-700"
                :aria-label="playing ? $t('nexus.pause') : $t('nexus.play')"
                :title="playing ? $t('nexus.pause') : $t('nexus.play')"
                @click="play"
            >
                <Pause v-if="playing" class="h-4 w-4" />
                <Play v-else class="h-4 w-4 translate-x-px" />
            </button>

            <div class="w-36 flex-shrink-0">
                <div class="text-[10px] font-semibold uppercase tracking-wider text-gray-400">{{ $t('nexus.viewing') }}</div>
                <div class="truncate text-base font-semibold tabular-nums text-gray-900 dark:text-gray-100">{{ label }}</div>
            </div>

            <div
                ref="track"
                class="relative h-12 flex-1 cursor-ew-resize touch-none select-none rounded-md outline-none focus-visible:ring-2 focus-visible:ring-indigo-500"
                role="slider"
                tabindex="0"
                :aria-label="$t('nexus.slider')"
                :aria-valuemin="0"
                :aria-valuemax="months.length - 1"
                :aria-valuenow="index"
                :aria-valuetext="label"
                @pointerdown="onPointerDown"
                @pointermove="onPointerMove"
                @pointerup="onPointerUp"
                @pointercancel="onPointerUp"
                @keydown="onKey"
            >
                <svg
                    class="absolute inset-x-0 top-0 h-8 w-full"
                    :viewBox="`0 0 ${months.length} 1`"
                    preserveAspectRatio="none"
                    aria-hidden="true"
                >
                    <rect
                        v-for="(count, i) in counts"
                        :key="months[i]"
                        :x="i + 0.15"
                        width="0.7"
                        :y="1 - count / peak"
                        :height="count / peak"
                        :class="i <= index ? 'fill-indigo-500/70' : 'fill-gray-300 dark:fill-gray-600'"
                    />
                </svg>
                <div
                    v-for="band in sealBands"
                    :key="band.id"
                    data-sealed-band
                    class="pointer-events-none absolute top-0 h-8 rounded-sm border border-stone-400/70 dark:border-stone-500/70"
                    :class="revealed ? 'opacity-40' : ''"
                    :style="{
                        left: band.left,
                        width: band.width,
                        background: 'repeating-linear-gradient(135deg, rgba(120, 113, 108, 0.35) 0 2px, transparent 2px 6px)',
                    }"
                ></div>
                <div class="absolute inset-x-0 top-8 h-px bg-gray-200 dark:bg-[#3a3a3c]"></div>
                <span
                    v-for="tick in ticks"
                    :key="tick.year"
                    class="absolute top-9 -translate-x-1/2 text-[10px] tabular-nums text-gray-400"
                    :style="{ left: tick.left }"
                >{{ tick.year }}</span>
                <div
                    class="pointer-events-none absolute top-0 h-8 w-0.5 -translate-x-1/2 bg-indigo-600 dark:bg-indigo-400"
                    :style="{ left: pct(index) }"
                >
                    <div class="absolute -bottom-1.5 left-1/2 h-3 w-3 -translate-x-1/2 rounded-full border-2 border-indigo-600 bg-white dark:border-indigo-400 dark:bg-[#242426]"></div>
                </div>
            </div>
        </template>
        <p v-else class="flex-1 text-sm text-gray-500 dark:text-gray-400">{{ $t('nexus.no_dates') }}</p>

        <!-- What the strip carries beside sealing: the panels that read this
             span. There are eight of them now, so they get a row of their own
             below the scrubber rather than squeezing it — the scrubber is the
             control people came for, and it needs the width. -->
        <div
            v-if="$slots.actions"
            data-actions
            class="flex w-full flex-wrap items-center justify-end gap-x-1 gap-y-2 border-t border-gray-100 pt-2 dark:border-[#3a3a3c]"
        >
            <slot name="actions" />
        </div>

        <div v-if="frame" class="relative flex-shrink-0">
            <button
                type="button"
                class="flex h-9 items-center gap-1.5 rounded-full px-3 text-xs font-semibold text-gray-600 transition-colors hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-[#3a3a3c]"
                :aria-expanded="sealPanelOpen"
                @click="sealPanelOpen = !sealPanelOpen"
            >
                <EyeOff class="h-3.5 w-3.5" /> {{ $t('nexus.seal_period') }}
            </button>

            <div
                v-if="sealPanelOpen"
                class="absolute bottom-full right-0 mb-3 w-80 space-y-3 rounded-xl border border-gray-200 bg-white p-4 shadow-xl dark:border-[#3a3a3c] dark:bg-[#242426]"
            >
                <p class="text-xs font-semibold text-gray-900 dark:text-gray-100">{{ $t('nexus.seals_title') }}</p>
                <p class="text-[11px] leading-relaxed text-gray-500 dark:text-gray-400">{{ $t('nexus.seal_explain') }}</p>

                <ul v-if="sealed.length" class="space-y-1">
                    <li
                        v-for="period in sealed"
                        :key="period.id"
                        class="flex items-center justify-between rounded-lg bg-gray-50 px-2.5 py-1.5 text-xs tabular-nums text-gray-700 dark:bg-[#1e1e20] dark:text-gray-300"
                    >
                        <span>{{ period.from_text }} – {{ period.to_text }}</span>
                        <button
                            type="button"
                            class="text-gray-400 transition-colors hover:text-red-500"
                            :aria-label="$t('nexus.remove_seal')"
                            :title="$t('nexus.remove_seal')"
                            @click="emit('remove-seal', period.id)"
                        >
                            <X class="h-3.5 w-3.5" />
                        </button>
                    </li>
                </ul>

                <div class="grid grid-cols-[1fr_1fr_auto] items-end gap-2">
                    <label class="block">
                        <span class="mb-1 block text-[10px] font-semibold uppercase tracking-wider text-gray-400">{{ $t('nexus.from') }}</span>
                        <input
                            v-model="sealFrom"
                            type="text"
                            inputmode="numeric"
                            :placeholder="$t('nexus.period_hint')"
                            class="w-full rounded-lg border border-gray-200 bg-gray-50 px-2 py-1.5 text-xs tabular-nums outline-none focus:ring-2 focus:ring-indigo-500 dark:border-[#3a3a3c] dark:bg-[#1e1e20]"
                        />
                    </label>
                    <label class="block">
                        <span class="mb-1 block text-[10px] font-semibold uppercase tracking-wider text-gray-400">{{ $t('nexus.to') }}</span>
                        <input
                            v-model="sealTo"
                            type="text"
                            inputmode="numeric"
                            :placeholder="$t('nexus.period_hint')"
                            class="w-full rounded-lg border border-gray-200 bg-gray-50 px-2 py-1.5 text-xs tabular-nums outline-none focus:ring-2 focus:ring-indigo-500 dark:border-[#3a3a3c] dark:bg-[#1e1e20]"
                        />
                    </label>
                    <button
                        type="button"
                        class="h-8 rounded-lg bg-stone-700 px-3 text-xs font-semibold text-white transition-opacity disabled:opacity-40 dark:bg-stone-300 dark:text-stone-900"
                        :disabled="!sealValid"
                        @click="submitSeal"
                    >
                        {{ $t('nexus.seal') }}
                    </button>
                </div>

                <button
                    v-if="sealed.length"
                    type="button"
                    class="w-full rounded-lg border border-gray-200 py-1.5 text-xs font-medium text-gray-600 transition-colors hover:bg-gray-50 dark:border-[#3a3a3c] dark:text-gray-300 dark:hover:bg-[#1e1e20]"
                    @click="emit('update:revealed', !revealed)"
                >
                    {{ revealed ? $t('nexus.hide_again') : $t('nexus.reveal') }}
                </button>
            </div>
        </div>

        <button
            type="button"
            class="flex h-9 flex-shrink-0 items-center gap-1.5 rounded-full px-3 text-xs font-semibold text-gray-600 transition-colors hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-[#3a3a3c]"
            @click="close"
        >
            <X class="h-3.5 w-3.5" /> {{ $t('nexus.back_to_now') }}
        </button>
    </div>
</template>
