<script setup lang="ts">
import { ref, watch, computed } from 'vue';
import { X, Scale } from 'lucide-vue-next';
import AppDialog from '../../shared/components/AppDialog.vue';
import { formatAmountInput, formatCurrency, formatMinorForInput, parseAmountInput } from './currency';

const props = defineProps<{
    show: boolean;
    accountId: string;
    accountName: string;
    currentBalance: number;
}>();

const emit = defineEmits<{
    (e: 'close'): void;
    (e: 'adjust', diff: number): void;
}>();

const actualBalanceStr = ref<string>('');

/** Grouped and read the way the vault's own currency is written. */
const formatAmount = (val: string) => formatAmountInput(val);

const handleInput = (e: Event) => {
    const target = e.target as HTMLInputElement;
    actualBalanceStr.value = formatAmount(target.value);
};

// formatCurrency is imported from ./currency

watch(() => props.show, (newVal) => {
    if (newVal) {
        actualBalanceStr.value = formatMinorForInput(props.currentBalance);
    }
});

const actualBalance = computed(() => {
    return parseAmountInput(actualBalanceStr.value);
});

const difference = computed(() => {
    return actualBalance.value - props.currentBalance;
});

const save = () => {
    if (difference.value !== 0) {
        emit('adjust', difference.value);
    }
    emit('close');
};
</script>

<template>
  <AppDialog :show="show" labelledby="finance-adjust-title" size="sm" @close="emit('close')">
      
      <!-- Header -->
      <div class="flex items-center justify-between p-4 border-b border-border dark:border-border-dark">
        <h3 id="finance-adjust-title" class="font-bold text-lg text-text dark:text-text-dark flex items-center gap-2">
            <Scale class="w-5 h-5 text-blue-500" />
            {{ $t('finance.adjust_balance') }}
        </h3>
        <button @click="emit('close')" class="p-1 rounded-lg text-gray-500 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors" :aria-label="$t('finance.close')" :title="$t('finance.close')">
            <X class="w-5 h-5" />
        </button>
      </div>

      <!-- Body -->
      <div class="p-5 space-y-4">
          <div class="flex flex-col">
              <span class="text-sm font-medium text-text dark:text-text-dark">{{ accountName }}</span>
              <span class="text-xs text-gray-500 dark:text-gray-400">{{ $t('finance.current_system_balance', { amount: formatCurrency(currentBalance) }) }}</span>
          </div>

          <div>
              <label class="block text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-1">{{ $t('finance.actual_wallet_balance') }}</label>
              <div class="relative">
                  <input type="text" inputmode="decimal" :value="actualBalanceStr" @input="handleInput" class="w-full bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl px-3 py-3 text-lg font-bold text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-accent pr-8" placeholder="0" />
              </div>
          </div>

          <div v-if="difference !== 0" class="p-3 rounded-xl text-sm" :class="difference > 0 ? 'bg-green-50 dark:bg-green-900/20 text-green-700 dark:text-green-400' : 'bg-red-50 dark:bg-red-900/20 text-red-700 dark:text-red-400'">
              {{ $t('finance.system_balance_off') }} 
              <span class="font-bold">{{ difference > 0 ? '+' : '-' }}{{ formatCurrency(Math.abs(difference)) }}</span> {{ $t('finance.recorded_for_month') }}
          </div>
          <div v-else class="p-3 rounded-xl bg-gray-50 dark:bg-gray-800/50 text-gray-500 dark:text-gray-400 text-sm text-center">
              {{ $t('finance.balance_perfect') }}
          </div>
      </div>

      <!-- Footer -->
      <div class="p-4 border-t border-border dark:border-border-dark flex justify-end gap-3 bg-gray-50/50 dark:bg-gray-800/50">
        <button @click="emit('close')" class="px-4 py-2 rounded-xl text-sm font-medium text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors">
            {{ $t('finance.cancel') }}
        </button>
        <button @click="save" :disabled="difference === 0" class="btn-primary">
            {{ $t('finance.balance_accounts') }}
        </button>
      </div>

  </AppDialog>
</template>
