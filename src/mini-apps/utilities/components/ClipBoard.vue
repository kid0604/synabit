<script setup lang="ts">
/**
 * One video and a stretch of it: the part to keep (cut), the part to make a
 * GIF of, or the moment to take a picture at (frames).
 *
 * The WebView plays MP4 and QuickTime itself, so those are watched as they
 * are. Anything else (MKV, AVI, WMV…) cannot play here; for those the frame at
 * the playhead is made by ffmpeg in the sealed room instead, a moment after
 * the playhead stops moving.
 *
 * The bar under the picture holds two handles for a stretch, one for a moment.
 * Each is a slider to the keyboard: arrows move a tenth of a second, with
 * Shift a second; Home and End go to the ends.
 */
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { Pause, Play } from 'lucide-vue-next';
import { fileUrl } from '../render';
import { videoPicture } from '../api';
import { clock, parseClock, LONGEST_GIF } from '../tools';
import { useWorkbench } from '../useWorkbench';

const { t } = useI18n();
const wb = useWorkbench();

const item = computed(() => wb.usable.value.find((i) => i.kind === 'video' && i.probe));
const duration = computed(() => item.value?.probe?.duration ?? 0);

/** Which numbers the bar moves: a stretch for cut and GIF, a moment for a frame. */
const range = computed<{ start: number; end: number } | null>(() => {
  if (wb.tool.value === 'vidtrim') return wb.options.vidtrim;
  if (wb.tool.value === 'vidgif') return wb.options.vidgif;
  return null;
});
const moment = computed(() => wb.tool.value === 'vidframes' && wb.options.vidframes.mode === 'at');

// ── The picture ──

const player = ref<HTMLVideoElement | null>(null);
/** False once the WebView says it cannot play this file. */
const plays = ref(true);
const playing = ref(false);
const position = ref(0);

watch(item, () => {
  plays.value = true;
  playing.value = false;
  position.value = moment.value ? wb.options.vidframes.at : range.value?.start ?? 0;
});

function onTime() {
  const p = player.value;
  if (!p) return;
  position.value = p.currentTime;
  // Watching a stretch stops at its end.
  if (range.value && playing.value && p.currentTime >= range.value.end) {
    p.pause();
    p.currentTime = range.value.end;
  }
}

function seek(to: number) {
  const at = Math.min(Math.max(0, to), duration.value);
  position.value = at;
  if (player.value && plays.value) player.value.currentTime = at;
}

function toggle() {
  const p = player.value;
  if (!p || !plays.value) return;
  if (p.paused) {
    // From the start of the stretch when the playhead is outside it.
    if (range.value && (p.currentTime < range.value.start || p.currentTime >= range.value.end - 0.05)) p.currentTime = range.value.start;
    void p.play();
  } else {
    p.pause();
  }
}

/** For a file that does not play: the frame at the playhead, from the room. */
const still = ref<string | null>(null);
let stillTimer: ReturnType<typeof setTimeout> | undefined;
let stillSerial = 0;
watch(
  [plays, position, item],
  ([canPlay, at, it]) => {
    clearTimeout(stillTimer);
    if (canPlay || !it) return;
    const mine = ++stillSerial;
    stillTimer = setTimeout(async () => {
      try {
        const url = URL.createObjectURL(await videoPicture(it.input.id, at, 960));
        if (mine !== stillSerial) return URL.revokeObjectURL(url);
        if (still.value) URL.revokeObjectURL(still.value);
        still.value = url;
      } catch {
        // The last frame stays; the run itself will say if the file is bad.
      }
    }, 250);
  },
  { immediate: true },
);
onBeforeUnmount(() => {
  clearTimeout(stillTimer);
  stillSerial++;
  if (still.value) URL.revokeObjectURL(still.value);
});

// ── The bar ──

const bar = ref<HTMLElement | null>(null);
const share = (x: number) => (duration.value ? `${(Math.min(Math.max(x, 0), duration.value) / duration.value) * 100}%` : '0%');

type Handle = 'start' | 'end' | 'at';
let dragging: Handle | 'seek' | null = null;

function timeAt(e: PointerEvent): number {
  const r = bar.value!.getBoundingClientRect();
  return Math.min(Math.max(0, (e.clientX - r.left) / r.width), 1) * duration.value;
}

/** Moves a handle, keeping a stretch at least a tenth of a second long, and a GIF no longer than allowed. */
function move(handle: Handle, to: number) {
  const d = duration.value;
  const at = Math.round(Math.min(Math.max(0, to), d) * 10) / 10;
  if (handle === 'at') {
    wb.options.vidframes.at = at;
  } else if (range.value) {
    const r = range.value;
    const longest = wb.tool.value === 'vidgif' ? LONGEST_GIF : Infinity;
    if (handle === 'start') {
      r.start = Math.min(at, r.end - 0.1);
      if (r.end - r.start > longest) r.end = r.start + longest;
    } else {
      r.end = Math.max(at, r.start + 0.1);
      if (r.end - r.start > longest) r.start = r.end - longest;
    }
  }
  seek(handle === 'at' ? wb.options.vidframes.at : handle === 'start' ? range.value!.start : range.value!.end);
}

function onDown(e: PointerEvent, what: Handle | 'seek') {
  if (!bar.value || !duration.value) return;
  dragging = what;
  (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  if (what === 'seek') seek(timeAt(e));
  e.stopPropagation();
}
function onMove(e: PointerEvent) {
  if (!dragging || !bar.value) return;
  if (dragging === 'seek') seek(timeAt(e));
  else move(dragging, timeAt(e));
}
function onUp() {
  dragging = null;
}

function onKey(e: KeyboardEvent, handle: Handle) {
  const value = handle === 'at' ? wb.options.vidframes.at : handle === 'start' ? range.value!.start : range.value!.end;
  const step = e.shiftKey ? 1 : 0.1;
  const to = { ArrowLeft: value - step, ArrowDown: value - step, ArrowRight: value + step, ArrowUp: value + step, Home: 0, End: duration.value }[e.key];
  if (to === undefined) return;
  e.preventDefault();
  move(handle, to);
}

// ── Typed times ──

const typed = ref<Record<Handle, string>>({ start: '', end: '', at: '' });
watch(
  () => [range.value?.start, range.value?.end, wb.options.vidframes.at],
  () => {
    typed.value = {
      start: clock(range.value?.start ?? 0),
      end: clock(range.value?.end ?? 0),
      at: clock(wb.options.vidframes.at),
    };
  },
  { immediate: true, deep: true },
);
function typedIn(handle: Handle) {
  const seconds = parseClock(typed.value[handle]);
  if (Number.isFinite(seconds)) move(handle, seconds);
  else typed.value[handle] = clock(handle === 'at' ? wb.options.vidframes.at : handle === 'start' ? range.value!.start : range.value!.end);
}

const length = computed(() => (range.value ? range.value.end - range.value.start : 0));
</script>

<template>
  <div v-if="item" class="flex flex-col gap-3 max-w-[760px]">
    <div class="relative rounded-lg overflow-hidden bg-black grid place-items-center aspect-video">
      <video
        v-if="plays"
        ref="player"
        :src="fileUrl(item.input.path)"
        preload="auto"
        playsinline
        class="w-full h-full object-contain"
        @loadedmetadata="seek(position)"
        @timeupdate="onTime"
        @play="playing = true"
        @pause="playing = false"
        @error="plays = false"
        @click="toggle"
      />
      <img v-else-if="still" :src="still" alt="" class="w-full h-full object-contain" />
      <div v-else class="w-full h-full animate-pulse bg-surface-hover dark:bg-surface-hover-dark" aria-hidden="true" />
    </div>

    <div class="flex items-center gap-3">
      <button
        type="button"
        class="btn-icon"
        :disabled="!plays"
        :aria-label="playing ? t('utilities.clip.pause') : t('utilities.clip.play')"
        :title="plays ? undefined : t('utilities.clip.no_play')"
        @click="toggle"
      >
        <Pause v-if="playing" class="w-4 h-4" aria-hidden="true" /><Play v-else class="w-4 h-4" aria-hidden="true" />
      </button>
      <span class="text-[13px] tabular-nums text-text-secondary dark:text-text-secondary-dark">{{ clock(position) }} / {{ clock(duration) }}</span>
      <span v-if="!plays" class="text-xs text-muted dark:text-muted-dark">{{ t('utilities.clip.no_play') }}</span>
    </div>

    <!-- The bar: the whole video, the stretch or moment chosen, and where the playhead is. -->
    <div
      ref="bar"
      class="relative h-11 rounded-md bg-surface-hover dark:bg-surface-hover-dark cursor-pointer select-none touch-none"
      @pointerdown="onDown($event, 'seek')"
      @pointermove="onMove"
      @pointerup="onUp"
      @pointercancel="onUp"
    >
      <template v-if="range">
        <div class="absolute inset-y-0 bg-accent/25 border-y-2 border-accent" :style="{ left: share(range.start), width: `calc(${share(range.end)} - ${share(range.start)})` }" />
        <button
          v-for="h in (['start', 'end'] as const)"
          :key="h"
          type="button"
          role="slider"
          class="absolute inset-y-[-3px] w-3 -ml-1.5 rounded-sm bg-accent focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 cursor-ew-resize"
          :style="{ left: share(range[h]) }"
          :aria-label="t(`utilities.clip.${h}`)"
          :aria-valuemin="0"
          :aria-valuemax="Math.round(duration * 10) / 10"
          :aria-valuenow="range[h]"
          :aria-valuetext="clock(range[h])"
          @pointerdown="onDown($event, h)"
          @pointermove="onMove"
          @pointerup="onUp"
          @keydown="onKey($event, h)"
        />
      </template>
      <button
        v-else-if="moment"
        type="button"
        role="slider"
        class="absolute inset-y-[-3px] w-3 -ml-1.5 rounded-sm bg-accent focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 cursor-ew-resize"
        :style="{ left: share(wb.options.vidframes.at) }"
        :aria-label="t('utilities.clip.at')"
        :aria-valuemin="0"
        :aria-valuemax="Math.round(duration * 10) / 10"
        :aria-valuenow="wb.options.vidframes.at"
        :aria-valuetext="clock(wb.options.vidframes.at)"
        @pointerdown="onDown($event, 'at')"
        @pointermove="onMove"
        @pointerup="onUp"
        @keydown="onKey($event, 'at')"
      />
      <div class="absolute inset-y-0 w-px bg-text dark:bg-text-dark pointer-events-none" :style="{ left: share(position) }" aria-hidden="true" />
    </div>

    <div v-if="range" class="flex flex-wrap items-end gap-3 text-[13px]">
      <label v-for="h in (['start', 'end'] as const)" :key="h" class="flex flex-col gap-1">
        <span class="text-text-secondary dark:text-text-secondary-dark">{{ t(`utilities.clip.${h}`) }}</span>
        <span class="flex gap-1.5">
          <input v-model="typed[h]" class="utility-field !w-[92px] tabular-nums" inputmode="decimal" @change="typedIn(h)" @keydown.enter="typedIn(h)" />
          <button type="button" class="btn-secondary !h-[30px] !px-2.5 !text-[13px]" @click="move(h, position)">{{ t('utilities.clip.here') }}</button>
        </span>
      </label>
      <span class="ml-auto text-text-secondary dark:text-text-secondary-dark tabular-nums pb-1.5">{{ t('utilities.clip.length', { length: clock(length) }) }}</span>
    </div>
    <label v-else-if="moment" class="flex items-end gap-1.5 text-[13px]">
      <span class="flex flex-col gap-1">
        <span class="text-text-secondary dark:text-text-secondary-dark">{{ t('utilities.clip.at') }}</span>
        <input v-model="typed.at" class="utility-field !w-[92px] tabular-nums" inputmode="decimal" @change="typedIn('at')" @keydown.enter="typedIn('at')" />
      </span>
      <button type="button" class="btn-secondary !h-[30px] !px-2.5 !text-[13px]" @click="move('at', position)">{{ t('utilities.clip.here') }}</button>
    </label>
  </div>
  <div v-else class="w-full max-w-[760px] aspect-video rounded-lg bg-surface-hover dark:bg-surface-hover-dark animate-pulse" aria-hidden="true" />
</template>
