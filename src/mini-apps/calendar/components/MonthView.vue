<script setup lang="ts">
import { ref, computed, nextTick } from 'vue';
import { CheckSquare } from 'lucide-vue-next';
import type { EventMetadata, TaskMetadata } from '../types';
import { dayNamesShort, formatDateString, isSameDay } from '../helpers';
import { i18n } from '../../../i18n';

const props = defineProps<{
    calendarDays: { date: Date; inMonth: boolean }[];
    selectedDate: Date;
    getEventsForDate: (dateStr: string) => EventMetadata[];
    getTasksForDate: (dateStr: string) => TaskMetadata[];
    getMonthViewItems: (dateStr: string) => { display: any[]; moreCount: number };
}>();

const emit = defineEmits<{
    (e: 'click-day', date: Date): void;
    (e: 'focus-day', date: Date): void;
    (e: 'edit-event', ev: EventMetadata, dateStr: string): void;
    (e: 'toggle-task', task: { id: string; status: string }): void;
    (e: 'open-task', id: string): void;
}>();

/**
 * The month grid, reachable by keyboard.
 *
 * It was forty-two `<div>`s with a click handler — the default view of the
 * app, and no way into it without a mouse. What it is now is the pattern a
 * grid of dates is supposed to be: one cell in the tab order at a time, and
 * the arrow keys moving between them.
 *
 * A roving `tabindex` rather than forty-two tab stops: nobody wants to press
 * Tab six times to leave a month.
 */
const cells = ref<HTMLElement[]>([]);
/**
 * Null until the grid has been moved through, not zero.
 *
 * Zero is a cell — the first one — so `active || tabStop` sent the tab stop
 * back to the selected day the moment somebody arrowed onto Sunday of the
 * leading week, and tabbing away and back returned them somewhere else.
 */
const active = ref<number | null>(null);

/** The cell that holds the tab stop — the selected day, or the first one. */
const tabStop = computed(() => {
    const chosen = props.calendarDays.findIndex(d => isSameDay(d.date, props.selectedDate));
    return chosen >= 0 ? chosen : 0;
});

const focusCell = async (index: number) => {
    const clamped = Math.max(0, Math.min(props.calendarDays.length - 1, index));
    active.value = clamped;
    await nextTick();
    cells.value[clamped]?.focus();
    // Moving through the grid moves the selection with it, the way arrowing
    // through a list of files selects as it goes.
    emit('focus-day', props.calendarDays[clamped].date);
};

const onKey = (e: KeyboardEvent, index: number) => {
    const step: Record<string, number> = {
        ArrowLeft: -1, ArrowRight: 1, ArrowUp: -7, ArrowDown: 7,
    };
    if (e.key in step) {
        e.preventDefault();
        focusCell(index + step[e.key]);
        return;
    }
    if (e.key === 'Home' || e.key === 'End') {
        e.preventDefault();
        // The start or end of the week this cell is in.
        const weekStart = index - (index % 7);
        focusCell(e.key === 'Home' ? weekStart : weekStart + 6);
    }
};

/**
 * What a screen reader says about a day.
 *
 * The date on its own is not enough — the whole reason to move through a
 * month is to find the days with something on them, and a grid of forty-two
 * bare numbers hides exactly that.
 */
const cellLabel = (date: Date) => {
    const dateStr = formatDateString(date);
    const written = date.toLocaleDateString(i18n.global.locale.value, {
        weekday: 'long', day: 'numeric', month: 'long', year: 'numeric',
    });
    const events = props.getEventsForDate(dateStr).length;
    const tasks = props.getTasksForDate(dateStr).length;
    if (!events && !tasks) return written;

    // Counted separately and only when there is something to count: "1 events,
    // 0 tasks" is worse than saying nothing.
    const parts = [written];
    // Named values first, plural count second. The other argument order —
    // `t(key, plural, named)` — reads more naturally but is not one of `t`'s
    // signatures: the third parameter there is `TranslateOptions`, so `{ n }`
    // matched nothing and the call fell through to the wrong overload.
    if (events) parts.push(i18n.global.t('calendar.cell_events', { n: events }, events));
    if (tasks) parts.push(i18n.global.t('calendar.cell_tasks', { n: tasks }, tasks));
    return parts.join(', ');
};
</script>

<template>
    <div class="h-full flex flex-col select-none">
        <div class="grid grid-cols-7 mb-2 flex-shrink-0 border-b border-[#e6e6e6] dark:border-[#333] pb-2 px-1">
            <div v-for="(day, di) in dayNamesShort()" :key="'d'+di" aria-hidden="true" class="text-center text-xs font-bold uppercase tracking-wider text-[#8b8b8b] dark:text-[#71717a]">
                {{ day }}
            </div>
        </div>
        <div class="flex-1 overflow-y-auto no-scrollbar pb-2 px-1">
            <div role="grid" :aria-label="$t('calendar.month_grid')"
                 class="grid grid-cols-7 grid-rows-6 gap-2 min-h-[500px] md:min-h-[650px] h-full">
            <div v-for="(dayObj, idx) in calendarDays" :key="idx"
                 ref="cells"
                 role="gridcell"
                 :tabindex="idx === (active ?? tabStop) ? 0 : -1"
                 :aria-selected="isSameDay(dayObj.date, selectedDate)"
                 :aria-label="cellLabel(dayObj.date)"
                 @click="emit('click-day', dayObj.date)"
                 @keydown="onKey($event, idx)"
                 @keydown.enter.prevent="emit('click-day', dayObj.date)"
                 @keyup.space.prevent="emit('click-day', dayObj.date)"
                 @focus="active = idx"
                 class="relative flex flex-col rounded-xl border border-[#ececeb] dark:border-[#2f2f2f] cursor-pointer transition-all duration-200 overflow-hidden group hover:border-[#d4d4d8] dark:hover:border-[#4f4f4f] hover:shadow-sm focus-visible:outline focus-visible:outline-2 focus-visible:outline-purple-500 focus-visible:outline-offset-2"
                 :class="[
                     dayObj.inMonth ? 'bg-white dark:bg-[#262626]' : 'bg-gray-50/50 dark:bg-[#1f1f1f]',
                     isSameDay(dayObj.date, selectedDate) ? 'ring-2 ring-purple-500 border-transparent dark:border-transparent' : '',
                     isSameDay(dayObj.date, new Date()) ? 'bg-gradient-to-br from-purple-50/50 to-transparent dark:from-purple-900/10' : ''
                 ]"
            >
                <div class="w-full flex justify-between items-start p-2 pointer-events-none">
                    <span class="text-sm font-medium w-6 h-6 flex items-center justify-center rounded-full"
                          :class="[
                              !dayObj.inMonth ? 'text-gray-400 dark:text-gray-600' : 'text-[#1c1c1e] dark:text-[#f4f4f5]',
                              isSameDay(dayObj.date, new Date()) ? 'bg-purple-600 text-white dark:text-white' : ''
                          ]"
                    >
                        {{ dayObj.date.getDate() }}
                    </span>
                </div>
                <div class="flex-1 px-1 md:px-2 pb-1 md:pb-2 overflow-y-auto w-full no-scrollbar md:space-y-1">
                    <!-- Mobile Dots -->
                    <div class="flex flex-wrap gap-1 md:hidden pt-1 px-1">
                        <div v-for="ev in getEventsForDate(formatDateString(dayObj.date))" :key="'evt-dot-'+ev.id" class="w-1.5 h-1.5 rounded-full bg-blue-500"></div>
                        <div v-for="tk in getTasksForDate(formatDateString(dayObj.date))" :key="'tsk-dot-'+tk.id" class="w-1.5 h-1.5 rounded-full" :class="tk.status === 'done' ? 'bg-green-500' : 'bg-gray-400 dark:bg-gray-500'"></div>
                    </div>
                    <!-- Desktop Text -->
                    <div class="hidden md:flex flex-col gap-1 w-full" v-for="(dayData, idx) in [getMonthViewItems(formatDateString(dayObj.date))]" :key="'ddata-'+dayObj.date.getTime()+'-'+idx">
                        <template v-for="item in dayData.display" :key="item.type + '-' + item.id">
                            <div v-if="item.type === 'event'" class="w-full text-left truncate px-1.5 py-0.5 rounded text-[10px] font-medium bg-blue-100/80 text-blue-800 border border-blue-200/50 dark:bg-blue-900/30 dark:text-blue-300 dark:border-blue-800/30 shadow-[0_1px_2px_rgba(0,0,0,0.02)] transition-colors hover:brightness-95 cursor-pointer" @click.stop="emit('edit-event', item.event, formatDateString(dayObj.date))">
                                <span v-if="item.event_time" class="opacity-70 mr-0.5">{{ item.event_time }}</span> {{ item.title }}
                            </div>
                            <div v-else class="w-full text-left truncate px-1.5 py-0.5 rounded text-[10px] font-medium border border-gray-200/80 dark:border-[#3a3a3a]/80 text-gray-700 dark:text-gray-300 flex items-center gap-1 bg-white dark:bg-[#252525] shadow-[0_1px_2px_rgba(0,0,0,0.02)] transition-colors hover:bg-gray-50 dark:hover:bg-[#2a2a2a] cursor-pointer hover:brightness-95" :class="item.status === 'done' ? 'opacity-60' : ''" @click.stop="emit('open-task', item.id)">
                                <CheckSquare class="w-2.5 h-2.5 shrink-0 hover:text-purple-500 transition-colors" :class="item.status === 'done' ? 'text-green-500' : 'text-gray-400'" @click.stop="emit('toggle-task', item)" /> <span :class="item.status === 'done' ? 'line-through' : ''">{{ item.title }}</span>
                            </div>
                        </template>
                        <div v-if="dayData.moreCount > 0" 
                             class="text-[10px] font-semibold text-gray-500 hover:text-gray-800 dark:text-gray-400 dark:hover:text-gray-200 cursor-pointer px-1 py-0.5 hover:bg-gray-100 dark:hover:bg-[#333] rounded transition-colors w-max" 
                             @click.stop="emit('click-day', dayObj.date)">
                            +{{ dayData.moreCount }} more
                        </div>
                    </div>
                </div>
            </div>
        </div>
        </div>
    </div>
</template>
