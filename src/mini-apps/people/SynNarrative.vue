<script setup lang="ts">
/**
 * "Syn kể lại": a few sentences about the relationship, each resting on a record.
 *
 * The sentences arrive already checked: any the model wrote without a record
 * behind it were removed in Rust before they got here, and the card says how
 * many (`src-tauri/src/syn/narrative.rs`). Every number opens the record it
 * names.
 *
 * Asked for, never automatic. It spends a model call, and somebody opening a
 * contact to find a phone number has not asked for one.
 */
import { ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import { Sparkles, Loader2 } from 'lucide-vue-next';
import { logger } from '../../utils/logger';
import { errorText } from '../../shared/errorText';

interface Source {
    n: number;
    node_id: string;
    node_type: string;
    title: string;
    date: string;
    what: string;
}

interface Sentence {
    text: string;
    sources: number[];
}

interface Narrative {
    sentences: Sentence[];
    sources: Source[];
    dropped: number;
    withheld: boolean;
}

const props = defineProps<{
    person: any;
    vaultPath: string;
}>();

const emit = defineEmits<{
    (e: 'open-node', id: string, type: string): void;
}>();

const { t, locale } = useI18n();
const narrative = ref<Narrative | null>(null);
const busy = ref(false);
const failure = ref('');

// Which request is the latest, so an answer about the person before does not
// land on the card of the person after.
let asking = 0;

// A different person is a different story.
watch(() => props.person?.id, () => {
    asking++;
    narrative.value = null;
    failure.value = '';
    busy.value = false;
});

const tell = async () => {
    if (!props.person?.id || busy.value) return;
    const request = ++asking;
    busy.value = true;
    failure.value = '';
    try {
        const told = await invoke<Narrative>('syn_narrate_person', {
            vaultPath: props.vaultPath,
            personId: props.person.id,
            locale: locale.value,
        });
        if (request === asking) narrative.value = told;
    } catch (e) {
        if (request !== asking) return;
        const message = errorText(e);
        failure.value = message.includes('Syn is switched off') ? t('people.narrative_off') : message;
        logger.error('Syn could not tell it back', e);
    } finally {
        if (request === asking) busy.value = false;
    }
};

const sourceOf = (n: number) => narrative.value?.sources.find(s => s.n === n);

const open = (n: number) => {
    const source = sourceOf(n);
    if (source) emit('open-node', source.node_id, source.node_type);
};
</script>

<template>
    <div v-if="!narrative?.withheld" class="rounded-2xl border border-border dark:border-border-dark bg-surface dark:bg-surface-dark p-5 space-y-3">
        <div class="flex items-center justify-between gap-3">
            <h3 class="flex items-center gap-2 text-sm font-semibold text-gray-700 dark:text-gray-200">
                <Sparkles class="w-4 h-4 text-violet-500" /> {{ $t('people.narrative_title') }}
            </h3>
            <button
                type="button"
                class="flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium text-violet-600 dark:text-violet-400 hover:bg-violet-50 dark:hover:bg-violet-900/20 transition-colors disabled:opacity-50"
                :disabled="busy"
                @click="tell"
            >
                <Loader2 v-if="busy" class="w-3.5 h-3.5 animate-spin" />
                {{ busy ? $t('people.narrative_loading') : narrative ? $t('people.narrative_again') : $t('people.narrative_ask') }}
            </button>
        </div>

        <p v-if="failure" class="text-xs text-red-500">{{ failure }}</p>

        <template v-else-if="narrative">
            <p v-if="narrative.sources.length === 0" class="text-sm text-gray-500 dark:text-gray-400">{{ $t('people.narrative_empty') }}</p>
            <p v-else-if="narrative.sentences.length === 0" class="text-sm text-gray-500 dark:text-gray-400">{{ $t('people.narrative_nothing_left') }}</p>
            <p v-else class="text-sm leading-relaxed text-gray-800 dark:text-gray-200">
                <template v-for="(sentence, i) in narrative.sentences" :key="i">
                    <span>{{ sentence.text }}</span><button
                        v-for="n in sentence.sources"
                        :key="n"
                        type="button"
                        data-citation
                        class="mx-0.5 align-super rounded px-1 text-[10px] font-semibold tabular-nums text-violet-600 dark:text-violet-400 hover:bg-violet-100 dark:hover:bg-violet-900/30"
                        :title="sourceOf(n) ? `${sourceOf(n)!.title} · ${sourceOf(n)!.date}` : ''"
                        @click="open(n)"
                    >{{ n }}</button>{{ ' ' }}
                </template>
            </p>
            <p v-if="narrative.sentences.length" class="text-[11px] text-gray-400">
                {{ $t('people.narrative_rule') }}
                <template v-if="narrative.dropped"> {{ $t('people.narrative_dropped', { count: narrative.dropped }) }}</template>
            </p>
        </template>
    </div>
</template>
