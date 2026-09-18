<script setup lang="ts">
/**
 * The tray for what Syn read out of the person's notes (Nhát E).
 *
 * Every row is a proposal: nothing here has reached a note until "Keep" is
 * pressed, and until then the timeline leaves it out of every answer. Reading
 * is off until turned on here, and turning it on says first what is sent where
 * (§8.6 of `docs/timeline-2026-09-17.md`). See `src-tauri/src/timeline/extract.rs`.
 */
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import { Sparkles, Check, X, Loader2 } from 'lucide-vue-next';
import { logger } from '../../../utils/logger';
import { errorText } from '../../../shared/errorText';
import MediaSurrogates from './MediaSurrogates.vue';

interface PersonRef { id: string; title: string }

export interface Proposal {
    id: string;
    node_id: string;
    node_title: string;
    node_type: string;
    recorded: string;
    happened_from: string;
    happened_to: string;
    precision: string;
    title: string;
    people: PersonRef[];
    names: string[];
    quote: string;
    confidence: number;
    model: string;
    stale: boolean;
}

export interface ExtractStatus {
    config: { enabled: boolean; allow_cloud: boolean; folders: string[]; tags: string[]; conversations: boolean };
    syn_enabled: boolean;
    provider: string;
    local: boolean;
    model: string | null;
    desktop: boolean;
    running: boolean;
    pending: number;
    stale: number;
    old_version: number;
    done: number;
    estimate_ms: number;
    estimate_all_ms: number;
    estimate_measured: boolean;
    unreadable: string[];
    proposals: Proposal[];
}

interface ExtractRun { read: number; items: number; dropped: number; failed: string[]; remaining: number; skipped: string | null }

const props = defineProps<{ vaultPath: string }>();
const emit = defineEmits<{ (e: 'changed'): void }>();

const { t } = useI18n();
const open = ref(false);
const status = ref<ExtractStatus | null>(null);
const busy = ref(false);
const failure = ref('');
const lastRun = ref<ExtractRun | null>(null);
const allowCloud = ref(false);

const load = async () => {
    try {
        status.value = await invoke<ExtractStatus>('timeline_extract_status', { vaultPath: props.vaultPath });
        allowCloud.value = status.value.config.allow_cloud;
    } catch (e) {
        logger.error('Could not read the extraction status', e);
    }
};

onMounted(load);

const count = computed(() => status.value?.proposals.length ?? 0);

/** Reading would send notes off this machine, and that has not been allowed. */
const blocked = computed(() => !!status.value && !status.value.local && !status.value.config.allow_cloud);

const minutes = (ms: number) => Math.max(1, Math.round(ms / 60_000));

const configure = async (enabled: boolean) => {
    if (!status.value) return;
    failure.value = '';
    try {
        await invoke('timeline_extract_configure', {
            vaultPath: props.vaultPath,
            settings: { ...status.value.config, enabled, allow_cloud: allowCloud.value },
        });
        await load();
    } catch (e) {
        failure.value = errorText(e);
    }
};

const run = async (scope: 'new' | 'stale' | 'old') => {
    busy.value = true;
    failure.value = '';
    try {
        lastRun.value = await invoke<ExtractRun>('timeline_extract_run', {
            vaultPath: props.vaultPath,
            scope,
            auto: false,
            limit: null,
        });
        await load();
    } catch (e) {
        failure.value = errorText(e);
    } finally {
        busy.value = false;
    }
};

const review = async (proposal: Proposal, accept: boolean) => {
    failure.value = '';
    try {
        await invoke('timeline_extract_review', { vaultPath: props.vaultPath, itemId: proposal.id, accept, nodeId: proposal.node_id });
        if (status.value) status.value.proposals = status.value.proposals.filter(p => p.id !== proposal.id);
        emit('changed');
    } catch (e) {
        failure.value = errorText(e);
    }
};

const when = (p: Proposal) => {
    if (p.precision === 'day') return p.happened_from;
    if (p.precision === 'month') return p.happened_from.slice(0, 7);
    if (p.precision === 'year') return p.happened_from.slice(0, 4);
    return `${p.happened_from} → ${p.happened_to}`;
};

const who = (p: Proposal) => [...p.people.map(person => person.title), ...p.names].join(', ');
</script>

<template>
    <div class="relative flex-shrink-0">
        <button
            type="button"
            class="flex h-9 items-center gap-1.5 rounded-full px-3 text-xs font-semibold text-gray-600 transition-colors hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-[#3a3a3c]"
            :aria-expanded="open"
            @click="open = !open; if (open) load()"
        >
            <Sparkles class="h-3.5 w-3.5" /> {{ $t('nexus.extract_button') }}
            <span
                v-if="count"
                data-proposal-count
                class="rounded-full bg-indigo-600 px-1.5 text-[10px] font-bold tabular-nums text-white"
            >{{ count }}</span>
        </button>

        <div
            v-if="open && status"
            class="absolute bottom-full right-0 mb-3 max-h-[70vh] w-96 space-y-3 overflow-y-auto rounded-xl border border-gray-200 bg-white p-4 shadow-xl dark:border-[#3a3a3c] dark:bg-[#242426]"
        >
            <p class="text-xs font-semibold text-gray-900 dark:text-gray-100">{{ $t('nexus.extract_title') }}</p>
            <p class="text-[11px] leading-relaxed text-gray-500 dark:text-gray-400">{{ $t('nexus.extract_explain') }}</p>

            <template v-if="!status.config.enabled">
                <p v-if="status.local" class="text-[11px] leading-relaxed text-gray-600 dark:text-gray-300">
                    {{ $t('nexus.extract_local', { provider: status.provider }) }}
                </p>
                <template v-else>
                    <p data-cloud-warning class="rounded-lg bg-amber-50 p-2 text-[11px] leading-relaxed text-amber-800 dark:bg-amber-900/20 dark:text-amber-300">
                        {{ $t('nexus.extract_cloud_warning', { provider: status.provider }) }}
                    </p>
                    <label class="flex items-center gap-2 text-[11px] text-gray-700 dark:text-gray-300">
                        <input v-model="allowCloud" type="checkbox" data-allow-cloud />
                        {{ $t('nexus.extract_allow_cloud') }}
                    </label>
                </template>
                <p class="text-[11px] leading-relaxed text-gray-500 dark:text-gray-400">{{ $t('nexus.extract_unmeasured') }}</p>
                <button
                    type="button"
                    data-enable
                    class="w-full rounded-lg bg-indigo-600 py-1.5 text-xs font-semibold text-white transition-colors hover:bg-indigo-700 disabled:opacity-40"
                    :disabled="!status.local && !allowCloud"
                    @click="configure(true)"
                >{{ $t('nexus.extract_enable') }}</button>
            </template>

            <template v-else>
                <!-- Reading is on, and Syn's model has since moved off this machine. -->
                <template v-if="blocked">
                    <p data-cloud-warning class="rounded-lg bg-amber-50 p-2 text-[11px] leading-relaxed text-amber-800 dark:bg-amber-900/20 dark:text-amber-300">
                        {{ $t('nexus.extract_cloud_warning', { provider: status.provider }) }}
                    </p>
                    <label class="flex items-center gap-2 text-[11px] text-gray-700 dark:text-gray-300">
                        <input v-model="allowCloud" type="checkbox" data-allow-cloud @change="configure(true)" />
                        {{ $t('nexus.extract_allow_cloud') }}
                    </label>
                </template>
                <div class="space-y-1.5 rounded-lg bg-gray-50 p-2.5 text-[11px] text-gray-600 dark:bg-[#1e1e20] dark:text-gray-300">
                    <p>
                        {{ $t('nexus.extract_pending', { count: status.pending, minutes: minutes(status.estimate_ms) }) }}
                        <template v-if="!status.estimate_measured"> {{ $t('nexus.extract_rough') }}</template>
                        <template v-if="status.local"> · {{ $t('nexus.extract_no_cloud') }}</template>
                    </p>
                    <p v-if="!status.desktop">{{ $t('nexus.extract_phone') }}</p>
                    <div class="flex flex-wrap gap-2">
                        <button
                            type="button"
                            data-run-new
                            class="flex items-center gap-1 rounded-md bg-indigo-600 px-2.5 py-1 font-semibold text-white disabled:opacity-40"
                            :disabled="busy || status.running || blocked || status.pending === 0"
                            @click="run('new')"
                        >
                            <Loader2 v-if="busy || status.running" class="h-3 w-3 animate-spin" />
                            {{ busy || status.running ? $t('nexus.extract_running') : $t('nexus.extract_run') }}
                        </button>
                        <button
                            v-if="status.stale"
                            type="button"
                            class="rounded-md border border-gray-200 px-2.5 py-1 dark:border-[#3a3a3c]"
                            :disabled="busy || status.running || blocked"
                            @click="run('stale')"
                        >{{ $t('nexus.extract_stale', { count: status.stale }) }}</button>
                        <button
                            v-if="status.old_version"
                            type="button"
                            class="rounded-md border border-gray-200 px-2.5 py-1 dark:border-[#3a3a3c]"
                            :disabled="busy || status.running || blocked"
                            @click="run('old')"
                        >{{ $t('nexus.extract_old', { count: status.old_version }) }}</button>
                    </div>
                    <p v-if="lastRun && !lastRun.skipped">
                        {{ $t('nexus.extract_last_run', { read: lastRun.read, items: lastRun.items, dropped: lastRun.dropped }) }}
                        <template v-if="lastRun.failed.length"> · {{ $t('nexus.extract_failed', { count: lastRun.failed.length }) }}</template>
                    </p>
                    <p v-for="file in status.unreadable" :key="file" class="text-red-500">{{ file }}</p>
                    <button type="button" class="text-gray-400 underline" @click="configure(false)">{{ $t('nexus.extract_disable') }}</button>
                </div>
            </template>

            <p v-if="failure" class="text-[11px] text-red-500">{{ failure }}</p>

            <ul v-if="status.proposals.length" class="space-y-2">
                <li
                    v-for="p in status.proposals"
                    :key="p.id"
                    data-proposal
                    class="space-y-1 rounded-lg border border-gray-200 p-2.5 dark:border-[#3a3a3c]"
                >
                    <div class="flex items-start justify-between gap-2">
                        <p class="text-xs font-semibold text-gray-900 dark:text-gray-100">{{ p.title }}</p>
                        <span class="flex-shrink-0 text-[10px] tabular-nums text-gray-400">{{ Math.round(p.confidence * 100) }}%</span>
                    </div>
                    <p class="text-[11px] tabular-nums text-gray-600 dark:text-gray-300">
                        {{ when(p) }}<template v-if="who(p)"> · {{ who(p) }}</template>
                    </p>
                    <p class="text-[11px] italic text-gray-500 dark:text-gray-400">“{{ p.quote }}”</p>
                    <p class="text-[10px] text-gray-400">
                        {{ $t('nexus.extract_from', { title: p.node_title, day: p.recorded }) }}
                        <span v-if="p.stale" class="ml-1 rounded bg-amber-100 px-1 text-amber-700 dark:bg-amber-900/30 dark:text-amber-300">{{ $t('nexus.extract_stale_badge') }}</span>
                    </p>
                    <div class="flex gap-2 pt-0.5">
                        <button
                            type="button"
                            data-accept
                            class="flex items-center gap-1 rounded-md bg-emerald-600 px-2 py-0.5 text-[11px] font-semibold text-white hover:bg-emerald-700 disabled:opacity-40"
                            :disabled="p.stale"
                            :title="p.stale ? $t('nexus.extract_stale_keep') : undefined"
                            @click="review(p, true)"
                        ><Check class="h-3 w-3" /> {{ $t('nexus.extract_accept') }}</button>
                        <button
                            type="button"
                            data-decline
                            class="flex items-center gap-1 rounded-md px-2 py-0.5 text-[11px] text-gray-500 hover:bg-gray-100 dark:hover:bg-[#3a3a3c]"
                            @click="review(p, false)"
                        ><X class="h-3 w-3" /> {{ $t('nexus.extract_decline') }}</button>
                    </div>
                </li>
            </ul>
            <p v-else-if="status.config.enabled" class="text-[11px] text-gray-400">{{ t('nexus.extract_none') }}</p>

            <MediaSurrogates :vault-path="vaultPath" />
        </div>
    </div>
</template>
