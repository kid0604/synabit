<script setup lang="ts">
import { computed } from 'vue';
import type { RecurrenceFields, Freq, EndMode } from '../rrule';
import { WEEKDAY_CODES, weekdayCodeOf, describeRecurrence } from '../rrule';
import { useI18n } from 'vue-i18n';
import { firstDayOfWeek } from '../helpers';

const props = defineProps<{
    modelValue: RecurrenceFields;
    /** The event's start, so a weekly rule can default to that weekday. */
    startAt: string;
}>();

const emit = defineEmits<{ (e: 'update:modelValue', v: RecurrenceFields): void }>();

const { t } = useI18n();

/**
 * The rule in one sentence.
 *
 * Four controls can be read four ways; a line saying "Every 2 weeks · Mon,
 * Wed · 10 times" is how someone checks they built what they meant before
 * saving it.
 */
const summary = computed(() => describeRecurrence(props.modelValue, t as never));

const patch = (over: Partial<RecurrenceFields>) =>
    emit('update:modelValue', { ...props.modelValue, ...over });

/** The day toggles read in the order this locale draws a week. */
const orderedDays = computed(() => {
    const start = firstDayOfWeek();      // 0 = Sunday
    // WEEKDAY_CODES starts at Monday; shift so the grid and this agree.
    const fromMonday = (start + 6) % 7;
    return Array.from({ length: 7 }, (_, i) => WEEKDAY_CODES[(fromMonday + i) % 7]);
});

const setFreq = (freq: Freq) => {
    // Turning on a weekly rule with no day chosen would repeat on nothing, so
    // it starts on the day the event itself falls on.
    const byDay = freq === 'weekly' && props.modelValue.byDay.length === 0
        ? [weekdayCodeOf(props.startAt)]
        : props.modelValue.byDay;
    patch({ freq, byDay });
};

const toggleDay = (code: string) => {
    const on = props.modelValue.byDay.includes(code);
    // Never leave a weekly rule with nothing selected.
    if (on && props.modelValue.byDay.length === 1) return;
    patch({ byDay: on ? props.modelValue.byDay.filter(c => c !== code) : [...props.modelValue.byDay, code] });
};

const unitKey = computed(() => {
    const f = props.modelValue.freq;
    if (f === 'none') return 'calendar.unit_daily';
    return props.modelValue.interval > 1
        ? `calendar.unit_${f}_plural`
        : `calendar.unit_${f}`;
});

const labelClass = 'block text-xs font-bold text-gray-500 uppercase tracking-wider mb-1.5';
const fieldClass = 'h-[38px] bg-gray-50 dark:bg-[#2a2a2a] border border-gray-200 dark:border-[#444] '
    + 'rounded-lg px-2 text-sm focus:outline-none focus:border-purple-500 text-black dark:text-white';
</script>

<template>
    <div class="flex flex-col gap-3">
        <div>
            <label :class="labelClass" for="rec-freq">{{ $t('calendar.repeat') }}</label>
            <select id="rec-freq" :value="modelValue.freq" :class="[fieldClass, 'w-full appearance-none cursor-pointer']"
                    @change="setFreq(($event.target as HTMLSelectElement).value as Freq)">
                <option value="none">{{ $t('calendar.does_not_repeat') }}</option>
                <option value="daily">{{ $t('calendar.daily') }}</option>
                <option value="weekly">{{ $t('calendar.weekly') }}</option>
                <option value="monthly">{{ $t('calendar.monthly') }}</option>
                <option value="yearly">{{ $t('calendar.yearly') }}</option>
            </select>
        </div>

        <template v-if="modelValue.freq !== 'none'">
            <div class="flex items-end gap-2">
                <div class="shrink-0">
                    <label :class="labelClass" for="rec-interval">{{ $t('calendar.repeat_every') }}</label>
                    <input id="rec-interval" type="number" min="1" max="999" :value="modelValue.interval"
                           :class="[fieldClass, 'w-20 text-center']"
                           @input="patch({ interval: Math.max(1, Number(($event.target as HTMLInputElement).value) || 1) })">
                </div>
                <span class="text-sm text-gray-500 dark:text-gray-400 pb-2">{{ $t(unitKey) }}</span>
            </div>

            <div v-if="modelValue.freq === 'weekly'">
                <span :class="labelClass">{{ $t('calendar.on_days') }}</span>
                <div class="flex gap-1 flex-wrap">
                    <button v-for="code in orderedDays" :key="code" type="button"
                            :aria-pressed="modelValue.byDay.includes(code)"
                            class="w-9 h-9 rounded-lg text-[11px] font-semibold border transition-colors"
                            :class="modelValue.byDay.includes(code)
                                ? 'bg-purple-600 border-purple-600 text-white'
                                : 'bg-gray-50 dark:bg-[#2a2a2a] border-gray-200 dark:border-[#444] text-gray-600 dark:text-gray-300 hover:border-purple-400'"
                            @click="toggleDay(code)">
                        {{ $t(`calendar.day_${code}`) }}
                    </button>
                </div>
            </div>

            <div>
                <label :class="labelClass" for="rec-end">{{ $t('calendar.ends') }}</label>
                <div class="flex items-center gap-2 flex-wrap">
                    <select id="rec-end" :value="modelValue.endMode" :class="[fieldClass, 'flex-1 min-w-[7rem] appearance-none cursor-pointer']"
                            @change="patch({ endMode: ($event.target as HTMLSelectElement).value as EndMode })">
                        <option value="never">{{ $t('calendar.ends_never') }}</option>
                        <option value="until">{{ $t('calendar.ends_on_date') }}</option>
                        <option value="count">{{ $t('calendar.ends_after_count') }}</option>
                    </select>

                    <input v-if="modelValue.endMode === 'until'" type="date" :value="modelValue.until"
                           :class="[fieldClass, 'flex-1 min-w-[9rem]']" :style="{ colorScheme: 'light dark' }"
                           :aria-label="$t('calendar.ends_on_date')"
                           @input="patch({ until: ($event.target as HTMLInputElement).value })">

                    <template v-if="modelValue.endMode === 'count'">
                        <input type="number" min="1" max="999" :value="modelValue.count"
                               :class="[fieldClass, 'w-20 text-center']" :aria-label="$t('calendar.ends_after_count')"
                               @input="patch({ count: Math.max(1, Number(($event.target as HTMLInputElement).value) || 1) })">
                        <span class="text-sm text-gray-500 dark:text-gray-400">{{ $t('calendar.occurrences') }}</span>
                    </template>
                </div>
            </div>
            <p class="text-[11px] text-gray-500 dark:text-gray-400 -mt-1">{{ summary }}</p>
        </template>
    </div>
</template>
