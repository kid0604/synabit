<script setup lang="ts">
/**
 * The tray for what Syn read out of the person's notes (Nhát E).
 *
 * The content only: where this is shown is `NexusApp`'s business, and it is
 * a screen of its own now. It used to carry its own button and open a 384px
 * popover — from a row of buttons inside another popover, behind one called
 * "Look back". Reviewing is a long read, and it was given the least room on
 * the screen.
 *
 * Every row is a proposal: nothing here has reached a note until "Keep" is
 * pressed, and until then the timeline leaves it out of every answer. Reading
 * is off until turned on here, and turning it on says first what is sent where
 * (§8.6 of `docs/timeline-2026-09-17.md`). See `src-tauri/src/timeline/extract.rs`.
 */
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import { Check, Pencil, X, Loader2 } from 'lucide-vue-next';
import { logger } from '../../../utils/logger';
import { useAppLockStore } from '../../../stores/useAppLockStore';
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
const emit = defineEmits<{
    (e: 'changed'): void;
    /** Open the note this was read from, at the line it was read from. */
    (e: 'open', id: string, type: string, quote: string): void;
}>();

const appLock = useAppLockStore();

const { t } = useI18n();
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

/**
 * The proposal being edited, and what it is being edited to.
 *
 * A proposal is often nearly right — the right day, the right people, a
 * sentence that is not quite what happened. Keep-or-discard makes a nearly
 * right one either kept wrong or thrown away, and both are worse than five
 * seconds of typing.
 *
 * Only the sentence. The quote is the line in the note this was read from, and
 * it is shown exactly as it was written — see the comment in
 * `timeline_extract_review`.
 */
const editing = ref<string | null>(null);
const edited = ref('');

const startEditing = (proposal: Proposal) => {
    editing.value = proposal.id;
    edited.value = proposal.title;
};

const review = async (proposal: Proposal, accept: boolean) => {
    failure.value = '';
    // Only when it differs: sending the same sentence back would record a
    // decision the person did not make.
    const edit =
        accept && editing.value === proposal.id && edited.value.trim() !== proposal.title.trim()
            ? edited.value.trim()
            : null;
    try {
        await invoke('timeline_extract_review', {
            vaultPath: props.vaultPath,
            itemId: proposal.id,
            accept,
            nodeId: proposal.node_id,
            title: edit,
        });
        editing.value = null;
        if (status.value) status.value.proposals = status.value.proposals.filter(p => p.id !== proposal.id);
        emit('changed');
    } catch (e) {
        failure.value = errorText(e);
    }
};

/**
 * The note a proposal was read from, opened where it stands.
 *
 * # Why in the card and not by leaving
 *
 * Some of these notes are months old. A sentence a model wrote out of one is
 * not enough to say *yes, that happened, on that day, with those people* —
 * the person has to read the paragraph around it. Until now the card said
 * «From 2026-06-07, written 2026-06-07» and gave no way to see it, so keeping
 * an old proposal meant trusting it.
 *
 * Opening the note would work, but it costs your place: a queue of twenty is
 * reviewed in one pass, and leaving it for every third row is how a queue
 * stops being reviewed. So the source comes to the card. Editing the note
 * itself is still a press away, and that one is worth leaving for.
 */
type Source = 'loading' | { content: string; locked: boolean };
const sources = ref<Record<string, Source>>({});

/* Read in the script, not cast in the template: a type assertion in an
   attribute is a `{` the template compiler reads as the start of an object. */
const openedSource = (id: string) => !!sources.value[id];
const readingSource = (id: string) => sources.value[id] === 'loading';
const lockedSource = (id: string) => {
    const found = sources.value[id];
    return typeof found === 'object' && found.locked;
};
const shownSource = (id: string) => {
    const found = sources.value[id];
    return typeof found === 'object' && !found.locked;
};
const sourceText = (id: string) => {
    const found = sources.value[id];
    return typeof found === 'object' ? found.content : '';
};

const showSource = async (p: Proposal) => {
    if (sources.value[p.id]) {
        // Pressing it again folds it away.
        const next = { ...sources.value };
        delete next[p.id];
        sources.value = next;
        return;
    }
    sources.value = { ...sources.value, [p.id]: 'loading' };
    // A locked note is not read at all here. The tray shows what the vault
    // holds, and `NexusApp` hides a protected note's words in its own list
    // for the same reason; a second surface must not be the way around it.
    if (appLock.isEnabled && appLock.isNoteProtected(p.node_id)) {
        sources.value = { ...sources.value, [p.id]: { content: '', locked: true } };
        return;
    }
    try {
        const item = await invoke<{ content: string }>('get_nexus_item', {
            vaultPath: props.vaultPath,
            id: p.node_id,
        });
        sources.value = { ...sources.value, [p.id]: { content: item.content ?? '', locked: false } };
    } catch (e) {
        const next = { ...sources.value };
        delete next[p.id];
        sources.value = next;
        failure.value = errorText(e);
        logger.error('Could not read the note this was proposed from', e);
    }
};

/**
 * The note, split around the line the proposal was read from.
 *
 * Plain text and a marked run rather than rendered markdown: what is being
 * checked is *the words the model read*, and rendering would hide the very
 * markup that sometimes ends up inside a quote.
 */
const around = (p: Proposal, content: string) => {
    const quote = (p.quote ?? '').trim();
    const at = quote ? content.indexOf(quote) : -1;
    if (at < 0) return [{ text: content, hit: false }];
    return [
        { text: content.slice(0, at), hit: false },
        { text: content.slice(at, at + quote.length), hit: true },
        { text: content.slice(at + quote.length), hit: false },
    ].filter(part => part.text.length > 0);
};

/**
 * A quote with its link syntax unwrapped.
 *
 * A real vault showed why: one proposal quoted
 * «Trao đổi với [Nguyễn Lê Vũ Phương Hoàng:](synabit://person/People/77a…md)»
 * — the person's name was there, wrapped in forty characters of uuid nobody
 * can read. The words stay exactly as written; only the brackets go.
 */
const readable = (text: string | null | undefined) =>
    (text ?? '').replace(/\[([^\]]*)\]\([^)]*\)/g, '$1').replace(/\s+/g, ' ').trim();

const when = (p: Proposal) => {
    if (p.precision === 'day') return p.happened_from;
    if (p.precision === 'month') return p.happened_from.slice(0, 7);
    if (p.precision === 'year') return p.happened_from.slice(0, 4);
    return `${p.happened_from} → ${p.happened_to}`;
};

const who = (p: Proposal) => [...p.people.map(person => person.title), ...p.names].join(', ');
</script>

<template>
    <div v-if="status" data-extract-tray class="space-y-4">
        <p data-is-an-event class="rounded-lg bg-indigo-50 px-3 py-2.5 text-[13px] font-medium leading-relaxed text-indigo-900 dark:bg-indigo-950/50 dark:text-indigo-200">
            {{ $t('nexus.extract_is_an_event') }}
        </p>
        <p class="text-[13px] leading-relaxed text-gray-500 dark:text-gray-400">{{ $t('nexus.extract_explain') }}</p>

            <p v-if="failure" class="text-[11px] text-red-500">{{ failure }}</p>

            <ul v-if="status.proposals.length" data-proposals class="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
                <li
                    v-for="p in status.proposals"
                    :key="p.id"
                    data-proposal
                    class="space-y-1 rounded-lg border border-gray-200 p-2.5 dark:border-[#3a3a3c]"
                >
                    <div class="flex items-start justify-between gap-2">
                        <!-- The sentence, which is the person's to fix. -->
                        <textarea
                            v-if="editing === p.id"
                            v-model="edited"
                            data-proposal-title
                            rows="2"
                            :aria-label="$t('nexus.extract_edit')"
                            class="min-w-0 flex-grow resize-none rounded-md border border-indigo-300 bg-white px-1.5 py-1 text-xs font-semibold text-gray-900 outline-none focus:border-indigo-500 dark:border-indigo-700 dark:bg-[#1c1c1e] dark:text-gray-100"
                            @keydown.enter.prevent="review(p, true)"
                            @keydown.esc="editing = null"
                        />
                        <p v-else class="text-xs font-semibold text-gray-900 dark:text-gray-100">{{ p.title }}</p>
                        <span class="flex-shrink-0 text-[10px] tabular-nums text-gray-400">{{ Math.round(p.confidence * 100) }}%</span>
                    </div>
                    <p class="text-[11px] tabular-nums text-gray-600 dark:text-gray-300">
                        {{ when(p) }}<template v-if="who(p)"> · {{ who(p) }}</template>
                    </p>
                    <p class="text-[11px] italic text-gray-500 dark:text-gray-400">“{{ readable(p.quote) }}”</p>
                    <p class="text-[10px] text-gray-400">
                        <!-- The note it came from is a button now. It used to
                             be this sentence and nothing else, so an old
                             proposal could only be trusted or thrown away. -->
                        <button
                            type="button"
                            data-show-source
                            :aria-expanded="openedSource(p.id)"
                            class="text-left underline decoration-dotted underline-offset-2 hover:text-gray-700 dark:hover:text-gray-200"
                            @click="showSource(p)"
                        >{{ $t('nexus.extract_from', { title: p.node_title, day: p.recorded }) }}</button>
                        <span v-if="p.stale" class="ml-1 rounded bg-amber-100 px-1 text-amber-700 dark:bg-amber-900/30 dark:text-amber-300">{{ $t('nexus.extract_stale_badge') }}</span>
                    </p>

                    <!-- The note itself, with the line this was read from
                         marked in it. -->
                    <div v-if="openedSource(p.id)" data-source class="rounded-md border border-gray-200 bg-gray-50 p-2 dark:border-[#3a3a3c] dark:bg-[#1e1e20]">
                        <p v-if="readingSource(p.id)" class="text-[10px] text-gray-400">{{ $t('nexus.extract_source_reading') }}</p>
                        <p v-else-if="lockedSource(p.id)" data-source-locked class="text-[11px] italic text-gray-400">
                            {{ $t('nexus.extract_source_locked') }}
                        </p>
                        <template v-else-if="shownSource(p.id)">
                                <p class="max-h-48 overflow-y-auto whitespace-pre-wrap text-[11px] leading-relaxed text-gray-700 dark:text-gray-300">
                                    <span
                                        v-for="(part, i) in around(p, sourceText(p.id))"
                                        :key="i"
                                        :data-hit="part.hit ? 'yes' : undefined"
                                        :class="part.hit ? 'rounded bg-amber-200/70 font-medium dark:bg-amber-500/30' : ''"
                                    >{{ part.text }}</span>
                                </p>
                                <!-- Reading it here is enough to decide;
                                     changing it is worth leaving for. -->
                                <button
                                    type="button"
                                    data-open-source
                                    class="mt-1.5 text-[10px] font-semibold text-indigo-600 underline decoration-dotted underline-offset-2 dark:text-indigo-400"
                                    @click="emit('open', p.node_id, p.node_type, p.quote)"
                                >{{ $t('nexus.extract_source_open') }}</button>
                        </template>
                    </div>
                    <div class="flex gap-2 pt-0.5">
                        <button
                            type="button"
                            data-accept
                            class="flex items-center gap-1 rounded-md bg-emerald-600 px-2 py-0.5 text-[11px] font-semibold text-white hover:bg-emerald-700 disabled:opacity-40"
                            :disabled="p.stale"
                            :title="p.stale ? $t('nexus.extract_stale_keep') : undefined"
                            @click="review(p, true)"
                        ><Check class="h-3 w-3" /> {{ $t('nexus.extract_accept') }}</button>
                        <!-- Between keeping it wrong and throwing it away. -->
                        <button
                            v-if="editing !== p.id"
                            type="button"
                            data-edit
                            class="flex items-center gap-1 rounded-md px-2 py-0.5 text-[11px] text-gray-500 hover:bg-gray-100 dark:hover:bg-[#3a3a3c]"
                            @click="startEditing(p)"
                        ><Pencil class="h-3 w-3" /> {{ $t('nexus.extract_edit') }}</button>
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


        <!-- The settings, under the queue rather than over it.
             Whoever opened this came to look at what is waiting; turning
             reading on, allowing a cloud model and asking for another pass
             are things you do once and then forget. They used to sit above
             the list, so the first thing a queue of proposals showed was
             sixty lines of configuration. -->
        <details data-extract-settings class="rounded-xl border border-gray-200 dark:border-[#3a3a3c]" :open="!status.config.enabled">
            <summary class="cursor-pointer px-4 py-3 text-xs font-semibold text-gray-700 dark:text-gray-300">
                {{ $t('nexus.extract_settings') }}
            </summary>
            <div class="space-y-3 border-t border-gray-100 px-4 py-3 dark:border-[#3a3a3c]">
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

                <!-- Captions and transcripts are what stands in for a
                     picture or a recording when the extractor reads a note,
                     so they belong with the reading settings rather than
                     beside the queue. -->
                <MediaSurrogates :vault-path="vaultPath" />
            </div>
        </details>
    </div>
</template>
