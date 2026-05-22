<script setup lang="ts">
import { ref, watch } from 'vue';
import { X } from 'lucide-vue-next';
import type { Transaction, TransactionType, FinanceAccount } from './types';

const props = defineProps<{
  show: boolean;
  transaction?: Transaction | null;
  incomeCategories: string[];
  expenseCategories: string[];
  accounts: FinanceAccount[];
  projects?: {id: string, title: string}[];
  defaultProjectId?: string;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'save', tx: Transaction): void;
}>();

const type = ref<TransactionType>('expense');
const amount = ref<string>('');
const category = ref<string>('');
const accountId = ref<string>('');
const toAccountId = ref<string>('');
const date = ref<string>('');
const note = ref<string>('');
const projectId = ref<string>('');
const showErrors = ref(false);

import { computed } from 'vue';

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

const initForm = () => {
    if (props.transaction) {
        type.value = props.transaction.type;
        amount.value = props.transaction.amount.toLocaleString('vi-VN');
        category.value = props.transaction.category;
        accountId.value = props.transaction.accountId;
        toAccountId.value = props.transaction.toAccountId || '';
        // datetime-local expects YYYY-MM-DDThh:mm
        const d = new Date(props.transaction.date);
        date.value = new Date(d.getTime() - d.getTimezoneOffset() * 60000).toISOString().slice(0, 16);
        note.value = props.transaction.note;
        projectId.value = props.transaction.projectId || '';
    } else {
        type.value = 'expense';
        amount.value = '';
        category.value = props.expenseCategories.length ? props.expenseCategories[0] : '';
        accountId.value = props.accounts.length ? props.accounts[0].id : '';
        toAccountId.value = props.accounts.length > 1 ? props.accounts[1].id : '';
        const now = new Date();
        date.value = new Date(now.getTime() - now.getTimezoneOffset() * 60000).toISOString().slice(0, 16);
        note.value = '';
        projectId.value = props.defaultProjectId || '';
    }
    showErrors.value = false;
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
        amount: numericAmount,
        category: type.value === 'transfer' ? 'Transfer' : category.value,
        accountId: accountId.value,
        date: new Date(date.value).toISOString(),
        note: note.value.trim(),
        projectId: type.value === 'expense' && projectId.value ? projectId.value : undefined
    };
    
    if (type.value === 'transfer') {
        tx.toAccountId = toAccountId.value;
    }
    
    emit('save', tx);
};

// Computed property for save validation
const canSave = computed(() => {
    const numericAmount = Number(amount.value.replace(/\D/g, ''));
    if (!numericAmount || numericAmount <= 0) return false;
    if (!accountId.value) return false;
    if (type.value === 'transfer' && (!toAccountId.value || accountId.value === toAccountId.value)) return false;
    return true;
});

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
            <label class="block text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-1">Amount <span v-if="showErrors && (!amount || Number(amount.replace(/\\D/g, '')) <= 0)" class="text-red-500 normal-case font-normal ml-1">*Must be > 0</span></label>
            <div :class="['relative rounded-xl transition-all', showErrors && (!amount || Number(amount.replace(/\\D/g, '')) <= 0) ? 'ring-2 ring-red-500' : '']">
                <input type="text" inputmode="numeric" :value="amount" @input="handleAmountInput" class="w-full bg-transparent border border-border dark:border-border-dark rounded-xl px-4 py-3 text-2xl font-bold text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500 transition-all pr-12" placeholder="0" />
                <span class="absolute right-4 top-1/2 -translate-y-1/2 text-gray-400 font-medium">đ</span>
            </div>
        </div>

        <div class="grid grid-cols-2 gap-4">
            <!-- Category (Hidden for Transfer) -->
            <div v-if="type !== 'transfer'">
                <label class="block text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-1">Category</label>
                <select v-model="category" class="w-full bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl px-3 py-2.5 text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500 appearance-none">
                    <option v-for="cat in availableCategories" :key="cat" :value="cat">{{ cat }}</option>
                </select>
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

      </div>

      <!-- Footer -->
      <div class="p-4 border-t border-border dark:border-border-dark flex justify-end gap-3 bg-gray-50/50 dark:bg-gray-800/50">
        <button @click="emit('close')" class="px-4 py-2 rounded-xl text-sm font-medium text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors">
            Cancel
        </button>
        <button @click="save" class="px-5 py-2 rounded-xl text-sm font-medium bg-blue-500 hover:bg-blue-600 text-white shadow-sm transition-colors">
            Save Transaction
        </button>
      </div>

    </div>
  </div>
</template>
