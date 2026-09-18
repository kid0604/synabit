<script setup lang="ts">
/**
 * The stretches of a life something was part of.
 *
 * Worked out from the events that name it, never declared — so a person the
 * vault says nothing about comes back with nothing, and this says so rather
 * than drawing an empty year. See `src-tauri/src/timeline/presence.rs` and
 * §9 of `docs/timeline-2026-09-17.md`.
 */
import { ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../../utils/logger';

export interface Presence { from_day: string; to_day: string; events: number }

const props = defineProps<{ vaultPath: string; nodeId: string }>();

const stretches = ref<Presence[]>([]);
const loaded = ref(false);

/** A span still running has no end the vault knows; it is drawn as open. */
const OPEN_END = '9999-12-31';

const year = (day: string) => day.slice(0, 4);
const label = (stretch: Presence) => {
    const from = year(stretch.from_day);
    if (stretch.to_day === OPEN_END) return `${from} →`;
    const to = year(stretch.to_day);
    return from === to ? from : `${from} – ${to}`;
};

const load = async () => {
    if (!props.vaultPath || !props.nodeId) {
        stretches.value = [];
        loaded.value = true;
        return;
    }
    try {
        stretches.value = await invoke<Presence[]>('timeline_presence', {
            vaultPath: props.vaultPath,
            nodeId: props.nodeId,
        });
    } catch (e) {
        logger.error('Could not read the worldline:', e);
        stretches.value = [];
    } finally {
        loaded.value = true;
    }
};

watch(() => [props.vaultPath, props.nodeId], load, { immediate: true });
</script>

<template>
    <div v-if="loaded" data-worldline class="rounded-xl border border-gray-200 px-4 py-3 dark:border-[#3a3a3c]">
        <p class="mb-2 text-[11px] font-semibold uppercase tracking-wide text-gray-500 dark:text-gray-400">
            {{ $t('people.worldline') }}
        </p>
        <div v-if="stretches.length" class="flex flex-wrap gap-2">
            <span
                v-for="stretch in stretches"
                :key="stretch.from_day"
                data-stretch
                class="flex items-center gap-1.5 rounded-full bg-gray-100 px-3 py-1 text-xs text-gray-700 dark:bg-[#3a3a3c] dark:text-gray-200"
                :title="`${stretch.from_day} → ${stretch.to_day}`"
            >
                <span class="font-medium">{{ label(stretch) }}</span>
                <span data-events class="text-gray-500 dark:text-gray-400">
                    {{ $t('people.worldline_events', { count: stretch.events }) }}
                </span>
            </span>
        </div>
        <p v-else data-worldline-empty class="text-xs text-gray-500 dark:text-gray-400">
            {{ $t('people.worldline_none') }}
        </p>
    </div>
</template>
