import { computed, type Ref } from 'vue';

export type HealthStatus = 'thriving' | 'on_track' | 'due_soon' | 'overdue' | 'unknown';

export interface RelationshipHealth {
    status: HealthStatus;
    label: string;
    color: string;           // Tailwind text color class
    bgColor: string;         // Tailwind bg color class
    dotColor: string;        // Tailwind bg for dot
    percent: number;         // 0-100, for progress ring
    daysSinceContact: number | null;
    nextContactDue: number | null; // days until next contact is "due"
    interactionCount: number;
    relationshipAge: string; // human-readable duration
}

const FREQUENCY_DAYS: Record<string, number> = {
    weekly: 7,
    biweekly: 14,
    monthly: 30,
    quarterly: 90,
    yearly: 365,
};

export function useRelationshipHealth(person: Ref<any>) {

    const daysSinceContact = computed<number | null>(() => {
        const last = person.value?.properties?.last_contacted;
        if (!last) return null;
        return Math.floor((Date.now() - new Date(last).getTime()) / (1000 * 60 * 60 * 24));
    });

    const frequencyDays = computed<number | null>(() => {
        const freq = person.value?.properties?.contact_frequency;
        return freq ? (FREQUENCY_DAYS[freq] ?? null) : null;
    });

    const nextContactDue = computed<number | null>(() => {
        if (daysSinceContact.value === null || frequencyDays.value === null) return null;
        return frequencyDays.value - daysSinceContact.value;
    });

    const percent = computed(() => {
        if (daysSinceContact.value === null || frequencyDays.value === null) return 100;
        const ratio = 1 - (daysSinceContact.value / frequencyDays.value);
        return Math.max(0, Math.min(100, Math.round(ratio * 100)));
    });

    const status = computed<HealthStatus>(() => {
        if (daysSinceContact.value === null || frequencyDays.value === null) return 'unknown';
        const ratio = daysSinceContact.value / frequencyDays.value;
        if (ratio <= 0.5) return 'thriving';
        if (ratio <= 0.85) return 'on_track';
        if (ratio <= 1.2) return 'due_soon';
        return 'overdue';
    });

    const statusConfig: Record<HealthStatus, { label: string; color: string; bgColor: string; dotColor: string }> = {
        thriving:  { label: 'Thriving',    color: 'text-green-600 dark:text-green-400',  bgColor: 'bg-green-100 dark:bg-green-900/20',  dotColor: 'bg-green-500' },
        on_track:  { label: 'On Track',    color: 'text-blue-600 dark:text-blue-400',    bgColor: 'bg-blue-100 dark:bg-blue-900/20',    dotColor: 'bg-blue-500' },
        due_soon:  { label: 'Due Soon',    color: 'text-yellow-600 dark:text-yellow-400',bgColor: 'bg-yellow-100 dark:bg-yellow-900/20', dotColor: 'bg-yellow-500' },
        overdue:   { label: 'Overdue',     color: 'text-red-600 dark:text-red-400',      bgColor: 'bg-red-100 dark:bg-red-900/20',      dotColor: 'bg-red-500' },
        unknown:   { label: 'Not Tracked', color: 'text-gray-500 dark:text-gray-400',    bgColor: 'bg-gray-100 dark:bg-gray-800',       dotColor: 'bg-gray-400' },
    };

    const interactionCount = computed(() => {
        return person.value?.properties?.interactions?.length ?? 0;
    });

    const relationshipAge = computed(() => {
        const created = person.value?.created_at;
        if (!created) return '';
        const diffMs = Date.now() - new Date(created).getTime();
        const days = Math.floor(diffMs / (1000 * 60 * 60 * 24));
        if (days < 30) return `${days}d`;
        if (days < 365) return `${Math.floor(days / 30)}mo`;
        const years = Math.floor(days / 365);
        const months = Math.floor((days % 365) / 30);
        return months > 0 ? `${years}y ${months}mo` : `${years}y`;
    });

    const health = computed<RelationshipHealth>(() => {
        const s = status.value;
        const config = statusConfig[s];
        return {
            status: s,
            label: config.label,
            color: config.color,
            bgColor: config.bgColor,
            dotColor: config.dotColor,
            percent: percent.value,
            daysSinceContact: daysSinceContact.value,
            nextContactDue: nextContactDue.value,
            interactionCount: interactionCount.value,
            relationshipAge: relationshipAge.value,
        };
    });

    return { health, daysSinceContact, nextContactDue, status, percent };
}
