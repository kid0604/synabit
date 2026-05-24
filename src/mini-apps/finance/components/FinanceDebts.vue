<script setup lang="ts">
import { ref, computed } from 'vue';
import { Plus, BookOpen, User, Calendar, CheckCircle2, AlertCircle, ArrowUpRight, ArrowDownLeft } from 'lucide-vue-next';
import type { Debt, FinanceAccount, Transaction } from '../types';
import DebtModal from '../DebtModal.vue';
import DebtRepaymentModal from '../DebtRepaymentModal.vue';
import { formatCurrency } from '../currency';

const props = defineProps<{
    debts: Debt[];
    accounts: FinanceAccount[];
}>();

const emit = defineEmits<{
    (e: 'save-debts', debts: Debt[]): void;
    (e: 'create-transaction', tx: Transaction): void;
}>();

// --- State ---
const currentTab = ref<'lend' | 'borrow'>('lend'); // lend = Phải thu, borrow = Phải trả
const showDebtModal = ref(false);
const showRepayModal = ref(false);
const editingDebt = ref<Debt | null>(null);

// --- Computed ---
const displayDebts = computed(() => {
    return props.debts.filter(d => d.type === currentTab.value).sort((a, b) => {
        // Sort active first, then by date
        if (a.status !== b.status) return a.status === 'active' ? -1 : 1;
        return new Date(b.startDate).getTime() - new Date(a.startDate).getTime();
    });
});

const totalLend = computed(() => {
    return props.debts.filter(d => d.type === 'lend' && d.status === 'active').reduce((sum, d) => sum + (d.totalAmount - d.paidAmount), 0);
});

const totalBorrow = computed(() => {
    return props.debts.filter(d => d.type === 'borrow' && d.status === 'active').reduce((sum, d) => sum + (d.totalAmount - d.paidAmount), 0);
});

// --- Methods ---
// formatCurrency is imported from ../currency

const formatDate = (isoStr: string) => {
    if (!isoStr) return '';
    const d = new Date(isoStr);
    return `${d.getDate().toString().padStart(2, '0')}/${(d.getMonth()+1).toString().padStart(2, '0')}/${d.getFullYear()}`;
};

const isOverdue = (debt: Debt) => {
    if (debt.status === 'completed' || !debt.dueDate) return false;
    return new Date(debt.dueDate).getTime() < new Date().getTime();
};

const openAddDebt = () => {
    editingDebt.value = null;
    showDebtModal.value = true;
};

const openEditDebt = (debt: Debt) => {
    editingDebt.value = debt;
    showDebtModal.value = true;
};

const openRepayment = (debt: Debt) => {
    editingDebt.value = debt;
    showRepayModal.value = true;
};

const handleSaveDebt = (debt: Debt, initialTx?: Transaction) => {
    const updatedDebts = [...props.debts];
    const idx = updatedDebts.findIndex(d => d.id === debt.id);
    if (idx >= 0) {
        updatedDebts[idx] = debt;
    } else {
        updatedDebts.push(debt);
    }
    emit('save-debts', updatedDebts);
    
    // If it's a new debt and has an initial transaction
    if (initialTx) {
        emit('create-transaction', initialTx);
    }
    showDebtModal.value = false;
};

const handleRepayment = (debt: Debt, _amount: number, tx: Transaction) => {
    const updatedDebts = [...props.debts];
    const idx = updatedDebts.findIndex(d => d.id === debt.id);
    if (idx >= 0) {
        updatedDebts[idx] = debt;
    }
    emit('save-debts', updatedDebts);
    emit('create-transaction', tx);
    showRepayModal.value = false;
};

const toggleStatus = (debt: Debt) => {
    const updatedDebts = [...props.debts];
    const idx = updatedDebts.findIndex(d => d.id === debt.id);
    if (idx >= 0) {
        updatedDebts[idx].status = updatedDebts[idx].status === 'active' ? 'completed' : 'active';
        emit('save-debts', updatedDebts);
    }
};
</script>

<template>
    <div class="h-full overflow-y-auto hidden-scrollbar flex flex-col gap-6">
        
        <!-- Dashboard Summary -->
        <div class="grid grid-cols-1 md:grid-cols-2 gap-6 shrink-0">
            <!-- Khoản Phải Thu (Lend) -->
            <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl p-6 shadow-sm relative overflow-hidden group hover:border-green-500/50 transition-colors">
                <div class="absolute right-0 top-0 opacity-5 pointer-events-none group-hover:opacity-10 transition-opacity">
                    <ArrowUpRight class="w-32 h-32 -mt-6 -mr-6 text-green-500" />
                </div>
                <div class="flex items-center gap-3 mb-4">
                    <div class="p-2.5 rounded-xl bg-green-50 dark:bg-green-900/20 text-green-600 dark:text-green-400">
                        <ArrowUpRight class="w-6 h-6" />
                    </div>
                    <div>
                        <h3 class="font-bold text-gray-500 dark:text-gray-400 uppercase tracking-wider text-sm">Total Lent</h3>
                        <p class="text-xs text-gray-400 mt-0.5">Money others owe you</p>
                    </div>
                </div>
                <p class="text-3xl font-bold text-text dark:text-text-dark">{{ formatCurrency(totalLend) }}</p>
            </div>
            
            <!-- Khoản Phải Trả (Borrow) -->
            <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl p-6 shadow-sm relative overflow-hidden group hover:border-red-500/50 transition-colors">
                <div class="absolute right-0 top-0 opacity-5 pointer-events-none group-hover:opacity-10 transition-opacity">
                    <ArrowDownLeft class="w-32 h-32 -mt-6 -mr-6 text-red-500" />
                </div>
                <div class="flex items-center gap-3 mb-4">
                    <div class="p-2.5 rounded-xl bg-red-50 dark:bg-red-900/20 text-red-600 dark:text-red-400">
                        <ArrowDownLeft class="w-6 h-6" />
                    </div>
                    <div>
                        <h3 class="font-bold text-gray-500 dark:text-gray-400 uppercase tracking-wider text-sm">Total Borrowed</h3>
                        <p class="text-xs text-gray-400 mt-0.5">Money you owe others</p>
                    </div>
                </div>
                <p class="text-3xl font-bold text-text dark:text-text-dark">{{ formatCurrency(totalBorrow) }}</p>
            </div>
        </div>

        <!-- Debt Ledger -->
        <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl shadow-sm flex flex-col overflow-hidden relative shrink-0">
            <!-- Tabs & Actions -->
            <div class="p-4 border-b border-border dark:border-border-dark flex flex-wrap gap-4 justify-between items-center bg-gray-50/50 dark:bg-gray-800/50">
                <div class="flex p-1 bg-gray-200/50 dark:bg-gray-800 rounded-xl">
                    <button 
                        @click="currentTab = 'lend'"
                        :class="['px-6 py-2 rounded-lg font-medium text-sm transition-colors', currentTab === 'lend' ? 'bg-white dark:bg-gray-700 text-green-600 dark:text-green-400 shadow-sm' : 'text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-200']"
                    >
                        Receivables
                    </button>
                    <button 
                        @click="currentTab = 'borrow'"
                        :class="['px-6 py-2 rounded-lg font-medium text-sm transition-colors', currentTab === 'borrow' ? 'bg-white dark:bg-gray-700 text-red-600 dark:text-red-400 shadow-sm' : 'text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-200']"
                    >
                        Payables
                    </button>
                </div>

                <button @click="openAddDebt" class="flex items-center gap-2 px-4 py-2 rounded-xl bg-blue-50 text-blue-600 dark:bg-blue-900/30 dark:text-blue-400 hover:bg-blue-100 dark:hover:bg-blue-900/50 transition-colors font-medium text-sm">
                    <Plus class="w-4 h-4" />
                    Add {{ currentTab === 'lend' ? 'receivable' : 'payable' }}
                </button>
            </div>

            <!-- List -->
            <div class="p-0">
                <div v-if="displayDebts.length === 0" class="p-12 flex flex-col items-center justify-center text-gray-400">
                    <BookOpen class="w-12 h-12 mb-4 opacity-20" />
                    <p>No {{ currentTab === 'lend' ? 'receivables' : 'payables' }} found.</p>
                </div>
                
                <div v-else class="divide-y divide-border dark:divide-border-dark">
                    <div v-for="debt in displayDebts" :key="debt.id" class="p-5 flex flex-col sm:flex-row gap-4 hover:bg-gray-50 dark:hover:bg-gray-800/50 transition-colors group">
                        
                        <!-- Status Icon -->
                        <div class="shrink-0 flex items-start">
                            <div v-if="debt.status === 'completed'" class="w-10 h-10 rounded-full bg-gray-100 dark:bg-gray-800 flex items-center justify-center text-gray-500">
                                <CheckCircle2 class="w-5 h-5" />
                            </div>
                            <div v-else-if="isOverdue(debt)" class="w-10 h-10 rounded-full bg-red-50 dark:bg-red-900/20 flex items-center justify-center text-red-500 relative">
                                <AlertCircle class="w-5 h-5" />
                                <span class="absolute -top-1 -right-1 flex h-3 w-3">
                                  <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-red-400 opacity-75"></span>
                                  <span class="relative inline-flex rounded-full h-3 w-3 bg-red-500"></span>
                                </span>
                            </div>
                            <div v-else class="w-10 h-10 rounded-full flex items-center justify-center" :class="debt.type === 'lend' ? 'bg-green-50 dark:bg-green-900/20 text-green-500' : 'bg-red-50 dark:bg-red-900/20 text-red-500'">
                                <User class="w-5 h-5" />
                            </div>
                        </div>

                        <!-- Content -->
                        <div class="flex-1 min-w-0 flex flex-col gap-3">
                            <div class="flex justify-between items-start gap-4">
                                <div>
                                    <h4 class="font-bold text-text dark:text-text-dark text-lg flex items-center gap-2">
                                        {{ debt.person }}
                                        <span v-if="debt.status === 'completed'" class="text-xs font-bold px-2 py-0.5 rounded-full bg-gray-100 dark:bg-gray-800 text-gray-500 uppercase">Completed</span>
                                    </h4>
                                    <p class="text-sm text-gray-500 mt-0.5 truncate">{{ debt.note || (debt.type === 'lend' ? 'Lent money' : 'Borrowed money') }}</p>
                                </div>
                                <div class="text-right shrink-0">
                                    <p class="font-bold text-lg" :class="[debt.status === 'completed' ? 'text-gray-500' : (debt.type === 'lend' ? 'text-green-600 dark:text-green-400' : 'text-red-600 dark:text-red-400')]">
                                        {{ formatCurrency(debt.totalAmount) }}
                                    </p>
                                    <p class="text-xs text-gray-500 mt-0.5 font-medium">Paid: {{ formatCurrency(debt.paidAmount) }}</p>
                                </div>
                            </div>

                            <!-- Progress Bar -->
                            <div class="w-full bg-gray-100 dark:bg-gray-800 rounded-full h-2 mt-1 relative overflow-hidden">
                                <div class="h-2 rounded-full transition-all" 
                                    :class="[debt.status === 'completed' ? 'bg-gray-400' : (debt.type === 'lend' ? 'bg-green-500' : 'bg-red-500')]"
                                    :style="{ width: `${Math.min(100, Math.max(0, (debt.paidAmount / debt.totalAmount) * 100))}%` }">
                                </div>
                            </div>

                            <div class="flex flex-wrap items-center justify-between gap-2 mt-1">
                                <div class="flex items-center gap-4 text-xs text-gray-500 font-medium">
                                    <div class="flex items-center gap-1.5">
                                        <Calendar class="w-3.5 h-3.5" />
                                        {{ formatDate(debt.startDate) }}
                                    </div>
                                    <div v-if="debt.dueDate" class="flex items-center gap-1.5" :class="{ 'text-red-500 font-bold': isOverdue(debt) }">
                                        <AlertCircle class="w-3.5 h-3.5" />
                                        Due: {{ formatDate(debt.dueDate) }}
                                    </div>
                                </div>
                                
                                <!-- Actions -->
                                <div class="flex items-center gap-2 opacity-100 sm:opacity-0 group-hover:opacity-100 transition-opacity">
                                    <button 
                                        @click="toggleStatus(debt)" 
                                        class="px-3 py-1.5 text-xs font-bold rounded-lg border border-gray-200 dark:border-gray-700 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
                                    >
                                        {{ debt.status === 'active' ? 'Close debt' : 'Reopen' }}
                                    </button>
                                    <button 
                                        @click="openEditDebt(debt)" 
                                        class="px-3 py-1.5 text-xs font-bold rounded-lg border border-gray-200 dark:border-gray-700 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
                                    >
                                        Edit
                                    </button>
                                    <button 
                                        v-if="debt.status === 'active' && debt.paidAmount < debt.totalAmount"
                                        @click="openRepayment(debt)" 
                                        class="px-4 py-1.5 text-xs font-bold rounded-lg text-white shadow-sm transition-colors flex items-center gap-1"
                                        :class="debt.type === 'lend' ? 'bg-green-500 hover:bg-green-600' : 'bg-blue-500 hover:bg-blue-600'"
                                    >
                                        {{ debt.type === 'lend' ? 'Collect' : 'Repay' }}
                                    </button>
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        </div>

        <DebtModal 
            v-if="showDebtModal"
            :show="showDebtModal"
            :accounts="accounts"
            :editingDebt="editingDebt"
            :defaultType="currentTab"
            @close="showDebtModal = false"
            @save="handleSaveDebt"
        />

        <DebtRepaymentModal
            v-if="showRepayModal && editingDebt"
            :show="showRepayModal"
            :debt="editingDebt"
            :accounts="accounts"
            @close="showRepayModal = false"
            @save="handleRepayment"
        />
    </div>
</template>
