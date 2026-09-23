<script setup lang="ts">
/**
 * A moment already kept, opened to be put right or let go.
 *
 * # Why this exists
 *
 * Every field of a proposal is the person's to correct before they keep it
 * (`ExtractTray`), and then the moment became a file nothing in the app could
 * open: a timeline row opened the note it was read from, a moment written by
 * hand opened nothing at all, and the only way to change a date was a text
 * editor. A decision you cannot revisit is not much of a decision.
 *
 * # What is theirs
 *
 * Everything here. Whatever they change joins `hand` in the moment's file, so
 * no later reading of the note proposes over it (§15.2) — the same promise the
 * review already carries. The quote and the note it came from are not editable
 * for the same reason they are not editable there: they are the evidence.
 */
import { computed, ref, watch } from 'vue';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import { Loader2, Trash2 } from 'lucide-vue-next';
import { errorText } from '../errorText';
import { logger } from '../../utils/logger';

interface PersonRef { id: string; title: string }

export interface MomentDetail {
    path: string;
    title: string;
    happened_from: string;
    happened_to: string;
    precision: string;
    time: string | null;
    place: string | null;
    category: string | null;
    amount: { value: number; unit: string } | null;
    about: string[];
    people: PersonRef[];
    names: string[];
    hand: string[];
    source_node: string | null;
    quote: string | null;
    origin: string | null;
    categories: string[];
    known_people: PersonRef[];
}

const props = defineProps<{ vaultPath: string; path: string | null }>();
const emit = defineEmits<{
    (e: 'close'): void;
    (e: 'changed'): void;
    /** Read the note it came from, where it came from. */
    (e: 'open', id: string, quote: string): void;
}>();

const { t, te, locale } = useI18n();
const moment = ref<MomentDetail | null>(null);
const reading = ref(false);
const saving = ref(false);
const failure = ref('');
const askingToDelete = ref(false);

interface Form {
    title: string;
    from: string;
    to: string;
    time: string;
    people: { id: string | null; name: string }[];
    place: string;
    category: string;
    amount: string;
    unit: string;
    about: string;
}
const form = ref<Form | null>(null);
const adding = ref('');

const load = async (path: string) => {
    reading.value = true;
    failure.value = '';
    askingToDelete.value = false;
    try {
        const found = await invoke<MomentDetail>('timeline_moment', { vaultPath: props.vaultPath, path });
        moment.value = found;
        form.value = {
            title: found.title,
            from: found.happened_from,
            to: found.happened_to,
            time: found.time ?? '',
            people: [
                ...found.people.map(person => ({ id: person.id, name: person.title })),
                ...found.names.map(name => ({ id: null, name })),
            ],
            place: found.place ?? '',
            category: found.category ?? 'other',
            amount: found.amount ? String(found.amount.value) : '',
            unit: found.amount?.unit ?? 'VND',
            about: found.about.join(', '),
        };
    } catch (e) {
        failure.value = errorText(e);
        logger.error('Could not read the moment', e);
    } finally {
        reading.value = false;
    }
};

watch(
    () => props.path,
    path => {
        moment.value = null;
        form.value = null;
        if (path) void load(path);
    },
    { immediate: true },
);

const kinds = computed(() => moment.value?.categories ?? []);
const kindName = (kind: string) => (te(`nexus.moment_category_${kind}`) ? t(`nexus.moment_category_${kind}`) : kind);

const addPerson = () => {
    const name = adding.value.trim();
    if (!name || !form.value) return;
    form.value.people.push({ id: null, name });
    adding.value = '';
};

const assign = (at: number, id: string) => {
    const person = moment.value?.known_people.find(p => p.id === id);
    if (!person || !form.value) return;
    form.value.people[at] = { id, name: person.title };
};

const save = async () => {
    if (!form.value || !moment.value || saving.value) return;
    saving.value = true;
    failure.value = '';
    try {
        await invoke('timeline_moment_write', {
            vaultPath: props.vaultPath,
            path: moment.value.path,
            edits: {
                title: form.value.title,
                happened_from: form.value.from,
                happened_to: form.value.to || form.value.from,
                precision: (form.value.to || form.value.from) === form.value.from ? 'day' : 'range',
                time: form.value.time,
                people: form.value.people.map(person => person.id ?? person.name),
                place: form.value.place,
                category: form.value.category,
                amount: form.value.amount ? Number(form.value.amount) : 0,
                unit: form.value.unit,
                about: form.value.about.split(',').map(a => a.trim()).filter(Boolean),
            },
        });
        emit('changed');
        emit('close');
    } catch (e) {
        failure.value = errorText(e);
    } finally {
        saving.value = false;
    }
};

const letGo = async () => {
    if (!moment.value) return;
    saving.value = true;
    failure.value = '';
    try {
        await invoke('timeline_moment_delete', { vaultPath: props.vaultPath, path: moment.value.path });
        emit('changed');
        emit('close');
    } catch (e) {
        failure.value = errorText(e);
    } finally {
        saving.value = false;
    }
};
</script>

<template>
    <div
        v-if="path"
        data-moment-sheet
        class="rounded-xl border border-gray-200 bg-white p-3 shadow-sm dark:border-[#3a3a3c] dark:bg-[#1e1e20]"
        @keydown.esc="emit('close')"
    >
        <p v-if="reading" class="text-[11px] text-gray-400">{{ $t('nexus.moment_reading') }}</p>
        <p v-if="failure" data-moment-failed class="text-[11px] text-red-500">{{ failure }}</p>

        <div v-if="form && moment" class="space-y-2">
            <div class="flex items-start justify-between gap-2">
                <textarea
                    v-model="form.title"
                    data-moment-title
                    rows="2"
                    :aria-label="$t('nexus.extract_field_title')"
                    class="min-w-0 flex-grow resize-none rounded-md border border-gray-200 bg-white px-1.5 py-1 text-[13px] font-semibold text-gray-900 outline-none focus:border-indigo-500 dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-100"
                />
                <button
                    type="button"
                    data-moment-close
                    :aria-label="$t('nexus.moment_close')"
                    class="flex-shrink-0 px-1 text-gray-400 hover:text-gray-700 dark:hover:text-gray-200"
                    @click="emit('close')"
                >×</button>
            </div>

            <div class="flex flex-wrap gap-1">
                <input v-model="form.from" data-moment-from type="date" :aria-label="$t('nexus.extract_field_from')" class="h-6 rounded border border-gray-200 bg-white px-1 text-[11px] dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-100" />
                <input v-model="form.to" data-moment-to type="date" :aria-label="$t('nexus.extract_field_to')" class="h-6 rounded border border-gray-200 bg-white px-1 text-[11px] dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-100" />
                <input v-model="form.time" data-moment-time type="text" placeholder="14:00" :aria-label="$t('nexus.extract_field_time')" class="h-6 w-16 rounded border border-gray-200 bg-white px-1 text-[11px] dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-100" />
            </div>

            <div class="flex flex-wrap items-center gap-1" data-moment-people>
                <span
                    v-for="(person, at) in form.people"
                    :key="`${person.name}-${at}`"
                    class="flex items-center gap-1 rounded-full bg-gray-100 px-1.5 py-0.5 text-[11px] dark:bg-[#2c2c2e]"
                >
                    {{ person.name }}
                    <select
                        v-if="!person.id"
                        data-moment-assign
                        :aria-label="$t('nexus.extract_assign')"
                        class="max-w-24 bg-transparent text-[10px] text-indigo-600 dark:text-indigo-400"
                        @change="assign(at, ($event.target as HTMLSelectElement).value)"
                    >
                        <option value="">{{ $t('nexus.extract_assign') }}</option>
                        <option v-for="known in moment.known_people" :key="known.id" :value="known.id">{{ known.title }}</option>
                    </select>
                    <button type="button" class="text-gray-400 hover:text-red-500" @click="form.people.splice(at, 1)">×</button>
                </span>
                <input
                    v-model="adding"
                    data-moment-add-person
                    type="text"
                    :placeholder="$t('nexus.extract_add_person')"
                    class="h-6 w-24 rounded border border-gray-200 bg-white px-1 text-[11px] dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-100"
                    @keydown.enter.prevent="addPerson()"
                />
            </div>

            <div class="flex flex-wrap gap-1">
                <input v-model="form.place" data-moment-where type="text" :placeholder="$t('nexus.extract_field_where')" :aria-label="$t('nexus.extract_field_where')" class="h-6 min-w-0 flex-1 rounded border border-gray-200 bg-white px-1 text-[11px] dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-100" />
                <select v-model="form.category" data-moment-category :aria-label="$t('nexus.extract_field_category')" class="h-6 rounded border border-gray-200 bg-white px-1 text-[11px] dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-100">
                    <option v-for="kind in kinds" :key="kind" :value="kind">{{ kindName(kind) }}</option>
                </select>
            </div>

            <div class="flex flex-wrap gap-1">
                <input v-model="form.amount" data-moment-amount type="number" min="0" :placeholder="$t('nexus.extract_field_amount')" :aria-label="$t('nexus.extract_field_amount')" class="h-6 w-24 rounded border border-gray-200 bg-white px-1 text-[11px] dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-100" />
                <input v-model="form.unit" data-moment-unit type="text" :aria-label="$t('nexus.extract_field_unit')" class="h-6 w-14 rounded border border-gray-200 bg-white px-1 text-[11px] dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-100" />
                <input v-model="form.about" data-moment-about type="text" :placeholder="$t('nexus.extract_field_about')" :aria-label="$t('nexus.extract_field_about')" class="h-6 min-w-0 flex-1 rounded border border-gray-200 bg-white px-1 text-[11px] dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-100" />
            </div>

            <!-- Where it came from: readable, and not editable. The words are
                 what makes it answerable to the note. -->
            <p v-if="moment.quote" data-moment-quote class="text-[11px] italic text-gray-500 dark:text-gray-400">“{{ moment.quote }}”</p>
            <p v-if="moment.source_node" class="text-[10px] text-gray-400">
                <button
                    type="button"
                    data-moment-source
                    class="underline decoration-dotted underline-offset-2 hover:text-gray-700 dark:hover:text-gray-200"
                    @click="emit('open', moment.source_node, moment.quote ?? '')"
                >{{ $t('nexus.moment_from', { title: moment.source_node.split('/').pop() }) }}</button>
            </p>
            <p v-else class="text-[10px] text-gray-400">{{ $t('nexus.moment_by_hand') }}</p>

            <div class="flex items-center gap-2 pt-0.5">
                <button
                    type="button"
                    data-moment-save
                    class="flex items-center gap-1 rounded-md bg-indigo-600 px-2.5 py-1 text-[11px] font-semibold text-white hover:bg-indigo-700 disabled:opacity-40"
                    :disabled="saving || !form.title.trim()"
                    @click="save()"
                >
                    <Loader2 v-if="saving" class="h-3 w-3 animate-spin" />
                    {{ $t('nexus.moment_save') }}
                </button>
                <!-- Letting it go is two presses: it is a decision being undone,
                     and the file goes to the trash rather than away. -->
                <template v-if="askingToDelete">
                    <span class="text-[11px] text-gray-500 dark:text-gray-400">{{ $t('nexus.moment_delete_sure') }}</span>
                    <button
                        type="button"
                        data-moment-delete-yes
                        class="rounded-md bg-red-600 px-2 py-0.5 text-[11px] font-semibold text-white hover:bg-red-700"
                        @click="letGo()"
                    >{{ $t('nexus.moment_delete_yes') }}</button>
                    <button
                        type="button"
                        class="text-[11px] text-gray-500 hover:text-gray-700 dark:hover:text-gray-300"
                        @click="askingToDelete = false"
                    >{{ $t('nexus.moment_delete_no') }}</button>
                </template>
                <button
                    v-else
                    type="button"
                    data-moment-delete
                    class="ml-auto flex items-center gap-1 rounded-md px-2 py-0.5 text-[11px] text-gray-500 hover:bg-red-50 hover:text-red-600 dark:hover:bg-red-900/20"
                    @click="askingToDelete = true"
                ><Trash2 class="h-3 w-3" /> {{ $t('nexus.moment_delete') }}</button>
            </div>
        </div>
    </div>
</template>
