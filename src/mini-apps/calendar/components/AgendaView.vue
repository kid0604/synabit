<script setup lang="ts">
import { computed } from 'vue';
import { Search, X, User, MapPin, Calendar as CalendarIcon } from 'lucide-vue-next';
import type { EventMetadata } from '../types';
import type { AgendaDay } from '../composables/useAgenda';
import { formatDateString, isSameDay, shiftDateString } from '../helpers';
import { reviewOf, asHours } from '../review';
import { isSubscribed, styleForEvent } from '../subscriptions';
import { isElsewhere, shortZoneName } from '../timezone';
import { i18n } from '../../../i18n';

const props = defineProps<{
    days: AgendaDay[];
    loading: boolean;
    query: string;
    personName: string;
    narrowed: boolean;
    subscriptionColours: Record<string, string>;
}>();

const emit = defineEmits<{
    (e: 'update:query', v: string): void;
    (e: 'clear-person'): void;
    (e: 'open-event', ev: EventMetadata, dateStr: string): void;
}>();

const today = computed(() => formatDateString(new Date()));
const tomorrow = computed(() => shiftDateString(today.value, 1));

/** "Today", "Tomorrow", or the date written out. */
const heading = (dateStr: string) => {
    if (dateStr === today.value) return i18n.global.t('calendar.agenda_today');
    if (dateStr === tomorrow.value) return i18n.global.t('calendar.agenda_tomorrow');
    return new Date(`${dateStr}T00:00:00`).toLocaleDateString(i18n.global.locale.value, {
        weekday: 'long', day: 'numeric', month: 'long',
        year: dateStr.slice(0, 4) === today.value.slice(0, 4) ? undefined : 'numeric',
    });
};

const isPast = (dateStr: string) => dateStr < today.value;

const clock = (event: EventMetadata) => {
    if (event.is_all_day) return i18n.global.t('calendar.all_day');
    const at = (stamp: string) => stamp.includes('T') ? stamp.split('T')[1].slice(0, 5) : '';
    const start = at(event.start_at);
    const end = at(event.end_at || '');
    return end && end !== start ? `${start} – ${end}` : start;
};

const dotOf = (event: EventMetadata) =>
    styleForEvent(event, props.subscriptionColours).dot;

/**
 * What this stretch of calendar went on.
 *
 * Summed from the occurrences already listed, so the figures are the same
 * ones on screen. Hidden when there is nothing to add up — a summary of one
 * meeting is not a summary.
 */
const review = computed(() => reviewOf(props.days));
const worthSummarising = computed(() => review.value.events >= 3);

const dayName = (dateStr: string) =>
    new Date(`${dateStr}T00:00:00`).toLocaleDateString(i18n.global.locale.value, {
        day: 'numeric', month: 'short',
    });
</script>

<template>
    <div class="h-full flex flex-col">
        <!-- What is being asked -->
        <div class="flex items-center gap-2 mb-3 shrink-0">
            <div class="relative flex-1 min-w-0">
                <Search class="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400 pointer-events-none" />
                <input :value="query" type="search"
                       :placeholder="$t('calendar.agenda_search_ph')"
                       :aria-label="$t('calendar.agenda_search_ph')"
                       class="w-full h-[38px] pl-9 pr-3 bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] rounded-lg text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white"
                       @input="emit('update:query', ($event.target as HTMLInputElement).value)">
            </div>
            <button v-if="personName" type="button" @click="emit('clear-person')"
                    :aria-label="$t('calendar.agenda_clear_person')"
                    class="flex items-center gap-1.5 h-[38px] px-3 rounded-lg bg-purple-100 dark:bg-purple-900/40 text-purple-700 dark:text-purple-200 text-[12px] font-semibold shrink-0">
                <User class="w-3.5 h-3.5" />
                {{ $t('calendar.agenda_with_person', { name: personName }) }}
                <X class="w-3.5 h-3.5" />
            </button>
        </div>

        <!-- What it all came to -->
        <section v-if="worthSummarising" class="shrink-0 mb-3 px-3 py-2.5 rounded-xl border border-[#f0f0f0] dark:border-[#333] bg-white dark:bg-[#232323]">
            <p class="text-[12px] font-semibold text-[#1c1c1e] dark:text-[#f4f4f5]">
                {{ $t('calendar.review_summary', { events: review.events, hours: asHours(review.minutes) }) }}
                <span v-if="review.allDayEvents" class="font-normal text-gray-400">
                    · {{ $t('calendar.review_all_day', { n: review.allDayEvents }) }}
                </span>
            </p>
            <p v-if="review.busiestDay" class="text-[11px] text-gray-500 dark:text-gray-400 mt-0.5">
                {{ $t('calendar.review_busiest', { date: dayName(review.busiestDay.date), hours: asHours(review.busiestDay.minutes) }) }}
            </p>
            <div v-if="review.people.length || review.tags.length" class="flex flex-wrap gap-x-6 gap-y-1 mt-2">
                <div v-if="review.people.length" class="min-w-0">
                    <p class="text-[10px] font-bold uppercase tracking-wider text-gray-400">{{ $t('calendar.review_people') }}</p>
                    <p class="text-[11px] text-gray-600 dark:text-gray-300 truncate">
                        <span v-for="(p, i) in review.people" :key="p.label">
                            <span v-if="i">, </span>{{ p.label }} <span class="text-gray-400">{{ asHours(p.minutes) }}</span>
                        </span>
                    </p>
                </div>
                <div v-if="review.tags.length" class="min-w-0">
                    <p class="text-[10px] font-bold uppercase tracking-wider text-gray-400">{{ $t('calendar.review_tags') }}</p>
                    <p class="text-[11px] text-gray-600 dark:text-gray-300 truncate">
                        <span v-for="(t, i) in review.tags" :key="t.label">
                            <span v-if="i">, </span>{{ t.label }} <span class="text-gray-400">{{ asHours(t.minutes) }}</span>
                        </span>
                    </p>
                </div>
            </div>
        </section>

        <div class="flex-1 min-h-0 overflow-y-auto no-scrollbar pr-1">
            <p v-if="loading" class="text-[13px] text-gray-400 italic py-6 text-center">…</p>
            <p v-else-if="days.length === 0" class="text-[13px] text-gray-400 italic py-6 text-center">
                {{ narrowed ? $t('calendar.agenda_no_matches') : $t('calendar.agenda_empty') }}
            </p>

            <section v-for="day in days" :key="day.date" class="mb-4">
                <h3 class="sticky top-0 z-10 py-1.5 bg-[#fdfdfc] dark:bg-[#242424] text-[11px] font-bold uppercase tracking-wider"
                    :class="[
                        isPast(day.date) ? 'text-gray-400 dark:text-gray-600' : 'text-gray-500 dark:text-gray-400',
                        isSameDay(new Date(`${day.date}T00:00:00`), new Date()) ? 'text-purple-600 dark:text-purple-400' : '',
                    ]">
                    {{ heading(day.date) }}
                </h3>
                <ul class="flex flex-col gap-1.5 mt-1">
                    <li v-for="ev in day.events" :key="ev.id + day.date">
                        <button type="button" @click="emit('open-event', ev, day.date)"
                                class="w-full flex items-start gap-3 px-3 py-2 rounded-xl text-left border border-[#f0f0f0] dark:border-[#333] bg-white dark:bg-[#232323] hover:border-purple-300 dark:hover:border-purple-500/50 transition-colors"
                                :class="isPast(day.date) ? 'opacity-60' : ''">
                            <span class="w-2 h-2 rounded-full mt-1.5 shrink-0" :class="dotOf(ev)"></span>
                            <span class="min-w-0 flex-1">
                                <span class="block text-[13px] font-semibold truncate text-[#1c1c1e] dark:text-[#f4f4f5]">
                                    {{ ev.title }}
                                </span>
                                <span class="flex flex-wrap items-center gap-x-3 gap-y-0.5 text-[11px] text-gray-500 dark:text-gray-400 mt-0.5">
                                    <span>{{ clock(ev) }}</span>
                                    <span v-if="ev.location" class="flex items-center gap-1 truncate">
                                        <MapPin class="w-3 h-3 shrink-0" />{{ ev.location }}
                                    </span>
                                    <span v-if="isElsewhere(ev.tzid)">
                                        {{ $t('calendar.written_in', { zone: shortZoneName(ev.tzid || '') }) }}
                                    </span>
                                    <span v-if="isSubscribed(ev)" class="flex items-center gap-1">
                                        <CalendarIcon class="w-3 h-3 shrink-0" />{{ $t('calendar.subscribe_badge') }}
                                    </span>
                                </span>
                            </span>
                        </button>
                    </li>
                </ul>
            </section>
        </div>
    </div>
</template>
