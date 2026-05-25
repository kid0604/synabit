<script setup lang="ts">
import { ref, onMounted, computed } from 'vue';
import { X, Wallet, Users, FileText } from 'lucide-vue-next';
import type { Debt, FinanceAccount, Transaction } from './types';

const props = defineProps<{
    show: boolean;
    accounts: FinanceAccount[];
    people?: {id: string, title: string}[];
    editingDebt?: Debt | null;
    defaultType?: 'lend' | 'borrow';
}>();

const emit = defineEmits<{
    (e: 'close'): void;
    (e: 'save', debt: Debt, initialTx?: Transaction): void;
}>();

// Form state
const type = ref<'lend' | 'borrow'>(props.defaultType || 'lend');
const person = ref('');
const totalAmountStr = ref('');
const startDate = ref('');
const dueDate = ref('');
const accountId = ref('');
const note = ref('');

// To track if creating a transaction is applicable (only for new debts)
const isNew = computed(() => !props.editingDebt);

onMounted(() => {
    if (props.editingDebt) {
        type.value = props.editingDebt.type;
        person.value = props.editingDebt.person;
        totalAmountStr.value = props.editingDebt.totalAmount.toString();
        startDate.value = props.editingDebt.startDate.split('T')[0];
        dueDate.value = props.editingDebt.dueDate ? props.editingDebt.dueDate.split('T')[0] : '';
        accountId.value = props.editingDebt.accountId || '';
        note.value = props.editingDebt.note || '';
    } else {
        const today = new Date();
        startDate.value = today.toISOString().split('T')[0];
        
        // Suggest a default account
        if (props.accounts.length > 0) {
            accountId.value = props.accounts[0].id;
        }
    }
});

const formatCurrencyInput = (e: Event) => {
    const input = e.target as HTMLInputElement;
    let val = input.value.replace(/\D/g, '');
    if (val) {
        val = new Intl.NumberFormat('en-US').format(parseInt(val));
    }
    input.value = val;
    totalAmountStr.value = val.replace(/\./g, '');
};

const save = () => {
    if (!person.value || !totalAmountStr.value || !startDate.value || !accountId.value) return;

    const amount = parseInt(totalAmountStr.value);
    if (isNaN(amount) || amount <= 0) return;

    const now = new Date();
    const dStart = new Date(startDate.value);
    dStart.setHours(now.getHours(), now.getMinutes(), now.getSeconds());

    let dDue = undefined;
    if (dueDate.value) {
        dDue = new Date(dueDate.value);
        dDue.setHours(23, 59, 59);
    }

    const matchedPerson = props.people?.find(p => p.title === person.value);

    const debt: Debt = {
        id: props.editingDebt ? props.editingDebt.id : `debt-${Date.now()}-${Math.floor(Math.random()*1000)}`,
        type: type.value,
        person: person.value,
        personId: matchedPerson ? matchedPerson.id : undefined,
        totalAmount: amount,
        paidAmount: props.editingDebt ? props.editingDebt.paidAmount : 0,
        startDate: dStart.toISOString(),
        dueDate: dDue ? dDue.toISOString() : undefined,
        accountId: accountId.value,
        note: note.value,
        status: props.editingDebt ? props.editingDebt.status : 'active'
    };

    let initialTx: Transaction | undefined;
    
    // Only generate initial transaction if it's a NEW debt
    if (isNew.value) {
        initialTx = {
            id: `tx-${Date.now()}-${Math.floor(Math.random()*1000)}`,
            type: debt.type === 'lend' ? 'expense' : 'income',
            amount: debt.totalAmount,
            category: debt.type === 'lend' ? 'Lending' : 'Borrowing',
            accountId: debt.accountId,
            date: debt.startDate,
            note: `${debt.type === 'lend' ? `Lent to ${debt.person}` : `Borrowed from ${debt.person}`}${debt.note ? ` - ${debt.note}` : ''}`,
            debtId: debt.id,
            personId: debt.personId
        };
    }

    emit('save', debt, initialTx);
};

</script>

<template>
    <div v-if="show" class="fixed inset-0 z-[100] flex items-center justify-center p-4 sm:p-6">
        <!-- Backdrop -->
        <div class="absolute inset-0 bg-gray-900/40 dark:bg-black/60 backdrop-blur-sm" @click="emit('close')"></div>
        
        <!-- Modal -->
        <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-3xl shadow-2xl w-full max-w-md relative flex flex-col max-h-full overflow-hidden animate-in fade-in zoom-in-95 duration-200">
            
            <!-- Header -->
            <div class="px-6 py-5 border-b border-border dark:border-border-dark flex justify-between items-center bg-gray-50/50 dark:bg-gray-800/30">
                <h3 class="text-xl font-bold text-text dark:text-text-dark flex items-center gap-2">
                    {{ isNew ? 'New Debt' : 'Edit Debt' }}
                </h3>
                <button @click="emit('close')" class="p-2 rounded-full hover:bg-gray-200 dark:hover:bg-gray-700 transition-colors text-gray-500">
                    <X class="w-5 h-5" />
                </button>
            </div>
            
            <!-- Body -->
            <div class="p-6 overflow-y-auto hidden-scrollbar flex flex-col gap-5">
                
                <!-- Type Selection (Only for new) -->
                <div v-if="isNew" class="flex p-1 bg-gray-100 dark:bg-gray-800 rounded-xl">
                    <button 
                        @click="type = 'lend'"
                        :class="['flex-1 py-2.5 rounded-lg font-bold text-sm transition-all', type === 'lend' ? 'bg-white dark:bg-gray-700 text-green-600 dark:text-green-400 shadow-sm' : 'text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-200']"
                    >
                        Lend (Receivable)
                    </button>
                    <button 
                        @click="type = 'borrow'"
                        :class="['flex-1 py-2.5 rounded-lg font-bold text-sm transition-all', type === 'borrow' ? 'bg-white dark:bg-gray-700 text-red-600 dark:text-red-400 shadow-sm' : 'text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-200']"
                    >
                        Borrow (Payable)
                    </button>
                </div>

                <!-- Form Fields -->
                <div class="flex flex-col gap-4">
                    <!-- Person -->
                    <div>
                        <label class="block text-sm font-bold text-gray-700 dark:text-gray-300 mb-1.5 ml-1">Person</label>
                        <div class="relative">
                            <div class="absolute inset-y-0 left-0 pl-3.5 flex items-center pointer-events-none text-gray-400">
                                <Users class="w-5 h-5" />
                            </div>
                            <input 
                                v-model="person" 
                                type="text" 
                                list="debt-people-list"
                                placeholder="Enter person's name..."
                                class="w-full pl-11 pr-4 py-3 bg-gray-50 dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-xl focus:ring-2 focus:ring-blue-500 outline-none transition-all text-text dark:text-text-dark font-medium placeholder-gray-400 dark:placeholder-gray-600"
                            />
                            <datalist id="debt-people-list" v-if="people">
                                <option v-for="p in people" :key="p.id" :value="p.title"></option>
                            </datalist>
                        </div>
                    </div>

                    <!-- Amount -->
                    <div>
                        <label class="block text-sm font-bold text-gray-700 dark:text-gray-300 mb-1.5 ml-1">Amount</label>
                        <div class="relative">
                            <input 
                                :value="new Intl.NumberFormat('en-US').format(Number(totalAmountStr) || 0) === '0' ? '' : new Intl.NumberFormat('en-US').format(Number(totalAmountStr) || 0)"
                                @input="formatCurrencyInput"
                                type="text" 
                                placeholder="0"
                                class="w-full pl-4 pr-12 py-3 bg-gray-50 dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-xl focus:ring-2 focus:ring-blue-500 outline-none transition-all text-text dark:text-text-dark font-bold text-lg placeholder-gray-400 dark:placeholder-gray-600"
                            />
                        </div>
                    </div>

                    <!-- Account -->
                    <div v-if="isNew">
                        <label class="block text-sm font-bold text-gray-700 dark:text-gray-300 mb-1.5 ml-1">{{ type === 'lend' ? 'Withdraw from' : 'Deposit to' }} Account</label>
                        <div class="relative">
                            <div class="absolute inset-y-0 left-0 pl-3.5 flex items-center pointer-events-none text-gray-400">
                                <Wallet class="w-5 h-5" />
                            </div>
                            <select 
                                v-model="accountId" 
                                class="w-full pl-11 pr-4 py-3 bg-gray-50 dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-xl focus:ring-2 focus:ring-blue-500 outline-none transition-all text-text dark:text-text-dark font-medium appearance-none cursor-pointer"
                            >
                                <option value="" disabled>Select account...</option>
                                <option v-for="acc in accounts" :key="acc.id" :value="acc.id">{{ acc.name }}</option>
                            </select>
                        </div>
                    </div>

                    <!-- Dates -->
                    <div class="grid grid-cols-2 gap-4">
                        <div>
                            <label class="block text-sm font-bold text-gray-700 dark:text-gray-300 mb-1.5 ml-1">Date</label>
                            <div class="relative">
                                <input 
                                    v-model="startDate" 
                                    type="date" 
                                    class="w-full pl-3 pr-3 py-3 bg-gray-50 dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-xl focus:ring-2 focus:ring-blue-500 outline-none transition-all text-text dark:text-text-dark font-medium"
                                />
                            </div>
                        </div>
                        <div>
                            <label class="block text-sm font-bold text-gray-700 dark:text-gray-300 mb-1.5 ml-1">Due Date (Optional)</label>
                            <div class="relative">
                                <input 
                                    v-model="dueDate" 
                                    type="date" 
                                    class="w-full pl-3 pr-3 py-3 bg-gray-50 dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-xl focus:ring-2 focus:ring-blue-500 outline-none transition-all text-text dark:text-text-dark font-medium"
                                />
                            </div>
                        </div>
                    </div>

                    <!-- Note -->
                    <div>
                        <label class="block text-sm font-bold text-gray-700 dark:text-gray-300 mb-1.5 ml-1">Note (Optional)</label>
                        <div class="relative">
                            <div class="absolute top-3.5 left-3.5 pointer-events-none text-gray-400">
                                <FileText class="w-5 h-5" />
                            </div>
                            <textarea 
                                v-model="note" 
                                placeholder="Reason for debt..."
                                rows="2"
                                class="w-full pl-11 pr-4 py-3 bg-gray-50 dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-xl focus:ring-2 focus:ring-blue-500 outline-none transition-all text-text dark:text-text-dark font-medium placeholder-gray-400 dark:placeholder-gray-600 resize-none"
                            ></textarea>
                        </div>
                    </div>

                </div>
            </div>
            
            <!-- Footer -->
            <div class="p-5 border-t border-border dark:border-border-dark bg-gray-50/50 dark:bg-gray-800/30 flex gap-3">
                <button @click="emit('close')" class="flex-1 px-4 py-3 rounded-xl font-bold text-gray-600 dark:text-gray-300 bg-gray-200 dark:bg-gray-700 hover:bg-gray-300 dark:hover:bg-gray-600 transition-colors">
                    Cancel
                </button>
                <button 
                    @click="save" 
                    :disabled="!person || !totalAmountStr || !startDate || !accountId"
                    class="flex-1 px-4 py-3 rounded-xl font-bold text-white bg-blue-500 hover:bg-blue-600 transition-colors disabled:opacity-50 disabled:cursor-not-allowed shadow-sm"
                >
                    Save Debt
                </button>
            </div>
            
        </div>
    </div>
</template>
