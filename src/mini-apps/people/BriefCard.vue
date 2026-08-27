<script setup lang="ts">
/**
 * What to know before you see somebody, and what has passed between you.
 *
 * Two things a contact app cannot show, for the same reason: it does not have
 * your calendar, your tasks or your accounts. This one does, because People
 * is a screen in the vault those live in rather than an app of its own.
 *
 * Everything here comes from one answer — `person_brief` — so the card, the
 * assistant and the coloured dot cannot tell three different stories.
 */
import { ref, computed, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { CalendarClock, CheckSquare, Gift, ArrowLeftRight, Cake, MessageSquare } from 'lucide-vue-next';
import { logger } from '../../utils/logger';

const props = defineProps<{ person: any }>();
const emit = defineEmits(['open-node']);

interface Brief {
    title: string;
    status: string;
    last_contact: string | null;
    days_since_contact: number | null;
    days_until_birthday: number | null;
    next_meeting: { id: string; title: string; start_at: string; days_away: number } | null;
    open_tasks: Array<{ id: string; title: string; due_date: string | null; overdue: boolean }>;
    last_interaction: { id: string; date: string; kind: string; note: string } | null;
    interaction_count: number;
    reciprocity: {
        gifts_given: number;
        gifts_received: number;
        money_out: number;
        money_in: number;
        outstanding: number;
        has_history: boolean;
    };
}

const brief = ref<Brief | null>(null);

const load = async () => {
    if (!props.person?.id) { brief.value = null; return; }
    try {
        brief.value = await invoke<Brief | null>('person_brief', { personId: props.person.id });
    } catch (e) {
        logger.error('Failed to load the brief', e);
        brief.value = null;
    }
};

watch(() => props.person?.id, load, { immediate: true });

/**
 * Whether there is anything worth interrupting the page for.
 *
 * A card that appears for everybody, saying nothing, is a card people stop
 * reading. This one appears when there is a meeting coming, work outstanding,
 * a birthday near, or money between you — and otherwise not at all.
 */
const worthShowing = computed(() => {
    const b = brief.value;
    if (!b) return false;
    return Boolean(
        b.next_meeting ||
        b.open_tasks.length > 0 ||
        (b.days_until_birthday !== null && b.days_until_birthday <= 30) ||
        b.reciprocity.outstanding !== 0
    );
});

const meetingWhen = computed(() => {
    const days = brief.value?.next_meeting?.days_away;
    if (days === undefined || days === null) return '';
    if (days === 0) return 'today';
    if (days === 1) return 'tomorrow';
    return `in ${days} days`;
});

const birthdayWhen = computed(() => {
    const days = brief.value?.days_until_birthday;
    if (days === null || days === undefined) return '';
    if (days === 0) return 'today';
    if (days === 1) return 'tomorrow';
    return `in ${days} days`;
});

const lastSeen = computed(() => {
    const days = brief.value?.days_since_contact;
    if (days === null || days === undefined) return '';
    if (days <= 0) return 'today';
    if (days === 1) return 'yesterday';
    if (days < 30) return `${days} days ago`;
    if (days < 365) return `${Math.floor(days / 30)} months ago`;
    return `${Math.floor(days / 365)} years ago`;
});

const money = (amount: number) =>
    new Intl.NumberFormat(undefined, { maximumFractionDigits: 0 }).format(Math.abs(amount));

const owed = computed(() => brief.value?.reciprocity.outstanding ?? 0);
</script>

<template>
    <div v-if="brief && worthShowing"
        class="mb-4 rounded-xl border border-blue-200 dark:border-blue-900/40 bg-blue-50/50 dark:bg-blue-900/10 overflow-hidden">

        <!-- Coming up -->
        <div v-if="brief.next_meeting" class="px-4 pt-3 pb-2">
            <button @click="emit('open-node', brief.next_meeting.id, 'event')"
                class="w-full text-left group">
                <p class="text-[10px] font-bold uppercase tracking-wider text-blue-600 dark:text-blue-400 flex items-center gap-1.5">
                    <CalendarClock class="w-3 h-3" /> {{ $t('people.coming_up') }}
                </p>
                <p class="text-sm font-medium mt-1 group-hover:text-blue-600 dark:group-hover:text-blue-400 transition-colors">
                    {{ brief.next_meeting.title }}
                    <span class="text-gray-500 dark:text-gray-400 font-normal">· {{ meetingWhen }}</span>
                </p>
            </button>
        </div>

        <!-- What you'd want to remember -->
        <div class="px-4 py-2.5 space-y-2 border-t border-blue-100 dark:border-blue-900/30"
            :class="{ 'border-t-0': !brief.next_meeting }">

            <p v-if="brief.last_interaction" class="text-xs text-gray-600 dark:text-gray-400 flex items-start gap-2">
                <MessageSquare class="w-3.5 h-3.5 mt-0.5 flex-shrink-0 text-gray-400" />
                <span>
                    <span class="text-gray-500">{{ $t('people.last_time') }} ({{ lastSeen }}):</span>
                    {{ brief.last_interaction.note || brief.last_interaction.kind }}
                </span>
            </p>
            <p v-else-if="lastSeen" class="text-xs text-gray-600 dark:text-gray-400 flex items-center gap-2">
                <MessageSquare class="w-3.5 h-3.5 flex-shrink-0 text-gray-400" />
                <span class="text-gray-500">{{ $t('people.last_in_touch') }} {{ lastSeen }}</span>
            </p>

            <div v-if="brief.open_tasks.length > 0" class="flex items-start gap-2">
                <CheckSquare class="w-3.5 h-3.5 mt-0.5 flex-shrink-0 text-gray-400" />
                <div class="flex-1 min-w-0 space-y-0.5">
                    <button v-for="task in brief.open_tasks.slice(0, 3)" :key="task.id"
                        @click="emit('open-node', task.id, 'task')"
                        class="block w-full text-left text-xs truncate hover:text-blue-600 dark:hover:text-blue-400 transition-colors"
                        :class="task.overdue ? 'text-red-600 dark:text-red-400' : 'text-gray-600 dark:text-gray-400'">
                        {{ task.title }}
                    </button>
                </div>
            </div>

            <p v-if="brief.days_until_birthday !== null && brief.days_until_birthday <= 30"
                class="text-xs text-pink-600 dark:text-pink-400 flex items-center gap-2">
                <Cake class="w-3.5 h-3.5 flex-shrink-0" />
                {{ $t('people.birthday_is') }} {{ birthdayWhen }}
            </p>
        </div>

        <!-- What has passed between you -->
        <div v-if="brief.reciprocity.has_history"
            class="px-4 py-2.5 border-t border-blue-100 dark:border-blue-900/30 flex flex-wrap items-center gap-x-4 gap-y-1.5">
            <span v-if="brief.reciprocity.gifts_given || brief.reciprocity.gifts_received"
                class="text-xs text-gray-600 dark:text-gray-400 flex items-center gap-1.5">
                <Gift class="w-3.5 h-3.5 text-gray-400" />
                {{ brief.reciprocity.gifts_given }} {{ $t('people.given') }} ·
                {{ brief.reciprocity.gifts_received }} {{ $t('people.received') }}
            </span>
            <span v-if="owed !== 0" class="text-xs flex items-center gap-1.5 font-medium"
                :class="owed > 0 ? 'text-green-700 dark:text-green-400' : 'text-orange-700 dark:text-orange-400'">
                <ArrowLeftRight class="w-3.5 h-3.5" />
                {{ owed > 0 ? $t('people.they_owe_you') : $t('people.you_owe_them') }} {{ money(owed) }}
            </span>
        </div>
    </div>
</template>
