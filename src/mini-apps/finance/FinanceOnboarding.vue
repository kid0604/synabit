<script setup lang="ts">
import { ref } from 'vue';
import { Wallet, Check, Plus, Trash2 } from 'lucide-vue-next';
import type { FinanceAccount } from './types';
import { DEFAULT_INCOME_CATEGORIES, DEFAULT_EXPENSE_CATEGORIES } from './types';

const emit = defineEmits<{
  (e: 'complete', config: { incomeCategories: string[], expenseCategories: string[], accounts: FinanceAccount[] }): void;
}>();

const accounts = ref<FinanceAccount[]>([
    { id: `acc-${Date.now()}-1`, name: 'Cash', initialBalance: 0 },
    { id: `acc-${Date.now()}-2`, name: 'Bank Account', initialBalance: 0 }
]);

const newAccountName = ref('');
const newAccountBalance = ref('');

// Format number input with commas
const formatAmount = (val: string) => {
    const num = val.replace(/\D/g, '');
    if (!num) return '';
    return Number(num).toLocaleString('en-US');
};

const handleBalanceInput = (e: Event) => {
    const target = e.target as HTMLInputElement;
    newAccountBalance.value = formatAmount(target.value);
};

const addAccount = () => {
    const name = newAccountName.value.trim();
    const balanceNum = Number(newAccountBalance.value.replace(/\D/g, '')) || 0;
    
    if (name) {
        if (!accounts.value.some(a => a.name === name)) {
            accounts.value.push({
                id: `acc-${Date.now()}-${Math.floor(Math.random()*1000)}`,
                name,
                initialBalance: balanceNum
            });
            newAccountName.value = '';
            newAccountBalance.value = '';
        }
    }
};

const removeAccount = (idx: number) => {
    accounts.value.splice(idx, 1);
};

// Also let them edit inline
const updateBalance = (idx: number, e: Event) => {
    const target = e.target as HTMLInputElement;
    const formatted = formatAmount(target.value);
    target.value = formatted;
    accounts.value[idx].initialBalance = Number(formatted.replace(/\D/g, '')) || 0;
};

const finish = () => {
    if (accounts.value.length === 0) return;
    
    emit('complete', {
        incomeCategories: [...DEFAULT_INCOME_CATEGORIES],
        expenseCategories: [...DEFAULT_EXPENSE_CATEGORIES],
        accounts: accounts.value
    });
};
</script>

<template>
  <div class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-base dark:bg-base-dark">
      <div class="max-w-xl w-full flex flex-col gap-8 animate-in fade-in slide-in-from-bottom-8 duration-500">
          
          <div class="text-center">
              <div class="w-20 h-20 bg-blue-100 dark:bg-blue-900/30 text-blue-500 rounded-full flex items-center justify-center mx-auto mb-6 shadow-sm">
                  <Wallet class="w-10 h-10" />
              </div>
              <h1 class="text-3xl font-bold text-text dark:text-text-dark mb-2">{{ $t('finance.welcome') }}</h1>
              <p class="text-gray-500 dark:text-gray-400">{{ $t('finance.setup_desc') }}</p>
          </div>
          
          <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-3xl p-6 shadow-xl">
              <h2 class="text-lg font-bold text-text dark:text-text-dark mb-4">{{ $t('finance.declare_assets') }}</h2>
              
              <div class="space-y-3 mb-6">
                  <div v-for="(acc, idx) in accounts" :key="acc.id" class="flex items-center gap-3 p-3 bg-gray-50 dark:bg-gray-800/50 rounded-xl border border-border dark:border-border-dark">
                      <div class="flex-1">
                          <input type="text" v-model="acc.name" class="w-full bg-transparent border-none font-medium text-text dark:text-text-dark focus:outline-none focus:ring-0 p-0 mb-1 text-sm" placeholder="Account Name" />
                          <div class="relative">
                              <input type="text" inputmode="numeric" :value="acc.initialBalance.toLocaleString('en-US')" @input="updateBalance(idx, $event)" class="w-full bg-transparent border-none text-xl font-bold text-gray-700 dark:text-gray-300 focus:outline-none focus:ring-0 p-0" placeholder="0" />
                              <span class="absolute left-0 bottom-full text-[10px] uppercase text-gray-400 font-bold tracking-wider">Balance</span>
                          </div>
                      </div>
                      <button @click="removeAccount(idx)" class="p-3 text-gray-400 hover:text-red-500 transition-colors rounded-xl hover:bg-white dark:hover:bg-gray-700">
                          <Trash2 class="w-5 h-5" />
                      </button>
                  </div>
              </div>
              
              <!-- Add new inline -->
              <div class="flex flex-col gap-2 p-3 bg-blue-50/50 dark:bg-blue-900/10 rounded-xl border border-blue-100 dark:border-blue-900/30 border-dashed mb-6">
                  <input type="text" v-model="newAccountName" class="w-full bg-white dark:bg-gray-800 border border-border dark:border-border-dark rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500" :placeholder="$t('finance.add_another_acc')" />
                  <div class="flex gap-2">
                      <div class="relative flex-1">
                          <input type="text" inputmode="numeric" :value="newAccountBalance" @input="handleBalanceInput" class="w-full bg-white dark:bg-gray-800 border border-border dark:border-border-dark rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 pr-4" :placeholder="$t('finance.current_balance_ph')" />
                      </div>
                      <button @click="addAccount" :disabled="!newAccountName" class="px-4 bg-blue-500 text-white rounded-lg hover:bg-blue-600 transition-colors font-medium text-sm whitespace-nowrap disabled:opacity-50">
                          <Plus class="w-4 h-4" />
                      </button>
                  </div>
              </div>
              
              <button @click="finish" :disabled="accounts.length === 0" class="w-full py-4 rounded-xl bg-text dark:bg-text-dark text-base dark:text-base-dark font-bold flex items-center justify-center gap-2 hover:opacity-90 transition-opacity disabled:opacity-50">
                  <Check class="w-5 h-5" />
                  Start Using
              </button>
          </div>
          
      </div>
  </div>
</template>
