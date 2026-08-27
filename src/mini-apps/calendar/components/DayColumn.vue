<script setup lang="ts">
import { computed } from 'vue';
import { MapPin } from 'lucide-vue-next';
import type { EventMetadata } from '../types';
import { layoutDay, clockOf, MINUTES_PER_DAY } from '../layout';
import { isElsewhere, shortZoneName } from '../timezone';
import { isSubscribed, styleForEvent } from '../subscriptions';
import type { DragDraft } from '../composables/useTimeGridDrag';

const props = defineProps<{
    dateStr: string;
    events: EventMetadata[];
    hourHeight: number;
    /** Minutes past midnight, when this column is today. */
    nowMinute: number | null;
    draft: DragDraft | null;
    /** The colour each subscribed calendar was given, by id. */
    subscriptionColours: Record<string, string>;
}>();

const emit = defineEmits<{
    (e: 'pointerdown-empty', ev: PointerEvent, dateStr: string): void;
    (e: 'pointerdown-block', ev: PointerEvent, event: EventMetadata, dateStr: string, startMinute: number, endMinute: number): void;
    (e: 'pointerdown-resize', ev: PointerEvent, event: EventMetadata, dateStr: string, startMinute: number): void;
    (e: 'open', event: EventMetadata, dateStr: string): void;
}>();

const blocks = computed(() => layoutDay(props.events, props.dateStr));

/** The block being dragged is drawn from the draft, so it follows the pointer. */
const draftHere = computed(() =>
    props.draft && props.draft.dateStr === props.dateStr ? props.draft : null);

const hiddenId = computed(() =>
    props.draft && props.draft.event ? props.draft.event.id : null);

const pct = (minute: number) => (minute / MINUTES_PER_DAY) * 100;

const styleOf = (event: EventMetadata) =>
    styleForEvent(event, props.subscriptionColours).block;

const timeLabel = (start: number, end: number) =>
    end > start ? `${clockOf(start)} – ${clockOf(end)}` : clockOf(start);

/**
 * What a block says its hours are.
 *
 * For a block clipped by midnight the clipped minutes are the wrong thing to
 * read out: an event running to 01:30 tomorrow was labelled "22:00 – 23:59",
 * which names a time it does not end at. The instance's own timestamps are
 * used instead, and the arrows say which edge is off-screen.
 */
const blockLabel = (b: { event: EventMetadata; startMinute: number; endMinute: number; continuesBefore: boolean; continuesAfter: boolean }) => {
    if (!b.continuesBefore && !b.continuesAfter) return timeLabel(b.startMinute, b.endMinute);
    const clock = (stampStr: string) => stampStr.includes('T') ? stampStr.split('T')[1].slice(0, 5) : '';
    const from = clock(b.event.start_at);
    const to = clock(b.event.end_at);
    return from && to ? `${from} – ${to}` : from || to;
};
</script>

<template>
    <div class="relative flex-1 min-w-0 border-r last:border-r-0 border-[#ececeb] dark:border-[#2f2f2f]"
         :style="{ height: hourHeight * 24 + 'px' }"
         @pointerdown.self="emit('pointerdown-empty', $event, dateStr)">

        <!-- Hour lines. Not interactive; the whole column takes the pointer. -->
        <div class="absolute inset-0 pointer-events-none" aria-hidden="true">
            <div v-for="h in 24" :key="'line-' + h"
                 class="border-b border-gray-100 dark:border-[#2a2a2a]"
                 :style="{ height: hourHeight + 'px' }"></div>
        </div>

        <!-- Events -->
        <button v-for="b in blocks" :key="b.key"
                v-show="b.event.id !== hiddenId"
                type="button"
                class="absolute text-left rounded-md px-1.5 py-0.5 overflow-hidden group border
                       hover:z-20 focus-visible:z-20 shadow-sm
                       focus-visible:outline focus-visible:outline-2 focus-visible:outline-purple-500"
                :class="[styleOf(b.event), isSubscribed(b.event) ? 'cursor-pointer' : 'cursor-grab']"
                :style="{
                    top: b.topPct + '%',
                    height: b.heightPct + '%',
                    left: `calc(${b.leftPct}% + 2px)`,
                    width: `calc(${b.widthPct}% - 4px)`,
                }"
                :aria-label="isElsewhere(b.event.tzid)
                    ? `${b.event.title}, ${blockLabel(b)}, ${$t('calendar.written_in', { zone: shortZoneName(b.event.tzid || '') })}`
                    : `${b.event.title}, ${blockLabel(b)}`"
                @pointerdown="isSubscribed(b.event)
                    ? undefined
                    : emit('pointerdown-block', $event, b.event, dateStr, b.startMinute, b.endMinute)"
                @click="isSubscribed(b.event) ? emit('open', b.event, dateStr) : undefined"
                @keydown.enter.prevent="emit('open', b.event, dateStr)"
                @keydown.space.prevent="emit('open', b.event, dateStr)">
            <span class="block text-[11px] font-semibold leading-tight truncate">
                <span v-if="b.continuesBefore" aria-hidden="true">↑ </span>{{ b.event.title }}
            </span>
            <span v-if="b.endMinute - b.startMinute >= 45" class="block text-[10px] opacity-75 leading-tight truncate">
                {{ blockLabel(b) }}<span v-if="b.continuesAfter" aria-hidden="true"> ↓</span>
            </span>
            <!--
              A time that was written somewhere else says so. The block already
              shows the reader's clock — the grid converted it — and without
              this there is nothing to explain why a call with Tokyo is at
              seven in the evening.
            -->
            <span v-if="isElsewhere(b.event.tzid) && b.endMinute - b.startMinute >= 60"
                  class="block text-[10px] opacity-60 leading-tight truncate">
                {{ $t('calendar.written_in', { zone: shortZoneName(b.event.tzid || '') }) }}
            </span>
            <span v-if="b.event.location && b.endMinute - b.startMinute >= 75"
                  class="flex items-center gap-1 text-[10px] opacity-70 truncate">
                <MapPin class="w-2.5 h-2.5 shrink-0" />{{ b.event.location }}
            </span>

            <!-- Resize grip. Wider than it looks so it can actually be hit. -->
            <span v-if="!b.continuesAfter && !isSubscribed(b.event)"
                  class="absolute inset-x-0 bottom-0 h-2 cursor-ns-resize opacity-0 group-hover:opacity-100"
                  @pointerdown.stop="emit('pointerdown-resize', $event, b.event, dateStr, b.startMinute)">
                <span class="block mx-auto mt-1 h-0.5 w-6 rounded bg-blue-500/70"></span>
            </span>
        </button>

        <!-- What the pointer is currently drawing. -->
        <div v-if="draftHere"
             class="absolute left-0.5 right-0.5 rounded-md border-2 border-dashed border-purple-500
                    bg-purple-200/50 dark:bg-purple-500/25 pointer-events-none z-30 px-1.5 py-0.5"
             :style="{ top: pct(draftHere.startMinute) + '%', height: pct(draftHere.endMinute - draftHere.startMinute) + '%' }">
            <span class="block text-[11px] font-semibold leading-tight truncate text-purple-900 dark:text-purple-100">
                {{ draftHere.event?.title ?? draftHere.label ?? timeLabel(draftHere.startMinute, draftHere.endMinute) }}
            </span>
            <span v-if="draftHere.event || draftHere.label" class="block text-[10px] leading-tight text-purple-900/80 dark:text-purple-100/80">
                {{ timeLabel(draftHere.startMinute, draftHere.endMinute) }}
            </span>
        </div>

        <!-- Now. -->
        <div v-if="nowMinute !== null" class="absolute inset-x-0 z-10 pointer-events-none"
             :style="{ top: pct(nowMinute) + '%' }" aria-hidden="true">
            <div class="relative border-t-2 border-red-500">
                <div class="absolute -left-1 -top-[5px] w-2 h-2 rounded-full bg-red-500"></div>
            </div>
        </div>
    </div>
</template>
