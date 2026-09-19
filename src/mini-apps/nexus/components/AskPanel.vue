<script setup lang="ts">
/**
 * "Chuyện gì đã xảy ra với…" — §7.4 of `docs/timeline-2026-09-17.md`.
 *
 * One thing that ran through a few months of writing and then stopped, with no
 * ending written anywhere. The app states what it counted and asks. It does not
 * say the thing was left unfinished, because it does not know that and never
 * will — the word is banned by §7.4 and by a test.
 *
 * Answering writes a real event, linked to the thing, so the timeline learns
 * the ending. Not answering is free: closing this costs nothing, and the same
 * thing is not asked about again for a year either way.
 *
 * See `src-tauri/src/timeline/asking.rs`.
 */
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { MessageCircleQuestion } from 'lucide-vue-next';
import { logger } from '../../../utils/logger';

export interface Question {
    about: string;
    name: string;
    node_type: string;
    times: number;
    first: string;
    last: string;
    quiet_for: number;
}

const props = defineProps<{ vaultPath: string; format: string; tag: string }>();
const emit = defineEmits<{ (e: 'changed'): void }>();

const open = ref(false);
const question = ref<Question | null>(null);
const answer = ref('');
const when = ref('');
const saving = ref(false);

/** Asking is a write, so it happens only when the person opens this. */
const show = async () => {
    open.value = !open.value;
    if (!open.value || question.value) return;
    try {
        question.value = await invoke<Question | null>('timeline_ask', {
            vaultPath: props.vaultPath,
        });
        when.value = question.value?.last ?? '';
    } catch (e) {
        logger.error('Could not look for something to ask about', e);
    }
};

const save = async () => {
    const asked = question.value;
    if (!asked || !answer.value.trim() || saving.value) return;
    saving.value = true;
    try {
        await invoke<string>('timeline_write_event', {
            vaultPath: props.vaultPath,
            title: answer.value.trim(),
            happenedFrom: when.value || asked.last,
            happenedTo: when.value || asked.last,
            precision: 'day',
            with: asked.node_type === 'person' ? [asked.about] : [],
            place: null,
            about: asked.node_type === 'person' ? [] : [asked.about],
            formatStr: props.format,
            tag: props.tag,
        });
        question.value = null;
        answer.value = '';
        open.value = false;
        emit('changed');
    } catch (e) {
        logger.error('Could not write that down', e);
    } finally {
        saving.value = false;
    }
};
</script>

<template>
    <div class="relative flex-shrink-0">
        <button
            type="button"
            class="flex h-9 items-center gap-1.5 rounded-full px-3 text-xs font-semibold text-gray-600 transition-colors hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-[#3a3a3c]"
            :aria-expanded="open"
            @click="show"
        >
            <MessageCircleQuestion class="h-3.5 w-3.5" /> {{ $t('nexus.ask_button') }}
        </button>

        <div
            v-if="open"
            data-ask-panel
            class="absolute bottom-full right-0 z-30 mb-3 w-96 max-w-[calc(100vw-2rem)] space-y-3 rounded-xl border border-gray-200 bg-white p-4 shadow-xl dark:border-[#3a3a3c] dark:bg-[#242426]"
        >
            <p v-if="!question" class="text-[11px] text-gray-400">{{ $t('nexus.ask_none') }}</p>

            <template v-else>
                <!-- Counted, then a question. Never a verdict. -->
                <p data-ask-counted class="text-[13px] leading-relaxed text-gray-800 dark:text-gray-200">
                    {{
                        $t('nexus.ask_counted', {
                            name: question.name,
                            times: question.times,
                            from: question.first,
                            to: question.last,
                        })
                    }}
                </p>
                <p data-ask-question class="text-[13px] font-medium text-gray-900 dark:text-gray-100">
                    {{ $t('nexus.ask_question') }}
                </p>

                <textarea
                    v-model="answer"
                    data-ask-answer
                    rows="2"
                    class="w-full resize-none rounded-lg border border-gray-200 bg-white px-3 py-2 text-[13px] text-gray-900 outline-none placeholder:text-gray-400 focus:border-gray-400 dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-100"
                    :placeholder="$t('nexus.ask_placeholder')"
                ></textarea>

                <div class="flex items-center gap-2">
                    <input
                        v-model="when"
                        data-ask-when
                        type="date"
                        class="rounded-lg border border-gray-200 bg-white px-2 py-1 text-[11px] tabular-nums text-gray-700 outline-none dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-200"
                    />
                    <button
                        type="button"
                        data-ask-save
                        class="rounded-lg bg-gray-900 px-3 py-1.5 text-[11px] font-semibold text-white disabled:opacity-40 dark:bg-gray-100 dark:text-gray-900"
                        :disabled="!answer.trim() || saving"
                        @click="save"
                    >
                        {{ $t('nexus.ask_save') }}
                    </button>
                    <button
                        type="button"
                        data-ask-skip
                        class="text-[10px] text-gray-400 transition-colors hover:text-gray-700 dark:hover:text-gray-200"
                        @click="open = false"
                    >
                        {{ $t('nexus.ask_skip') }}
                    </button>
                </div>
            </template>
        </div>
    </div>
</template>
