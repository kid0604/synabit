<script setup lang="ts">
import { computed } from 'vue';
import { AlertCircle, Gift, ChevronRight, CalendarClock, Check, Clock } from 'lucide-vue-next';
import { useNodeService } from '../../composables/useNodeService';
import { useKeepInTouch } from './composables/useKeepInTouch';
import { contactStatus, daysSinceContact, daysUntilDue } from './composables/useRelationshipHealth';
import { daysUntilAnnual } from './composables/anniversaries';

const props = defineProps<{
    people: any[];
}>();

const emit = defineEmits(['select-person', 'updated']);

// Answering the nudge without leaving the list. Anything that takes three
// clicks to resolve gets dismissed instead of resolved.
const keepInTouch = useKeepInTouch(useNodeService());

const answer = async (person: any, action: 'contacted' | 'snooze') => {
    const done = action === 'contacted'
        ? await keepInTouch.markContacted(person)
        : await keepInTouch.snooze(person);
    if (done) emit('updated');
};

// Both lists below read their status from the same place the person's own
// card does. They used to carry a copy of the cadence table with different
// boundaries, so a contact could appear under Overdue here and read "Due
// Soon" when opened.
const byStatus = (status: string) => computed(() => {
    const now = Date.now();
    return props.people
        .filter(p => contactStatus(p, now) === status)
        .map(p => ({
            ...p,
            daysSince: daysSinceContact(p, now) ?? 0,
            // Past due, so `daysUntilDue` is negative; report it the way the
            // label reads it.
            overdueDays: -(daysUntilDue(p, now) ?? 0),
            daysLeft: daysUntilDue(p, now) ?? 0,
        }));
});

const overdueContacts = computed(() =>
    byStatus('overdue').value.sort((a, b) => b.overdueDays - a.overdueDays));

const dueSoonContacts = computed(() =>
    byStatus('due_soon').value.sort((a, b) => a.daysLeft - b.daysLeft));

// Birthdays within the next month, soonest first.
const upcomingBirthdays = computed(() => {
    const now = new Date();
    return props.people
        .map(p => {
            const daysUntil = daysUntilAnnual(p.properties?.birthday ?? '', now);
            return daysUntil === null ? null : { ...p, daysUntil };
        })
        .filter((p): p is NonNullable<typeof p> => p !== null && p.daysUntil <= 30)
        .sort((a, b) => a.daysUntil - b.daysUntil);
});

const hasReminders = computed(() => overdueContacts.value.length > 0 || upcomingBirthdays.value.length > 0 || dueSoonContacts.value.length > 0);

const formatBirthdayLabel = (daysUntil: number) => {
    if (daysUntil === 0) return 'Today! 🎉';
    if (daysUntil === 1) return 'Tomorrow';
    return `in ${daysUntil}d`;
};
</script>

<template>
    <div v-if="hasReminders" class="space-y-2 mb-3">
        <!-- Overdue -->
        <div v-if="overdueContacts.length > 0">
            <div class="px-2 mb-1">
                <span class="text-[10px] font-bold uppercase tracking-wider text-red-500 flex items-center gap-1">
                    <AlertCircle class="w-3 h-3" /> {{ $t('people.overdue_count', { count: overdueContacts.length }) }}
                </span>
            </div>
            <div v-for="p in overdueContacts.slice(0, 3)" :key="p.id"
                class="rounded-lg bg-red-50/50 dark:bg-red-900/10 border border-red-100 dark:border-red-900/20 mb-1 overflow-hidden">
                <button @click="emit('select-person', p)"
                    class="w-full text-left px-3 pt-2 flex items-center gap-2.5 hover:bg-red-100/50 dark:hover:bg-red-900/20 transition-colors">
                    <div class="w-2 h-2 rounded-full bg-red-500 flex-shrink-0 animate-pulse"></div>
                    <div class="flex-1 min-w-0">
                        <p class="text-xs font-medium truncate">{{ p.title }}</p>
                        <p class="text-[10px] text-red-500">{{ $t('people.days_overdue', { days: p.overdueDays }) }}</p>
                    </div>
                    <ChevronRight class="w-3 h-3 text-red-400 flex-shrink-0" />
                </button>
                <!-- The two answers anybody actually has: I have, and not yet. -->
                <div class="flex items-center gap-1 px-3 pb-2 pt-1.5 pl-8">
                    <button @click="answer(p, 'contacted')"
                        class="flex items-center gap-1 px-2 py-0.5 rounded-md text-[10px] font-medium text-green-700 dark:text-green-400 bg-green-100/70 dark:bg-green-900/25 hover:bg-green-200/70 dark:hover:bg-green-900/50 transition-colors">
                        <Check class="w-2.5 h-2.5" /> {{ $t('people.ive_been_in_touch') }}
                    </button>
                    <button @click="answer(p, 'snooze')"
                        class="flex items-center gap-1 px-2 py-0.5 rounded-md text-[10px] font-medium text-gray-600 dark:text-gray-400 hover:bg-gray-200/70 dark:hover:bg-gray-700/60 transition-colors">
                        <Clock class="w-2.5 h-2.5" /> {{ $t('people.remind_next_week') }}
                    </button>
                </div>
            </div>
        </div>

        <!-- Due Soon -->
        <div v-if="dueSoonContacts.length > 0">
            <div class="px-2 mb-1">
                <span class="text-[10px] font-bold uppercase tracking-wider text-yellow-600 dark:text-yellow-400 flex items-center gap-1">
                    <CalendarClock class="w-3 h-3" /> {{ $t('people.due_soon_count', { count: dueSoonContacts.length }) }}
                </span>
            </div>
            <button
                v-for="p in dueSoonContacts.slice(0, 3)" :key="p.id"
                @click="emit('select-person', p)"
                class="w-full text-left px-3 py-2 rounded-lg flex items-center gap-2.5 bg-yellow-50/50 dark:bg-yellow-900/10 hover:bg-yellow-100/50 dark:hover:bg-yellow-900/20 transition-colors border border-yellow-100 dark:border-yellow-900/20 mb-1"
            >
                <div class="w-2 h-2 rounded-full bg-yellow-500 flex-shrink-0"></div>
                <div class="flex-1 min-w-0">
                    <p class="text-xs font-medium truncate">{{ p.title }}</p>
                    <p class="text-[10px] text-yellow-600 dark:text-yellow-400">{{ $t('people.days_left', { days: p.daysLeft }) }}</p>
                </div>
            </button>
        </div>

        <!-- Upcoming Birthdays -->
        <div v-if="upcomingBirthdays.length > 0">
            <div class="px-2 mb-1">
                <span class="text-[10px] font-bold uppercase tracking-wider text-pink-500 flex items-center gap-1">
                    <Gift class="w-3 h-3" /> {{ $t('people.birthdays') }}
                </span>
            </div>
            <button
                v-for="p in upcomingBirthdays" :key="p.id"
                @click="emit('select-person', p)"
                class="w-full text-left px-3 py-2 rounded-lg flex items-center gap-2.5 bg-pink-50/50 dark:bg-pink-900/10 hover:bg-pink-100/50 dark:hover:bg-pink-900/20 transition-colors border border-pink-100 dark:border-pink-900/20 mb-1"
            >
                <Gift class="w-3.5 h-3.5 text-pink-500 flex-shrink-0" />
                <div class="flex-1 min-w-0">
                    <p class="text-xs font-medium truncate">{{ p.title }}</p>
                </div>
                <span class="text-[10px] font-bold" :class="p.daysUntil === 0 ? 'text-pink-600' : 'text-pink-400'">{{ formatBirthdayLabel(p.daysUntil) }}</span>
            </button>
        </div>
    </div>
</template>
