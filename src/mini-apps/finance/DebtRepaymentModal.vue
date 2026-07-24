<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { X, Wallet, FileText, CheckCircle2 } from 'lucide-vue-next';
import type { Debt, FinanceAccount, Transaction } from './types';
import { formatCurrency } from './currency';

const props = defineProps<{
    show: boolean;
    debt: Debt;
    accounts: FinanceAccount[];
}>();

const emit = defineEmits<{
    (e: 'close'): void;
    (e: 'save', debt: Debt, amount: number, tx: Transaction): void;
}>();

// Form state
const amountStr = ref('');
const accountId = ref('');
const date = ref('');
const note = ref('');
const markCompleted = ref(false);

const remainingAmount = props.debt.totalAmount - props.debt.paidAmount;

onMounted(() => {
    const today = new Date();
    date.value = today.toISOString().split('T')[0];
    
    // Default account to the same account used when creating
    accountId.value = props.debt.accountId || (props.accounts.length > 0 ? props.accounts[0].id : '');
    
    // Default amount to remaining
    amountStr.value = remainingAmount.toString();
    markCompleted.value = true;
});

const formatCurrencyInput = (e: Event) => {
    const input = e.target as HTMLInputElement;
    let val = input.value.replace(/\D/g, '');
    if (val) {
        val = new Intl.NumberFormat('en-US').format(parseInt(val));
    }
    input.value = val;
    amountStr.value = val.replace(/\./g, '');
    
    // Auto toggle mark completed
    const amt = parseInt(amountStr.value);
    if (!isNaN(amt) && amt >= remainingAmount) {
        markCompleted.value = true;
    } else {
        markCompleted.value = false;
    }
};

// formatCurrency is imported from ./currency

const save = () => {
    if (!amountStr.value || !date.value || !accountId.value) return;

    const amount = parseInt(amountStr.value);
    if (isNaN(amount) || amount <= 0) return;

    const now = new Date();
    const dDate = new Date(date.value);
    dDate.setHours(now.getHours(), now.getMinutes(), now.getSeconds());

    const updatedPaidAmount = props.debt.paidAmount + amount;
    
    const updatedDebt: Debt = {
        ...props.debt,
        paidAmount: updatedPaidAmount,
        status: (markCompleted.value || updatedPaidAmount >= props.debt.totalAmount) ? 'completed' : 'active'
    };

    const actionText = props.debt.type === 'lend' ? 'Collect from' : 'Repay to';
    const tx: Transaction = {
        id: `tx-${Date.now()}-${Math.floor(Math.random()*1000)}`,
        type: props.debt.type === 'lend' ? 'income' : 'expense',
        amount: amount,
        category: props.debt.type === 'lend' ? 'Debt Collection' : 'Debt Repayment',
        accountId: accountId.value,
        date: dDate.toISOString(),
        note: `${props.debt.type === 'lend' ? 'Collected from' : 'Repaid to'} ${props.debt.person}${note.value ? ` - ${note.value}` : ''}`,
        debtId: props.debt.id
    };

    emit('save', updatedDebt, amount, tx);
};

</script>

<template>
    <div v-if="show" class="fixed inset-0 z-[100] flex items-center justify-center p-4 sm:p-6">
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-gray-900/40 dark:bg-black/60 backdrop-blur-sm" @click="emit('close')"></div>
        
        <!-- Modal -->
        <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-3xl shadow-2xl w-full max-w-md relative flex flex-col max-h-full overflow-hidden animate-in fade-in zoom-in-95 duration-200">
            
            <!-- Header -->
            <div class="px-6 py-5 border-b border-border dark:border-border-dark flex justify-between items-center" :class="debt.type === 'lend' ? 'bg-green-50/50 dark:bg-green-900/10' : 'bg-blue-50/50 dark:bg-blue-900/10'">
                <h3 class="text-xl font-bold text-text dark:text-text-dark flex items-center gap-2">
                    {{ debt.type === 'lend' ? 'Log Repayment' : 'Log Debt Payment' }}
                </h3>
                <button @click="emit('close')" class="p-2 rounded-full hover:bg-gray-200 dark:hover:bg-gray-700 transition-colors text-gray-500" aria-label="More Options">
                    <X class="w-5 h-5" />
                </button>
            </div>
            
            <!-- Body -->
            <div class="p-6 overflow-y-auto hidden-scrollbar flex flex-col gap-5">
                
                <!-- Debt Info -->
                <div class="flex flex-col gap-1 text-center p-4 rounded-xl bg-gray-50 dark:bg-gray-800/50">
                    <p class="text-sm font-medium text-gray-500">{{ debt.type === 'lend' ? 'Collect from' : 'Pay to' }} <span class="text-text dark:text-text-dark font-bold">{{ debt.person }}</span></p>
                    <p class="text-2xl font-bold" :class="debt.type === 'lend' ? 'text-green-600 dark:text-green-400' : 'text-blue-600 dark:text-blue-400'">
                        {{ debt.type === 'lend' ? 'Remaining' : 'Owed' }}: {{ formatCurrency(remainingAmount) }}
                    </p>
                </div>

                <!-- Form Fields -->
                <div class="flex flex-col gap-4">
                    <!-- Amount -->
                    <div>
                        <label class="block text-sm font-bold text-gray-700 dark:text-gray-300 mb-1.5 ml-1">{{ $t('finance.transaction_amount') }}</label>
                        <div class="relative">
                            <input 
                                :value="new Intl.NumberFormat('en-US').format(Number(amountStr) || 0) === '0' ? '' : new Intl.NumberFormat('en-US').format(Number(amountStr) || 0)"
                                @input="formatCurrencyInput"
                                type="text" 
                                placeholder="0"
                                class="w-full pl-4 pr-12 py-3 bg-gray-50 dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-xl focus:ring-2 outline-none transition-all text-text dark:text-text-dark font-bold text-lg placeholder-gray-400 dark:placeholder-gray-600"
                                :class="debt.type === 'lend' ? 'focus:ring-green-500' : 'focus:ring-blue-500'"
                            />
                            <!-- Remove hardcoded currency symbol, or add currentCurrency symbol logic if needed -->
                        </div>
                    </div>

                    <!-- Account -->
                    <div>
                        <label class="block text-sm font-bold text-gray-700 dark:text-gray-300 mb-1.5 ml-1">{{ debt.type === 'lend' ? 'Deposit' : 'Withdraw' }} Account</label>
                        <div class="relative">
                            <div class="absolute inset-y-0 left-0 pl-3.5 flex items-center pointer-events-none text-gray-400">
                                <Wallet class="w-5 h-5" />
                            </div>
                            <select 
                                v-model="accountId" 
                                class="w-full pl-11 pr-4 py-3 bg-gray-50 dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-xl focus:ring-2 outline-none transition-all text-text dark:text-text-dark font-medium appearance-none cursor-pointer"
                                :class="debt.type === 'lend' ? 'focus:ring-green-500' : 'focus:ring-blue-500'"
                            >
                                <option value="" disabled>{{ $t('finance.select_account') }}</option>
                                <option v-for="acc in accounts" :key="acc.id" :value="acc.id">{{ acc.name }}</option>
                            </select>
                        </div>
                    </div>

                    <!-- Date -->
                    <div>
                        <label class="block text-sm font-bold text-gray-700 dark:text-gray-300 mb-1.5 ml-1">{{ $t('finance.date') }}</label>
                        <div class="relative">
                            <input 
                                v-model="date" 
                                type="date" 
                                class="w-full pl-3 pr-3 py-3 bg-gray-50 dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-xl focus:ring-2 outline-none transition-all text-text dark:text-text-dark font-medium"
                                :class="debt.type === 'lend' ? 'focus:ring-green-500' : 'focus:ring-blue-500'"
                            />
                        </div>
                    </div>

                    <!-- Note -->
                    <div>
                        <label class="block text-sm font-bold text-gray-700 dark:text-gray-300 mb-1.5 ml-1">{{ $t('finance.note_opt') }}</label>
                        <div class="relative">
                            <div class="absolute top-3.5 left-3.5 pointer-events-none text-gray-400">
                                <FileText class="w-5 h-5" />
                            </div>
                            <textarea 
                                v-model="note" 
                                :placeholder="$t('finance.add_note_ph')"
                                rows="2"
                                class="w-full pl-11 pr-4 py-3 bg-gray-50 dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-xl focus:ring-2 outline-none transition-all text-text dark:text-text-dark font-medium placeholder-gray-400 dark:placeholder-gray-600 resize-none"
                                :class="debt.type === 'lend' ? 'focus:ring-green-500' : 'focus:ring-blue-500'"
                            ></textarea>
                        </div>
                    </div>

                    <!-- Toggle Completed -->
                    <label class="flex items-center gap-3 cursor-pointer p-3 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors mt-2">
                        <div class="relative flex items-center justify-center w-6 h-6 shrink-0">
                            <input type="checkbox" v-model="markCompleted" class="peer sr-only" />
                            <div class="w-5 h-5 border-2 border-gray-300 dark:border-gray-600 rounded-md peer-checked:bg-blue-500 peer-checked:border-blue-500 transition-colors"></div>
                            <CheckCircle2 class="absolute text-white w-4 h-4 opacity-0 peer-checked:opacity-100 transition-opacity" />
                        </div>
                        <span class="text-sm font-medium text-gray-700 dark:text-gray-300">{{ $t('finance.mark_completed') }}</span>
                    </label>

                </div>
            </div>
            
            <!-- Footer -->
            <div class="p-5 border-t border-border dark:border-border-dark bg-gray-50/50 dark:bg-gray-800/30 flex gap-3">
                <button @click="emit('close')" class="flex-1 px-4 py-3 rounded-xl font-bold text-gray-600 dark:text-gray-300 bg-gray-200 dark:bg-gray-700 hover:bg-gray-300 dark:hover:bg-gray-600 transition-colors">
                    Cancel
                </button>
                <button 
                    @click="save" 
                    :disabled="!amountStr || !date || !accountId"
                    class="flex-1 px-4 py-3 rounded-xl font-bold text-white transition-colors disabled:opacity-50 disabled:cursor-not-allowed shadow-sm"
                    :class="debt.type === 'lend' ? 'bg-green-500 hover:bg-green-600' : 'bg-blue-500 hover:bg-blue-600'"
                >
                    Save Record
                </button>
            </div>
            
        </div>
    </div>
</template>
