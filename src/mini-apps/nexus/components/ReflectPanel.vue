<script setup lang="ts">
/**
 * Chiêm nghiệm: decisions, what was expected of them, and what happened (Nhát F).
 *
 * Each decision keeps three marks — when it was made, when to look again, and
 * what actually happened — and asks for the third on the day of the second.
 * A pattern across decisions is shown only as Rust hands it over: resting on
 * at least three of them, each sentence citing, nothing that reads as advice,
 * nothing sealed. See `src-tauri/src/timeline/reflect.rs`.
 *
 * Switched off, this shows the switch and nothing else.
 */
import { computed, onMounted, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import { Scale, Loader2, ChevronDown, ChevronRight } from 'lucide-vue-next';
import { logger } from '../../../utils/logger';
import { errorText } from '../../../shared/errorText';

export interface Review { on: string; happened: string; outcome: string | null }

export interface Decision {
    id: string;
    title: string;
    decided_on: string;
    expected: string;
    review_on: string | null;
    reasoning: string;
    tags: string[];
    reviews: Review[];
    due: boolean;
}

export interface Group {
    tag: string;
    cases: string[];
    tally: { as_expected: number; partly: number; otherwise: number; unrated: number };
    enough: boolean;
}

export interface Overview { enabled: boolean; decisions: Decision[]; groups: Group[] }

interface Source { n: number; node_id: string; node_type: string; title: string; date: string; what: string }

export interface Pattern {
    tag: string;
    sentences: { text: string; sources: number[] }[];
    sources: Source[];
    dropped: number;
    advice_dropped: number;
    cited: number;
    withheld: string | null;
}

const props = withDefaults(defineProps<{
    vaultPath: string;
    /** A decision to open on, from a reminder. */
    focus?: string | null;
    /** Which way the panel opens from its button. */
    align?: 'left' | 'right';
}>(), { focus: null, align: 'right' });

const emit = defineEmits<{ (e: 'changed'): void }>();

const { t, locale } = useI18n();

const OUTCOMES = ['as_expected', 'partly', 'otherwise'] as const;
const DAY = /^\d{4}-\d{2}-\d{2}$/;
const FIELD = 'w-full rounded-md border border-gray-200 bg-white px-2 py-1 text-[11px] text-gray-800 placeholder:text-gray-400 focus:border-indigo-400 focus:outline-none dark:border-[#3a3a3c] dark:bg-[#1e1e20] dark:text-gray-200';
const LABEL = 'text-[10px] font-semibold uppercase tracking-wide text-gray-400';
const PRIMARY = 'flex items-center gap-1 rounded-md bg-indigo-600 px-2.5 py-1 text-[11px] font-semibold text-white hover:bg-indigo-700 disabled:opacity-40';

const open = ref(false);
const overview = ref<Overview | null>(null);
const failure = ref('');
const expanded = ref<string | null>(null);
const answers = ref<Record<string, { happened: string; outcome: string | null; next: string }>>({});

const load = async () => {
    try {
        overview.value = await invoke<Overview>('reflect_overview', { vaultPath: props.vaultPath });
        for (const decision of overview.value.decisions.filter(d => d.due)) {
            if (!answers.value[decision.id]) answers.value[decision.id] = { happened: '', outcome: null, next: '' };
        }
    } catch (e) {
        logger.error('Could not read decisions', e);
    }
};

onMounted(load);

watch(() => props.focus, async id => {
    if (!id) return;
    open.value = true;
    await load();
    expanded.value = id;
}, { immediate: true });

const due = computed(() => overview.value?.decisions.filter(d => d.due) ?? []);

const outcomeLabel = (outcome: string | null) => (outcome ? t(`nexus.reflect_outcome_${outcome}`) : '');

const setEnabled = async (enabled: boolean) => {
    failure.value = '';
    try {
        await invoke('reflect_configure', { vaultPath: props.vaultPath, enabled });
        patterns.value = {};
        await load();
        emit('changed');
    } catch (e) {
        failure.value = errorText(e);
    }
};

const saveReview = async (decision: Decision) => {
    const answer = answers.value[decision.id];
    if (!answer?.happened.trim()) return;
    const next = answer.next.trim();
    if (next && !DAY.test(next)) {
        failure.value = t('nexus.reflect_bad_day');
        return;
    }
    failure.value = '';
    try {
        await invoke('reflect_add_review', {
            vaultPath: props.vaultPath,
            id: decision.id,
            happened: answer.happened,
            outcome: answer.outcome,
            nextReviewOn: next || null,
        });
        delete answers.value[decision.id];
        await load();
        emit('changed');
    } catch (e) {
        failure.value = errorText(e);
    }
};

const writing = ref(false);
const draft = ref({ title: '', expected: '', reasoning: '', reviewOn: '', tags: '' });
const draftValid = computed(() => {
    const reviewOn = draft.value.reviewOn.trim();
    return draft.value.title.trim().length > 0 && (!reviewOn || DAY.test(reviewOn));
});

const create = async () => {
    if (!draftValid.value) return;
    failure.value = '';
    try {
        await invoke('reflect_create_decision', {
            vaultPath: props.vaultPath,
            title: draft.value.title,
            expected: draft.value.expected,
            reasoning: draft.value.reasoning,
            reviewOn: draft.value.reviewOn.trim() || null,
            tags: draft.value.tags.split(',').map(tag => tag.trim()).filter(Boolean),
        });
        draft.value = { title: '', expected: '', reasoning: '', reviewOn: '', tags: '' };
        writing.value = false;
        await load();
        emit('changed');
    } catch (e) {
        failure.value = errorText(e);
    }
};

const patterns = ref<Record<string, Pattern>>({});
const looking = ref<string | null>(null);

const lookFor = async (tag: string) => {
    looking.value = tag;
    failure.value = '';
    try {
        const pattern = await invoke<Pattern>('reflect_pattern', { vaultPath: props.vaultPath, tag, locale: locale.value });
        patterns.value = { ...patterns.value, [tag]: pattern };
    } catch (e) {
        failure.value = errorText(e);
    } finally {
        looking.value = null;
    }
};

const openCase = (pattern: Pattern, n: number) => {
    const source = pattern.sources.find(s => s.n === n);
    if (source) expanded.value = source.node_id;
};
</script>

<template>
    <div class="relative flex-shrink-0">
        <button
            type="button"
            class="flex h-9 items-center gap-1.5 rounded-full px-3 text-xs font-semibold text-gray-600 transition-colors hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-[#3a3a3c]"
            :class="align === 'left' ? 'border border-gray-200 bg-white/80 shadow-lg backdrop-blur-md dark:border-[#3a3a3c] dark:bg-[#242426]/80' : ''"
            :aria-expanded="open"
            @click="open = !open; if (open) load()"
        >
            <Scale class="h-3.5 w-3.5" /> {{ $t('nexus.reflect_button') }}
            <span
                v-if="due.length"
                data-due-count
                class="rounded-full bg-amber-500 px-1.5 text-[10px] font-bold tabular-nums text-white"
            >{{ due.length }}</span>
        </button>

        <div
            v-if="open && overview"
            data-reflect-panel
            class="absolute bottom-full z-30 mb-3 max-h-[70vh] w-96 max-w-[calc(100vw-2rem)] space-y-4 overflow-y-auto rounded-xl border border-gray-200 bg-white p-4 shadow-xl dark:border-[#3a3a3c] dark:bg-[#242426]"
            :class="align === 'left' ? 'left-0' : 'right-0'"
        >
            <div class="space-y-1">
                <p class="text-xs font-semibold text-gray-900 dark:text-gray-100">{{ $t('nexus.reflect_title') }}</p>
                <p class="text-[11px] leading-relaxed text-gray-500 dark:text-gray-400">{{ $t('nexus.reflect_explain') }}</p>
            </div>

            <template v-if="!overview.enabled">
                <p class="text-[11px] leading-relaxed text-gray-600 dark:text-gray-300">{{ $t('nexus.reflect_off') }}</p>
                <button type="button" data-reflect-enable :class="PRIMARY" @click="setEnabled(true)">{{ $t('nexus.reflect_enable') }}</button>
            </template>

            <template v-else>
                <section v-if="due.length" class="space-y-2">
                    <p :class="LABEL">{{ $t('nexus.reflect_due') }}</p>
                    <div
                        v-for="d in due"
                        :key="d.id"
                        data-due
                        class="space-y-1.5 rounded-lg border border-amber-200 bg-amber-50/60 p-2.5 dark:border-amber-900/40 dark:bg-amber-900/10"
                    >
                        <p class="text-xs font-semibold text-gray-900 dark:text-gray-100">{{ d.title }}</p>
                        <p class="text-[11px] tabular-nums text-gray-500 dark:text-gray-400">{{ $t('nexus.reflect_decided', { day: d.decided_on }) }}</p>
                        <p v-if="d.expected" class="text-[11px] italic text-gray-600 dark:text-gray-300">{{ $t('nexus.reflect_you_expected', { expected: d.expected }) }}</p>
                        <template v-if="answers[d.id]">
                            <textarea
                                v-model="answers[d.id].happened"
                                data-happened
                                rows="3"
                                :class="FIELD"
                                :placeholder="$t('nexus.reflect_what_happened')"
                            />
                            <div class="flex flex-wrap gap-1" role="group" :aria-label="$t('nexus.reflect_compared')">
                                <button
                                    v-for="o in OUTCOMES"
                                    :key="o"
                                    type="button"
                                    :data-outcome="o"
                                    :aria-pressed="answers[d.id].outcome === o"
                                    class="rounded-md border px-2 py-0.5 text-[11px]"
                                    :class="answers[d.id].outcome === o
                                        ? 'border-indigo-500 bg-indigo-50 text-indigo-700 dark:bg-indigo-900/30 dark:text-indigo-300'
                                        : 'border-gray-200 text-gray-600 dark:border-[#3a3a3c] dark:text-gray-300'"
                                    @click="answers[d.id].outcome = answers[d.id].outcome === o ? null : o"
                                >{{ outcomeLabel(o) }}</button>
                            </div>
                            <input
                                v-model="answers[d.id].next"
                                type="text"
                                inputmode="numeric"
                                placeholder="YYYY-MM-DD"
                                :aria-label="$t('nexus.reflect_next')"
                                :class="FIELD"
                            />
                            <p class="text-[10px] text-gray-400">{{ $t('nexus.reflect_next') }}</p>
                            <button
                                type="button"
                                data-save-review
                                :class="PRIMARY"
                                :disabled="!answers[d.id].happened.trim()"
                                @click="saveReview(d)"
                            >{{ $t('nexus.reflect_save') }}</button>
                        </template>
                    </div>
                </section>

                <section class="space-y-2">
                    <button
                        type="button"
                        data-write
                        class="flex items-center gap-1 text-[11px] font-semibold text-indigo-600 dark:text-indigo-400"
                        :aria-expanded="writing"
                        @click="writing = !writing"
                    >
                        <component :is="writing ? ChevronDown : ChevronRight" class="h-3 w-3" /> {{ $t('nexus.reflect_write') }}
                    </button>
                    <div v-if="writing" class="space-y-1.5">
                        <input v-model="draft.title" data-draft-title type="text" :class="FIELD" :placeholder="$t('nexus.reflect_draft_title')" />
                        <textarea v-model="draft.reasoning" rows="3" :class="FIELD" :placeholder="$t('nexus.reflect_draft_reasoning')" />
                        <textarea v-model="draft.expected" rows="2" :class="FIELD" :placeholder="$t('nexus.reflect_draft_expected')" />
                        <input v-model="draft.reviewOn" data-draft-review type="text" inputmode="numeric" :class="FIELD" :placeholder="$t('nexus.reflect_draft_review_on')" />
                        <input v-model="draft.tags" type="text" :class="FIELD" :placeholder="$t('nexus.reflect_draft_tags')" />
                        <button type="button" data-create :class="PRIMARY" :disabled="!draftValid" @click="create">{{ $t('nexus.reflect_create') }}</button>
                    </div>
                </section>

                <section v-if="overview.groups.length" class="space-y-2">
                    <p :class="LABEL">{{ $t('nexus.reflect_patterns') }}</p>
                    <div
                        v-for="g in overview.groups"
                        :key="g.tag"
                        data-group
                        class="space-y-1.5 rounded-lg border border-gray-200 p-2.5 dark:border-[#3a3a3c]"
                    >
                        <div class="flex items-center justify-between gap-2">
                            <p class="text-xs font-semibold text-gray-900 dark:text-gray-100">#{{ g.tag }}</p>
                            <span class="text-[10px] tabular-nums text-gray-400">{{ $t('nexus.reflect_cases', { count: g.cases.length }) }}</span>
                        </div>
                        <p class="text-[11px] tabular-nums text-gray-600 dark:text-gray-300">{{ $t('nexus.reflect_tally', g.tally) }}</p>
                        <template v-if="g.enough">
                            <button
                                type="button"
                                data-look-for-pattern
                                :class="PRIMARY"
                                :disabled="looking !== null"
                                @click="lookFor(g.tag)"
                            >
                                <Loader2 v-if="looking === g.tag" class="h-3 w-3 animate-spin" /> {{ $t('nexus.reflect_look') }}
                            </button>
                            <div v-if="patterns[g.tag]" data-pattern class="space-y-1 rounded-md bg-gray-50 p-2 dark:bg-[#1e1e20]">
                                <p :class="LABEL">{{ $t('nexus.reflect_observation') }}</p>
                                <p v-if="patterns[g.tag].withheld" class="text-[11px] text-gray-500 dark:text-gray-400">
                                    {{ $t(`nexus.reflect_withheld_${patterns[g.tag].withheld}`) }}
                                </p>
                                <p v-else class="text-[11px] leading-relaxed text-gray-800 dark:text-gray-200">
                                    <template v-for="(s, i) in patterns[g.tag].sentences" :key="i">
                                        <span>{{ s.text }}</span><button
                                            v-for="n in s.sources"
                                            :key="n"
                                            type="button"
                                            data-citation
                                            class="mx-0.5 align-super rounded px-1 text-[9px] font-semibold tabular-nums text-indigo-600 hover:bg-indigo-100 dark:text-indigo-400 dark:hover:bg-indigo-900/30"
                                            :title="patterns[g.tag].sources.find(x => x.n === n)?.title"
                                            @click="openCase(patterns[g.tag], n)"
                                        >{{ n }}</button>{{ ' ' }}
                                    </template>
                                </p>
                                <p v-if="patterns[g.tag].dropped + patterns[g.tag].advice_dropped" class="text-[10px] text-gray-400">
                                    {{ $t('nexus.reflect_dropped', { count: patterns[g.tag].dropped + patterns[g.tag].advice_dropped }) }}
                                </p>
                            </div>
                        </template>
                        <p v-else class="text-[10px] text-gray-400">{{ $t('nexus.reflect_not_enough', { count: g.cases.length }) }}</p>
                    </div>
                </section>

                <section v-if="overview.decisions.length" class="space-y-1">
                    <p :class="LABEL">{{ $t('nexus.reflect_decisions') }}</p>
                    <div
                        v-for="d in overview.decisions"
                        :key="d.id"
                        data-decision
                        :data-expanded="expanded === d.id"
                        class="rounded-lg border"
                        :class="expanded === d.id ? 'border-indigo-300 dark:border-indigo-800' : 'border-gray-200 dark:border-[#3a3a3c]'"
                    >
                        <button
                            type="button"
                            class="flex w-full items-center justify-between gap-2 px-2.5 py-1.5 text-left"
                            :aria-expanded="expanded === d.id"
                            @click="expanded = expanded === d.id ? null : d.id"
                        >
                            <span class="text-[11px] font-medium text-gray-800 dark:text-gray-200">{{ d.title }}</span>
                            <span class="flex-shrink-0 text-[10px] tabular-nums text-gray-400">{{ d.decided_on }}</span>
                        </button>
                        <dl v-if="expanded === d.id" class="space-y-1 border-t border-gray-100 px-2.5 py-2 text-[11px] text-gray-700 dark:border-[#3a3a3c] dark:text-gray-300">
                            <div>
                                <dt :class="LABEL">{{ $t('nexus.reflect_mark_decided') }} · {{ d.decided_on }}</dt>
                                <dd v-if="d.reasoning" class="whitespace-pre-line">{{ d.reasoning }}</dd>
                            </div>
                            <div v-if="d.expected">
                                <dt :class="LABEL">{{ $t('nexus.reflect_mark_expected') }}</dt>
                                <dd>{{ d.expected }}</dd>
                            </div>
                            <div v-if="d.review_on">
                                <dt :class="LABEL">{{ $t('nexus.reflect_mark_review_on') }}</dt>
                                <dd class="tabular-nums">{{ d.review_on }}</dd>
                            </div>
                            <div v-for="r in d.reviews" :key="r.on">
                                <dt :class="LABEL">{{ $t('nexus.reflect_mark_happened') }} · {{ r.on }}</dt>
                                <dd>{{ r.happened }}<template v-if="r.outcome"> · {{ outcomeLabel(r.outcome) }}</template></dd>
                            </div>
                        </dl>
                    </div>
                </section>
                <p v-else class="text-[11px] text-gray-400">{{ $t('nexus.reflect_none') }}</p>

                <button type="button" data-reflect-disable class="text-[11px] text-gray-400 underline" @click="setEnabled(false)">
                    {{ $t('nexus.reflect_disable') }}
                </button>
            </template>

            <p v-if="failure" class="text-[11px] text-red-500">{{ failure }}</p>
        </div>
    </div>
</template>
