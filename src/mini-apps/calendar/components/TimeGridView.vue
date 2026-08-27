<script setup lang="ts">
import { ref, computed, watch, onMounted, nextTick } from 'vue';
import { Calendar as CalendarIcon, CheckSquare } from 'lucide-vue-next';
import type { EventMetadata, TaskMetadata } from '../types';
import { dayNamesShort, isSameDay, weekdayOffset } from '../helpers';
import { splitDayEvents, MINUTES_PER_DAY } from '../layout';
import { useNow } from '../composables/useNow';
import { useTimeGridDrag } from '../composables/useTimeGridDrag';
import TimeAxis from './TimeAxis.vue';
import DayColumn from './DayColumn.vue';

const HOUR_HEIGHT = 48;
const AXIS_WIDTH = 56;

const props = defineProps<{
    /** One column per day, left to right. */
    days: { date: Date; dateStr: string }[];
    getTasksForDate: (dateStr: string) => TaskMetadata[];
    getEventsForDate: (dateStr: string) => EventMetadata[];
    /** Week view labels each column; day view does not need to. */
    showDayHeaders: boolean;
    /** The colour each subscribed calendar was given, by id. */
    subscriptionColours: Record<string, string>;
}>();

const emit = defineEmits<{
    (e: 'click-day', date: Date): void;
    (e: 'add-event', date: Date, startAt?: string, endAt?: string): void;
    (e: 'edit-event', ev: EventMetadata, dateStr: string): void;
    (e: 'reschedule', ev: EventMetadata, dateStr: string, startAt: string, endAt: string): void;
    (e: 'block-task', task: { id: string; title: string }, dateStr: string, startAt: string, endAt: string): void;
    (e: 'toggle-task', task: { id: string; status: string }): void;
    (e: 'open-task', id: string): void;
}>();

const now = useNow();
const scroller = ref<HTMLElement | null>(null);
const gridEl = ref<HTMLElement | null>(null);

const dayStrings = computed(() => props.days.map(d => d.dateStr));
const split = computed(() =>
    props.days.map(d => ({ ...d, ...splitDayEvents(props.getEventsForDate(d.dateStr), d.dateStr) })));

const nowMinuteFor = (date: Date): number | null =>
    isSameDay(date, now.value) ? now.value.getHours() * 60 + now.value.getMinutes() : null;

const { draft, startMove, startResize, startCreate, startBlock } = useTimeGridDrag({
    gridEl,
    days: dayStrings,
    onOpen: (ev, dateStr) => emit('edit-event', ev, dateStr),
    onCreate: (dateStr, startMinute, endMinute) => {
        const day = props.days.find(d => d.dateStr === dateStr);
        if (!day) return;
        const clock = (m: number) => `${String(Math.floor(m / 60)).padStart(2, '0')}:${String(m % 60).padStart(2, '0')}`;
        emit('add-event', day.date, `${dateStr}T${clock(startMinute)}`, `${dateStr}T${clock(endMinute)}`);
    },
    onReschedule: (ev, dateStr, startAt, endAt) => emit('reschedule', ev, dateStr, startAt, endAt),
    onBlock: (task, dateStr, startAt, endAt) => emit('block-task', task, dateStr, startAt, endAt),
});

/**
 * Open on the working day, not on midnight.
 *
 * The grid used to start at 12 AM every time, so every visit began by
 * scrolling past eight empty hours.
 */
const scrollToNow = () => {
    const el = scroller.value;
    if (!el) return;
    const minute = now.value.getHours() * 60 + now.value.getMinutes();
    const target = ((minute - 90) / MINUTES_PER_DAY) * HOUR_HEIGHT * 24;
    el.scrollTop = Math.max(0, target);
};

onMounted(() => { nextTick(scrollToNow); });
watch(() => props.days[0]?.dateStr, () => { nextTick(scrollToNow); });
</script>

<template>
    <div class="w-full h-full flex flex-col border border-[#ececeb] dark:border-[#333] rounded-2xl bg-white dark:bg-[#1a1a1a] overflow-hidden select-none">

        <!-- Day headings and the all-day row -->
        <div class="flex border-b border-[#ececeb] dark:border-[#333] z-10 bg-white dark:bg-[#1a1a1a] shrink-0">
            <div class="flex items-end justify-center pb-1 border-r border-[#ececeb] dark:border-[#333] bg-gray-50/50 dark:bg-[#222]"
                 :style="{ width: AXIS_WIDTH + 'px' }">
                <span class="text-[9px] font-bold text-gray-400 uppercase tracking-wider">{{ $t('calendar.all_day') }}</span>
            </div>
            <div v-for="day in split" :key="'head-' + day.dateStr"
                 class="flex-1 min-w-0 flex flex-col border-r last:border-r-0 border-[#ececeb] dark:border-[#333]">
                <button v-if="showDayHeaders" type="button" @click="emit('click-day', day.date)"
                        class="text-center py-1.5 border-b border-[#ececeb] dark:border-[#333] transition-colors"
                        :class="isSameDay(day.date, now) ? 'bg-purple-50 dark:bg-purple-900/20 text-purple-700 dark:text-purple-300' : 'bg-gray-50/50 dark:bg-[#222] text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-[#282828]'">
                    <span class="text-[10px] uppercase font-bold tracking-wider block">{{ dayNamesShort()[weekdayOffset(day.date)] }}</span>
                    <span class="text-base font-bold"
                          :class="{ 'bg-purple-600 text-white rounded-full w-6 h-6 flex items-center justify-center mx-auto': isSameDay(day.date, now) }">
                        {{ day.date.getDate() }}
                    </span>
                </button>
                <div class="p-1 min-h-[36px] max-h-24 overflow-y-auto no-scrollbar flex flex-col gap-1 bg-gray-50/20 dark:bg-[#1d1d1d]">
                    <!-- Drag one down onto an hour to decide when to do it. -->
                    <button v-for="tk in getTasksForDate(day.dateStr)" :key="'tsk-' + tk.id" type="button"
                            :title="$t('calendar.block_task_hint')"
                            class="truncate px-1.5 py-0.5 rounded text-[10px] font-medium border border-gray-200 dark:border-[#3a3a3a] text-gray-600 dark:text-gray-300 flex items-center gap-1 bg-white dark:bg-[#2c2c2c] text-left cursor-grab"
                            @pointerdown="startBlock($event, { id: tk.id, title: tk.title }, day.dateStr)"
                            @click="emit('open-task', tk.id)">
                        <CheckSquare class="w-2.5 h-2.5 shrink-0" :class="tk.status === 'done' ? 'text-green-500' : ''"
                                     @click.stop="emit('toggle-task', tk)" />
                        <span class="truncate" :class="tk.status === 'done' ? 'line-through' : ''">{{ tk.title }}</span>
                    </button>
                    <button v-for="ev in day.allDay" :key="'ad-' + ev.id" type="button"
                            class="truncate px-1.5 py-0.5 rounded text-[10px] font-medium border border-blue-200 dark:border-blue-800/50 text-blue-800 dark:text-blue-200 bg-blue-50 dark:bg-blue-900/30 flex items-center gap-1 text-left"
                            @click="emit('edit-event', ev, day.dateStr)">
                        <CalendarIcon class="w-2.5 h-2.5 shrink-0" /><span class="truncate">{{ ev.title }}</span>
                    </button>
                </div>
            </div>
        </div>

        <!-- The axis itself -->
        <div ref="scroller" class="flex-1 overflow-y-auto no-scrollbar">
            <div class="flex">
                <div class="shrink-0 border-r border-[#ececeb] dark:border-[#2f2f2f] bg-white dark:bg-[#1a1a1a] sticky left-0 z-10"
                     :style="{ width: AXIS_WIDTH + 'px' }">
                    <TimeAxis :hour-height="HOUR_HEIGHT" />
                </div>
                <div ref="gridEl" class="flex flex-1 min-w-0">
                    <DayColumn v-for="day in split" :key="'col-' + day.dateStr"
                        :date-str="day.dateStr"
                        :events="day.timed"
                        :hour-height="HOUR_HEIGHT"
                        :now-minute="nowMinuteFor(day.date)"
                        :draft="draft"
                        :subscription-colours="subscriptionColours"
                        @pointerdown-empty="(e, ds) => startCreate(e, ds)"
                        @pointerdown-block="(e, ev, ds, s, en) => startMove(e, ev, ds, s, en)"
                        @pointerdown-resize="(e, ev, ds, s) => startResize(e, ev, ds, s)"
                        @open="(ev, ds) => emit('edit-event', ev, ds)"
                    />
                </div>
            </div>
        </div>
    </div>
</template>
