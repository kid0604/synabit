<script setup lang="ts">
import { ref, watch } from 'vue';
import { X, Plus, Trash2, Edit2, Check, Lock } from 'lucide-vue-next';
import { type FinanceAccount, SYSTEM_INCOME_CATEGORIES, SYSTEM_EXPENSE_CATEGORIES } from './types';
import { formatCurrency } from './currency';

const props = defineProps<{
  show: boolean;
  initialIncomeCategories: string[];
  initialExpenseCategories: string[];
  initialAccounts: FinanceAccount[];
  initialCurrency?: string;
  currentBalances?: { id: string, name: string, balance: number }[];
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'save', config: { incomeCategories: string[], expenseCategories: string[], accounts: FinanceAccount[], currency: string }): void;
}>();

const incomeCategories = ref<string[]>([]);
const expenseCategories = ref<string[]>([]);
const accounts = ref<FinanceAccount[]>([]);
const selectedCurrency = ref('USD');

const newIncomeCategory = ref('');
const newExpenseCategory = ref('');
const newAccountName = ref('');
const newAccountBalance = ref('');

const editingAccountId = ref<string | null>(null);

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

// formatCurrency is imported from ./currency

const getCurrentBalance = (id: string, fallbackInitial: number) => {
    if (!props.currentBalances) return fallbackInitial;
    const found = props.currentBalances.find(b => b.id === id);
    return found ? found.balance : fallbackInitial;
};

watch(() => props.show, (newVal) => {
    if (newVal) {
        incomeCategories.value = [...props.initialIncomeCategories];
        expenseCategories.value = [...props.initialExpenseCategories];
        // Deep clone accounts
        accounts.value = props.initialAccounts.map(a => ({ ...a }));
        selectedCurrency.value = props.initialCurrency || 'USD';
        newIncomeCategory.value = '';
        newExpenseCategory.value = '';
        newAccountName.value = '';
        newAccountBalance.value = '';
        editingAccountId.value = null;
    }
});

const addIncomeCategory = () => {
    const cat = newIncomeCategory.value.trim();
    if (cat && !incomeCategories.value.includes(cat)) {
        incomeCategories.value.push(cat);
        newIncomeCategory.value = '';
    }
};

const removeIncomeCategory = (idx: number) => {
    if (SYSTEM_INCOME_CATEGORIES.includes(incomeCategories.value[idx])) return;
    incomeCategories.value.splice(idx, 1);
};

const addExpenseCategory = () => {
    const cat = newExpenseCategory.value.trim();
    if (cat && !expenseCategories.value.includes(cat)) {
        expenseCategories.value.push(cat);
        newExpenseCategory.value = '';
    }
};

const removeExpenseCategory = (idx: number) => {
    if (SYSTEM_EXPENSE_CATEGORIES.includes(expenseCategories.value[idx])) return;
    expenseCategories.value.splice(idx, 1);
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

const save = () => {
    editingAccountId.value = null; // Exit edit mode
    emit('save', {
        incomeCategories: [...incomeCategories.value],
        expenseCategories: [...expenseCategories.value],
        accounts: accounts.value.map(a => ({ ...a })),
        currency: selectedCurrency.value
    });
    emit('close');
};

</script>

<template>
  <div v-if="show" class="fixed inset-0 z-[60] flex items-center justify-center p-4 bg-black/50 dark:bg-black/70 backdrop-blur-sm" @click.self="emit('close')">
    <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl shadow-xl w-full max-w-md overflow-hidden animate-in zoom-in-95 duration-200 flex flex-col max-h-[85vh]">
      
      <!-- Header -->
      <div class="flex items-center justify-between p-4 border-b border-border dark:border-border-dark shrink-0">
        <h3 class="font-bold text-lg text-text dark:text-text-dark">Finance Settings</h3>
        <button @click="emit('close')" class="p-1 rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors" aria-label="More Options">
            <X class="w-5 h-5" />
        </button>
      </div>

      <!-- Body -->
      <div class="p-5 overflow-y-auto space-y-6">
          
        <!-- General Settings -->
        <div>
            <h4 class="text-sm font-semibold text-text dark:text-text-dark mb-3">General Settings</h4>
            <div class="flex items-center justify-between p-3 bg-gray-50 dark:bg-gray-800/50 rounded-xl border border-border dark:border-border-dark">
                <div class="flex flex-col">
                    <span class="font-medium text-sm text-text dark:text-text-dark">Currency</span>
                    <span class="text-xs text-gray-500">Base currency for your transactions</span>
                </div>
                <select v-model="selectedCurrency" class="bg-white dark:bg-gray-900 border border-border dark:border-border-dark rounded-lg px-3 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 text-text dark:text-text-dark cursor-pointer">
                    <option value="USD">USD ($)</option>
                    <option value="VND">VND (₫)</option>
                    <option value="EUR">EUR (€)</option>
                    <option value="GBP">GBP (£)</option>
                    <option value="JPY">JPY (¥)</option>
                </select>
            </div>
        </div>

        <hr class="border-border dark:border-border-dark" />

        <!-- Income Categories -->
        <div>
            <h4 class="text-sm font-semibold text-green-600 dark:text-green-400 mb-3">Income Categories</h4>
            <div class="flex gap-2 mb-3">
                <input type="text" v-model="newIncomeCategory" @keyup.enter="addIncomeCategory" class="flex-1 bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500" :placeholder="$t('finance.new_income_cat')" />
                <button @click="addIncomeCategory" class="p-2 bg-blue-500 text-white rounded-xl hover:bg-blue-600 transition-colors" aria-label="Add Income Category">
                    <Plus class="w-5 h-5" />
                </button>
            </div>
            <div class="flex flex-wrap gap-2">
                <div v-for="(cat, idx) in incomeCategories" :key="cat" class="flex items-center gap-1.5 px-3 py-1.5 bg-green-50 dark:bg-green-900/10 border border-green-200 dark:border-green-900/30 rounded-lg text-sm text-green-700 dark:text-green-400">
                    <span>{{ cat }}</span>
                    <button v-if="!SYSTEM_INCOME_CATEGORIES.includes(cat)" @click="removeIncomeCategory(idx)" class="text-green-500/50 hover:text-red-500 transition-colors" aria-label="Remove Income Category">
                        <X class="w-3.5 h-3.5" />
                    </button>
                    <div v-else class="text-green-500/30 ml-1">
                        <Lock class="w-3 h-3" />
                    </div>
                </div>
                <div v-if="!incomeCategories.length" class="text-sm text-gray-400 italic">No categories yet.</div>
            </div>
        </div>

        <hr class="border-border dark:border-border-dark" />

        <!-- Expense Categories -->
        <div>
            <h4 class="text-sm font-semibold text-red-600 dark:text-red-400 mb-3">Expense Categories</h4>
            <div class="flex gap-2 mb-3">
                <input type="text" v-model="newExpenseCategory" @keyup.enter="addExpenseCategory" class="flex-1 bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500" :placeholder="$t('finance.new_expense_cat')" />
                <button @click="addExpenseCategory" class="p-2 bg-blue-500 text-white rounded-xl hover:bg-blue-600 transition-colors" aria-label="More Options">
                    <Plus class="w-5 h-5" />
                </button>
            </div>
            <div class="flex flex-wrap gap-2">
                <div v-for="(cat, idx) in expenseCategories" :key="cat" class="flex items-center gap-1.5 px-3 py-1.5 bg-red-50 dark:bg-red-900/10 border border-red-200 dark:border-red-900/30 rounded-lg text-sm text-red-700 dark:text-red-400">
                    <span>{{ cat }}</span>
                    <button v-if="!SYSTEM_EXPENSE_CATEGORIES.includes(cat)" @click="removeExpenseCategory(idx)" class="text-red-500/50 hover:text-red-500 transition-colors" aria-label="More Options">
                        <X class="w-3.5 h-3.5" />
                    </button>
                    <div v-else class="text-red-500/30 ml-1">
                        <Lock class="w-3 h-3" />
                    </div>
                </div>
                <div v-if="!expenseCategories.length" class="text-sm text-gray-400 italic">No categories yet.</div>
            </div>
        </div>

        <hr class="border-border dark:border-border-dark" />

        <!-- Accounts -->
        <div>
            <h4 class="text-sm font-semibold text-text dark:text-text-dark mb-3">Accounts & Balances</h4>
            
            <div class="flex flex-col gap-2 mb-4 p-3 bg-gray-50 dark:bg-gray-800/50 rounded-xl border border-border dark:border-border-dark">
                <input type="text" v-model="newAccountName" class="w-full bg-white dark:bg-gray-800 border border-border dark:border-border-dark rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500" :placeholder="$t('finance.new_acc_name')" />
                <div class="flex gap-2">
                    <div class="relative flex-1">
                        <input type="text" inputmode="numeric" :value="newAccountBalance" @input="handleBalanceInput" class="w-full bg-white dark:bg-gray-800 border border-border dark:border-border-dark rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 pr-4" :placeholder="$t('finance.initial_balance')" />
                    </div>
                    <button @click="addAccount" :disabled="!newAccountName" class="px-4 bg-blue-500 text-white rounded-lg hover:bg-blue-600 transition-colors font-medium text-sm whitespace-nowrap disabled:opacity-50">
                        Add
                    </button>
                </div>
            </div>

            <div class="flex flex-col gap-2">
                <div v-for="(acc, idx) in accounts" :key="acc.id" class="flex flex-col px-3 py-2.5 bg-gray-50 dark:bg-gray-800/50 border border-border dark:border-border-dark rounded-xl text-sm">
                    
                    <div v-if="editingAccountId !== acc.id" class="flex items-center justify-between text-text dark:text-text-dark">
                        <div class="flex flex-col">
                            <span class="font-medium">{{ acc.name }}</span>
                            <span class="text-xs text-gray-500">Current Balance: <span class="font-semibold text-gray-700 dark:text-gray-300">{{ formatCurrency(getCurrentBalance(acc.id, acc.initialBalance)) }}</span></span>
                        </div>
                        <div class="flex items-center gap-1 shrink-0">
                            <button @click="editingAccountId = acc.id" class="text-gray-400 hover:text-blue-500 transition-colors p-2 rounded-lg hover:bg-gray-200 dark:hover:bg-gray-700" aria-label="More Options">
                                <Edit2 class="w-4 h-4" />
                            </button>
                            <button @click="removeAccount(idx)" class="text-gray-400 hover:text-red-500 transition-colors p-2 rounded-lg hover:bg-gray-200 dark:hover:bg-gray-700" aria-label="Remove Account">
                                <Trash2 class="w-4 h-4" />
                            </button>
                        </div>
                    </div>
                    
                    <div v-else class="flex flex-col gap-2">
                        <input type="text" v-model="acc.name" class="w-full bg-white dark:bg-gray-800 border border-border dark:border-border-dark rounded-lg px-2 py-1 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500" placeholder="Account Name" />
                        <div class="flex items-center justify-between text-xs text-gray-500">
                            <span>Current Balance: <span class="font-semibold">{{ formatCurrency(getCurrentBalance(acc.id, acc.initialBalance)) }}</span></span>
                            <button @click="editingAccountId = null" class="px-3 py-1 bg-green-500 text-white rounded-lg hover:bg-green-600 transition-colors flex items-center justify-center" aria-label="More Options">
                                <Check class="w-4 h-4" />
                            </button>
                        </div>
                    </div>
                    
                </div>
                <div v-if="!accounts.length" class="text-sm text-gray-400 italic">No accounts yet.</div>
            </div>
        </div>

      </div>

      <!-- Footer -->
      <div class="p-4 border-t border-border dark:border-border-dark flex justify-end gap-3 shrink-0 bg-gray-50/50 dark:bg-gray-800/50">
        <button @click="emit('close')" class="px-4 py-2 rounded-xl text-sm font-medium text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors">
            Cancel
        </button>
        <button @click="save" class="px-5 py-2 rounded-xl text-sm font-medium bg-blue-500 hover:bg-blue-600 text-white shadow-sm transition-colors">
            Save Changes
        </button>
      </div>

    </div>
  </div>
</template>
