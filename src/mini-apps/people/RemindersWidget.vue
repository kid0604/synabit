<script setup lang="ts">
import { computed } from 'vue';
import { AlertCircle, Gift, ChevronRight, CalendarClock } from 'lucide-vue-next';

const props = defineProps<{
    people: any[];
}>();

const emit = defineEmits(['select-person']);

const FREQ_DAYS: Record<string, number> = { weekly: 7, biweekly: 14, monthly: 30, quarterly: 90, yearly: 365 };

// Overdue contacts — have contact_frequency + last_contacted and are past due
const overdueContacts = computed(() => {
    const now = Date.now();
    return props.people
        .filter(p => {
            const freq = p.properties?.contact_frequency;
            const last = p.properties?.last_contacted;
            if (!freq || !last) return false;
            const daysSince = Math.floor((now - new Date(last).getTime()) / (1000 * 60 * 60 * 24));
            const threshold = FREQ_DAYS[freq] || 60;
            return daysSince > threshold;
        })
        .map(p => {
            const daysSince = Math.floor((now - new Date(p.properties.last_contacted).getTime()) / (1000 * 60 * 60 * 24));
            const threshold = FREQ_DAYS[p.properties.contact_frequency] || 60;
            const overdueDays = daysSince - threshold;
            return { ...p, daysSince, overdueDays };
        })
        .sort((a, b) => b.overdueDays - a.overdueDays);
});

// Upcoming birthdays — within 30 days
const upcomingBirthdays = computed(() => {
    const now = new Date();
    const thisYear = now.getFullYear();
    return props.people
        .filter(p => p.properties?.birthday)
        .map(p => {
            const bday = p.properties.birthday;
            // Parse birthday — could be YYYY-MM-DD or MM-DD
            const parts = bday.split('-');
            let month: number, day: number;
            if (parts.length === 3) {
                month = parseInt(parts[1]) - 1;
                day = parseInt(parts[2]);
            } else if (parts.length === 2) {
                month = parseInt(parts[0]) - 1;
                day = parseInt(parts[1]);
            } else {
                return null;
            }

            // Next birthday occurrence
            let nextBday = new Date(thisYear, month, day);
            if (nextBday.getTime() < now.getTime() - 86400000) { // past yesterday
                nextBday = new Date(thisYear + 1, month, day);
            }
            const daysUntil = Math.floor((nextBday.getTime() - now.getTime()) / (1000 * 60 * 60 * 24));
            return { ...p, nextBday, daysUntil, month, day };
        })
        .filter((p): p is NonNullable<typeof p> => p !== null && p.daysUntil >= 0 && p.daysUntil <= 30)
        .sort((a, b) => a.daysUntil - b.daysUntil);
});

// Due soon contacts — between 85% and 100% of frequency
const dueSoonContacts = computed(() => {
    const now = Date.now();
    return props.people
        .filter(p => {
            const freq = p.properties?.contact_frequency;
            const last = p.properties?.last_contacted;
            if (!freq || !last) return false;
            const daysSince = Math.floor((now - new Date(last).getTime()) / (1000 * 60 * 60 * 24));
            const threshold = FREQ_DAYS[freq] || 60;
            const ratio = daysSince / threshold;
            return ratio > 0.85 && ratio <= 1.0;
        })
        .map(p => {
            const daysSince = Math.floor((now - new Date(p.properties.last_contacted).getTime()) / (1000 * 60 * 60 * 24));
            const threshold = FREQ_DAYS[p.properties.contact_frequency] || 60;
            const daysLeft = threshold - daysSince;
            return { ...p, daysSince, daysLeft };
        })
        .sort((a, b) => a.daysLeft - b.daysLeft);
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
                    <AlertCircle class="w-3 h-3" /> Overdue ({{ overdueContacts.length }})
                </span>
            </div>
            <button
                v-for="p in overdueContacts.slice(0, 3)" :key="p.id"
                @click="emit('select-person', p)"
                class="w-full text-left px-3 py-2 rounded-lg flex items-center gap-2.5 bg-red-50/50 dark:bg-red-900/10 hover:bg-red-100/50 dark:hover:bg-red-900/20 transition-colors border border-red-100 dark:border-red-900/20 mb-1"
            >
                <div class="w-2 h-2 rounded-full bg-red-500 flex-shrink-0 animate-pulse"></div>
                <div class="flex-1 min-w-0">
                    <p class="text-xs font-medium truncate">{{ p.title }}</p>
                    <p class="text-[10px] text-red-500">{{ p.overdueDays }}d overdue</p>
                </div>
                <ChevronRight class="w-3 h-3 text-red-400 flex-shrink-0" />
            </button>
        </div>

        <!-- Due Soon -->
        <div v-if="dueSoonContacts.length > 0">
            <div class="px-2 mb-1">
                <span class="text-[10px] font-bold uppercase tracking-wider text-yellow-600 dark:text-yellow-400 flex items-center gap-1">
                    <CalendarClock class="w-3 h-3" /> Due Soon ({{ dueSoonContacts.length }})
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
                    <p class="text-[10px] text-yellow-600 dark:text-yellow-400">{{ p.daysLeft }}d left</p>
                </div>
            </button>
        </div>

        <!-- Upcoming Birthdays -->
        <div v-if="upcomingBirthdays.length > 0">
            <div class="px-2 mb-1">
                <span class="text-[10px] font-bold uppercase tracking-wider text-pink-500 flex items-center gap-1">
                    <Gift class="w-3 h-3" /> Birthdays
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
