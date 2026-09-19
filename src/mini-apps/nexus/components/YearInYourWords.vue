<script setup lang="ts">
/**
 * "Một năm bằng chính lời mày" — §7.6 of `docs/timeline-2026-09-17.md`.
 *
 * Ten to fifteen sentences the person wrote, in the order they wrote them,
 * each with its day and a way back into the note. That is the whole page.
 *
 * There is no opening line, no closing line, and no adjective anywhere in this
 * component, because §7.6 forbids all three. What the app is allowed to
 * contribute is a date, a separator and a way to remove a line — everything
 * else on the screen is the person's own writing. The feeling comes from the
 * distance between who they are now and who wrote those sentences, and every
 * word the app adds shortens that distance.
 *
 * See `src-tauri/src/timeline/year.rs`.
 */
import { computed, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { BookOpen } from 'lucide-vue-next';
import { logger } from '../../../utils/logger';
import { todayIso } from '../../../shared/localDay';

export interface Line {
    day: string;
    node_id: string;
    text: string;
}

const props = defineProps<{ vaultPath: string }>();
const emit = defineEmits<{ (e: 'open', id: string, route: string): void }>();

const open = ref(false);
const lines = ref<Line[]>([]);
const reading = ref(false);
const refused = ref<string | null>(null);
const year = ref(Number(todayIso().slice(0, 4)));

const years = computed(() => {
    const now = Number(todayIso().slice(0, 4));
    return [now, now - 1, now - 2, now - 3];
});

const read = async () => {
    reading.value = true;
    refused.value = null;
    try {
        lines.value = await invoke<Line[]>('timeline_year', {
            vaultPath: props.vaultPath,
            year: year.value,
        });
    } catch (e) {
        logger.error('Could not read the year', e);
        refused.value = String(e);
        lines.value = [];
    } finally {
        reading.value = false;
    }
};

const show = async () => {
    open.value = !open.value;
    if (open.value && !lines.value.length) await read();
};

const pick = async (next: number) => {
    year.value = next;
    await read();
};

/** Dropped for good: §7.6 says a line left out does not come back. */
const drop = async (line: Line) => {
    try {
        await invoke('timeline_drop_line', {
            vaultPath: props.vaultPath,
            nodeId: line.node_id,
            text: line.text,
        });
        lines.value = lines.value.filter((l) => l.text !== line.text || l.node_id !== line.node_id);
    } catch (e) {
        logger.error('Could not drop that line', e);
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
            <BookOpen class="h-3.5 w-3.5" /> {{ $t('nexus.year_button') }}
        </button>

        <div
            v-if="open"
            data-year-panel
            class="absolute bottom-full right-0 z-30 mb-3 max-h-[70vh] w-[28rem] max-w-[calc(100vw-2rem)] overflow-y-auto rounded-xl border border-gray-200 bg-white p-5 shadow-xl dark:border-[#3a3a3c] dark:bg-[#242426]"
        >
            <div class="mb-4 flex items-center gap-1.5">
                <button
                    v-for="option in years"
                    :key="option"
                    type="button"
                    data-year-pick
                    class="rounded-full px-2.5 py-1 text-[11px] font-semibold tabular-nums transition-colors"
                    :class="
                        option === year
                            ? 'bg-gray-900 text-white dark:bg-gray-100 dark:text-gray-900'
                            : 'text-gray-500 hover:bg-gray-100 dark:text-gray-400 dark:hover:bg-[#3a3a3c]'
                    "
                    @click="pick(option)"
                >
                    {{ option }}
                </button>
            </div>

            <p v-if="reading" class="text-[11px] text-gray-400">{{ $t('nexus.year_reading') }}</p>
            <p v-else-if="refused" data-year-refused class="text-[11px] text-gray-500">{{ refused }}</p>
            <p v-else-if="!lines.length" class="text-[11px] text-gray-400">
                {{ $t('nexus.year_none') }}
            </p>

            <!-- Their sentences, in the order they were written. Nothing above
                 them, nothing below them. -->
            <ol v-else class="space-y-4">
                <li
                    v-for="line in lines"
                    :key="line.node_id + line.text"
                    data-year-line
                    class="group space-y-1"
                >
                    <p class="text-[11px] tabular-nums text-gray-400">{{ line.day }}</p>
                    <button
                        type="button"
                        data-year-text
                        class="block w-full text-left text-[14px] leading-relaxed text-gray-900 hover:underline dark:text-gray-100"
                        @click="emit('open', line.node_id, 'note')"
                    >
                        {{ line.text }}
                    </button>
                    <button
                        type="button"
                        data-year-drop
                        class="text-[10px] text-gray-300 opacity-0 transition-opacity hover:text-gray-700 group-hover:opacity-100 dark:text-gray-600 dark:hover:text-gray-200"
                        @click="drop(line)"
                    >
                        {{ $t('nexus.year_drop') }}
                    </button>
                </li>
            </ol>
        </div>
    </div>
</template>
