<script setup lang="ts">
/**
 * "Ngày này, những năm trước" — §7.1 of `docs/timeline-2026-09-17.md`.
 *
 * Everything here is the person's own sentence, verbatim, with its day and a
 * way back to the note it came from. Nothing on this screen introduces,
 * summarises or labels what is quoted: §6.1's rule is that the sentence was the
 * only thing worth having, and a caption written over it takes that away.
 *
 * So the panel is deliberately plain. No heading over a memory, no emoji, no
 * count of how long ago "already". When there is nothing to quote it says so in
 * one line and stops, rather than filling the space.
 *
 * See `src-tauri/src/timeline/onthisday.rs`.
 */
import { computed, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { CalendarClock } from 'lucide-vue-next';
import { logger } from '../../../utils/logger';
import { todayIso } from '../../../shared/localDay';

export interface Looking {
    day: string;
    years_ago: number;
    title: string;
    quote: string;
    node_id: string;
    kind: string;
}

const props = defineProps<{
    vaultPath: string;
    /** The day the strip is at, `YYYY-MM-DD`; today when it is not anywhere. */
    atDate: string | null;
}>();

const emit = defineEmits<{ (e: 'open', id: string, route: string): void }>();

const open = ref(false);
const looking = ref<Looking[]>([]);

const day = computed(() => props.atDate ?? todayIso());

const load = async () => {
    try {
        looking.value = await invoke<Looking[]>('timeline_on_this_day', {
            vaultPath: props.vaultPath,
            day: day.value,
        });
    } catch (e) {
        logger.error('Could not look back on this day', e);
        looking.value = [];
    }
};

watch(day, load, { immediate: true });

/**
 * One click, and it is remembered: the refusal is a file in the vault, so it
 * holds on every device and through every rebuild of the index.
 */
const notAgain = async (entry: Looking) => {
    try {
        await invoke('timeline_not_again', {
            vaultPath: props.vaultPath,
            nodeId: entry.node_id,
            day: entry.day,
        });
        looking.value = looking.value.filter((l) => l.day !== entry.day);
    } catch (e) {
        logger.error('Could not set that aside', e);
    }
};
</script>

<template>
    <div class="relative flex-shrink-0">
        <button
            type="button"
            class="flex h-9 items-center gap-1.5 rounded-full px-3 text-xs font-semibold text-gray-600 transition-colors hover:bg-gray-100 disabled:opacity-40 dark:text-gray-300 dark:hover:bg-[#3a3a3c]"
            :aria-expanded="open"
            :disabled="!looking.length"
            @click="open = !open"
        >
            <CalendarClock class="h-3.5 w-3.5" /> {{ $t('nexus.onthisday_button') }}
            <span
                v-if="looking.length"
                data-looking-count
                class="rounded-full bg-gray-200 px-1.5 text-[10px] font-bold tabular-nums text-gray-700 dark:bg-[#3a3a3c] dark:text-gray-200"
                >{{ looking.length }}</span
            >
        </button>

        <div
            v-if="open"
            data-onthisday-panel
            class="absolute bottom-full right-0 z-30 mb-3 max-h-[70vh] w-96 max-w-[calc(100vw-2rem)] space-y-3 overflow-y-auto rounded-xl border border-gray-200 bg-white p-4 shadow-xl dark:border-[#3a3a3c] dark:bg-[#242426]"
        >
            <p class="text-xs font-semibold text-gray-900 dark:text-gray-100">
                {{ $t('nexus.onthisday_title') }}
            </p>

            <p v-if="!looking.length" class="text-[11px] text-gray-400">
                {{ $t('nexus.onthisday_none') }}
            </p>

            <article
                v-for="entry in looking"
                :key="entry.day + entry.node_id"
                data-looking
                class="space-y-1.5 border-l-2 border-gray-200 pl-3 dark:border-[#3a3a3c]"
            >
                <p class="text-[11px] tabular-nums text-gray-400">
                    {{ $t('nexus.onthisday_years', { count: entry.years_ago }) }} · {{ entry.day }}
                </p>

                <!-- The sentence itself, and nothing written over it. -->
                <button
                    type="button"
                    data-quote
                    class="block w-full text-left text-[13px] leading-relaxed text-gray-800 hover:underline dark:text-gray-200"
                    @click="emit('open', entry.node_id, 'note')"
                >
                    «{{ entry.quote }}»
                </button>

                <button
                    type="button"
                    data-not-again
                    class="text-[10px] text-gray-400 transition-colors hover:text-gray-700 dark:hover:text-gray-200"
                    @click="notAgain(entry)"
                >
                    {{ $t('nexus.onthisday_not_again') }}
                </button>
            </article>
        </div>
    </div>
</template>
