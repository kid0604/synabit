<script setup lang="ts">
import { computed, toRef } from 'vue';
import { Heart, Calendar, Globe, ExternalLink, Gift, Sparkles, Bell } from 'lucide-vue-next';
import { openUrl } from '@tauri-apps/plugin-opener';
import { useRelationshipHealth } from './composables/useRelationshipHealth';

const props = defineProps<{
    person: any;
    allDebts?: any[];
}>();

const personRef = toRef(props, 'person');
const { health } = useRelationshipHealth(personRef);

const howWeMet = computed(() => {
    const d = props.person?.properties?.details?.find((d: any) => d.label.toLowerCase().includes('how we met'));
    return d?.value || props.person?.properties?.how_we_met || '';
});
const relationshipType = computed(() => props.person?.properties?.relationship_type || '');
const contactFrequency = computed(() => props.person?.properties?.contact_frequency || '');

// Social links from details[] or legacy social object
const socialItems = computed(() => {
    const items: Array<{ label: string; url: string; color: string }> = [];
    const details = props.person?.properties?.details || [];
    const colorMap: Record<string, string> = {
        linkedin: 'text-blue-600 dark:text-blue-400',
        twitter: 'text-sky-500',
        github: 'text-gray-700 dark:text-gray-300',
        website: 'text-purple-600 dark:text-purple-400',
    };
    for (const d of details) {
        if (d.type === 'url') {
            const key = d.label.toLowerCase();
            items.push({ label: d.label, url: d.value, color: colorMap[key] || 'text-blue-500' });
        }
    }
    // Legacy fallback
    if (items.length === 0) {
        const s = props.person?.properties?.social || {};
        if (s.linkedin) items.push({ label: 'LinkedIn', url: s.linkedin, color: colorMap.linkedin });
        if (s.twitter) items.push({ label: 'Twitter / X', url: s.twitter, color: colorMap.twitter });
        if (s.github) items.push({ label: 'GitHub', url: s.github, color: colorMap.github });
        if (s.website) items.push({ label: 'Website', url: s.website, color: colorMap.website });
    }
    return items;
});
const hasSocial = computed(() => socialItems.value.length > 0);

const importantDates = computed(() => props.person?.properties?.important_dates || []);
const gifts = computed(() => props.person?.properties?.gifts || []);
const recentGifts = computed(() => gifts.value.slice(0, 5));

// All notable dates (birthday + important_dates) with countdown
const upcomingDates = computed(() => {
    const now = new Date();
    const thisYear = now.getFullYear();
    const dates: Array<{ label: string; date: string; daysUntil: number | null; isUpcoming: boolean }> = [];

    // Birthday
    const bday = props.person?.properties?.birthday;
    if (bday) {
        const daysUntil = getDaysUntilAnnual(bday, now, thisYear);
        dates.push({ label: '🎂 Birthday', date: bday, daysUntil, isUpcoming: daysUntil !== null && daysUntil >= 0 && daysUntil <= 30 });
    }

    // Important dates
    for (const d of importantDates.value) {
        const daysUntil = getDaysUntilAnnual(d.date, now, thisYear);
        dates.push({ label: d.label, date: d.date, daysUntil, isUpcoming: daysUntil !== null && daysUntil >= 0 && daysUntil <= 30 });
    }

    // Sort: upcoming first (by daysUntil ASC), then the rest
    dates.sort((a, b) => {
        if (a.isUpcoming && !b.isUpcoming) return -1;
        if (!a.isUpcoming && b.isUpcoming) return 1;
        if (a.isUpcoming && b.isUpcoming) return (a.daysUntil ?? 999) - (b.daysUntil ?? 999);
        return 0;
    });

    return dates;
});

const getDaysUntilAnnual = (dateStr: string, now: Date, thisYear: number): number | null => {
    if (!dateStr) return null;
    const parts = dateStr.split('-');
    let month: number, day: number;
    if (parts.length === 3) {
        month = parseInt(parts[1]) - 1;
        day = parseInt(parts[2]);
    } else if (parts.length === 2) {
        month = parseInt(parts[0]) - 1;
        day = parseInt(parts[1]);
    } else return null;

    let next = new Date(thisYear, month, day);
    if (next.getTime() < now.getTime() - 86400000) {
        next = new Date(thisYear + 1, month, day);
    }
    return Math.floor((next.getTime() - now.getTime()) / (1000 * 60 * 60 * 24));
};

const hasUpcomingDates = computed(() => upcomingDates.value.length > 0);

// --- Finance Data ---
const personDebts = computed(() => {
    if (!props.allDebts || !props.person) return [];
    return props.allDebts.filter(d => {
        if (d.personId && d.personId === props.person.id) return true;
        if (!d.personId && d.person && d.person.toLowerCase() === props.person.title.toLowerCase()) return true;
        return false;
    }).sort((a, b) => new Date(b.startDate).getTime() - new Date(a.startDate).getTime());
});

const totalLent = computed(() => {
    return personDebts.value.filter(d => d.type === 'lend').reduce((sum, d) => sum + (d.totalAmount - d.paidAmount), 0);
});

const totalBorrowed = computed(() => {
    return personDebts.value.filter(d => d.type === 'borrow').reduce((sum, d) => sum + (d.totalAmount - d.paidAmount), 0);
});

const hasDebts = computed(() => personDebts.value.length > 0);

const formatCurrency = (amount: number) => {
    return new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD' }).format(amount);
};
// --------------------

const formatDate = (dateStr: string) => {
    if (!dateStr) return '';
    return new Date(dateStr).toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' });
};

const formatCountdown = (days: number | null) => {
    if (days === null) return '';
    if (days === 0) return 'Today!';
    if (days === 1) return 'Tomorrow';
    return `in ${days}d`;
};

const openLink = async (url: string) => {
    if (!url) return;
    const fullUrl = url.startsWith('http') ? url : `https://${url}`;
    try { await openUrl(fullUrl); } catch (_) { /* ignore */ }
};



const hasOverviewContent = computed(() => {
    return howWeMet.value || relationshipType.value || hasSocial.value || hasUpcomingDates.value || recentGifts.value.length > 0 || contactFrequency.value || health.value.status !== 'unknown' || hasDebts.value;
});
</script>

<template>
    <div class="space-y-6">
        <!-- Relationship Strength Card -->
        <div v-if="health.status !== 'unknown'" :class="['rounded-2xl p-5 border', health.bgColor, health.status === 'overdue' ? 'border-red-200 dark:border-red-900/30' : health.status === 'due_soon' ? 'border-yellow-200 dark:border-yellow-900/30' : 'border-transparent']">
            <div class="flex items-center gap-5">
                <!-- Large Progress Ring -->
                <div class="relative w-20 h-20 flex-shrink-0">
                    <svg class="w-20 h-20 -rotate-90" viewBox="0 0 36 36">
                        <circle cx="18" cy="18" r="15.5" fill="none" stroke="currentColor" stroke-width="2" class="text-gray-200 dark:text-gray-700" />
                        <circle cx="18" cy="18" r="15.5" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"
                            :class="health.color"
                            :stroke-dasharray="`${health.percent * 0.975} 100`"
                            style="transition: stroke-dasharray 0.6s ease" />
                    </svg>
                    <div class="absolute inset-0 flex flex-col items-center justify-center">
                        <span class="text-lg font-bold" :class="health.color">{{ health.percent }}</span>
                        <span class="text-[8px] text-gray-400 uppercase tracking-wider">score</span>
                    </div>
                </div>

                <div class="flex-1 min-w-0">
                    <h3 class="text-lg font-bold mb-1" :class="health.color">{{ health.label }}</h3>
                    <div class="grid grid-cols-2 gap-x-6 gap-y-1 text-xs text-gray-600 dark:text-gray-400">
                        <div v-if="health.daysSinceContact !== null">
                            <span class="text-gray-400">Last contact:</span>
                            <span class="ml-1 font-medium" :class="health.color">{{ health.daysSinceContact }}d ago</span>
                        </div>
                        <div v-if="health.nextContactDue !== null">
                            <span class="text-gray-400">Next due:</span>
                            <span class="ml-1 font-medium" :class="health.nextContactDue <= 0 ? 'text-red-500' : ''">
                                {{ health.nextContactDue > 0 ? `in ${health.nextContactDue}d` : `${Math.abs(health.nextContactDue)}d overdue` }}
                            </span>
                        </div>
                        <div v-if="health.interactionCount > 0">
                            <span class="text-gray-400">Interactions:</span>
                            <span class="ml-1 font-medium">{{ health.interactionCount }}</span>
                        </div>

                    </div>
                </div>
            </div>
        </div>

        <!-- Quick Stats (only if no health tracking — show basic stats) -->
        <div v-else-if="health.interactionCount > 0" class="grid grid-cols-2 md:grid-cols-3 gap-3">
            <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-xl p-3 text-center">
                <p class="text-xs text-gray-500 dark:text-gray-400 mb-1">Interactions</p>
                <p class="text-lg font-bold text-gray-800 dark:text-gray-100">{{ health.interactionCount }}</p>
            </div>
        </div>

        <!-- Financial Summary -->
        <div v-if="hasDebts" class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-xl p-5">
            <h3 class="text-sm font-semibold text-gray-600 dark:text-gray-300 mb-4 flex items-center gap-2">
                <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4 text-blue-500"><path d="M20 12V8H6a2 2 0 0 1-2-2c0-1.1.9-2 2-2h12v4"/><path d="M4 6v12c0 1.1.9 2 2 2h14v-4"/><path d="M18 12a2 2 0 0 0-2 2c0 1.1.9 2 2 2h4v-4h-4z"/></svg>
                Financial Summary
            </h3>
            
            <div class="grid grid-cols-2 gap-4 mb-5">
                <div class="bg-green-50 dark:bg-green-900/10 border border-green-100 dark:border-green-900/20 rounded-xl p-3 text-center">
                    <p class="text-xs font-semibold text-green-600 dark:text-green-400 uppercase tracking-wider mb-1">Total Lent</p>
                    <p class="text-lg font-bold text-green-700 dark:text-green-300">{{ formatCurrency(totalLent) }}</p>
                </div>
                <div class="bg-red-50 dark:bg-red-900/10 border border-red-100 dark:border-red-900/20 rounded-xl p-3 text-center">
                    <p class="text-xs font-semibold text-red-600 dark:text-red-400 uppercase tracking-wider mb-1">Total Borrowed</p>
                    <p class="text-lg font-bold text-red-700 dark:text-red-300">{{ formatCurrency(totalBorrowed) }}</p>
                </div>
            </div>

            <div class="space-y-2">
                <div v-for="debt in personDebts" :key="debt.id" class="flex flex-col sm:flex-row sm:items-center justify-between gap-2 text-sm py-2 px-3 rounded-lg bg-gray-50 dark:bg-gray-800/50 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors">
                    <div class="flex items-center gap-2">
                        <span class="w-2 h-2 rounded-full" :class="debt.status === 'completed' ? 'bg-gray-400' : (debt.type === 'lend' ? 'bg-green-500' : 'bg-red-500')"></span>
                        <div class="flex flex-col">
                            <span class="font-medium text-gray-800 dark:text-gray-200" :class="{ 'line-through text-gray-400 dark:text-gray-500': debt.status === 'completed' }">
                                {{ debt.type === 'lend' ? 'Lent' : 'Borrowed' }}: {{ formatCurrency(debt.totalAmount) }}
                            </span>
                            <span v-if="debt.note" class="text-xs text-gray-500">{{ debt.note }}</span>
                        </div>
                    </div>
                    <div class="flex items-center gap-3 text-xs">
                        <span class="text-gray-500">{{ formatDate(debt.startDate) }}</span>
                        <span v-if="debt.status === 'active'" class="px-2 py-0.5 rounded-full font-medium" :class="debt.type === 'lend' ? 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-300' : 'bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-300'">
                            Remaining: {{ formatCurrency(debt.totalAmount - debt.paidAmount) }}
                        </span>
                        <span v-else class="px-2 py-0.5 rounded-full bg-gray-200 dark:bg-gray-700 text-gray-600 dark:text-gray-400 font-medium">Paid</span>
                    </div>
                </div>
            </div>
        </div>

        <!-- Empty state -->
        <div v-if="!hasOverviewContent" class="text-center py-10 bg-gray-50 dark:bg-gray-800/50 rounded-xl border border-dashed border-gray-300 dark:border-gray-700">
            <Sparkles class="w-10 h-10 mx-auto mb-3 text-gray-300 dark:text-gray-600" />
            <p class="text-gray-500 dark:text-gray-400 font-medium">No relationship details yet.</p>
            <p class="text-xs text-gray-400 mt-1">Edit this contact to add how you met, social links, and more.</p>
        </div>

        <!-- How We Met -->
        <div v-if="howWeMet" class="bg-amber-50/50 dark:bg-amber-900/10 border border-amber-200 dark:border-amber-900/30 rounded-xl p-5">
            <h3 class="text-sm font-semibold text-amber-700 dark:text-amber-400 mb-2 flex items-center gap-2">
                <Heart class="w-4 h-4" /> How We Met
            </h3>
            <p class="text-sm text-gray-700 dark:text-gray-300">{{ howWeMet }}</p>
        </div>

        <!-- Social Links -->
        <div v-if="hasSocial" class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-xl p-5">
            <h3 class="text-sm font-semibold text-gray-600 dark:text-gray-300 mb-3 flex items-center gap-2">
                <Globe class="w-4 h-4" /> Social Links
            </h3>
            <div class="grid grid-cols-2 gap-2">
                <button v-for="item in socialItems" :key="item.label" @click="openLink(item.url)"
                    class="flex items-center gap-2 px-3 py-2 rounded-lg bg-gray-50 dark:bg-gray-800 hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors text-sm">
                    <span :class="item.color" class="font-medium">{{ item.label }}</span>
                    <ExternalLink class="w-3 h-3 text-gray-400 ml-auto" />
                </button>
            </div>
        </div>

        <!-- Important Dates -->
        <div v-if="hasUpcomingDates" class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-xl p-5">
            <h3 class="text-sm font-semibold text-gray-600 dark:text-gray-300 mb-3 flex items-center gap-2">
                <Calendar class="w-4 h-4" /> Important Dates
            </h3>
            <div class="space-y-2">
                <div v-for="(d, i) in upcomingDates" :key="i"
                    :class="['flex items-center justify-between text-sm py-1.5 px-2 rounded-lg transition-colors',
                        d.isUpcoming ? 'bg-pink-50/50 dark:bg-pink-900/10 border border-pink-100 dark:border-pink-900/20' : '']"
                >
                    <div class="flex items-center gap-2">
                        <Bell v-if="d.isUpcoming" class="w-3.5 h-3.5 text-pink-500 flex-shrink-0 animate-pulse" />
                        <span class="text-gray-700 dark:text-gray-300">{{ d.label }}</span>
                    </div>
                    <div class="flex items-center gap-2">
                        <span v-if="d.isUpcoming" class="text-xs font-bold px-2 py-0.5 rounded-full"
                            :class="d.daysUntil === 0 ? 'bg-pink-500 text-white' : d.daysUntil === 1 ? 'bg-pink-100 text-pink-700 dark:bg-pink-900/30 dark:text-pink-300' : 'bg-yellow-100 text-yellow-700 dark:bg-yellow-900/30 dark:text-yellow-300'">
                            {{ formatCountdown(d.daysUntil) }}
                        </span>
                        <span class="text-gray-500 dark:text-gray-400 font-mono text-xs">{{ formatDate(d.date) }}</span>
                    </div>
                </div>
            </div>
        </div>

        <!-- Recent Gifts -->
        <div v-if="recentGifts.length > 0" class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-xl p-5">
            <h3 class="text-sm font-semibold text-gray-600 dark:text-gray-300 mb-3 flex items-center gap-2">
                <Gift class="w-4 h-4 text-pink-500" /> Gifts
            </h3>
            <div class="space-y-2">
                <div v-for="g in recentGifts" :key="g.id" class="flex items-center gap-3 text-sm py-1.5 border-b border-gray-100 dark:border-gray-800 last:border-0">
                    <span class="px-1.5 py-0.5 text-xs rounded-md" :class="g.direction === 'given' ? 'bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-300' : 'bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-300'">
                        {{ g.direction === 'given' ? '→' : '←' }}
                    </span>
                    <span class="flex-1 text-gray-700 dark:text-gray-300">{{ g.description }}</span>
                    <span v-if="g.occasion" class="text-xs text-gray-400 italic">{{ g.occasion }}</span>
                    <span class="text-xs text-gray-400">{{ formatDate(g.date) }}</span>
                </div>
            </div>
        </div>
    </div>
</template>
