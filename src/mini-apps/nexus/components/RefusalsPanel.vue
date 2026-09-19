<script setup lang="ts">
/**
 * Everything the person has told the app not to do, in one place, each with a
 * way to take it back.
 *
 * Every other panel can *make* one of these decisions: a day set aside from
 * "on this day", a person left out of "gone quiet", a sentence dropped from a
 * year, something pinned. Until this existed there was nowhere to see them and
 * nowhere to undo them, so a mistaken tap was permanent unless the person went
 * into the vault and deleted a file by hand. A refusal you cannot take back is
 * not a pause, it is a seal — and §7.3 is explicit that a hush is the lighter
 * of the two.
 *
 * The dead are listed apart and cannot be undone, because nobody decided them.
 * Offering to "un-quiet" somebody's father is the one thing this screen must
 * never do.
 *
 * See `src-tauri/src/timeline/quiet.rs` and `pin.rs`.
 */
import { computed, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { Undo2, ShieldOff } from 'lucide-vue-next';
import { logger } from '../../../utils/logger';

export interface Hush {
    id: string;
    about: 'person' | 'moment' | 'period' | 'line';
    who?: string;
    node?: string;
    day?: string;
    from?: string;
    to?: string;
    line?: string;
    until: string | null;
    made: string;
}

export interface Hushed {
    hushes: Hush[];
    dead: string[];
}

export interface Pin {
    id: string;
    node: string;
    day: string;
}

const props = defineProps<{ vaultPath: string }>();
const emit = defineEmits<{ (e: 'changed'): void }>();

const open = ref(false);
const hushed = ref<Hushed>({ hushes: [], dead: [] });
const pins = ref<Pin[]>([]);

const total = computed(() => hushed.value.hushes.length + pins.value.length);

const load = async () => {
    try {
        const [q, p] = await Promise.all([
            invoke<Hushed>('timeline_quiet', { vaultPath: props.vaultPath }),
            invoke<Pin[]>('timeline_pins', { vaultPath: props.vaultPath }),
        ]);
        hushed.value = q;
        pins.value = p;
    } catch (e) {
        logger.error('Could not read what has been refused', e);
    }
};

void load();

const show = async () => {
    open.value = !open.value;
    if (open.value) await load();
};

const name = (id: string) => id.split('/').pop()?.replace(/\.md$/, '') ?? id;

/** What one refusal covers, in the person's terms rather than the schema's. */
const covers = (h: Hush) => {
    if (h.about === 'person') return name(h.who ?? '');
    if (h.about === 'moment') return `${name(h.node ?? '')} · ${h.day}`;
    if (h.about === 'period') return `${h.from} – ${h.to}`;
    return name(h.node ?? '');
};

const undo = async (h: Hush) => {
    try {
        await invoke('timeline_unhush', { vaultPath: props.vaultPath, id: h.id });
        hushed.value.hushes = hushed.value.hushes.filter((x) => x.id !== h.id);
        emit('changed');
    } catch (e) {
        logger.error('Could not take that back', e);
    }
};

const unpin = async (pin: Pin) => {
    try {
        await invoke('timeline_unpin', { vaultPath: props.vaultPath, id: pin.id });
        pins.value = pins.value.filter((x) => x.id !== pin.id);
        emit('changed');
    } catch (e) {
        logger.error('Could not take that back', e);
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
            <ShieldOff class="h-3.5 w-3.5" /> {{ $t('nexus.refusals_button') }}
            <span
                v-if="total"
                data-refusal-count
                class="rounded-full bg-gray-200 px-1.5 text-[10px] font-bold tabular-nums text-gray-700 dark:bg-[#3a3a3c] dark:text-gray-200"
                >{{ total }}</span
            >
        </button>

        <div
            v-if="open"
            data-refusals-panel
            class="absolute bottom-full right-0 z-30 mb-3 max-h-[70vh] w-96 max-w-[calc(100vw-2rem)] space-y-4 overflow-y-auto rounded-xl border border-gray-200 bg-white p-4 shadow-xl dark:border-[#3a3a3c] dark:bg-[#242426]"
        >
            <p class="text-xs font-semibold text-gray-900 dark:text-gray-100">
                {{ $t('nexus.refusals_title') }}
            </p>

            <p v-if="!total && !hushed.dead.length" class="text-[11px] text-gray-400">
                {{ $t('nexus.refusals_none') }}
            </p>

            <section v-if="hushed.hushes.length" class="space-y-2">
                <p class="text-[10px] font-semibold uppercase tracking-wider text-gray-400">
                    {{ $t('nexus.refusals_hushes') }}
                </p>
                <div
                    v-for="h in hushed.hushes"
                    :key="h.id"
                    data-hush
                    class="flex items-baseline justify-between gap-2"
                >
                    <span class="min-w-0 flex-1 truncate text-[12px] text-gray-800 dark:text-gray-200">
                        {{ covers(h) }}
                    </span>
                    <span class="flex-shrink-0 text-[10px] tabular-nums text-gray-400">
                        {{ h.until ? $t('nexus.refusals_until', { day: h.until }) : $t('nexus.refusals_forever') }}
                    </span>
                    <button
                        type="button"
                        data-undo
                        class="flex-shrink-0 text-gray-400 transition-colors hover:text-gray-800 dark:hover:text-gray-100"
                        :title="$t('nexus.refusals_undo')"
                        @click="undo(h)"
                    >
                        <Undo2 class="h-3.5 w-3.5" />
                    </button>
                </div>
            </section>

            <section v-if="pins.length" class="space-y-2">
                <p class="text-[10px] font-semibold uppercase tracking-wider text-gray-400">
                    {{ $t('nexus.refusals_pins') }}
                </p>
                <div
                    v-for="pin in pins"
                    :key="pin.id"
                    data-pin
                    class="flex items-baseline justify-between gap-2"
                >
                    <span class="min-w-0 flex-1 truncate text-[12px] text-gray-800 dark:text-gray-200">
                        {{ name(pin.node) }}
                    </span>
                    <span class="flex-shrink-0 text-[10px] tabular-nums text-gray-400">{{ pin.day }}</span>
                    <button
                        type="button"
                        data-unpin
                        class="flex-shrink-0 text-gray-400 transition-colors hover:text-gray-800 dark:hover:text-gray-100"
                        :title="$t('nexus.refusals_undo')"
                        @click="unpin(pin)"
                    >
                        <Undo2 class="h-3.5 w-3.5" />
                    </button>
                </div>
            </section>

            <!-- Nobody decided these, so there is nothing here to undo. -->
            <section v-if="hushed.dead.length" class="space-y-2">
                <p class="text-[10px] font-semibold uppercase tracking-wider text-gray-400">
                    {{ $t('nexus.refusals_dead') }}
                </p>
                <p class="text-[11px] leading-relaxed text-gray-500 dark:text-gray-400">
                    {{ $t('nexus.refusals_dead_explain') }}
                </p>
                <p
                    v-for="who in hushed.dead"
                    :key="who"
                    data-dead
                    class="truncate text-[12px] text-gray-700 dark:text-gray-300"
                >
                    {{ name(who) }}
                </p>
            </section>
        </div>
    </div>
</template>
