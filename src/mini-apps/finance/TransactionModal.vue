<script setup lang="ts">
import { ref, watch, computed } from 'vue';
import { X, RefreshCw, Plus, Check, Trash2 } from 'lucide-vue-next';
import type { Transaction, TransactionType, FinanceAccount } from './types';
import { currentCurrency, fetchExchangeRate } from './currency';

const props = defineProps<{
  show: boolean;
  transaction?: Transaction | null;
  incomeCategories: string[];
  expenseCategories: string[];
  accounts: FinanceAccount[];
  projects?: {id: string, title: string}[];
  people?: {id: string, title: string}[];
  defaultProjectId?: string;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'save', tx: Transaction): void;
  (e: 'delete', txId: string): void;
  (e: 'addCategory', payload: { type: 'income' | 'expense', name: string }): void;
}>();

const type = ref<TransactionType>('expense');
const amount = ref<string>('');
const category = ref<string>('');
const accountId = ref<string>('');
const toAccountId = ref<string>('');
const date = ref<string>('');
const note = ref<string>('');
const projectId = ref<string>('');
const personId = ref<string>('');
const showErrors = ref(false);

const inputCurrency = ref(currentCurrency.value);
const isFetchingRate = ref(false);
const exchangeRate = ref<number | null>(null);
const exchangeRateStr = ref<string>('');
const calculatedBaseAmount = ref<number>(0);
const CURRENCIES = ['VND', 'USD', 'EUR', 'GBP', 'JPY'];

const isAddingCategory = ref(false);
const newCategoryName = ref('');

const availableCategories = computed(() => {
    if (type.value === 'income') return props.incomeCategories;
    if (type.value === 'expense') return props.expenseCategories;
    // For transfers, we can either hide the category field or use a fixed category.
    // In V1, we'll just return expenseCategories or empty
    return [];
});

watch(type, (newType, oldType) => {
    if (newType !== oldType) {
        if (availableCategories.value.length > 0) {
            category.value = availableCategories.value[0];
        } else {
            category.value = '';
        }
    }
});

// Format number input with commas
const formatAmount = (val: string) => {
    const num = val.replace(/\D/g, '');
    if (!num) return '';
    return Number(num).toLocaleString('vi-VN');
};

const handleAmountInput = (e: Event) => {
    const target = e.target as HTMLInputElement;
    amount.value = formatAmount(target.value);
};

watch([amount, inputCurrency], async ([newAmount, newCurrency], [oldAmount, oldCurrency]) => {
    const numAmount = Number(newAmount.replace(/\D/g, '')) || 0;
    
    if (newCurrency === currentCurrency.value) {
        exchangeRate.value = null;
        exchangeRateStr.value = '';
        calculatedBaseAmount.value = numAmount;
        return;
    }
    
    if (newCurrency !== oldCurrency && newCurrency !== currentCurrency.value) {
        isFetchingRate.value = true;
        const rate = await fetchExchangeRate(newCurrency, currentCurrency.value);
        isFetchingRate.value = false;
        
        if (rate) {
            exchangeRate.value = rate;
            // Limit to 2 decimals if it's fiat normally, but keep precision if very small. Rounding appropriately.
            const rateStr = rate > 1 ? Math.round(rate).toString() : rate.toString();
            exchangeRateStr.value = formatAmount(rateStr);
        }
    }
    
    if (exchangeRate.value) {
        calculatedBaseAmount.value = Math.round(numAmount * exchangeRate.value);
    }
});

const handleRateInput = (e: Event) => {
    const target = e.target as HTMLInputElement;
    const cleanStr = target.value.replace(/[^\d.]/g, ''); // Allow decimals in rate
    exchangeRateStr.value = cleanStr; // Store exact string so decimal typing works
    exchangeRate.value = Number(cleanStr) || 0;
    
    const numAmount = Number(amount.value.replace(/\D/g, '')) || 0;
    calculatedBaseAmount.value = Math.round(numAmount * (exchangeRate.value || 0));
};

const initForm = () => {
    if (props.transaction) {
        type.value = props.transaction.type;
        category.value = props.transaction.category;
        accountId.value = props.transaction.accountId;
        toAccountId.value = props.transaction.toAccountId || '';
        
        if (props.transaction.originalCurrency && props.transaction.originalCurrency !== currentCurrency.value) {
            inputCurrency.value = props.transaction.originalCurrency;
            amount.value = props.transaction.originalAmount ? props.transaction.originalAmount.toLocaleString('vi-VN') : '';
            exchangeRate.value = props.transaction.exchangeRate || null;
            exchangeRateStr.value = exchangeRate.value ? exchangeRate.value.toString() : '';
            calculatedBaseAmount.value = props.transaction.amount;
        } else {
            inputCurrency.value = currentCurrency.value;
            amount.value = props.transaction.amount.toLocaleString('vi-VN');
            exchangeRate.value = null;
            exchangeRateStr.value = '';
            calculatedBaseAmount.value = props.transaction.amount;
        }

        const d = new Date(props.transaction.date);
        date.value = new Date(d.getTime() - d.getTimezoneOffset() * 60000).toISOString().slice(0, 16);
        note.value = props.transaction.note;
        projectId.value = props.transaction.projectId || '';
        personId.value = props.transaction.personId || '';
    } else {
        type.value = 'expense';
        inputCurrency.value = currentCurrency.value;
        amount.value = '';
        exchangeRate.value = null;
        exchangeRateStr.value = '';
        calculatedBaseAmount.value = 0;
        
        category.value = props.expenseCategories.length ? props.expenseCategories[0] : '';
        accountId.value = props.accounts.length ? props.accounts[0].id : '';
        toAccountId.value = props.accounts.length > 1 ? props.accounts[1].id : '';
        const now = new Date();
        date.value = new Date(now.getTime() - now.getTimezoneOffset() * 60000).toISOString().slice(0, 16);
        note.value = '';
        projectId.value = props.defaultProjectId || '';
        personId.value = '';
    }
    showErrors.value = false;
    isAddingCategory.value = false;
    newCategoryName.value = '';
};

watch(() => props.show, (newVal) => {
    if (newVal) {
        initForm();
    }
});

const save = () => {
    if (!canSave.value) {
        showErrors.value = true;
        return;
    }
    
    const numericAmount = Number(amount.value.replace(/\D/g, ''));
    
    // Prevent saving if it's a transfer between the same account
    if (type.value === 'transfer' && accountId.value === toAccountId.value) {
        showErrors.value = true;
        return;
    }
    
    const tx: Transaction = {
        id: props.transaction?.id || `tx-${Date.now()}-${Math.floor(Math.random()*1000)}`,
        type: type.value,
        amount: calculatedBaseAmount.value,
        category: type.value === 'transfer' ? 'Transfer' : category.value,
        accountId: accountId.value,
        date: new Date(date.value).toISOString(),
        note: note.value.trim(),
        projectId: type.value === 'expense' && projectId.value ? projectId.value : undefined,
        personId: personId.value ? personId.value : undefined
    };
    
    if (inputCurrency.value !== currentCurrency.value) {
        tx.originalCurrency = inputCurrency.value;
        tx.originalAmount = Number(amount.value.replace(/\D/g, ''));
        tx.exchangeRate = exchangeRate.value || 1;
    }
    
    if (type.value === 'transfer') {
        tx.toAccountId = toAccountId.value;
    }
    
    emit('save', tx);
};

const saveNewCategory = () => {
    const name = newCategoryName.value.trim();
    if (name) {
        emit('addCategory', { type: type.value as 'income' | 'expense', name });
        category.value = name;
    }
    isAddingCategory.value = false;
    newCategoryName.value = '';
};

// Computed property for save validation
const canSave = computed(() => {
    const numericAmount = Number(amount.value.replace(/\D/g, ''));
    if (!numericAmount || numericAmount <= 0) return false;
    if (!accountId.value) return false;
    if (type.value === 'transfer' && (!toAccountId.value || accountId.value === toAccountId.value)) return false;
    return true;
});

const isDebtCategory = computed(() => {
    const debtKeywords = ['vay', 'nợ', 'borrow', 'lend', 'debt', 'trả', 'thu', 'mượn', 'loan'];
    const catLower = category.value.toLowerCase();
    return debtKeywords.some(k => catLower.includes(k));
});

const personSearch = ref('');
const isPersonDropdownOpen = ref(false);

const filteredPeople = computed(() => {
    if (!props.people) return [];
    if (!personSearch.value) return props.people;
    const q = personSearch.value.toLowerCase();
    return props.people.filter(p => p.title.toLowerCase().includes(q));
});

const getPersonName = (id: string) => {
    return props.people?.find(p => p.id === id)?.title || 'No person';
};

const openPersonDropdown = () => {
    personSearch.value = '';
    isPersonDropdownOpen.value = true;
};

const closePersonDropdown = () => {
    setTimeout(() => {
        isPersonDropdownOpen.value = false;
    }, 200);
};

</script>

<template>
  <div v-if="show" class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/50 dark:bg-black/70 backdrop-blur-sm" @click.self="emit('close')">
    <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl shadow-xl w-full max-w-md overflow-hidden animate-in zoom-in-95 duration-200">
      
      <!-- Header -->
      <div class="flex items-center justify-between p-4 border-b border-border dark:border-border-dark">
        <h3 class="font-bold text-lg text-text dark:text-text-dark">
            {{ transaction ? 'Edit Transaction' : 'New Transaction' }}
        </h3>
        <button @click="emit('close')" class="p-1 rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors">
            <X class="w-5 h-5" />
        </button>
      </div>

      <!-- Body -->
      <div class="p-5 space-y-4">
          
        <!-- Type Segmented Control -->
        <div class="flex p-1 bg-gray-100 dark:bg-gray-800 rounded-xl">
            <button @click="type = 'expense'" :class="['flex-1 py-1.5 text-sm font-medium rounded-lg transition-colors', type === 'expense' ? 'bg-white dark:bg-gray-700 text-red-500 shadow-sm' : 'text-gray-500 dark:text-gray-400 hover:text-gray-700']">
                Expense
            </button>
            <button @click="type = 'income'" :class="['flex-1 py-1.5 text-sm font-medium rounded-lg transition-colors', type === 'income' ? 'bg-white dark:bg-gray-700 text-green-500 shadow-sm' : 'text-gray-500 dark:text-gray-400 hover:text-gray-700']">
                Income
            </button>
            <button @click="type = 'transfer'" :class="['flex-1 py-1.5 text-sm font-medium rounded-lg transition-colors', type === 'transfer' ? 'bg-white dark:bg-gray-700 text-blue-500 shadow-sm' : 'text-gray-500 dark:text-gray-400 hover:text-gray-700']">
                Transfer
            </button>
        </div>

        <!-- Amount -->
        <div>
            <label class="block text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-1">Amount <span v-if="showErrors && calculatedBaseAmount <= 0" class="text-red-500 normal-case font-normal ml-1">*Must be > 0</span></label>
            <div class="flex gap-2">
                <div :class="['relative rounded-xl transition-all flex-1', showErrors && calculatedBaseAmount <= 0 ? 'ring-2 ring-red-500' : '']">
                    <input type="text" inputmode="numeric" :value="amount" @input="handleAmountInput" class="w-full bg-transparent border border-border dark:border-border-dark rounded-xl px-4 py-3 text-2xl font-bold text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500 transition-all pr-4" placeholder="0" />
                </div>
                <select v-model="inputCurrency" class="bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl px-3 py-3 font-bold text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500 appearance-none min-w-[80px] text-center cursor-pointer">
                    <option v-for="c in CURRENCIES" :key="c" :value="c">{{ c }}</option>
                </select>
            </div>
            
            <!-- Exchange Rate UI -->
            <div v-if="inputCurrency !== currentCurrency" class="mt-3 p-3 bg-blue-50 dark:bg-blue-900/10 rounded-xl border border-blue-100 dark:border-blue-900/30 flex flex-col gap-2">
                <div class="flex items-center justify-between">
                    <span class="text-xs font-semibold text-blue-600 dark:text-blue-400">Exchange Rate ({{ inputCurrency }} &rarr; {{ currentCurrency }})</span>
                    <span v-if="isFetchingRate" class="text-xs text-blue-500 animate-pulse flex items-center gap-1"><RefreshCw class="w-3 h-3 animate-spin" /> Fetching...</span>
                </div>
                <div class="flex gap-2 items-center">
                    <input type="text" inputmode="decimal" :value="exchangeRateStr" @input="handleRateInput" class="w-full bg-white dark:bg-gray-900 border border-blue-200 dark:border-blue-800 rounded-lg px-3 py-2 text-sm font-bold text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500" placeholder="Custom rate..." :disabled="isFetchingRate" />
                    <span class="text-sm font-bold text-blue-700 dark:text-blue-300 whitespace-nowrap">
                        ≈ {{ new Intl.NumberFormat(currentCurrency === 'VND' ? 'vi-VN' : 'en-US', { style: 'currency', currency: currentCurrency }).format(calculatedBaseAmount) }}
                    </span>
                </div>
            </div>
        </div>

        <div class="grid grid-cols-2 gap-4">
            <!-- Category (Hidden for Transfer) -->
            <div v-if="type !== 'transfer'">
                <label class="block text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-1">Category</label>
                <div class="flex items-center gap-2">
                    <template v-if="!isAddingCategory">
                        <select v-model="category" class="w-full bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl px-3 py-2.5 text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500 appearance-none">
                            <option v-for="cat in availableCategories" :key="cat" :value="cat">{{ cat }}</option>
                        </select>
                        <button @click="isAddingCategory = true" class="p-2.5 text-gray-400 hover:text-blue-500 hover:bg-blue-50 dark:hover:bg-blue-900/30 rounded-xl transition-colors shrink-0 border border-border dark:border-border-dark bg-gray-50 dark:bg-gray-800" title="Add new category">
                            <Plus class="w-4 h-4" />
                        </button>
                    </template>
                    <template v-else>
                        <input type="text" v-model="newCategoryName" @keyup.enter="saveNewCategory" class="w-full bg-white dark:bg-gray-900 border border-blue-300 dark:border-blue-700 rounded-xl px-3 py-2.5 text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500" placeholder="Type new category..." autofocus />
                        <button @click="saveNewCategory" class="p-2.5 text-white bg-blue-500 hover:bg-blue-600 rounded-xl transition-colors shrink-0" title="Save category">
                            <Check class="w-4 h-4" />
                        </button>
                        <button @click="isAddingCategory = false; newCategoryName = ''" class="p-2.5 text-gray-500 bg-gray-100 dark:bg-gray-800 hover:bg-gray-200 dark:hover:bg-gray-700 rounded-xl transition-colors shrink-0" title="Cancel">
                            <X class="w-4 h-4" />
                        </button>
                    </template>
                </div>
            </div>
            
            <!-- From Account -->
            <div>
                <label class="block text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-1">{{ type === 'transfer' ? 'From Account' : 'Account' }} <span v-if="showErrors && !accountId" class="text-red-500 normal-case font-normal ml-1">*Required</span></label>
                <select v-model="accountId" :class="['w-full bg-gray-50 dark:bg-gray-800 border rounded-xl px-3 py-2.5 text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500 appearance-none', showErrors && !accountId ? 'border-red-500' : 'border-border dark:border-border-dark']">
                    <option v-for="acc in accounts" :key="acc.id" :value="acc.id">{{ acc.name }}</option>
                </select>
            </div>
            
            <!-- To Account (Only for Transfer) -->
            <div v-if="type === 'transfer'">
                <label class="block text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-1">To Account <span v-if="showErrors && (!toAccountId || accountId === toAccountId)" class="text-red-500 normal-case font-normal ml-1">*Invalid</span></label>
                <select v-model="toAccountId" :class="['w-full bg-gray-50 dark:bg-gray-800 border rounded-xl px-3 py-2.5 text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500 appearance-none', showErrors && (!toAccountId || accountId === toAccountId) ? 'border-red-500' : 'border-border dark:border-border-dark']">
                    <option v-for="acc in accounts" :key="acc.id" :value="acc.id">{{ acc.name }}</option>
                </select>
            </div>
        </div>

        <!-- Date -->
        <div>
            <label class="block text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-1">Date</label>
            <input type="datetime-local" v-model="date" class="w-full bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl px-3 py-2.5 text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500" />
        </div>

        <!-- Note -->
        <div>
            <label class="block text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-1">Note</label>
            <input type="text" v-model="note" class="w-full bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl px-3 py-2.5 text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500" placeholder="Transaction details..." />
        </div>
        
        <!-- Project Link (Only for Expense) -->
        <div v-if="type === 'expense' && projects && projects.length > 0">
            <label class="block text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-1">Link to Project</label>
            <select v-model="projectId" class="w-full bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl px-3 py-2.5 text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500 appearance-none">
                <option value="">No project</option>
                <option v-for="p in projects" :key="p.id" :value="p.id">{{ p.title }}</option>
            </select>
        </div>

        <!-- Person Link (Only for Debt categories) -->
        <div v-if="people && people.length > 0 && isDebtCategory">
            <label class="block text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-1">Link to Person</label>
            <div class="relative">
                <input 
                    type="text" 
                    v-model="personSearch" 
                    @focus="openPersonDropdown"
                    @blur="closePersonDropdown"
                    class="w-full bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl px-3 py-2.5 text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500" 
                    :placeholder="personId ? getPersonName(personId) : 'Search person...'" 
                />
                <X v-if="personId" @click="personId = ''; personSearch = ''" class="w-4 h-4 absolute right-3 top-1/2 -translate-y-1/2 text-gray-400 cursor-pointer hover:text-red-500 transition-colors" />
                
                <div v-if="isPersonDropdownOpen" class="absolute z-10 w-full mt-1 bg-white dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl shadow-lg max-h-48 overflow-y-auto hidden-scrollbar py-1">
                    <div 
                        class="px-3 py-2 text-sm text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-700 cursor-pointer transition-colors"
                        @click="personId = ''; personSearch = ''; isPersonDropdownOpen = false"
                    >
                        No person
                    </div>
                    <div 
                        v-for="p in filteredPeople" 
                        :key="p.id" 
                        class="px-3 py-2 text-sm text-text dark:text-text-dark hover:bg-blue-50 dark:hover:bg-blue-900/20 cursor-pointer transition-colors"
                        @click="personId = p.id; personSearch = ''; isPersonDropdownOpen = false"
                    >
                        {{ p.title }}
                    </div>
                    <div v-if="filteredPeople.length === 0" class="px-3 py-2 text-sm text-gray-400 italic">
                        No matching people
                    </div>
                </div>
            </div>
        </div>

      </div>

      <!-- Footer -->
      <div class="p-4 border-t border-border dark:border-border-dark flex justify-between gap-3 bg-gray-50/50 dark:bg-gray-800/50">
        <div>
            <button v-if="transaction" @click="emit('delete', transaction.id)" class="px-3 py-2 rounded-xl text-sm font-medium text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors flex items-center gap-1">
                <Trash2 class="w-4 h-4" />
                Delete
            </button>
        </div>
        <div class="flex gap-3">
            <button @click="emit('close')" class="px-4 py-2 rounded-xl text-sm font-medium text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors">
                Cancel
            </button>
            <button @click="save" class="px-5 py-2 rounded-xl text-sm font-medium bg-blue-500 hover:bg-blue-600 text-white shadow-sm transition-colors">
                Save Transaction
            </button>
        </div>
      </div>

    </div>
  </div>
</template>
