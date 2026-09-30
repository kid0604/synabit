<script setup lang="ts">
/**
 * Bringing an address book in.
 *
 * Four steps, and most files skip two of them: a Google or Outlook export
 * with nobody already in the vault goes straight from picking the file to the
 * summary. The mapping step appears only when a column was not recognised,
 * and the duplicates step only when somebody is already there — a step shown
 * with nothing to decide is a step that teaches people to click through.
 */
import AppDialog from '../../shared/components/AppDialog.vue';
import { ref, computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { X, Upload, FileText, Users, AlertTriangle, Check, Loader } from 'lucide-vue-next';
import { useNodeService } from '../../composables/useNodeService';
import { useContactExchange, type ContactImport, type DuplicateReport, type Decision, type ImportReport } from './composables/useContactExchange';
import { logger } from '../../utils/logger';

const props = defineProps<{ vaultPath: string }>();
const emit = defineEmits(['close', 'imported']);

const ns = useNodeService();
const { t } = useI18n();
const exchange = useContactExchange(ns, () => props.vaultPath);

type Step = 'pick' | 'map' | 'review' | 'working' | 'done';
const step = ref<Step>('pick');
const error = ref('');

const source = ref('');
const format = ref<'vcard' | 'csv'>('vcard');
const contacts = ref<ContactImport[]>([]);
const duplicates = ref<DuplicateReport[]>([]);
const decisions = ref<Decision[]>([]);
const skippedRows = ref(0);
const report = ref<ImportReport | null>(null);

// The mapping step.
const headers = ref<string[]>([]);
const columns = ref<any[]>([]);
const sample = ref<string[][]>([]);

const FIELD_CHOICES = [
    { value: '', label: 'people.col_none' },
    { value: 'full_name', label: 'people.col_full_name' },
    { value: 'given_name', label: 'people.col_given_name' },
    { value: 'family_name', label: 'people.col_family_name' },
    { value: 'nickname', label: 'people.col_nickname' },
    { value: 'company', label: 'people.col_company' },
    { value: 'role', label: 'people.col_role' },
    { value: 'birthday', label: 'people.col_birthday' },
    { value: 'notes', label: 'people.col_notes' },
    { value: 'tags', label: 'people.col_tags' },
    { value: 'email', label: 'people.col_email' },
    { value: 'phone', label: 'people.col_phone' },
    { value: 'url', label: 'people.col_url' },
    { value: 'text', label: 'people.col_text' },
];

/** The column editor speaks in plain kinds; the backend wants its own shape. */
const kindOf = (column: any): string => column?.field?.kind ?? '';
const setKind = (index: number, kind: string) => {
    if (!kind) {
        columns.value[index] = { ...columns.value[index], field: null };
        return;
    }
    // A detail carries the label it will show under, and the header is the
    // best name available for it.
    const label = ['email', 'phone', 'url', 'text'].includes(kind)
        ? headers.value[index] || t('people.col_detail_default')
        : null;
    columns.value[index] = { ...columns.value[index], field: { kind, label } };
};

const load = async (withColumns?: any[]) => {
    error.value = '';
    try {
        const batch = await exchange.readContacts(source.value, withColumns);
        format.value = batch.format;
        contacts.value = batch.contacts;
        skippedRows.value = batch.skipped;
        decisions.value = batch.contacts.map(() => 'add');

        if (batch.contacts.length === 0 && batch.format === 'csv' && !withColumns) {
            // Nothing was recognised well enough to name anybody. The mapping
            // step is the answer, not an error message.
            await openMapping();
            return;
        }
        if (batch.unmapped.length > 0 && !withColumns) {
            await openMapping();
            return;
        }
        await review();
    } catch (e) {
        logger.error('Failed to read contacts', e);
        error.value = t('people.import_read_failed');
        step.value = 'pick';
    }
};

const openMapping = async () => {
    const table = await exchange.readColumns(source.value);
    headers.value = table.headers;
    columns.value = table.columns;
    sample.value = table.sample;
    step.value = 'map';
};

const review = async () => {
    duplicates.value = await exchange.findDuplicates(contacts.value);
    // Anything certain merges by default; a shared name defaults to adding,
    // because two people really can share one.
    for (const duplicate of duplicates.value) {
        if (duplicate.certain) decisions.value[duplicate.incoming] = 'merge';
    }
    step.value = 'review';
};

const choose = async () => {
    const picked = await exchange.pickFile();
    if (!picked) return;
    source.value = picked;
    await load();
};

const applyMapping = async () => {
    await load(columns.value);
};

const run = async () => {
    step.value = 'working';
    report.value = await exchange.commit(contacts.value, duplicates.value, [...decisions.value]);
    step.value = 'done';
    emit('imported');
};

const duplicateFor = (index: number) => duplicates.value.find(d => d.incoming === index);

/** The contacts worth showing a row for: everything with a decision to make. */
const decidable = computed(() =>
    duplicates.value
        .filter(d => d.existing_id !== null)
        .map(d => ({ duplicate: d, contact: contacts.value[d.incoming] }))
);

const counts = computed(() => {
    let add = 0, merge = 0, skip = 0;
    decisions.value.forEach((decision, i) => {
        if (duplicateFor(i)?.existing_incoming !== null && duplicateFor(i)?.existing_incoming !== undefined) {
            skip++;
            return;
        }
        if (decision === 'add') add++;
        else if (decision === 'merge') merge++;
        else skip++;
    });
    return { add, merge, skip };
});

const repeatedInFile = computed(() =>
    duplicates.value.filter(d => d.existing_incoming !== null && d.existing_incoming !== undefined).length
);

const reasonText = (d: DuplicateReport) => {
    if (d.reason.on === 'email') return t('people.dup_same_email', { email: d.reason.value });
    if (d.reason.on === 'phone') return t('people.dup_same_phone');
    return t('people.dup_same_name');
};

const fileName = computed(() => source.value.split(/[/\\]/).pop() || source.value);
</script>

<template>
    <AppDialog :show="true" labelledby="import-contacts-title" size="lg" unstyled
               panel-class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl shadow-2xl max-h-[90vh] flex flex-col overflow-hidden"
               @close="emit('close')">

            <div class="px-6 py-4 border-b border-border dark:border-border-dark flex items-center justify-between bg-gray-50/50 dark:bg-gray-800/50">
                <h2 id="import-contacts-title" class="text-lg font-semibold flex items-center gap-2">
                    <Upload class="w-5 h-5 text-blue-500" />
                    {{ $t('people.import_contacts') }}
                </h2>
                <button @click="emit('close')" class="p-1.5 rounded-lg hover:bg-gray-200 dark:hover:bg-gray-700 text-gray-500 dark:text-gray-400 transition-colors" :aria-label="$t('people.close')">
                    <X class="w-5 h-5" />
                </button>
            </div>

            <div class="flex-1 overflow-y-auto p-6">

                <!-- Pick a file -->
                <div v-if="step === 'pick'" class="text-center py-6">
                    <FileText class="w-12 h-12 mx-auto text-gray-500 dark:text-gray-400" aria-hidden="true" />
                    <p class="mt-4 text-sm text-gray-600 dark:text-gray-400 max-w-sm mx-auto">
                        {{ $t('people.import_desc') }}
                    </p>
                    <button @click="choose" class="mt-5 btn-primary">
                        {{ $t('people.choose_file') }}
                    </button>
                    <p v-if="error" class="mt-4 text-sm text-red-500">{{ error }}</p>
                </div>

                <!-- Map the columns -->
                <div v-else-if="step === 'map'">
                    <p class="text-sm text-gray-600 dark:text-gray-400 mb-4">
                        {{ $t('people.map_desc') }}
                    </p>
                    <div class="space-y-2">
                        <div v-for="(header, i) in headers" :key="i" class="flex items-center gap-3 py-1.5">
                            <div class="w-40 flex-shrink-0 min-w-0">
                                <p class="text-sm font-medium truncate">{{ header || '—' }}</p>
                                <p class="text-xs text-gray-500 dark:text-gray-400 truncate">{{ sample[0]?.[i] || '' }}</p>
                            </div>
                            <select
                                :value="kindOf(columns[i])"
                                @change="setKind(i, ($event.target as HTMLSelectElement).value)"
                                class="flex-1 px-2.5 py-1.5 bg-base dark:bg-base-dark border border-border dark:border-border-dark rounded-lg text-sm focus:ring-2 focus:ring-blue-500/40 outline-none"
                            >
                                <option v-for="choice in FIELD_CHOICES" :key="choice.value" :value="choice.value">{{ t(choice.label) }}</option>
                            </select>
                        </div>
                    </div>
                </div>

                <!-- Review -->
                <div v-else-if="step === 'review'">
                    <div class="flex items-baseline gap-2 mb-1">
                        <span class="text-3xl font-semibold tabular-nums">{{ counts.add }}</span>
                        <span class="text-sm text-gray-600 dark:text-gray-400">{{ t('people.import_new_label', counts.add) }}</span>
                    </div>
                    <p class="text-sm text-gray-500 dark:text-gray-400">
                        {{ $t('people.read_from') }} <span class="font-medium">{{ fileName }}</span>
                    </p>

                    <ul class="mt-4 space-y-1 text-sm text-gray-600 dark:text-gray-400">
                        <li v-if="counts.merge > 0">{{ t('people.import_will_merge', { count: counts.merge }, counts.merge) }}</li>
                        <li v-if="repeatedInFile > 0">{{ t('people.import_repeated', { count: repeatedInFile }, repeatedInFile) }}</li>
                        <li v-if="skippedRows > 0">{{ t('people.import_rows_no_name', { count: skippedRows }, skippedRows) }}</li>
                    </ul>

                    <div v-if="decidable.length > 0" class="mt-6">
                        <h3 class="text-xs font-semibold uppercase tracking-wider text-gray-500 dark:text-gray-400 flex items-center gap-1.5 mb-3">
                            <Users class="w-3.5 h-3.5" /> {{ $t('people.already_here') }} ({{ decidable.length }})
                        </h3>
                        <div class="space-y-1.5 max-h-64 overflow-y-auto pr-1">
                            <div v-for="{ duplicate, contact } in decidable" :key="duplicate.incoming"
                                class="flex items-center gap-3 px-3 py-2 rounded-lg border border-border dark:border-border-dark">
                                <div class="flex-1 min-w-0">
                                    <p class="text-sm font-medium truncate">{{ contact?.title }}</p>
                                    <p class="text-xs flex items-center gap-1"
                                        :class="duplicate.certain ? 'text-gray-500' : 'text-yellow-600 dark:text-yellow-400'">
                                        <AlertTriangle v-if="!duplicate.certain" class="w-3 h-3" />
                                        {{ reasonText(duplicate) }}
                                    </p>
                                </div>
                                <select v-model="decisions[duplicate.incoming]"
                                    class="px-2 py-1 bg-base dark:bg-base-dark border border-border dark:border-border-dark rounded-lg text-xs focus:ring-2 focus:ring-blue-500/40 outline-none">
                                    <option value="merge">{{ $t('people.merge_into_existing') }}</option>
                                    <option value="add">{{ $t('people.add_separately') }}</option>
                                    <option value="skip">{{ $t('people.skip') }}</option>
                                </select>
                            </div>
                        </div>
                    </div>
                </div>

                <!-- Working -->
                <div v-else-if="step === 'working'" class="text-center py-10">
                    <Loader class="w-8 h-8 mx-auto text-blue-500 animate-spin" />
                    <p class="mt-4 text-sm text-gray-600 dark:text-gray-400 tabular-nums">
                        {{ exchange.progress.value?.done ?? 0 }} / {{ exchange.progress.value?.total ?? 0 }}
                    </p>
                    <div class="mt-3 h-1.5 w-64 mx-auto rounded-full bg-gray-200 dark:bg-gray-700 overflow-hidden">
                        <div class="h-full bg-blue-500 transition-all"
                            :style="{ width: `${exchange.progress.value?.total ? (exchange.progress.value.done / exchange.progress.value.total) * 100 : 0}%` }"></div>
                    </div>
                </div>

                <!-- Done -->
                <div v-else-if="step === 'done'" class="text-center py-8">
                    <div class="w-12 h-12 mx-auto rounded-full bg-green-100 dark:bg-green-900/30 flex items-center justify-center">
                        <Check class="w-6 h-6 text-green-600 dark:text-green-400" />
                    </div>
                    <p class="mt-4 text-lg font-semibold">
                        {{ t('people.import_added', { count: report?.added ?? 0 }, report?.added ?? 0) }}
                    </p>
                    <ul class="mt-2 space-y-0.5 text-sm text-gray-600 dark:text-gray-400">
                        <li v-if="report?.merged">{{ t('people.import_merged', { count: report.merged }, report.merged) }}</li>
                        <li v-if="report?.skipped">{{ t('people.import_skipped', { count: report.skipped }, report.skipped) }}</li>
                    </ul>
                    <div v-if="report?.failed.length" class="mt-4 text-left mx-auto max-w-sm">
                        <p class="text-sm font-medium text-red-500 mb-1">{{ t('people.import_failed', { count: report.failed.length }, report.failed.length) }}</p>
                        <ul class="text-xs text-gray-500 dark:text-gray-400 space-y-0.5 max-h-24 overflow-y-auto">
                            <li v-for="failure in report.failed" :key="failure.title">{{ failure.title }}</li>
                        </ul>
                    </div>
                </div>
            </div>

            <div class="px-6 py-4 border-t border-border dark:border-border-dark flex items-center justify-end gap-2 bg-gray-50/50 dark:bg-gray-800/50">
                <button v-if="step === 'map'" @click="applyMapping"
                    class="btn-primary">
                    {{ $t('people.continue') }}
                </button>
                <button v-else-if="step === 'review'" @click="run" :disabled="counts.add + counts.merge === 0"
                    class="btn-primary disabled:opacity-50">
                    {{ $t('people.import') }}
                </button>
                <button v-else-if="step === 'done'" @click="emit('close')"
                    class="btn-primary">
                    {{ $t('people.done') }}
                </button>
                <button v-else-if="step === 'pick'" @click="emit('close')"
                    class="px-4 py-2 text-sm text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg transition-colors">
                    {{ $t('people.cancel') }}
                </button>
            </div>
    </AppDialog>
</template>
