<script setup lang="ts">
/**
 * Asking Nexus a question, and keeping the good ones.
 *
 * The design is `docs/nexus-lenses-2026-09-19.md`, step 2.
 *
 * The bar is a **receipt**, not a power-user feature. Press a person on the
 * graph, drag the strip, tap a tag, and it fills with the chips that describe
 * what just happened. Somebody who never reads it loses nothing; somebody who
 * reads it starts editing. That is the whole of §6.1, and it only works
 * because the chips are the text itself sliced (`shared/queryChips`) rather
 * than a second model that has to be kept in step.
 *
 * The answer picks how it is drawn (`views/shapeFor`), which is what stops a
 * new question costing new code: the rows say what they are, so one renderer
 * covers every question of that shape, including the ones nobody has thought
 * of. A person can overrule it in one press, and a lens remembers what they
 * chose.
 *
 * The claim the design rests on is visible here: the answer comes back as an
 * ordinary `QueryResult`, so `TableView` and `ListView` — written long before
 * any of this, for notes — draw a question about the timeline without being
 * taught what an event is. One result shape, and the views already know it.
 */
import { computed, nextTick, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { Search } from 'lucide-vue-next';
import TableView from '../../../shared/views/TableView.vue';
import ListView from '../../../shared/views/ListView.vue';
import DatedView from '../../../shared/views/DatedView.vue';
import BarsView from '../../../shared/views/BarsView.vue';
import { chosenShape, SHAPES, type Shape } from '../../../shared/views/shapeFor';
import type { QueryResult, QueryRow } from '../../../shared/views/types';
import type { Lens } from '../../../shared/lenses';
import { logger } from '../../../utils/logger';
import { chipsOf, withFilter, withTag, without, type Chip } from '../../../shared/queryChips';
import LensShelf from './LensShelf.vue';

const props = defineProps<{ vaultPath: string }>();
const emit = defineEmits<{ (e: 'open', id: string, type: string): void }>();

const query = ref('');
const typing = ref(false);
const field = ref<HTMLInputElement | null>(null);
const chips = computed(() => chipsOf(query.value));
const result = ref<QueryResult | null>(null);
const running = ref(false);
const refused = ref<string | null>(null);
const active = ref<string | null>(null);
/** What the person chose, or nothing — in which case the answer decides. */
const shape = ref<Shape>('auto');
const drawn = computed(() => chosenShape(shape.value, result.value));

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
    typing.value = false;
    active.value = lens.id;
    // A lens carries how it liked to be seen. `auto` here is not a gap — it
    // is a lens that never overruled the answer, and should keep not doing so
    // as the data behind it changes.
    shape.value = lens.render === 'auto' ? 'auto' : (lens.render as Shape);
    await run();
};

/** Overruling the answer, which is a choice worth keeping if the lens is. */
const pick = (next: Shape) => {
    shape.value = shape.value === next ? 'auto' : next;
};

const typed = () => {
    // Typing keeps the words on screen. Without this the field is swapped for
    // chips the moment the first character makes one, which takes the input
    // out from under the cursor mid-word.
    typing.value = true;
    // And once the words differ from the lens they came from, it is not that
    // lens any more — the shelf should stop claiming it is.
    active.value = null;
};

/** A row's `open` when it has one, else its id. See `timeline::query`. */
const open = (row: QueryRow) => emit('open', row.open ?? row.id, row.node_type);

/**
 * Taking a chip off takes **all** of it off.
 *
 * A chip is no longer one token: an alternative is one chip spanning several,
 * because removing half of it would leave `OR` with nothing on one side. See
 * `shared/queryChips`.
 */
const drop = (chip: Chip) => {
    query.value = without(query.value, chip.from, chip.to);
    active.value = null;
    if (query.value.trim()) void run();
    else result.value = null;
};

/**
 * What the rest of the screen presses. Adding a filter runs the question
 * straight away: the gesture *was* the question, and making somebody press
 * Ask afterwards would turn one act into two.
 */
const press = async (key: string, value: string) => {
    query.value = key === '#' ? withTag(query.value, value) : withFilter(query.value, key, value);
    // Pressing something on the screen is the other way of using the bar, so
    // the bar shows the other face: chips, not words.
    typing.value = false;
    active.value = null;
    if (query.value.trim()) await run();
    else result.value = null;
};
defineExpose({ press });

const edit = async () => {
    typing.value = true;
    await nextTick();
    field.value?.focus();
};
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
            <template v-else>
                <BarsView v-if="drawn === 'bars'" :result="result" />
                <DatedView v-else-if="drawn === 'dated'" :result="result" @open="open" />
                <TableView v-else-if="drawn === 'table'" :result="result" @open="open" />
                <ListView v-else :result="result" @open="open" />

                <!-- What the answer had to admit about itself: a ceiling it
                     hit, rows it had nowhere to put. §8 — owning up to a cut
                     beats staying quiet about it, and an answer nobody can
                     tell is partial is the worst kind. -->
                <p
                    v-if="result?.note"
                    data-answer-note
                    class="border-t border-gray-100 px-4 py-2 text-[11px] text-gray-500 dark:border-[#3a3a3c] dark:text-gray-400"
                >
                    {{ result.note }}
                </p>
            </template>
        </div>

        <div
            class="flex items-center gap-2 rounded-xl border border-gray-200 bg-white px-3 py-2 dark:border-[#3a3a3c] dark:bg-[#242426]"
        >
            <Search class="h-3.5 w-3.5 flex-shrink-0 text-gray-400" />

            <!-- The chips and the words are the same thing seen two ways, so
                 the bar shows whichever the person is using. Reading: chips.
                 Editing: the words, with the chips coming back as soon as
                 they stop. -->
            <input
                v-if="typing || !chips.length"
                ref="field"
                v-model="query"
                data-ask
                type="text"
                :aria-label="$t('nexus.lens_ask')"
                :placeholder="$t('nexus.lens_ask')"
                class="min-w-0 flex-grow bg-transparent text-[13px] text-gray-900 outline-none placeholder:text-gray-400 dark:text-gray-100"
                @input="typed"
                @keyup.enter="typing = false; run()"
                @blur="typing = false"
            />
            <span v-else data-chips class="flex min-w-0 flex-grow flex-wrap items-center gap-1.5">
                <!-- Four looks, because four different things: the table
                     being read, an alternative, a step of the pipeline, and a
                     plain filter. A chip asking for the *absence* of something
                     carries a minus — it used to be drawn identically to the
                     chip asking for its presence, which made two opposite
                     questions one picture. -->
                <span
                    v-for="(chip, i) in chips"
                    :key="`${i}-${chip.text}`"
                    data-chip
                    :data-chip-kind="['source', 'group', 'stage'].includes(chip.key) ? chip.key : undefined"
                    :data-chip-not="chip.negated ? 'yes' : undefined"
                    class="inline-flex h-6 items-center rounded-md border pl-2 text-[12px] font-medium"
                    :class="
                        chip.key === 'source'
                            ? 'border-gray-300 bg-gray-100 text-gray-700 dark:border-[#48484a] dark:bg-[#3a3a3c] dark:text-gray-200'
                            : chip.key === 'stage'
                              ? 'border-emerald-200 bg-emerald-50 text-emerald-800 dark:border-emerald-900 dark:bg-emerald-950 dark:text-emerald-200'
                              : chip.key === 'group'
                                ? 'border-dashed border-indigo-300 bg-indigo-50 text-indigo-700 dark:border-indigo-800 dark:bg-indigo-950 dark:text-indigo-200'
                                : 'border-indigo-200 bg-indigo-50 text-indigo-700 dark:border-indigo-900 dark:bg-indigo-950 dark:text-indigo-200'
                    "
                >
                    <span v-if="chip.negated" data-chip-negated class="mr-1 font-bold">−</span>
                    <!-- A step reads left to right, so it carries the pipe it
                         is written with rather than a key. -->
                    <span v-if="chip.key === 'stage'" class="mr-1 opacity-60">|</span>
                    <span
                        v-if="chip.key && !['#', 'source', 'group', 'stage'].includes(chip.key)"
                        class="mr-1 opacity-60"
                        >{{ chip.key }}</span
                    >
                    {{ chip.label }}
                    <button
                        type="button"
                        data-chip-drop
                        :aria-label="$t('nexus.lens_drop_chip', { what: chip.label })"
                        class="px-1.5 opacity-50 transition-opacity hover:opacity-100"
                        @click="drop(chip)"
                    >
                        ×
                    </button>
                </span>
                <button
                    type="button"
                    data-edit
                    class="text-[12px] text-gray-400 underline decoration-dotted underline-offset-2 hover:text-gray-700 dark:hover:text-gray-200"
                    @click="edit"
                >
                    {{ $t('nexus.lens_edit') }}
                </button>
            </span>
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

        <div class="flex flex-wrap items-center gap-x-4 gap-y-2">
            <LensShelf
                :vault-path="vaultPath"
                :query="query"
                :render="shape"
                :active="active"
                @run="runLens"
            />

            <!-- One press to overrule the answer. The one in use is marked
                 whether the answer chose it or the person did, because which
                 of them chose is not what somebody is looking for here. -->
            <div v-if="result" data-shapes class="flex items-center gap-1">
                <button
                    v-for="option in SHAPES"
                    :key="option"
                    type="button"
                    data-shape
                    :data-on="drawn === option ? 'yes' : undefined"
                    :aria-pressed="drawn === option"
                    class="h-[26px] rounded-full px-2.5 text-[11px] transition-colors"
                    :class="
                        drawn === option
                            ? 'bg-gray-100 font-semibold text-gray-900 dark:bg-[#3a3a3c] dark:text-gray-100'
                            : 'text-gray-400 hover:text-gray-700 dark:hover:text-gray-200'
                    "
                    @click="pick(option)"
                >
                    {{ $t(`nexus.shape_${option}`) }}
                </button>
            </div>
        </div>
    </div>
</template>
