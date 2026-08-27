<script setup lang="ts">
/**
 * The bills and the salary: what the ledger records without being asked.
 *
 * A list rather than a calendar, because the question this screen answers is
 * "what is my money already committed to", not "what happens on the 14th".
 */
import { computed } from 'vue';
import { Repeat, Plus, Pause, Play, Trash2, Pencil } from 'lucide-vue-next';
import type { Category, FinanceAccount } from '../types';
import type { RecurringRule } from '../recurring';
import { nextOccurrenceAfter, todayStr } from '../recurring';
import { categoryName } from '../categories';
import { formatCurrency } from '../currency';

const props = defineProps<{
    rules: RecurringRule[];
    accounts: FinanceAccount[];
    categories: Category[];
}>();

const emit = defineEmits<{
    (e: 'save-rules', rules: RecurringRule[]): void;
    (e: 'edit-rule', rule: RecurringRule): void;
    (e: 'add-rule'): void;
}>();

const rows = computed(() =>
    props.rules
        .map(rule => ({
            rule,
            next: nextOccurrenceAfter(rule, todayStr()),
        }))
        // Running rules first, then by when each is next due. A paused rule is
        // still worth seeing — it is a bill somebody chose to stop, not one
        // that went away.
        .sort((a, b) => {
            if (!!a.rule.paused !== !!b.rule.paused) return a.rule.paused ? 1 : -1;
            return (a.next ?? '9999').localeCompare(b.next ?? '9999');
        })
);

/** What one occurrence of a rule costs, signed the way the ledger shows it. */
const signedAmount = (rule: RecurringRule) =>
    (rule.template.type === 'income' ? '+' : '−') + formatCurrency(rule.template.amount);

const accountName = (id: string) => props.accounts.find(a => a.id === id)?.name ?? 'Unknown';

const formatDate = (iso: string | null) => {
    if (!iso) return '—';
    const [y, m, d] = iso.split('-');
    return `${d}/${m}/${y}`;
};

const togglePause = (rule: RecurringRule) => {
    emit('save-rules', props.rules.map(r => (r.id === rule.id ? { ...r, paused: !r.paused } : r)));
};

const remove = (rule: RecurringRule) => {
    // Only the rule goes. The transactions it has already made are records of
    // money that actually moved, and deleting those would be rewriting history
    // to tidy up a schedule.
    emit('save-rules', props.rules.filter(r => r.id !== rule.id));
};
</script>

<template>
  <div class="flex-1 flex flex-col gap-4 overflow-y-auto hidden-scrollbar pb-4">
      <div class="flex items-center justify-between">
          <div>
              <h2 class="text-lg font-bold text-text dark:text-text-dark">{{ $t('finance.recurring') }}</h2>
              <p class="text-sm text-gray-500 dark:text-gray-400">{{ $t('finance.recurring_desc') }}</p>
          </div>
          <button @click="emit('add-rule')" class="flex items-center gap-2 px-4 py-2.5 rounded-xl bg-blue-500 text-white hover:bg-blue-600 transition-colors shadow-sm font-medium text-sm shrink-0">
              <Plus class="w-4 h-4" />
              <span>{{ $t('finance.add_recurring') }}</span>
          </button>
      </div>

      <div v-if="rows.length === 0" class="flex flex-col items-center justify-center gap-3 py-16 text-center text-gray-400 dark:text-gray-500">
          <Repeat class="w-10 h-10" />
          <p class="max-w-sm text-sm">{{ $t('finance.recurring_empty') }}</p>
      </div>

      <div v-else class="flex flex-col gap-2">
          <div
              v-for="row in rows"
              :key="row.rule.id"
              class="flex items-center gap-4 p-4 rounded-2xl bg-surface dark:bg-surface-dark border border-border dark:border-border-dark"
              :class="row.rule.paused ? 'opacity-60' : ''"
          >
              <div class="p-2.5 rounded-xl shrink-0" :class="row.rule.template.type === 'income' ? 'bg-green-50 dark:bg-green-900/20 text-green-500' : 'bg-blue-50 dark:bg-blue-900/20 text-blue-500'">
                  <Repeat class="w-5 h-5" />
              </div>

              <div class="flex flex-col min-w-0 flex-1">
                  <span class="font-semibold text-text dark:text-text-dark truncate">
                      {{ row.rule.template.note || categoryName(categories, row.rule.template.category) }}
                  </span>
                  <span class="text-xs text-gray-500 dark:text-gray-400 truncate">
                      {{ $t(`finance.repeats_${row.rule.recurrence}`) }}
                      · {{ accountName(row.rule.template.accountId) }}
                      · <template v-if="row.rule.paused">{{ $t('finance.paused') }}</template>
                      <template v-else>{{ $t('finance.next_due') }} {{ formatDate(row.next) }}</template>
                  </span>
              </div>

              <span class="font-bold tabular-nums shrink-0" :class="row.rule.template.type === 'income' ? 'text-green-600 dark:text-green-400' : 'text-text dark:text-text-dark'">
                  {{ signedAmount(row.rule) }}
              </span>

              <div class="flex items-center gap-1 shrink-0">
                  <button @click="emit('edit-rule', row.rule)" class="p-2 rounded-lg text-gray-400 hover:text-blue-500 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors" :aria-label="$t('finance.edit')">
                      <Pencil class="w-4 h-4" />
                  </button>
                  <button @click="togglePause(row.rule)" class="p-2 rounded-lg text-gray-400 hover:text-blue-500 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors" :aria-label="row.rule.paused ? $t('finance.resume') : $t('finance.pause')">
                      <Play v-if="row.rule.paused" class="w-4 h-4" />
                      <Pause v-else class="w-4 h-4" />
                  </button>
                  <button @click="remove(row.rule)" class="p-2 rounded-lg text-gray-400 hover:text-red-500 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors" :aria-label="$t('finance.delete')">
                      <Trash2 class="w-4 h-4" />
                  </button>
              </div>
          </div>

          <p class="text-xs text-gray-500 dark:text-gray-400 px-1 pt-2">
              {{ $t('finance.recurring_delete_note') }}
          </p>
      </div>
  </div>
</template>
