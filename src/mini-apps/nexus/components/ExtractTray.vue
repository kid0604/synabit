<script setup lang="ts">
/**
 * The tray for what Syn read out of the person's notes (Nhát E).
 *
 * The content only: where this is shown is `NexusApp`'s business, and it is
 * a screen of its own now. It used to carry its own button and open a 384px
 * popover — from a row of buttons inside another popover, behind one called
 * "Look back". Reviewing is a long read, and it was given the least room on
 * the screen.
 *
 * Every row is a proposal: nothing here has reached a note until "Keep" is
 * pressed, and until then the timeline leaves it out of every answer. Reading
 * is off until turned on here, and turning it on says first what is sent where
 * (§8.6 of `docs/timeline-2026-09-17.md`). See `src-tauri/src/timeline/extract.rs`.
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { invoke } from '@tauri-apps/api/core';
import { Check, Pencil, X, Loader2 } from 'lucide-vue-next';
import { logger } from '../../../utils/logger';
import { useAppLockStore } from '../../../stores/useAppLockStore';
import { errorText } from '../../../shared/errorText';
import { withKind, saveKinds } from '../../../shared/timelineReading';
import type { ExtractStatus, MomentView, Proposal } from '../../../shared/timelineReading';

// The shapes moved to `shared/timelineReading` when the settings did; named
// here still so the screens that import them from the tray keep working.
export type { ExtractStatus, MomentView, Proposal };

const props = defineProps<{ vaultPath: string }>();
const emit = defineEmits<{
    (e: 'changed'): void;
    /** Open the note this was read from, at the line it was read from. */
    (e: 'open', id: string, type: string, quote: string): void;
    /** Show the reading settings, which are the app's settings and not this screen's. */
    (e: 'settings'): void;
}>();

const appLock = useAppLockStore();

const { t, te, locale } = useI18n();
const status = ref<ExtractStatus | null>(null);
const failure = ref('');

const load = async () => {
    try {
        status.value = await invoke<ExtractStatus>('timeline_extract_status', { vaultPath: props.vaultPath });
    } catch (e) {
        logger.error('Could not read the extraction status', e);
    }
};

onMounted(load);

/**
 * The proposals, by the day they are about, newest first.
 *
 * A queue read as one long list is read one card at a time; read by day it is
 * read as a day, which is how somebody remembers whether all of it happened —
 * and it is why a whole day can be kept in one press.
 */
const byDay = computed(() => {
    const days = new Map<string, Proposal[]>();
    for (const p of status.value?.proposals ?? []) {
        const day = p.happened_from;
        if (!days.has(day)) days.set(day, []);
        days.get(day)!.push(p);
    }
    return [...days.entries()].sort((a, b) => b[0].localeCompare(a[0])).map(([day, proposals]) => ({ day, proposals }));
});

/** A change to a moment already kept (§15) rather than something new. */
const isAChange = (p: Proposal) => !!p.about_moment;
const keptMoment = (p: Proposal) => (p.about_moment ? status.value?.moments[p.about_moment] ?? null : null);

/**
 * What a change would change, field by field: what is kept, what is proposed.
 *
 * Fields the person wrote themselves are never in it — the reader does not
 * write over a decision (§15.2).
 */
const diff = (p: Proposal) => {
    const was = keptMoment(p);
    if (!was) return [];
    const rows: { field: string; before: string; after: string }[] = [];
    const add = (field: string, before: string, after: string) => {
        if (was.hand.includes(field) || before.trim() === after.trim()) return;
        rows.push({ field: t(`nexus.extract_field_${field}`), before, after });
    };
    add('title', was.title, p.title);
    add('happened', was.happened, when(p));
    add('time', was.time ?? '', p.time ?? '');
    add('people', was.people.join(', '), who(p));
    add('where', was.place ?? '', p.place ?? '');
    add('category', was.category ? kindName(was.category) : '', p.category ? kindName(p.category) : '');
    add('amount', money(was.amount), money(p.amount));
    return rows;
};

/**
 * The proposal being edited, and every field of it as the person is putting
 * it right.
 *
 * A proposal is often nearly right — the right day, the right people, a
 * sentence that is not quite what happened. Keep-or-discard makes a nearly
 * right one either kept wrong or thrown away, and both are worse than five
 * seconds of typing.
 *
 * The quote is not here. It is the line in the note this was read from, and
 * it is shown exactly as it was written — see `timeline_extract_review`.
 */
interface Editing {
    title: string;
    from: string;
    to: string;
    time: string;
    /** Who took part: an id when somebody is known, a name otherwise. */
    people: { id: string | null; name: string }[];
    place: string;
    category: string;
    amount: string;
    unit: string;
    about: string;
}

const editing = ref<string | null>(null);
const form = ref<Editing | null>(null);
const adding = ref('');

const startEditing = (p: Proposal) => {
    editing.value = p.id;
    adding.value = '';
    form.value = {
        title: p.title,
        from: p.happened_from,
        to: p.happened_to,
        time: p.time ?? '',
        people: [
            ...p.people.map(person => ({ id: person.id, name: person.title })),
            ...p.names.map(name => ({ id: null, name })),
        ],
        place: p.place ?? '',
        category: p.category ?? 'other',
        amount: p.amount ? String(p.amount.value) : '',
        unit: p.amount?.unit ?? 'VND',
        about: p.about.join(', '),
    };
};

const stopEditing = () => {
    editing.value = null;
    form.value = null;
};

const addPerson = () => {
    const name = adding.value.trim();
    if (!name || !form.value) return;
    form.value.people.push({ id: null, name });
    adding.value = '';
};

/** Saying who a name is: it becomes that person's alias, once and for all. */
const assign = (at: number, id: string) => {
    if (!form.value) return;
    const person = status.value?.people.find(p => p.id === id);
    if (!person) return;
    // The name as the note wrote it is what becomes the alias — the point is
    // that the next reading knows who "Cam" is.
    assigned.value.push({ name: form.value.people[at].name, node: id });
    form.value.people[at] = { id, name: person.title };
};
const assigned = ref<{ name: string; node: string }[]>([]);

/**
 * The kinds a moment can be, and what to call each on screen.
 *
 * The list comes from the vault (`Config::categories`), not from here: a life
 * is not a list somebody else wrote, and the first reading of a real vault put
 * 60% of what was kept in `meal` and had no kind at all for the weather. The
 * ones this app ships with have a translated name; a kind the person added is
 * shown as they wrote it.
 */
const kinds = computed(() => status.value?.categories ?? []);
const kindName = (kind: string) => (te(`nexus.moment_category_${kind}`) ? t(`nexus.moment_category_${kind}`) : kind);

/** A kind of one's own, added where it was needed rather than in a settings screen. */
const addingKind = ref(false);
const newKind = ref('');

const addKind = async () => {
    if (!status.value) return;
    const kind = newKind.value.trim().toLowerCase();
    const list = withKind(kinds.value, kind);
    addingKind.value = false;
    newKind.value = '';
    if (!kind) return;
    if (list !== kinds.value) {
        failure.value = '';
        try {
            await saveKinds(props.vaultPath, status.value.config, list);
            status.value = { ...status.value, categories: list, config: { ...status.value.config, categories: list } };
        } catch (e) {
            failure.value = errorText(e);
        }
    }
    if (form.value) form.value.category = kind;
};

const review = async (proposal: Proposal, accept: boolean) => {
    failure.value = '';
    const edits =
        accept && editing.value === proposal.id && form.value
            ? {
                  title: form.value.title,
                  happened_from: form.value.from,
                  happened_to: form.value.to || form.value.from,
                  precision: (form.value.to || form.value.from) === form.value.from ? 'day' : 'range',
                  time: form.value.time,
                  people: form.value.people.map(person => person.id ?? person.name),
                  place: form.value.place,
                  category: form.value.category,
                  amount: form.value.amount ? Number(form.value.amount) : null,
                  unit: form.value.unit,
                  about: form.value.about.split(',').map(a => a.trim()).filter(Boolean),
              }
            : null;
    const said = editing.value === proposal.id ? assigned.value : [];
    try {
        await invoke('timeline_extract_review', {
            vaultPath: props.vaultPath,
            itemId: proposal.id,
            accept,
            nodeId: proposal.node_id,
            edits,
            assigned: said,
        });
        if (editing.value === proposal.id) {
            stopEditing();
            assigned.value = [];
        }
        // Decided: the next card takes the selection, so a pass done from the
        // keyboard carries on rather than stopping on a card that is gone.
        if (selected.value === proposal.id) {
            const all = inOrder.value;
            const at = all.findIndex(p => p.id === proposal.id);
            selected.value = (all[at + 1] ?? all[at - 1])?.id ?? null;
        }
        if (status.value) status.value.proposals = status.value.proposals.filter(p => p.id !== proposal.id);
        emit('changed');
    } catch (e) {
        failure.value = errorText(e);
    }
};

/** A whole day at once, for a day that reads right the way it stands. */
const keepDay = async (day: string) => {
    const of_the_day = (status.value?.proposals ?? []).filter(p => p.happened_from === day && !p.stale && !isAChange(p));
    for (const p of of_the_day) {
        await review(p, true);
    }
};

/**
 * Moving and deciding without the mouse: `j`/`k` to move, `Enter` to keep,
 * `e` to put right, `x` to discard.
 *
 * Twenty cards is a pass; a pass done by pointing at three buttons a card is
 * not done. Keys are ignored while something is being typed into, so `x` in a
 * sentence stays an `x`.
 */
const selected = ref<string | null>(null);
const inOrder = computed(() => byDay.value.flatMap(group => group.proposals));

const move = (by: number) => {
    const all = inOrder.value;
    if (!all.length) return;
    const at = all.findIndex(p => p.id === selected.value);
    const next = at < 0 ? 0 : Math.min(all.length - 1, Math.max(0, at + by));
    selected.value = all[next].id;
    document.querySelector(`[data-proposal="${all[next].id}"]`)?.scrollIntoView({ block: 'nearest' });
};

const onKey = (event: KeyboardEvent) => {
    const target = event.target as HTMLElement | null;
    if (target && (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName))) return;
    const current = inOrder.value.find(p => p.id === selected.value);
    switch (event.key) {
        case 'j':
            move(1);
            break;
        case 'k':
            move(-1);
            break;
        case 'Enter':
            if (current && !current.stale) void review(current, true);
            break;
        case 'e':
            if (current) startEditing(current);
            break;
        case 'x':
            if (current) void review(current, false);
            break;
        default:
            return;
    }
    event.preventDefault();
};

onMounted(() => window.addEventListener('keydown', onKey));
onBeforeUnmount(() => window.removeEventListener('keydown', onKey));

/**
 * The note a proposal was read from, opened where it stands.
 *
 * # Why in the card and not by leaving
 *
 * Some of these notes are months old. A sentence a model wrote out of one is
 * not enough to say *yes, that happened, on that day, with those people* —
 * the person has to read the paragraph around it. Until now the card said
 * «From 2026-06-07, written 2026-06-07» and gave no way to see it, so keeping
 * an old proposal meant trusting it.
 *
 * Opening the note would work, but it costs your place: a queue of twenty is
 * reviewed in one pass, and leaving it for every third row is how a queue
 * stops being reviewed. So the source comes to the card. Editing the note
 * itself is still a press away, and that one is worth leaving for.
 */
type Source = 'loading' | { content: string; locked: boolean };
const sources = ref<Record<string, Source>>({});

/* Read in the script, not cast in the template: a type assertion in an
   attribute is a `{` the template compiler reads as the start of an object. */
const openedSource = (id: string) => !!sources.value[id];
const readingSource = (id: string) => sources.value[id] === 'loading';
const lockedSource = (id: string) => {
    const found = sources.value[id];
    return typeof found === 'object' && found.locked;
};
const shownSource = (id: string) => {
    const found = sources.value[id];
    return typeof found === 'object' && !found.locked;
};
const sourceText = (id: string) => {
    const found = sources.value[id];
    return typeof found === 'object' ? found.content : '';
};

const showSource = async (p: Proposal) => {
    if (sources.value[p.id]) {
        // Pressing it again folds it away.
        const next = { ...sources.value };
        delete next[p.id];
        sources.value = next;
        return;
    }
    sources.value = { ...sources.value, [p.id]: 'loading' };
    // A locked note is not read at all here. The tray shows what the vault
    // holds, and `NexusApp` hides a protected note's words in its own list
    // for the same reason; a second surface must not be the way around it.
    if (appLock.isEnabled && appLock.isNoteProtected(p.node_id)) {
        sources.value = { ...sources.value, [p.id]: { content: '', locked: true } };
        return;
    }
    try {
        const item = await invoke<{ content: string }>('get_nexus_item', {
            vaultPath: props.vaultPath,
            id: p.node_id,
        });
        sources.value = { ...sources.value, [p.id]: { content: item.content ?? '', locked: false } };
    } catch (e) {
        const next = { ...sources.value };
        delete next[p.id];
        sources.value = next;
        failure.value = errorText(e);
        logger.error('Could not read the note this was proposed from', e);
    }
};

/**
 * The note, split around the line the proposal was read from.
 *
 * Plain text and a marked run rather than rendered markdown: what is being
 * checked is *the words the model read*, and rendering would hide the very
 * markup that sometimes ends up inside a quote.
 */
const around = (p: Proposal, content: string) => {
    const quote = (p.quote ?? '').trim();
    const at = quote ? content.indexOf(quote) : -1;
    if (at < 0) return [{ text: content, hit: false }];
    return [
        { text: content.slice(0, at), hit: false },
        { text: content.slice(at, at + quote.length), hit: true },
        { text: content.slice(at + quote.length), hit: false },
    ].filter(part => part.text.length > 0);
};

/**
 * A quote with its link syntax unwrapped.
 *
 * A real vault showed why: one proposal quoted
 * «Trao đổi với [Nguyễn Lê Vũ Phương Hoàng:](synabit://person/People/77a…md)»
 * — the person's name was there, wrapped in forty characters of uuid nobody
 * can read. The words stay exactly as written; only the brackets go.
 */
const readable = (text: string | null | undefined) =>
    (text ?? '').replace(/\[([^\]]*)\]\([^)]*\)/g, '$1').replace(/\s+/g, ' ').trim();

const when = (p: Proposal) => {
    if (p.precision === 'day') return p.happened_from;
    if (p.precision === 'month') return p.happened_from.slice(0, 7);
    if (p.precision === 'year') return p.happened_from.slice(0, 4);
    return `${p.happened_from} → ${p.happened_to}`;
};

const who = (p: Proposal) => [...p.people.map(person => person.title), ...p.names].join(', ');

/** A sum as the reader's own language writes one: 50.000 ₫, not 50000 VND. */
const money = (amount: Proposal['amount']) => {
    if (!amount) return '';
    try {
        return new Intl.NumberFormat(locale.value, { style: 'currency', currency: amount.unit, maximumFractionDigits: 0 }).format(amount.value);
    } catch {
        // A unit that is not a currency code: say it as written.
        return `${amount.value} ${amount.unit}`;
    }
};

/** What else was read about it, in one line: kind, where, how much. */
const details = (p: Proposal) =>
    [
        p.category && p.category !== 'other' ? kindName(p.category) : '',
        p.place ?? '',
        money(p.amount),
    ].filter(Boolean).join(' · ');
</script>

<template>
    <div v-if="status" data-extract-tray class="space-y-4">
        <p data-is-an-event class="rounded-lg bg-indigo-50 px-3 py-2.5 text-[13px] font-medium leading-relaxed text-indigo-900 dark:bg-indigo-950/50 dark:text-indigo-200">
            {{ $t('nexus.extract_is_an_event') }}
        </p>
        <p class="text-[13px] leading-relaxed text-gray-500 dark:text-gray-400">{{ $t('nexus.extract_explain') }}</p>

            <p v-if="failure" class="text-[11px] text-red-500">{{ failure }}</p>

            <template v-if="status.proposals.length">
                <p data-shortcuts class="text-[10px] text-gray-400">{{ $t('nexus.extract_shortcuts') }}</p>
                <section v-for="group in byDay" :key="group.day" data-day class="space-y-2">
                    <div class="flex items-center justify-between gap-2">
                        <h3 class="text-[11px] font-semibold uppercase tracking-wide text-gray-400">{{ group.day }}</h3>
                        <button
                            v-if="group.proposals.some(p => !isAChange(p) && !p.stale)"
                            type="button"
                            data-keep-day
                            class="rounded-md px-2 py-0.5 text-[11px] font-semibold text-emerald-700 hover:bg-emerald-50 dark:text-emerald-400 dark:hover:bg-emerald-900/20"
                            @click="keepDay(group.day)"
                        >{{ $t('nexus.extract_keep_day', { count: group.proposals.filter(p => !isAChange(p) && !p.stale).length }) }}</button>
                    </div>
                    <ul data-proposals class="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
                        <li
                            v-for="p in group.proposals"
                            :key="p.id"
                            :data-proposal="p.id"
                            :data-selected="selected === p.id ? 'yes' : undefined"
                            :class="[
                                'space-y-1 rounded-lg border p-2.5',
                                selected === p.id ? 'border-indigo-400 dark:border-indigo-500' : 'border-gray-200 dark:border-[#3a3a3c]',
                            ]"
                            @click="selected = p.id"
                        >
                            <!-- A change to a moment already kept (§15): what it
                                 says now, and what the note says now. -->
                            <template v-if="isAChange(p)">
                                <p data-change class="text-[11px] font-semibold text-amber-700 dark:text-amber-400">
                                    {{ p.verdict === 'gone' ? $t('nexus.extract_note_gone') : p.verdict === 'retracted' ? $t('nexus.extract_words_gone') : $t('nexus.extract_source_changed') }}
                                </p>
                                <p class="text-xs font-semibold text-gray-900 dark:text-gray-100">{{ keptMoment(p)?.title ?? p.title }}</p>
                                <ul v-if="diff(p).length" data-diff class="space-y-0.5 text-[11px] text-gray-600 dark:text-gray-300">
                                    <li v-for="row in diff(p)" :key="row.field">
                                        <span class="text-gray-400">{{ row.field }}:</span>
                                        <span class="line-through decoration-gray-400">{{ row.before || '—' }}</span>
                                        →
                                        <span class="font-medium">{{ row.after || '—' }}</span>
                                    </li>
                                </ul>
                                <p v-if="keptMoment(p)?.hand.length" data-hand class="text-[10px] text-gray-400">
                                    {{ $t('nexus.extract_hand', { fields: keptMoment(p)!.hand.map(f => $t(`nexus.extract_field_${f}`)).join(', ') }) }}
                                </p>
                                <p v-if="p.quote" class="text-[11px] italic text-gray-500 dark:text-gray-400">“{{ readable(p.quote) }}”</p>
                                <div class="flex gap-2 pt-0.5">
                                    <template v-if="p.verdict === 'changed'">
                                        <button type="button" data-accept class="flex items-center gap-1 rounded-md bg-emerald-600 px-2 py-0.5 text-[11px] font-semibold text-white hover:bg-emerald-700" @click="review(p, true)">
                                            <Check class="h-3 w-3" /> {{ $t('nexus.extract_apply') }}
                                        </button>
                                        <button type="button" data-decline class="flex items-center gap-1 rounded-md px-2 py-0.5 text-[11px] text-gray-500 hover:bg-gray-100 dark:hover:bg-[#3a3a3c]" @click="review(p, false)">
                                            {{ $t('nexus.extract_leave_as_is') }}
                                        </button>
                                    </template>
                                    <template v-else>
                                        <button type="button" data-decline class="flex items-center gap-1 rounded-md bg-gray-100 px-2 py-0.5 text-[11px] font-semibold text-gray-700 hover:bg-gray-200 dark:bg-[#3a3a3c] dark:text-gray-200" @click="review(p, false)">
                                            {{ $t('nexus.extract_keep_moment') }}
                                        </button>
                                        <button type="button" data-accept class="flex items-center gap-1 rounded-md px-2 py-0.5 text-[11px] text-red-600 hover:bg-red-50 dark:hover:bg-red-900/20" @click="review(p, true)">
                                            <X class="h-3 w-3" /> {{ $t('nexus.extract_drop_moment') }}
                                        </button>
                                    </template>
                                </div>
                            </template>

                            <template v-else>
                                <!-- Being put right: every field, because a
                                     proposal is wrong in whichever one it is
                                     wrong in. -->
                                <div v-if="editing === p.id && form" data-edit-form class="space-y-1.5">
                                    <textarea
                                        v-model="form.title"
                                        data-proposal-title
                                        rows="2"
                                        :aria-label="$t('nexus.extract_field_title')"
                                        class="w-full resize-none rounded-md border border-indigo-300 bg-white px-1.5 py-1 text-xs font-semibold text-gray-900 outline-none focus:border-indigo-500 dark:border-indigo-700 dark:bg-[#1c1c1e] dark:text-gray-100"
                                        @keydown.enter.prevent="review(p, true)"
                                        @keydown.esc="stopEditing()"
                                    />
                                    <div class="flex flex-wrap gap-1">
                                        <input v-model="form.from" data-field-from type="date" :aria-label="$t('nexus.extract_field_from')" class="h-6 rounded border border-gray-200 bg-white px-1 text-[11px] dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-100" />
                                        <input v-model="form.to" data-field-to type="date" :aria-label="$t('nexus.extract_field_to')" class="h-6 rounded border border-gray-200 bg-white px-1 text-[11px] dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-100" />
                                        <input v-model="form.time" data-field-time type="text" placeholder="14:00" :aria-label="$t('nexus.extract_field_time')" class="h-6 w-16 rounded border border-gray-200 bg-white px-1 text-[11px] dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-100" />
                                    </div>
                                    <div class="flex flex-wrap items-center gap-1" data-field-people>
                                        <span
                                            v-for="(person, at) in form.people"
                                            :key="`${person.name}-${at}`"
                                            class="flex items-center gap-1 rounded-full bg-gray-100 px-1.5 py-0.5 text-[11px] dark:bg-[#2c2c2e]"
                                        >
                                            {{ person.name }}
                                            <!-- A name that matched nobody: saying who
                                                 it is teaches the next reading. -->
                                            <select
                                                v-if="!person.id"
                                                :aria-label="$t('nexus.extract_assign')"
                                                data-assign
                                                class="max-w-24 bg-transparent text-[10px] text-indigo-600 dark:text-indigo-400"
                                                @change="assign(at, ($event.target as HTMLSelectElement).value)"
                                            >
                                                <option value="">{{ $t('nexus.extract_assign') }}</option>
                                                <option v-for="known in status.people" :key="known.id" :value="known.id">{{ known.title }}</option>
                                            </select>
                                            <button type="button" class="text-gray-400 hover:text-red-500" @click="form.people.splice(at, 1)">×</button>
                                        </span>
                                        <input
                                            v-model="adding"
                                            data-add-person
                                            type="text"
                                            :placeholder="$t('nexus.extract_add_person')"
                                            class="h-6 w-24 rounded border border-gray-200 bg-white px-1 text-[11px] dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-100"
                                            @keydown.enter.prevent="addPerson()"
                                        />
                                    </div>
                                    <div class="flex flex-wrap gap-1">
                                        <input v-model="form.place" data-field-where type="text" :placeholder="$t('nexus.extract_field_where')" :aria-label="$t('nexus.extract_field_where')" class="h-6 min-w-0 flex-1 rounded border border-gray-200 bg-white px-1 text-[11px] dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-100" />
                                        <select
                                            v-if="!addingKind"
                                            v-model="form.category"
                                            data-field-category
                                            :aria-label="$t('nexus.extract_field_category')"
                                            class="h-6 rounded border border-gray-200 bg-white px-1 text-[11px] dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-100"
                                            @change="($event.target as HTMLSelectElement).value === '__new' && (addingKind = true)"
                                        >
                                            <option v-for="kind in kinds" :key="kind" :value="kind">{{ kindName(kind) }}</option>
                                            <!-- A kind of one's own, added where it turned out to be
                                                 missing rather than in a settings screen somewhere else. -->
                                            <option value="__new">{{ $t('nexus.extract_new_kind') }}…</option>
                                        </select>
                                        <input
                                            v-else
                                            v-model="newKind"
                                            data-new-kind
                                            type="text"
                                            :placeholder="$t('nexus.extract_new_kind')"
                                            :aria-label="$t('nexus.extract_new_kind')"
                                            class="h-6 w-28 rounded border border-indigo-300 bg-white px-1 text-[11px] dark:border-indigo-700 dark:bg-[#1c1c1e] dark:text-gray-100"
                                            @keydown.enter.prevent="addKind()"
                                            @keydown.esc="addingKind = false"
                                            @blur="addKind()"
                                        />
                                    </div>
                                    <div class="flex flex-wrap gap-1">
                                        <input v-model="form.amount" data-field-amount type="number" min="0" :placeholder="$t('nexus.extract_field_amount')" :aria-label="$t('nexus.extract_field_amount')" class="h-6 w-24 rounded border border-gray-200 bg-white px-1 text-[11px] dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-100" />
                                        <input v-model="form.unit" data-field-unit type="text" :aria-label="$t('nexus.extract_field_unit')" class="h-6 w-14 rounded border border-gray-200 bg-white px-1 text-[11px] dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-100" />
                                        <input v-model="form.about" data-field-about type="text" :placeholder="$t('nexus.extract_field_about')" :aria-label="$t('nexus.extract_field_about')" class="h-6 min-w-0 flex-1 rounded border border-gray-200 bg-white px-1 text-[11px] dark:border-[#3a3a3c] dark:bg-[#1c1c1e] dark:text-gray-100" />
                                    </div>
                                </div>

                                <template v-else>
                                    <div class="flex items-start justify-between gap-2">
                                        <p class="text-xs font-semibold text-gray-900 dark:text-gray-100">{{ p.title }}</p>
                                        <!-- The model's own score was here, and it was always 98%. What
                                             is worth saying is whether the day was read or guessed. -->
                                        <span
                                            v-if="p.date_basis === 'inferred'"
                                            data-day-guessed
                                            class="flex-shrink-0 rounded bg-gray-100 px-1 text-[10px] text-gray-500 dark:bg-[#2c2c2e] dark:text-gray-400"
                                        >{{ $t('nexus.extract_day_guessed') }}</span>
                                    </div>
                                    <p class="text-[11px] tabular-nums text-gray-600 dark:text-gray-300">
                                        {{ [when(p), p.time].filter(Boolean).join(' ') }}<template v-if="who(p)"> · {{ who(p) }}</template>
                                    </p>
                                    <p v-if="details(p)" data-proposal-details class="text-[11px] text-gray-500 dark:text-gray-400">{{ details(p) }}</p>
                                </template>

                                <p class="text-[11px] italic text-gray-500 dark:text-gray-400">“{{ readable(p.quote) }}”</p>
                                <p class="text-[10px] text-gray-400">
                                    <!-- The note it came from is a button now. It used to
                                         be this sentence and nothing else, so an old
                                         proposal could only be trusted or thrown away. -->
                                    <button
                                        type="button"
                                        data-show-source
                                        :aria-expanded="openedSource(p.id)"
                                        class="text-left underline decoration-dotted underline-offset-2 hover:text-gray-700 dark:hover:text-gray-200"
                                        @click="showSource(p)"
                                    >{{ $t('nexus.extract_from', { title: p.node_title, day: p.recorded }) }}</button>
                                    <span v-if="p.stale" class="ml-1 rounded bg-amber-100 px-1 text-amber-700 dark:bg-amber-900/30 dark:text-amber-300">{{ $t('nexus.extract_stale_badge') }}</span>
                                </p>

                                <!-- The note itself, with the line this was read from
                                     marked in it. -->
                                <div v-if="openedSource(p.id)" data-source class="rounded-md border border-gray-200 bg-gray-50 p-2 dark:border-[#3a3a3c] dark:bg-[#1e1e20]">
                                    <p v-if="readingSource(p.id)" class="text-[10px] text-gray-400">{{ $t('nexus.extract_source_reading') }}</p>
                                    <p v-else-if="lockedSource(p.id)" data-source-locked class="text-[11px] italic text-gray-400">
                                        {{ $t('nexus.extract_source_locked') }}
                                    </p>
                                    <template v-else-if="shownSource(p.id)">
                                        <p class="max-h-48 overflow-y-auto whitespace-pre-wrap text-[11px] leading-relaxed text-gray-700 dark:text-gray-300">
                                            <span
                                                v-for="(part, i) in around(p, sourceText(p.id))"
                                                :key="i"
                                                :data-hit="part.hit ? 'yes' : undefined"
                                                :class="part.hit ? 'rounded bg-amber-200/70 font-medium dark:bg-amber-500/30' : ''"
                                            >{{ part.text }}</span>
                                        </p>
                                        <!-- Reading it here is enough to decide;
                                             changing it is worth leaving for. -->
                                        <button
                                            type="button"
                                            data-open-source
                                            class="mt-1.5 text-[10px] font-semibold text-indigo-600 underline decoration-dotted underline-offset-2 dark:text-indigo-400"
                                            @click="emit('open', p.node_id, p.node_type, p.quote)"
                                        >{{ $t('nexus.extract_source_open') }}</button>
                                    </template>
                                </div>
                                <div class="flex gap-2 pt-0.5">
                                    <button
                                        type="button"
                                        data-accept
                                        class="flex items-center gap-1 rounded-md bg-emerald-600 px-2 py-0.5 text-[11px] font-semibold text-white hover:bg-emerald-700 disabled:opacity-40"
                                        :disabled="p.stale"
                                        :title="p.stale ? $t('nexus.extract_stale_keep') : undefined"
                                        @click="review(p, true)"
                                    ><Check class="h-3 w-3" /> {{ $t('nexus.extract_accept') }}</button>
                                    <!-- Between keeping it wrong and throwing it away. -->
                                    <button
                                        v-if="editing !== p.id"
                                        type="button"
                                        data-edit
                                        class="flex items-center gap-1 rounded-md px-2 py-0.5 text-[11px] text-gray-500 hover:bg-gray-100 dark:hover:bg-[#3a3a3c]"
                                        @click="startEditing(p)"
                                    ><Pencil class="h-3 w-3" /> {{ $t('nexus.extract_edit') }}</button>
                                    <button
                                        type="button"
                                        data-decline
                                        class="flex items-center gap-1 rounded-md px-2 py-0.5 text-[11px] text-gray-500 hover:bg-gray-100 dark:hover:bg-[#3a3a3c]"
                                        @click="review(p, false)"
                                    ><X class="h-3 w-3" /> {{ $t('nexus.extract_decline') }}</button>
                                </div>
                            </template>
                        </li>
                    </ul>
                </section>
            </template>
            <!-- Nothing waiting, or nothing reading. Either way the next
                 thing to do is in the settings, so say where they are rather
                 than leaving somebody to find them. -->
            <div v-else data-nothing-waiting class="space-y-2">
                <p class="text-[13px] text-gray-400">
                    {{ status.config.enabled ? t('nexus.extract_none') : t('nexus.extract_off_here') }}
                </p>
                <button
                    type="button"
                    data-open-settings
                    class="rounded-md border border-gray-200 px-2.5 py-1 text-[12px] font-medium text-gray-700 hover:bg-gray-50 dark:border-[#3a3a3c] dark:text-gray-200 dark:hover:bg-[#2c2c2e]"
                    @click="emit('settings')"
                >{{ t('nexus.extract_open_settings') }}</button>
            </div>
    </div>
</template>
