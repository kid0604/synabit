<script setup lang="ts">
/**
 * What is about to be added, before it is added.
 *
 * Import is the one operation here that puts somebody else's data into the
 * user's ledger, and a column read wrong puts the date in the amount. So it
 * shows its working first: which column it thinks is which, how many rows it
 * can use, how many it has seen before, and what it could not read at all.
 */
import { computed } from 'vue';
import { X, FileUp, AlertTriangle, Check } from 'lucide-vue-next';
import type { ColumnMap, ImportResult } from '../csv';
import { formatCurrency } from '../currency';

const props = defineProps<{
    show: boolean;
    fileName: string;
    header: string[];
    map: ColumnMap | null;
    missing: string[];
    result: ImportResult | null;
}>();

const emit = defineEmits<{
    (e: 'close'): void;
    (e: 'confirm'): void;
}>();

const FIELD_LABEL: Record<keyof ColumnMap, string> = {
    date: 'Date',
    type: 'Type',
    amount: 'Amount',
    category: 'Category',
    account: 'Account',
    toAccount: 'To account',
    note: 'Note',
    receipt: 'Receipt',
};

/** Which column of the file each field was read from, for the user to check. */
const mapping = computed(() => {
    if (!props.map) return [];
    return (Object.keys(FIELD_LABEL) as (keyof ColumnMap)[]).map(field => ({
        field,
        label: FIELD_LABEL[field],
        column: props.map![field] >= 0 ? (props.header[props.map![field]] ?? '—') : null,
    }));
});

const ready = computed(() => props.result?.ready.length ?? 0);
const canImport = computed(() => props.missing.length === 0 && ready.value > 0);

/** The first few rows, so a wrong column is visible rather than described. */
const sample = computed(() => props.result?.ready.slice(0, 5) ?? []);
</script>

<template>
  <div v-if="show" class="fixed inset-0 z-[100] flex items-center justify-center p-4 sm:p-6">
      <div class="absolute inset-0 bg-gray-900/40 dark:bg-black/60 backdrop-blur-sm" @click="emit('close')"></div>

      <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-3xl shadow-2xl w-full max-w-lg relative flex flex-col max-h-full overflow-hidden animate-in fade-in zoom-in-95 duration-200">
          <div class="px-6 py-5 border-b border-border dark:border-border-dark flex justify-between items-center">
              <div class="flex items-center gap-3 min-w-0">
                  <FileUp class="w-5 h-5 text-blue-500 shrink-0" />
                  <div class="min-w-0">
                      <h3 class="text-lg font-bold text-text dark:text-text-dark truncate">{{ $t('finance.import_preview') }}</h3>
                      <p class="text-xs text-gray-500 truncate">{{ fileName }}</p>
                  </div>
              </div>
              <button @click="emit('close')" class="p-2 rounded-lg text-gray-400 hover:text-gray-600 dark:hover:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors shrink-0" aria-label="Close">
                  <X class="w-5 h-5" />
              </button>
          </div>

          <div class="flex-1 overflow-y-auto p-6 flex flex-col gap-5">
              <!-- A file missing a date or an amount cannot become transactions
                   at all, and saying which is missing is more use than refusing. -->
              <div v-if="missing.length > 0" class="flex items-start gap-2 p-3 rounded-xl border border-amber-300 bg-amber-50 text-sm text-amber-900 dark:border-amber-700/60 dark:bg-amber-950/40 dark:text-amber-200">
                  <AlertTriangle class="w-4 h-4 shrink-0 mt-0.5" />
                  <span>{{ $t('finance.import_missing') }} <strong>{{ missing.join(', ') }}</strong></span>
              </div>

              <div class="grid grid-cols-3 gap-3">
                  <div class="flex flex-col p-3 rounded-xl bg-gray-50 dark:bg-gray-800/50 border border-border dark:border-border-dark">
                      <span class="text-2xl font-bold tabular-nums text-green-600 dark:text-green-400">{{ ready }}</span>
                      <span class="text-xs text-gray-500">{{ $t('finance.import_new') }}</span>
                  </div>
                  <div class="flex flex-col p-3 rounded-xl bg-gray-50 dark:bg-gray-800/50 border border-border dark:border-border-dark">
                      <span class="text-2xl font-bold tabular-nums text-gray-500">{{ result?.duplicates ?? 0 }}</span>
                      <span class="text-xs text-gray-500">{{ $t('finance.import_duplicate') }}</span>
                  </div>
                  <div class="flex flex-col p-3 rounded-xl bg-gray-50 dark:bg-gray-800/50 border border-border dark:border-border-dark">
                      <span class="text-2xl font-bold tabular-nums" :class="(result?.problems.length ?? 0) > 0 ? 'text-amber-600 dark:text-amber-400' : 'text-gray-500'">{{ result?.problems.length ?? 0 }}</span>
                      <span class="text-xs text-gray-500">{{ $t('finance.import_unreadable') }}</span>
                  </div>
              </div>

              <div class="flex flex-col gap-2">
                  <h4 class="text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">{{ $t('finance.import_columns') }}</h4>
                  <div class="flex flex-wrap gap-1.5">
                      <span
                          v-for="item in mapping"
                          :key="item.field"
                          class="px-2.5 py-1 rounded-lg text-xs border"
                          :class="item.column
                              ? 'bg-blue-50 border-blue-200 text-blue-700 dark:bg-blue-900/20 dark:border-blue-900/40 dark:text-blue-300'
                              : 'bg-gray-50 border-gray-200 text-gray-400 dark:bg-gray-800/50 dark:border-gray-700 dark:text-gray-500'"
                      >
                          {{ item.label }} → {{ item.column ?? $t('finance.import_not_found') }}
                      </span>
                  </div>
              </div>

              <div v-if="sample.length > 0" class="flex flex-col gap-2">
                  <h4 class="text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">{{ $t('finance.import_sample') }}</h4>
                  <div class="flex flex-col gap-1">
                      <div v-for="tx in sample" :key="tx.id" class="flex items-center justify-between gap-3 text-sm px-3 py-2 rounded-lg bg-gray-50 dark:bg-gray-800/50">
                          <span class="text-gray-500 tabular-nums shrink-0">{{ tx.date.slice(0, 10) }}</span>
                          <span class="truncate flex-1 text-text dark:text-text-dark">{{ tx.note || tx.category }}</span>
                          <span class="font-semibold tabular-nums shrink-0" :class="tx.type === 'income' ? 'text-green-600 dark:text-green-400' : 'text-text dark:text-text-dark'">
                              {{ tx.type === 'income' ? '+' : '−' }}{{ formatCurrency(tx.amount) }}
                          </span>
                      </div>
                  </div>
              </div>

              <div v-if="result && result.problems.length > 0" class="flex flex-col gap-1">
                  <h4 class="text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">{{ $t('finance.import_problems') }}</h4>
                  <p v-for="problem in result.problems.slice(0, 5)" :key="problem.line" class="text-xs text-amber-700 dark:text-amber-400">
                      {{ $t('finance.import_line') }} {{ problem.line }}: {{ problem.reason }}
                  </p>
                  <p v-if="result.problems.length > 5" class="text-xs text-gray-500">
                      + {{ result.problems.length - 5 }}
                  </p>
              </div>
          </div>

          <div class="px-6 py-4 border-t border-border dark:border-border-dark flex justify-end gap-2">
              <button @click="emit('close')" class="px-4 py-2 rounded-xl text-sm font-medium text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors">
                  {{ $t('finance.cancel') }}
              </button>
              <button
                  @click="emit('confirm')"
                  :disabled="!canImport"
                  class="flex items-center gap-2 px-4 py-2 rounded-xl bg-blue-500 text-white text-sm font-medium hover:bg-blue-600 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                  <Check class="w-4 h-4" />
                  {{ $t('finance.import_confirm') }} {{ ready }}
              </button>
          </div>
      </div>
  </div>
</template>
