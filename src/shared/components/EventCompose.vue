<script setup lang="ts">
/**
 * Writing down something that happened.
 *
 * One form, four fields, always editable. When a model is set up there is also
 * a line to type into — "hôm qua ăn trưa với Khánh" — and the model fills the
 * form in; the person then corrects whatever it got wrong before anything is
 * written. That order matters: a reading nobody can correct is a reading that
 * quietly puts strangers at your wedding.
 *
 * A hand-written rule parser stood here first. It could not tell `voi` from
 * `với` or `tai` from `tại`, and a time at the end of a sentence ended up
 * inside somebody's name. The model does that job now
 * (`src-tauri/src/timeline/extract.rs`), through the guards the vault's own
 * notes go through. With no model the form still works — it just does not
 * fill itself.
 */
import { ref, computed, onMounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { CalendarPlus, Sparkles } from 'lucide-vue-next';
import { logger } from '../../utils/logger';
import { errorText } from '../errorText';
import { todayIso } from '../localDay';

interface ComposedPerson { name: string; node_id: string | null }
export interface Composed {
    title: string;
    happened_from: string;
    happened_to: string;
    precision: string;
    dated: boolean;
    with: ComposedPerson[];
    place: string | null;
}
export interface ComposedReply { read: Composed | null; model: string | null; refused: string | null }

const props = withDefaults(
    defineProps<{ vaultPath: string; format?: string; tag?: string }>(),
    { format: 'YYYY-MM-DD', tag: '' },
);
const emit = defineEmits<{ (e: 'changed'): void }>();

const open = ref(false);
const model = ref<string | null>(null);
const line = ref('');
const reading = ref(false);
const note = ref<string | null>(null);
const saving = ref(false);
const saved = ref<string | null>(null);
const failed = ref<string | null>(null);

const blank = () => ({ title: '', from: todayIso(), to: todayIso(), people: '', place: '' });
const form = ref(blank());

const ready = computed(() => form.value.title.trim().length > 0 && !!form.value.from);

onMounted(async () => {
    try {
        // An empty line asks one question: is there a model to read with?
        const reply = await invoke<ComposedReply>('timeline_read_line', { vaultPath: props.vaultPath, line: '' });
        model.value = reply.model;
    } catch (e) {
        logger.error('Could not ask whether a model is set up:', e);
    }
});

const read = async () => {
    if (!line.value.trim() || reading.value) return;
    reading.value = true;
    note.value = null;
    failed.value = null;
    try {
        const reply = await invoke<ComposedReply>('timeline_read_line', {
            vaultPath: props.vaultPath,
            line: line.value,
        });
        model.value = reply.model;
        note.value = reply.refused;
        if (reply.read) {
            form.value = {
                title: reply.read.title,
                from: reply.read.happened_from,
                to: reply.read.happened_to,
                people: reply.read.with.map(person => person.node_id ?? person.name).join(', '),
                place: reply.read.place ?? '',
            };
            if (!reply.read.dated) note.value = null;
        }
    } catch (e) {
        failed.value = errorText(e);
    } finally {
        reading.value = false;
    }
};

const write = async () => {
    if (!ready.value || saving.value) return;
    saving.value = true;
    failed.value = null;
    try {
        const to = form.value.to || form.value.from;
        const written = await invoke<string>('timeline_write_event', {
            vaultPath: props.vaultPath,
            title: form.value.title.trim(),
            happenedFrom: form.value.from,
            happenedTo: to,
            precision: form.value.from === to ? 'day' : 'range',
            with: form.value.people.split(',').map(name => name.trim()).filter(Boolean),
            place: form.value.place.trim() || null,
            about: [],
            formatStr: props.format,
            tag: props.tag,
        });
        saved.value = written.split('/').pop() ?? written;
        line.value = '';
        note.value = null;
        form.value = blank();
        emit('changed');
    } catch (e) {
        failed.value = errorText(e);
    } finally {
        saving.value = false;
    }
};
</script>

<template>
    <div class="relative" data-compose>
        <!-- No frame of its own: it sits inside the one the two doors share,
             so the two read as halves of one thing rather than as two
             unrelated buttons at opposite ends of the screen. -->
        <button
            type="button"
            data-compose-open
            class="flex h-full items-center gap-2 px-4 py-2 text-xs font-semibold text-gray-700 transition-colors hover:bg-gray-50 dark:text-gray-300 dark:hover:bg-[#3a3a3c]"
            :title="$t('nexus.compose_title')"
            @click.stop="open = !open"
        >
            <CalendarPlus class="h-4 w-4 text-indigo-500" />
            <span>{{ $t('nexus.compose_add') }}</span>
        </button>

        <div
            v-if="open"
            data-compose-panel
            class="absolute bottom-full left-0 z-30 mb-2 w-80 space-y-2 rounded-xl border border-gray-200 bg-white p-3 shadow-xl dark:border-[#3a3a3c] dark:bg-[#242426]"
        >
            <!-- Said once, at the top, because the whole complaint was that
                 nothing here told you what you were making. -->
            <p data-is-an-event class="rounded-lg bg-indigo-50 px-2.5 py-2 text-[11px] leading-relaxed text-indigo-900 dark:bg-indigo-950/50 dark:text-indigo-200">
                {{ $t('nexus.compose_is_an_event') }}
            </p>

            <!-- The model fills the form in; it never writes anything itself. -->
            <div v-if="model" data-ask>
                <div class="flex gap-1.5">
                    <input
                        v-model="line"
                        data-line
                        type="text"
                        class="min-w-0 flex-1 rounded-lg border border-gray-200 bg-gray-50 px-2.5 py-2 text-sm text-gray-800 outline-none focus:border-gray-400 dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-100"
                        :placeholder="$t('nexus.compose_placeholder')"
                        @keydown.enter.prevent="read"
                    />
                    <button
                        type="button"
                        data-read
                        class="shrink-0 rounded-lg bg-gray-100 px-2.5 text-xs font-medium text-gray-700 disabled:opacity-40 dark:bg-[#3a3a3c] dark:text-gray-200"
                        :disabled="!line.trim() || reading"
                        @click="read"
                    >
                        <Sparkles class="h-3.5 w-3.5" />
                    </button>
                </div>
                <p class="mt-1 text-[11px] text-gray-400">{{ $t('nexus.compose_ask', { model }) }}</p>
            </div>
            <p v-else data-no-model class="text-[11px] text-gray-500 dark:text-gray-400">
                {{ $t('nexus.compose_no_model') }}
            </p>

            <p v-if="note" data-note class="text-xs text-amber-600 dark:text-amber-400">{{ note }}</p>

            <label class="block text-[11px] font-semibold uppercase tracking-wide text-gray-500">{{ $t('nexus.compose_field_title') }}</label>
            <input v-model="form.title" data-field-title type="text" class="w-full rounded-lg border border-gray-200 bg-gray-50 px-2.5 py-1.5 text-sm dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-100" />

            <div class="flex gap-2">
                <div class="flex-1">
                    <label class="block text-[11px] font-semibold uppercase tracking-wide text-gray-500">{{ $t('nexus.compose_field_from') }}</label>
                    <input v-model="form.from" data-field-from type="date" class="w-full rounded-lg border border-gray-200 bg-gray-50 px-2 py-1.5 text-sm dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-100" />
                </div>
                <div class="flex-1">
                    <label class="block text-[11px] font-semibold uppercase tracking-wide text-gray-500">{{ $t('nexus.compose_field_to') }}</label>
                    <input v-model="form.to" data-field-to type="date" class="w-full rounded-lg border border-gray-200 bg-gray-50 px-2 py-1.5 text-sm dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-100" />
                </div>
            </div>

            <label class="block text-[11px] font-semibold uppercase tracking-wide text-gray-500">{{ $t('nexus.compose_field_people') }}</label>
            <input v-model="form.people" data-field-people type="text" class="w-full rounded-lg border border-gray-200 bg-gray-50 px-2.5 py-1.5 text-sm dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-100" />

            <label class="block text-[11px] font-semibold uppercase tracking-wide text-gray-500">{{ $t('nexus.compose_field_where') }}</label>
            <input v-model="form.place" data-field-where type="text" class="w-full rounded-lg border border-gray-200 bg-gray-50 px-2.5 py-1.5 text-sm dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-100" />

            <p v-if="saved" data-saved class="text-xs text-green-600 dark:text-green-400">{{ $t('nexus.compose_saved', { note: saved }) }}</p>
            <p v-if="failed" data-failed class="text-xs text-red-600 dark:text-red-400">{{ failed }}</p>

            <button
                type="button"
                data-write
                class="w-full rounded-lg bg-gray-900 px-3 py-2 text-xs font-semibold text-white disabled:opacity-40 dark:bg-white dark:text-gray-900"
                :disabled="!ready || saving"
                @click="write"
            >
                {{ $t('nexus.compose_add') }}
            </button>
        </div>
    </div>
</template>
