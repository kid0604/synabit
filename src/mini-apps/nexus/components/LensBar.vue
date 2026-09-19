<script setup lang="ts">
/**
 * Asking Nexus a question, and keeping the good ones.
 *
 * The design is `docs/nexus-lenses-2026-09-19.md`, step 2.
 *
 * What this is *not* yet: step 3 turns the input into a two-way bar of chips
 * that fills itself when somebody clicks a person on the graph, and step 4
 * lets the answer choose its own shape. Until then it is a plain input and a
 * plain table, which is honest about how far the work has got.
 *
 * What it already proves is the claim the design rests on: the answer comes
 * back as an ordinary `QueryResult`, so `TableView` — written long before any
 * of this, for notes — draws a question about the timeline without being
 * taught what an event is. One result shape, and the views already know it.
 */
import { computed, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { Search } from 'lucide-vue-next';
import TableView from '../../../shared/views/TableView.vue';
import type { QueryResult, QueryRow } from '../../../shared/views/types';
import type { Lens } from '../../../shared/lenses';
import { logger } from '../../../utils/logger';
import LensShelf from './LensShelf.vue';

const props = defineProps<{ vaultPath: string }>();
const emit = defineEmits<{ (e: 'open', id: string, type: string): void }>();

const query = ref('');
const result = ref<QueryResult | null>(null);
const running = ref(false);
const refused = ref<string | null>(null);
const active = ref<string | null>(null);

const asked = computed(() => query.value.trim().length > 0);

const run = async () => {
    if (!asked.value || running.value) return;
    running.value = true;
    refused.value = null;
    try {
        result.value = await invoke<QueryResult>('run_node_query', {
            vaultPath: props.vaultPath,
            query: query.value,
            offset: 0,
        });
    } catch (e) {
        // A question the engine will not answer says why — an unreadable date,
        // for one. Showing that is the whole point of refusing rather than
        // quietly answering a different question.
        refused.value = String(e);
        result.value = null;
        logger.error('Could not run the question', e);
    } finally {
        running.value = false;
    }
};

const runLens = async (lens: Lens) => {
    query.value = lens.query;
    active.value = lens.id;
    await run();
};

const typed = () => {
    // Once the words differ from the lens they came from, it is not that lens
    // any more — and the shelf should stop claiming it is.
    active.value = null;
};

/** A row's `open` when it has one, else its id. See `timeline::query`. */
const open = (row: QueryRow) => emit('open', row.open ?? row.id, row.node_type);
</script>

<template>
    <div data-lens-bar class="flex w-full flex-col gap-2">
        <div
            v-if="result || refused"
            data-answer
            class="max-h-64 overflow-y-auto rounded-xl border border-gray-200 bg-white dark:border-[#3a3a3c] dark:bg-[#242426]"
        >
            <p v-if="refused" data-refused class="px-4 py-3 text-[12px] text-gray-600 dark:text-gray-300">
                {{ refused }}
            </p>
            <TableView v-else :result="result" @open="open" />
        </div>

        <div
            class="flex items-center gap-2 rounded-xl border border-gray-200 bg-white px-3 py-2 dark:border-[#3a3a3c] dark:bg-[#242426]"
        >
            <Search class="h-3.5 w-3.5 flex-shrink-0 text-gray-400" />
            <input
                v-model="query"
                data-ask
                type="text"
                :aria-label="$t('nexus.lens_ask')"
                :placeholder="$t('nexus.lens_ask')"
                class="min-w-0 flex-grow bg-transparent text-[13px] text-gray-900 outline-none placeholder:text-gray-400 dark:text-gray-100"
                @input="typed"
                @keyup.enter="run"
            />
            <span
                v-if="result"
                data-found
                class="flex-shrink-0 text-[11px] tabular-nums text-gray-400"
            >
                {{ $t('nexus.lens_found', { total: result.total, ms: result.query_time_ms }) }}
            </span>
            <button
                type="button"
                data-run
                class="h-7 flex-shrink-0 rounded-full bg-indigo-600 px-3 text-[11px] font-semibold text-white disabled:opacity-40"
                :disabled="!asked || running"
                @click="run"
            >
                {{ $t('nexus.lens_run') }}
            </button>
        </div>

        <LensShelf
            :vault-path="vaultPath"
            :query="query"
            :active="active"
            @run="runLens"
        />
    </div>
</template>
