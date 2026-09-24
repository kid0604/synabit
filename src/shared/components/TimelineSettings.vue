<script setup lang="ts">
/**
 * How the vault is read into moments, and how to start the timeline again.
 *
 * # Why this is a settings screen
 *
 * It used to be a fold-out at the bottom of the review, which put the reading
 * settings — and the one button that clears the whole timeline — behind a
 * door labelled "Moment proposals". Somebody looking for either had to work
 * out that they lived inside a queue, and when the queue emptied the door
 * disappeared with them. A rare, destructive thing belongs where people go
 * looking for rare, destructive things: the settings, beside the vault's own
 * trash.
 *
 * The review keeps the queue, and nothing else.
 */
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import { Loader2 } from 'lucide-vue-next';
import { logger } from '../../utils/logger';
import { useEventBus } from '../../composables/useEventBus';
import { errorText } from '../errorText';
import { saveKinds, withKind } from '../timelineReading';
import type { ExtractRun, ExtractStatus, ReadingLeft, ResetDone, ResetPlan } from '../timelineReading';
import MediaSurrogates from '../../mini-apps/nexus/components/MediaSurrogates.vue';

const props = defineProps<{ vaultPath: string }>();
const emit = defineEmits<{ (e: 'changed'): void }>();

const { t, te } = useI18n();
// Whoever else is showing the timeline is in another tree entirely — see
// `timeline:changed`.
const bus = useEventBus();
const status = ref<ExtractStatus | null>(null);
/** How much is left to read: a second's work, so it lands after the screen. */
const left = ref<ReadingLeft | null>(null);
const busy = ref(false);
const failure = ref('');
const lastRun = ref<ExtractRun | null>(null);
const allowCloud = ref(false);

const load = async () => {
    try {
        status.value = await invoke<ExtractStatus>('timeline_extract_status', { vaultPath: props.vaultPath });
        allowCloud.value = status.value.config.allow_cloud;
    } catch (e) {
        failure.value = errorText(e);
        logger.error('Could not read the extraction status', e);
        return;
    }
    // Asked second, and never waited for: the settings above it are already
    // usable, and this is the one thing here that reads the whole vault.
    left.value = null;
    try {
        left.value = await invoke<ReadingLeft>('timeline_reading_left', { vaultPath: props.vaultPath });
    } catch (e) {
        logger.error('Could not work out what is left to read', e);
    }
};

onMounted(load);

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
        announce();
    } catch (e) {
        failure.value = errorText(e);
    } finally {
        busy.value = false;
    }
};

/** The timeline holds something different now, and two screens show it. */
const announce = () => {
    emit('changed');
    bus.emit('timeline:changed');
};

// ── the kinds a moment can be ───────────────────────────────────────────

const kinds = computed(() => status.value?.categories ?? []);
const kindName = (kind: string) => (te(`nexus.moment_category_${kind}`) ? t(`nexus.moment_category_${kind}`) : kind);
const newKind = ref('');

const keep = async (list: string[]) => {
    if (!status.value) return;
    failure.value = '';
    try {
        await saveKinds(props.vaultPath, status.value.config, list);
        status.value = { ...status.value, categories: list, config: { ...status.value.config, categories: list } };
    } catch (e) {
        failure.value = errorText(e);
    }
};

const addKind = async () => {
    const list = withKind(kinds.value, newKind.value);
    newKind.value = '';
    if (list !== kinds.value) await keep(list);
};

const dropKind = (kind: string) => keep(kinds.value.filter(k => k !== kind));

// ── starting again ──────────────────────────────────────────────────────

/**
 * Counted before it is offered, and confirmed against those same counts — a
 * moment arriving by sync between the question and the answer would make the
 * answer mean something else, and the Rust side refuses rather than guessing.
 */
const resetPlan = ref<ResetPlan | null>(null);
const resetting = ref(false);
const resetDone = ref<ResetDone | null>(null);

const askToReset = async () => {
    failure.value = '';
    resetDone.value = null;
    try {
        resetPlan.value = await invoke<ResetPlan>('timeline_reset_plan', { vaultPath: props.vaultPath });
    } catch (e) {
        failure.value = errorText(e);
    }
};

const startAgain = async () => {
    if (!resetPlan.value || resetting.value) return;
    resetting.value = true;
    failure.value = '';
    try {
        resetDone.value = await invoke<ResetDone>('timeline_reset', {
            vaultPath: props.vaultPath,
            expectMoments: resetPlan.value.moments,
        });
        resetPlan.value = null;
        await load();
        announce();
    } catch (e) {
        failure.value = errorText(e);
    } finally {
        resetting.value = false;
    }
};
</script>

<template>
    <div data-timeline-settings class="space-y-4">
        <p v-if="failure" data-settings-failed class="text-[11px] text-red-500">{{ failure }}</p>
        <template v-if="status">

        <!-- ── Reading: on or off, and what it sends where ────────────── -->
        <section class="space-y-2 rounded-xl border border-gray-200 p-3 dark:border-[#3a3a3c]">
            <h4 class="text-[13px] font-semibold text-gray-800 dark:text-gray-200">{{ $t('nexus.extract_settings') }}</h4>
            <p class="text-[12px] leading-relaxed text-gray-500 dark:text-gray-400">{{ $t('nexus.extract_explain') }}</p>

            <p v-if="status.local" class="text-[12px] leading-relaxed text-gray-600 dark:text-gray-300">
                {{ $t('nexus.extract_local', { provider: status.provider }) }}
            </p>
            <template v-else>
                <p data-cloud-warning class="rounded-lg bg-amber-50 p-2 text-[12px] leading-relaxed text-amber-800 dark:bg-amber-900/20 dark:text-amber-300">
                    {{ $t('nexus.extract_cloud_warning', { provider: status.provider }) }}
                </p>
                <label class="flex items-center gap-2 text-[12px] text-gray-700 dark:text-gray-300">
                    <input v-model="allowCloud" type="checkbox" data-allow-cloud @change="status.config.enabled && configure(true)" />
                    {{ $t('nexus.extract_allow_cloud') }}
                </label>
            </template>

            <template v-if="!status.config.enabled">
                <p class="text-[12px] leading-relaxed text-gray-500 dark:text-gray-400">{{ $t('nexus.extract_unmeasured') }}</p>
                <button
                    type="button"
                    data-enable
                    class="w-full rounded-lg bg-indigo-600 py-1.5 text-xs font-semibold text-white transition-colors hover:bg-indigo-700 disabled:opacity-40"
                    :disabled="!status.local && !allowCloud"
                    @click="configure(true)"
                >{{ $t('nexus.extract_enable') }}</button>
            </template>

            <template v-else>
                <div class="space-y-1.5 rounded-lg bg-gray-50 p-2.5 text-[12px] text-gray-600 dark:bg-[#1e1e20] dark:text-gray-300">
                    <!-- The one line here that costs a walk of the whole
                         vault. Everything above it is already usable. -->
                    <p v-if="!left" data-left-looking class="flex items-center gap-2 text-gray-400">
                        <Loader2 class="h-3 w-3 animate-spin" />
                        {{ $t('nexus.extract_looking') }}
                    </p>
                    <p v-else>
                        {{ $t('nexus.extract_pending', { count: left.pending, minutes: minutes(left.estimate_ms) }) }}
                        <template v-if="!left.estimate_measured"> {{ $t('nexus.extract_rough') }}</template>
                        <template v-if="status.local"> · {{ $t('nexus.extract_no_cloud') }}</template>
                    </p>
                    <p v-if="!status.desktop">{{ $t('nexus.extract_phone') }}</p>
                    <div class="flex flex-wrap gap-2">
                        <button
                            type="button"
                            data-run-new
                            class="flex items-center gap-1 rounded-md bg-indigo-600 px-2.5 py-1 font-semibold text-white disabled:opacity-40"
                            :disabled="busy || status.running || blocked"
                            @click="run('new')"
                        >
                            <Loader2 v-if="busy || status.running" class="h-3 w-3 animate-spin" />
                            {{ busy || status.running ? $t('nexus.extract_running') : $t('nexus.extract_run') }}
                        </button>
                        <button
                            v-if="left?.old_version"
                            type="button"
                            data-run-old
                            class="rounded-md border border-gray-200 px-2.5 py-1 dark:border-[#3a3a3c]"
                            :disabled="busy || status.running || blocked"
                            @click="run('old')"
                        >{{ $t('nexus.extract_old', { count: left.old_version }) }}</button>
                    </div>
                    <p v-if="lastRun && !lastRun.skipped">
                        {{ $t('nexus.extract_last_run', { read: lastRun.read, items: lastRun.items, dropped: lastRun.dropped }) }}
                        <template v-if="lastRun.failed.length"> · {{ $t('nexus.extract_failed', { count: lastRun.failed.length }) }}</template>
                    </p>
                    <p v-for="file in status.unreadable" :key="file" class="text-red-500">{{ file }}</p>
                </div>
                <button type="button" data-disable class="text-[12px] text-gray-400 underline" @click="configure(false)">
                    {{ $t('nexus.extract_disable') }}
                </button>
            </template>
        </section>

        <!-- ── The kinds a moment can be, whether reading is on or not ── -->
        <section data-kinds class="space-y-2 rounded-xl border border-gray-200 p-3 dark:border-[#3a3a3c]">
            <h4 class="text-[13px] font-semibold text-gray-800 dark:text-gray-200">{{ $t('nexus.extract_kinds') }}</h4>
            <p class="text-[12px] leading-relaxed text-gray-500 dark:text-gray-400">{{ $t('nexus.extract_kinds_explain') }}</p>
            <div class="flex flex-wrap items-center gap-1">
                <span
                    v-for="kind in kinds"
                    :key="kind"
                    data-kind
                    class="flex items-center gap-1 rounded-full bg-gray-100 px-2 py-0.5 text-[12px] dark:bg-[#2c2c2e]"
                >
                    {{ kindName(kind) }}
                    <button
                        v-if="kind !== 'other'"
                        type="button"
                        data-drop-kind
                        :aria-label="$t('nexus.extract_drop_kind', { kind: kindName(kind) })"
                        class="text-gray-400 hover:text-red-500"
                        @click="dropKind(kind)"
                    >×</button>
                </span>
                <input
                    v-model="newKind"
                    data-add-kind
                    type="text"
                    :placeholder="$t('nexus.extract_new_kind')"
                    :aria-label="$t('nexus.extract_new_kind')"
                    class="h-7 w-32 rounded border border-gray-200 bg-white px-1.5 text-[12px] dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-100"
                    @keydown.enter.prevent="addKind()"
                />
            </div>
        </section>

        <!-- Captions and transcripts stand in for a picture or a recording
             when the reader reads a note, so they belong with reading. -->
        <section class="rounded-xl border border-gray-200 p-3 dark:border-[#3a3a3c]">
            <MediaSurrogates :vault-path="vaultPath" />
        </section>

        <!-- ── Starting again: counted first, two presses, nothing destroyed ── -->
        <section data-reset class="space-y-2 rounded-xl border border-red-200 p-3 dark:border-red-900/40">
            <h4 class="text-[13px] font-semibold text-red-700 dark:text-red-400">{{ $t('nexus.reset_title') }}</h4>
            <p class="text-[12px] leading-relaxed text-gray-500 dark:text-gray-400">{{ $t('nexus.reset_explain') }}</p>
            <p v-if="resetDone" data-reset-done class="text-[12px] text-gray-600 dark:text-gray-300">
                {{ $t('nexus.reset_done', { moments: resetDone.moments, kept: resetDone.surrogates_kept }) }}
            </p>
            <template v-if="resetPlan">
                <p data-reset-plan class="text-[12px] leading-relaxed text-gray-700 dark:text-gray-200">
                    {{ $t('nexus.reset_counts', {
                        moments: resetPlan.moments,
                        proposals: resetPlan.proposals,
                        readings: resetPlan.readings,
                        decisions: resetPlan.decisions,
                    }) }}
                </p>
                <p class="text-[12px] text-gray-500 dark:text-gray-400">{{ $t('nexus.reset_safe') }}</p>
                <div class="flex items-center gap-2">
                    <button
                        type="button"
                        data-reset-confirm
                        class="flex items-center gap-1 rounded-md bg-red-600 px-2.5 py-1 text-[12px] font-semibold text-white hover:bg-red-700 disabled:opacity-40"
                        :disabled="resetting"
                        @click="startAgain()"
                    >
                        <Loader2 v-if="resetting" class="h-3 w-3 animate-spin" />
                        {{ $t('nexus.reset_confirm') }}
                    </button>
                    <button type="button" class="text-[12px] text-gray-500 hover:text-gray-700 dark:hover:text-gray-300" @click="resetPlan = null">
                        {{ $t('nexus.reset_cancel') }}
                    </button>
                </div>
            </template>
            <button
                v-else
                type="button"
                data-reset-ask
                class="rounded-md border border-red-300 px-2.5 py-1 text-[12px] font-semibold text-red-700 hover:bg-red-50 dark:border-red-900/60 dark:text-red-400 dark:hover:bg-red-900/20"
                @click="askToReset()"
            >{{ $t('nexus.reset_ask') }}</button>
        </section>
        </template>

        <!-- Not blank while it looks. Working out what is left to read walks
             every note in the vault, and a panel that shows nothing at all
             for a second reads as a panel that is broken. -->
        <section v-else data-settings-looking class="space-y-2 rounded-xl border border-gray-200 p-3 dark:border-[#3a3a3c]">
            <h4 class="text-[13px] font-semibold text-gray-800 dark:text-gray-200">{{ $t('nexus.extract_settings') }}</h4>
            <p class="text-[12px] leading-relaxed text-gray-500 dark:text-gray-400">{{ $t('nexus.extract_explain') }}</p>
            <p class="flex items-center gap-2 text-[12px] text-gray-400">
                <Loader2 class="h-3 w-3 animate-spin" />
                {{ $t('nexus.extract_looking') }}
            </p>
        </section>
    </div>
</template>
