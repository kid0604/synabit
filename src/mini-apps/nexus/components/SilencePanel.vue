<script setup lang="ts">
/**
 * "Khoảng lặng" — §7.2 of `docs/timeline-2026-09-17.md`.
 *
 * The most dangerous thing in the document, so the screen is built to make the
 * dangerous version impossible rather than merely discouraged. There is one
 * sentence per person and it is assembled entirely from counted numbers: how
 * many times, between which days, how long since. No advice, no reason, no
 * "maybe you should".
 *
 * People stop seeing each other because of a row, a move, a divorce, a death.
 * The app cannot tell which, so it states the count and stops. The only thing
 * offered beside the sentence is a way to make it go away.
 *
 * See `src-tauri/src/timeline/silence.rs`.
 */
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { AudioLines } from 'lucide-vue-next';
import { logger } from '../../../utils/logger';

export interface Absent {
    who: string;
    name: string;
    times: number;
    first: string;
    last: string;
    quiet_for: number;
    longest_before: number;
}

const props = defineProps<{ vaultPath: string }>();

const open = ref(false);
const absent = ref<Absent[]>([]);

const load = async () => {
    try {
        absent.value = await invoke<Absent[]>('timeline_silences', { vaultPath: props.vaultPath });
    } catch (e) {
        logger.error('Could not count who has gone quiet', e);
        absent.value = [];
    }
};

void load();

const setAside = async (person: Absent) => {
    try {
        await invoke('timeline_set_aside', { vaultPath: props.vaultPath, who: person.who });
        absent.value = absent.value.filter((a) => a.who !== person.who);
    } catch (e) {
        logger.error('Could not set them aside', e);
    }
};

const months = (days: number) => Math.floor(days / 30);
</script>

<template>
    <div class="relative flex-shrink-0">
        <button
            type="button"
            class="flex h-9 items-center gap-1.5 rounded-full px-3 text-xs font-semibold text-gray-600 transition-colors hover:bg-gray-100 disabled:opacity-40 dark:text-gray-300 dark:hover:bg-[#3a3a3c]"
            :aria-expanded="open"
            :disabled="!absent.length"
            @click="open = !open"
        >
            <AudioLines class="h-3.5 w-3.5" /> {{ $t('nexus.silence_button') }}
            <span
                v-if="absent.length"
                data-silence-count
                class="rounded-full bg-gray-200 px-1.5 text-[10px] font-bold tabular-nums text-gray-700 dark:bg-[#3a3a3c] dark:text-gray-200"
                >{{ absent.length }}</span
            >
        </button>

        <div
            v-if="open"
            data-silence-panel
            class="absolute bottom-full right-0 z-30 mb-3 max-h-[70vh] w-96 max-w-[calc(100vw-2rem)] space-y-3 overflow-y-auto rounded-xl border border-gray-200 bg-white p-4 shadow-xl dark:border-[#3a3a3c] dark:bg-[#242426]"
        >
            <p class="text-xs font-semibold text-gray-900 dark:text-gray-100">
                {{ $t('nexus.silence_title') }}
            </p>

            <p v-if="!absent.length" class="text-[11px] text-gray-400">
                {{ $t('nexus.silence_none') }}
            </p>

            <article
                v-for="person in absent"
                :key="person.who"
                data-absent
                class="space-y-1.5 border-l-2 border-gray-200 pl-3 dark:border-[#3a3a3c]"
            >
                <!-- Counted, not concluded. Every number here was measured. -->
                <p data-count class="text-[13px] leading-relaxed text-gray-800 dark:text-gray-200">
                    {{
                        $t('nexus.silence_counted', {
                            name: person.name,
                            times: person.times,
                            from: person.first,
                            to: person.last,
                        })
                    }}
                </p>
                <p class="text-[11px] tabular-nums text-gray-400">
                    {{ $t('nexus.silence_since', { months: months(person.quiet_for) }) }}
                </p>

                <button
                    type="button"
                    data-set-aside
                    class="text-[10px] text-gray-400 transition-colors hover:text-gray-700 dark:hover:text-gray-200"
                    @click="setAside(person)"
                >
                    {{ $t('nexus.silence_set_aside') }}
                </button>
            </article>
        </div>
    </div>
</template>
