<script setup lang="ts">
/**
 * Pictures and recordings in the month being looked at, grouped into moments.
 *
 * Every file shows what stands in for it — a caption, a transcript — and says
 * so when the file itself is on another device, rather than showing an empty
 * box (§4.6 of `docs/tua-lai-2026-09-14.md`). A line of a transcript opens the
 * recording at that moment. See `src-tauri/src/timeline/media.rs`.
 */
import { computed, ref, watch } from 'vue';
import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import { Images, CloudOff } from 'lucide-vue-next';
import { logger } from '../../../utils/logger';
import { clock, timeFragment } from '../../../shared/mediaTime';

interface Segment { start: number; end: number; text: string }

export interface MediaEntry {
    node_id: string;
    title: string;
    extension: string;
    kind: 'image' | 'audio' | 'video' | 'other';
    at: string;
    present: boolean;
    path: string | null;
    caption: string | null;
    caption_model: string | null;
    transcript: { text: string; segments: Segment[]; model: string } | null;
}

export interface MediaMoment { from: string; to: string; count: number; cover: MediaEntry; members: MediaEntry[] }

const props = defineProps<{
    vaultPath: string;
    /** The day the strip is at, `YYYY-MM-DD`. */
    atDate: string | null;
}>();

const emit = defineEmits<{ (e: 'open', id: string, route: string, query?: string): void }>();

const SHOWN_LINES = 6;

const open = ref(false);
const moments = ref<MediaMoment[]>([]);
const expandedLines = ref<Record<string, boolean>>({});

const month = computed(() => props.atDate?.slice(0, 7) ?? null);

const load = async () => {
    if (!month.value) {
        moments.value = [];
        return;
    }
    try {
        moments.value = await invoke<MediaMoment[]>('timeline_media_moments', { vaultPath: props.vaultPath, when: month.value });
    } catch (e) {
        logger.error('Could not read the moments', e);
    }
};

watch(month, load, { immediate: true });

const src = (entry: MediaEntry) => (entry.path ? convertFileSrc(entry.path) : '');

const openEntry = (entry: MediaEntry) => {
    if (entry.present) emit('open', entry.node_id, 'file');
};

const openAt = (entry: MediaEntry, segment: Segment) => {
    if (entry.present) emit('open', entry.node_id, 'file', timeFragment(segment.start, segment.end));
};

const lines = (entry: MediaEntry) => {
    const all = entry.transcript?.segments ?? [];
    return expandedLines.value[entry.node_id] ? all : all.slice(0, SHOWN_LINES);
};
</script>

<template>
    <div class="relative flex-shrink-0">
        <button
            type="button"
            class="flex h-9 items-center gap-1.5 rounded-full px-3 text-xs font-semibold text-gray-600 transition-colors hover:bg-gray-100 disabled:opacity-40 dark:text-gray-300 dark:hover:bg-[#3a3a3c]"
            :aria-expanded="open"
            :disabled="!month"
            @click="open = !open"
        >
            <Images class="h-3.5 w-3.5" /> {{ $t('nexus.moments_button') }}
            <span v-if="moments.length" data-moment-count class="rounded-full bg-gray-200 px-1.5 text-[10px] font-bold tabular-nums text-gray-700 dark:bg-[#3a3a3c] dark:text-gray-200">{{ moments.length }}</span>
        </button>

        <div
            v-if="open && month"
            data-moments-panel
            class="absolute bottom-full right-0 z-30 mb-3 max-h-[70vh] w-96 max-w-[calc(100vw-2rem)] space-y-3 overflow-y-auto rounded-xl border border-gray-200 bg-white p-4 shadow-xl dark:border-[#3a3a3c] dark:bg-[#242426]"
        >
            <div class="space-y-1">
                <p class="text-xs font-semibold text-gray-900 dark:text-gray-100">{{ $t('nexus.moments_title', { month }) }}</p>
                <p class="text-[11px] text-gray-500 dark:text-gray-400">{{ $t('nexus.moments_explain') }}</p>
            </div>

            <p v-if="!moments.length" class="text-[11px] text-gray-400">{{ $t('nexus.moments_none') }}</p>

            <article
                v-for="m in moments"
                :key="m.from + m.cover.node_id"
                data-moment
                class="space-y-2 rounded-lg border border-gray-200 p-2.5 dark:border-[#3a3a3c]"
            >
                <header class="flex items-baseline justify-between gap-2 text-[11px] tabular-nums">
                    <span class="font-semibold text-gray-800 dark:text-gray-200">{{ m.from === m.to ? m.from : `${m.from} → ${m.to}` }}</span>
                    <span class="text-gray-400">{{ $t('nexus.moments_count', { count: m.count }) }}</span>
                </header>

                <div v-for="entry in (m.count > 1 ? m.members : [m.cover])" :key="entry.node_id" data-entry class="space-y-1">
                    <button
                        v-if="entry.kind === 'image' && entry.present && entry.path && entry.node_id === m.cover.node_id"
                        type="button"
                        class="block w-full overflow-hidden rounded-md"
                        @click="openEntry(entry)"
                    >
                        <img :src="src(entry)" :alt="entry.caption ?? entry.title" loading="lazy" class="h-32 w-full object-cover" />
                    </button>

                    <div class="flex items-center justify-between gap-2 text-[11px]">
                        <button
                            type="button"
                            class="truncate text-left font-medium text-gray-700 hover:underline disabled:no-underline dark:text-gray-300"
                            :disabled="!entry.present"
                            @click="openEntry(entry)"
                        >{{ entry.title }}</button>
                        <span class="flex-shrink-0 tabular-nums text-gray-400">{{ entry.at.slice(11) || entry.at }}</span>
                    </div>

                    <p v-if="!entry.present" data-elsewhere class="flex items-center gap-1 text-[10px] text-amber-700 dark:text-amber-400">
                        <CloudOff class="h-3 w-3" /> {{ $t('nexus.moments_elsewhere') }}
                    </p>

                    <p v-if="entry.caption" data-caption class="text-[11px] text-gray-700 dark:text-gray-300">
                        {{ entry.caption }}
                        <span class="block text-[10px] text-gray-400">{{ $t('nexus.moments_caption_by', { model: entry.caption_model }) }}</span>
                    </p>

                    <div v-if="entry.transcript" data-transcript class="space-y-0.5">
                        <p class="text-[10px] text-gray-400">{{ $t('nexus.moments_transcript_by', { model: entry.transcript.model }) }}</p>
                        <p v-if="!entry.transcript.segments.length" class="text-[11px] text-gray-700 dark:text-gray-300">{{ entry.transcript.text }}</p>
                        <button
                            v-for="segment in lines(entry)"
                            :key="segment.start"
                            type="button"
                            data-segment
                            class="flex w-full gap-2 rounded px-1 text-left text-[11px] text-gray-700 hover:bg-indigo-50 disabled:hover:bg-transparent dark:text-gray-300 dark:hover:bg-indigo-900/20"
                            :disabled="!entry.present"
                            @click="openAt(entry, segment)"
                        >
                            <span class="flex-shrink-0 tabular-nums text-indigo-600 dark:text-indigo-400">{{ clock(segment.start) }}</span>
                            <span>{{ segment.text }}</span>
                        </button>
                        <button
                            v-if="!expandedLines[entry.node_id] && entry.transcript.segments.length > SHOWN_LINES"
                            type="button"
                            class="text-[10px] text-gray-400 underline"
                            @click="expandedLines[entry.node_id] = true"
                        >{{ $t('nexus.moments_more', { count: entry.transcript.segments.length - SHOWN_LINES }) }}</button>
                    </div>

                    <p v-if="!entry.present && !entry.caption && !entry.transcript" class="text-[10px] text-gray-400">{{ $t('nexus.moments_no_surrogate') }}</p>
                </div>
            </article>
        </div>
    </div>
</template>
