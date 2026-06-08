<script setup lang="ts">
import { ref, computed, watch } from 'vue';
import { Target, Plus, TrendingUp, AlertCircle, CheckCircle2, Calendar, ChevronDown, ChevronLeft, ChevronRight, Settings, X } from 'lucide-vue-next';
import type { Budget, BudgetItem, Transaction } from '../types';
import BudgetModal from './BudgetModal.vue';
import { formatCurrency } from '../currency';

const props = defineProps<{
    budgets: Budget[];
    transactions: Transaction[];
    allTransactions: Transaction[];
    currentMonth: string;
    expenseCategories: string[];
    selectedMonthNum: number;
    selectedYear: number;
    baseYear: number;
}>();

const emit = defineEmits<{
    (e: 'save-budgets', budgets: Budget[]): void;
    (e: 'change-month', month: number, year: number): void;
}>();

const MONTH_NAMES = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

const prevMonth = () => {
    let m = props.selectedMonthNum - 1;
    let y = props.selectedYear;
    if (m < 1) { m = 12; y--; }
    emit('change-month', m, y);
};

const nextMonth = () => {
    let m = props.selectedMonthNum + 1;
    let y = props.selectedYear;
    if (m > 12) { m = 1; y++; }
    emit('change-month', m, y);
};

const showItemModal = ref(false);
const editingItem = ref<BudgetItem | null>(null);
const selectedBudgetId = ref<string>('');
const showNewBudgetForm = ref(false);
const showEditBudgetForm = ref(false);

// New budget form state
const newBudgetName = ref('');
const newBudgetType = ref<'monthly' | 'custom'>('monthly');
const newBudgetStartDate = ref('');
const newBudgetEndDate = ref('');

// Auto-select first budget
watch(() => props.budgets, (newBudgets) => {
    if (newBudgets.length > 0 && !newBudgets.some(b => b.id === selectedBudgetId.value)) {
        selectedBudgetId.value = newBudgets[0].id;
    }
}, { immediate: true });

const selectedBudget = computed(() => props.budgets.find(b => b.id === selectedBudgetId.value) || null);
const isCustom = computed(() => selectedBudget.value?.type === 'custom');

// --- Item stats for selected budget ---

const getEffectiveAmount = (item: BudgetItem): number => {
    if (!isCustom.value && item.monthlyOverrides && item.monthlyOverrides[props.currentMonth]) {
        return item.monthlyOverrides[props.currentMonth];
    }
    return item.amount;
};

const getSpent = (item: BudgetItem): number => {
    const categories = item.categories;
    
    if (isCustom.value && selectedBudget.value?.startDate && selectedBudget.value?.endDate) {
        const start = new Date(selectedBudget.value.startDate).getTime();
        const end = new Date(selectedBudget.value.endDate + 'T23:59:59').getTime();
        return props.allTransactions
            .filter(tx => tx.type === 'expense' && categories.includes(tx.category))
            .filter(tx => {
                const t = new Date(tx.date).getTime();
                return t >= start && t <= end;
            })
            .reduce((sum, tx) => sum + tx.amount, 0);
    }
    
    return props.transactions
        .filter(tx => tx.type === 'expense' && categories.includes(tx.category))
        .reduce((sum, tx) => sum + tx.amount, 0);
};

const itemStats = computed(() => {
    if (!selectedBudget.value) return [];
    return selectedBudget.value.items.map(item => {
        const effectiveAmount = getEffectiveAmount(item);
        const spent = getSpent(item);
        const realPercent = effectiveAmount > 0 ? (spent / effectiveAmount) * 100 : 0;
        const isOverridden = !isCustom.value && item.monthlyOverrides && item.monthlyOverrides[props.currentMonth] !== undefined;

        let status: 'safe' | 'warning' | 'danger' = 'safe';
        if (realPercent >= 100) status = 'danger';
        else if (realPercent >= 80) status = 'warning';

        return {
            ...item,
            effectiveAmount,
            spent,
            percent: Math.min(Math.round(realPercent), 100),
            realPercent,
            status,
            remaining: Math.max(0, effectiveAmount - spent),
            isOverridden,
        };
    }).sort((a, b) => b.percent - a.percent);
});

// Overall stats for selected budget
const totalBudget = computed(() => itemStats.value.reduce((acc, b) => acc + b.effectiveAmount, 0));
const totalSpent = computed(() => itemStats.value.reduce((acc, b) => acc + b.spent, 0));
const overallPercent = computed(() => totalBudget.value > 0 ? Math.min(Math.round((totalSpent.value / totalBudget.value) * 100), 100) : 0);

const existingBudgetCategories = computed(() => {
    if (!selectedBudget.value) return [];
    return selectedBudget.value.items.flatMap(item => item.categories);
});

const formatDateRange = (start?: string, end?: string) => {
    if (!start || !end) return '';
    const fmt = (d: Date) => `${d.getDate().toString().padStart(2, '0')}/${(d.getMonth() + 1).toString().padStart(2, '0')}/${d.getFullYear()}`;
    return `${fmt(new Date(start))} → ${fmt(new Date(end))}`;
};

// --- Actions ---

const openAddItem = () => {
    editingItem.value = null;
    showItemModal.value = true;
};

const openEditItem = (item: BudgetItem) => {
    editingItem.value = item;
    showItemModal.value = true;
};

const handleSaveItem = (item: BudgetItem) => {
    if (!selectedBudget.value) return;
    const newBudgets = props.budgets.map(b => {
        if (b.id !== selectedBudget.value!.id) return b;
        const newItems = [...b.items];
        const idx = newItems.findIndex(i => i.id === item.id);
        if (idx >= 0) {
            newItems[idx] = item;
        } else {
            newItems.push(item);
        }
        return { ...b, items: newItems };
    });
    emit('save-budgets', newBudgets);
    showItemModal.value = false;
};

const handleDeleteItem = (id: string) => {
    if (!selectedBudget.value) return;
    const newBudgets = props.budgets.map(b => {
        if (b.id !== selectedBudget.value!.id) return b;
        return { ...b, items: b.items.filter(i => i.id !== id) };
    });
    emit('save-budgets', newBudgets);
    showItemModal.value = false;
};

// --- Budget container actions ---

const createBudget = () => {
    if (!newBudgetName.value.trim()) return;
    const newBudget: Budget = {
        id: `budget-${Date.now()}-${Math.floor(Math.random()*1000)}`,
        name: newBudgetName.value.trim(),
        type: newBudgetType.value,
        items: [],
    };
    if (newBudgetType.value === 'custom') {
        newBudget.startDate = newBudgetStartDate.value;
        newBudget.endDate = newBudgetEndDate.value;
    }
    const newBudgets = [...props.budgets, newBudget];
    emit('save-budgets', newBudgets);
    selectedBudgetId.value = newBudget.id;
    resetNewBudgetForm();
};

const updateBudget = () => {
    if (!selectedBudget.value || !newBudgetName.value.trim()) return;
    const newBudgets = props.budgets.map(b => {
        if (b.id !== selectedBudget.value!.id) return b;
        const updated = { ...b, name: newBudgetName.value.trim(), type: newBudgetType.value as 'monthly' | 'custom' };
        if (newBudgetType.value === 'custom') {
            updated.startDate = newBudgetStartDate.value;
            updated.endDate = newBudgetEndDate.value;
        } else {
            delete updated.startDate;
            delete updated.endDate;
        }
        return updated;
    });
    emit('save-budgets', newBudgets);
    showEditBudgetForm.value = false;
};

const deleteBudget = () => {
    if (!selectedBudget.value) return;
    const newBudgets = props.budgets.filter(b => b.id !== selectedBudget.value!.id);
    emit('save-budgets', newBudgets);
    showEditBudgetForm.value = false;
};

const openEditBudgetForm = () => {
    if (!selectedBudget.value) return;
    newBudgetName.value = selectedBudget.value.name;
    newBudgetType.value = selectedBudget.value.type || 'monthly';
    newBudgetStartDate.value = selectedBudget.value.startDate || '';
    newBudgetEndDate.value = selectedBudget.value.endDate || '';
    showEditBudgetForm.value = true;
};

const resetNewBudgetForm = () => {
    showNewBudgetForm.value = false;
    newBudgetName.value = '';
    newBudgetType.value = 'monthly';
    newBudgetStartDate.value = '';
    newBudgetEndDate.value = '';
};

</script>

<template>
  <div class="h-full flex flex-col gap-6 overflow-y-auto hidden-scrollbar pb-10 pr-2">
    
    <!-- Header: Budget Selector + Month Picker + Actions -->
    <div class="flex items-center justify-between shrink-0 flex-wrap gap-3">
        <div class="flex items-center gap-3">
            <div class="p-2.5 bg-blue-50 dark:bg-blue-900/20 text-blue-500 rounded-xl">
                <Target class="w-5 h-5" />
            </div>
            
            <!-- Budget Dropdown -->
            <div v-if="budgets.length > 0" class="flex items-center gap-2">
                <div class="relative">
                    <select v-model="selectedBudgetId" class="appearance-none bg-transparent text-lg font-bold text-text dark:text-text-dark pr-7 pl-1 py-1 focus:outline-none cursor-pointer hover:text-blue-500 transition-colors">
                        <option v-for="b in budgets" :key="b.id" :value="b.id">{{ b.name }}</option>
                    </select>
                    <ChevronDown class="w-4 h-4 text-gray-400 absolute right-0 top-1/2 -translate-y-1/2 pointer-events-none" />
                </div>
                <button @click="openEditBudgetForm" class="p-1.5 text-gray-400 hover:text-blue-500 hover:bg-blue-50 dark:hover:bg-blue-900/20 rounded-lg transition-colors" title="Edit budget">
                    <Settings class="w-4 h-4" />
                </button>
            </div>
            <h2 v-else class="text-lg font-bold text-text dark:text-text-dark">Budgets</h2>
        </div>

        <div class="flex items-center gap-3">
            <!-- Month Picker (for monthly budgets) -->
            <div v-if="selectedBudget && (selectedBudget.type || 'monthly') === 'monthly'" class="flex items-center gap-1 bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-xl px-1 py-1 shadow-sm">
                <button @click="prevMonth" class="p-1.5 text-gray-400 hover:text-text dark:hover:text-text-dark hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg transition-colors">
                    <ChevronLeft class="w-4 h-4" />
                </button>
                <span class="text-sm font-bold text-text dark:text-text-dark px-2 min-w-[100px] text-center">
                    {{ MONTH_NAMES[selectedMonthNum - 1] }} {{ selectedYear }}
                </span>
                <button @click="nextMonth" class="p-1.5 text-gray-400 hover:text-text dark:hover:text-text-dark hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg transition-colors">
                    <ChevronRight class="w-4 h-4" />
                </button>
            </div>

            <button @click="showNewBudgetForm = true" class="px-3 py-2 text-sm font-medium text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-xl transition-colors border border-border dark:border-border-dark">
                New Budget
            </button>
            <button v-if="selectedBudget" @click="openAddItem" class="flex items-center gap-2 px-4 py-2 bg-blue-500 text-white rounded-xl text-sm font-medium hover:bg-blue-600 transition-colors shadow-sm">
                <Plus class="w-4 h-4" />
                Add Item
            </button>
        </div>
    </div>

    <!-- New Budget Form (inline) -->
    <div v-if="showNewBudgetForm" class="bg-surface dark:bg-surface-dark border border-blue-200 dark:border-blue-800 rounded-2xl p-5 shadow-sm shrink-0 space-y-4">
        <div class="flex items-center justify-between">
            <h3 class="font-bold text-sm text-text dark:text-text-dark">Create New Budget</h3>
            <button @click="resetNewBudgetForm" class="p-1 text-gray-400 hover:text-gray-600 rounded-lg"><X class="w-4 h-4" /></button>
        </div>
        <div class="flex p-1 bg-gray-100 dark:bg-gray-800 rounded-xl">
            <button @click="newBudgetType = 'monthly'" :class="['flex-1 py-1.5 text-sm font-medium rounded-lg transition-colors', newBudgetType === 'monthly' ? 'bg-white dark:bg-gray-700 text-blue-500 shadow-sm' : 'text-gray-500']">Monthly</button>
            <button @click="newBudgetType = 'custom'" :class="['flex-1 py-1.5 text-sm font-medium rounded-lg transition-colors', newBudgetType === 'custom' ? 'bg-white dark:bg-gray-700 text-purple-500 shadow-sm' : 'text-gray-500']">Custom Period</button>
        </div>
        <input type="text" v-model="newBudgetName" class="w-full bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl px-3 py-2.5 text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500" :placeholder="newBudgetType === 'monthly' ? $t('finance.monthly_budget_ph') : $t('finance.business_budget_ph')" />
        <div v-if="newBudgetType === 'custom'" class="flex items-center gap-2">
            <input type="date" v-model="newBudgetStartDate" class="flex-1 bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl px-3 py-2.5 text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500" />
            <span class="text-gray-400 shrink-0">→</span>
            <input type="date" v-model="newBudgetEndDate" :min="newBudgetStartDate" class="flex-1 bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl px-3 py-2.5 text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500" />
        </div>
        <div class="flex justify-end gap-2">
            <button @click="resetNewBudgetForm" class="px-4 py-2 text-sm font-medium text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-xl transition-colors">{{ $t('finance.cancel') }}</button>
            <button @click="createBudget" :disabled="!newBudgetName.trim()" class="px-4 py-2 text-sm font-medium text-white bg-blue-500 hover:bg-blue-600 disabled:bg-blue-500/50 rounded-xl transition-colors shadow-sm">{{ $t('finance.create') }}</button>
        </div>
    </div>

    <!-- Edit Budget Form (inline) -->
    <div v-if="showEditBudgetForm && selectedBudget" class="bg-surface dark:bg-surface-dark border border-blue-200 dark:border-blue-800 rounded-2xl p-5 shadow-sm shrink-0 space-y-4">
        <div class="flex items-center justify-between">
            <h3 class="font-bold text-sm text-text dark:text-text-dark">Edit Budget</h3>
            <button @click="showEditBudgetForm = false" class="p-1 text-gray-400 hover:text-gray-600 rounded-lg"><X class="w-4 h-4" /></button>
        </div>
        <div class="flex p-1 bg-gray-100 dark:bg-gray-800 rounded-xl">
            <button @click="newBudgetType = 'monthly'" :class="['flex-1 py-1.5 text-sm font-medium rounded-lg transition-colors', newBudgetType === 'monthly' ? 'bg-white dark:bg-gray-700 text-blue-500 shadow-sm' : 'text-gray-500']">Monthly</button>
            <button @click="newBudgetType = 'custom'" :class="['flex-1 py-1.5 text-sm font-medium rounded-lg transition-colors', newBudgetType === 'custom' ? 'bg-white dark:bg-gray-700 text-purple-500 shadow-sm' : 'text-gray-500']">Custom Period</button>
        </div>
        <input type="text" v-model="newBudgetName" class="w-full bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl px-3 py-2.5 text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500" />
        <div v-if="newBudgetType === 'custom'" class="flex items-center gap-2">
            <input type="date" v-model="newBudgetStartDate" class="flex-1 bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl px-3 py-2.5 text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500" />
            <span class="text-gray-400 shrink-0">→</span>
            <input type="date" v-model="newBudgetEndDate" :min="newBudgetStartDate" class="flex-1 bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl px-3 py-2.5 text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500" />
        </div>
        <div class="flex justify-between">
            <button @click="deleteBudget" class="px-4 py-2 text-sm font-medium text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 rounded-xl transition-colors">Delete Budget</button>
            <div class="flex gap-2">
                <button @click="showEditBudgetForm = false" class="px-4 py-2 text-sm font-medium text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-xl transition-colors">{{ $t('finance.cancel') }}</button>
                <button @click="updateBudget" :disabled="!newBudgetName.trim()" class="px-4 py-2 text-sm font-medium text-white bg-blue-500 hover:bg-blue-600 disabled:bg-blue-500/50 rounded-xl transition-colors shadow-sm">{{ $t('finance.save') }}</button>
            </div>
        </div>
    </div>

    <!-- Empty State: No budgets at all -->
    <div v-if="budgets.length === 0 && !showNewBudgetForm" class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl p-10 shadow-sm text-center flex-1 flex flex-col items-center justify-center">
        <div class="w-14 h-14 bg-gray-100 dark:bg-gray-800 rounded-full flex items-center justify-center mb-4">
            <Target class="w-7 h-7 text-gray-400" />
        </div>
        <p class="text-gray-500 dark:text-gray-400 font-medium">No budgets yet</p>
        <p class="text-sm text-gray-400 dark:text-gray-500 mt-1 mb-4">{{ $t('finance.create_to_track') }}</p>
        <button @click="showNewBudgetForm = true" class="flex items-center gap-2 px-4 py-2 bg-blue-500 text-white rounded-xl text-sm font-medium hover:bg-blue-600 transition-colors shadow-sm">
            <Plus class="w-4 h-4" /> Create Budget
        </button>
    </div>

    <!-- Selected Budget Content -->
    <template v-if="selectedBudget">

        <!-- Summary Card -->
        <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl p-6 shadow-sm flex flex-col gap-5 shrink-0">
            <div class="flex items-center justify-between">
                <div>
                    <div class="flex items-center gap-2 mb-1">
                        <span :class="['px-2 py-0.5 rounded-lg text-[10px] font-bold uppercase tracking-wider',
                            isCustom ? 'bg-purple-100 dark:bg-purple-900/20 text-purple-600 dark:text-purple-400' : 'bg-blue-100 dark:bg-blue-900/20 text-blue-600 dark:text-blue-400']">
                            {{ isCustom ? 'Custom Period' : 'Monthly' }}
                        </span>
                    </div>
                    <div v-if="isCustom && selectedBudget.startDate && selectedBudget.endDate" class="flex items-center gap-1.5 text-xs text-purple-500 dark:text-purple-400 font-medium mt-1">
                        <Calendar class="w-3.5 h-3.5" />
                        {{ formatDateRange(selectedBudget.startDate, selectedBudget.endDate) }}
                    </div>
                </div>
                <div class="text-right">
                    <div class="text-sm font-bold" :class="overallPercent >= 100 ? 'text-red-500' : overallPercent >= 80 ? 'text-orange-500' : 'text-green-500'">
                        {{ overallPercent }}%
                    </div>
                </div>
            </div>

            <div v-if="itemStats.length > 0" class="flex flex-col gap-3">
                <div class="flex justify-between items-end">
                    <div>
                        <div class="text-sm font-medium text-gray-500 dark:text-gray-400 mb-1">{{ $t('finance.spent') }}</div>
                        <div class="text-2xl font-bold tracking-tight text-text dark:text-text-dark">
                            {{ formatCurrency(totalSpent) }}
                            <span class="text-lg font-medium text-gray-400 dark:text-gray-500">/ {{ formatCurrency(totalBudget) }}</span>
                        </div>
                    </div>
                </div>
                <div class="h-3 w-full bg-gray-100 dark:bg-gray-800 rounded-full overflow-hidden">
                    <div 
                        class="h-full transition-all duration-500 ease-out"
                        :class="overallPercent >= 100 ? 'bg-red-500' : overallPercent >= 80 ? 'bg-orange-500' : isCustom ? 'bg-purple-500' : 'bg-green-500'"
                        :style="{ width: `${overallPercent}%` }"
                    ></div>
                </div>
            </div>
            <div v-else class="text-center py-4 text-gray-500 dark:text-gray-400 text-sm">
                No items in this budget yet. Add an item to start tracking!
            </div>
        </div>

        <!-- Budget Items Grid (Cards) -->
        <div v-if="itemStats.length > 0" class="grid grid-cols-1 lg:grid-cols-2 gap-4">
            <div 
                v-for="b in itemStats" 
                :key="b.id"
                @click="openEditItem(b)"
                class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl p-5 hover:border-blue-300 dark:hover:border-blue-700/50 transition-colors cursor-pointer group shadow-sm flex flex-col gap-4 relative overflow-hidden"
            >
                <div class="absolute right-0 top-0 opacity-5 pointer-events-none transition-transform group-hover:scale-110">
                    <Target class="w-24 h-24 -mt-4 -mr-4" />
                </div>

                <div class="flex items-start justify-between gap-4">
                    <div class="flex flex-col gap-1">
                        <div class="font-bold text-lg text-text dark:text-text-dark flex items-center gap-2">
                            {{ b.name }}
                            <CheckCircle2 v-if="b.status === 'safe'" class="w-4 h-4 text-green-500 shrink-0" />
                            <AlertCircle v-else-if="b.status === 'warning'" class="w-4 h-4 text-orange-500 shrink-0" />
                            <TrendingUp v-else-if="b.status === 'danger'" class="w-4 h-4 text-red-500 shrink-0" />
                        </div>
                        <div class="flex flex-wrap gap-1.5 mt-0.5">
                            <span 
                                v-for="cat in b.categories" 
                                :key="cat"
                                class="px-2 py-0.5 rounded-md text-[10px] font-medium bg-gray-100 dark:bg-gray-800/50 text-gray-600 dark:text-gray-300 border border-gray-200 dark:border-gray-700/50"
                            >
                                {{ cat }}
                            </span>
                            <span v-if="b.isOverridden" class="px-2 py-0.5 rounded-md text-[10px] font-medium bg-amber-50 dark:bg-amber-900/20 text-amber-600 dark:text-amber-400 border border-amber-200 dark:border-amber-700/50">
                                Override
                            </span>
                        </div>
                    </div>
                    <div class="text-sm font-medium pt-1" :class="b.status === 'danger' ? 'text-red-500' : b.status === 'warning' ? 'text-orange-500' : 'text-green-500'">
                        {{ b.realPercent > 100 ? b.realPercent.toFixed(1) : Math.round(b.realPercent) }}%
                    </div>
                </div>

                <div class="flex flex-col gap-2 relative z-10">
                    <div class="h-2.5 w-full bg-gray-100 dark:bg-gray-800 rounded-full overflow-hidden">
                        <div 
                            class="h-full transition-all duration-500 ease-out"
                            :class="b.status === 'danger' ? 'bg-red-500' : b.status === 'warning' ? 'bg-orange-500' : 'bg-green-500'"
                            :style="{ width: `${Math.min(b.realPercent, 100)}%` }"
                        ></div>
                    </div>

                    <div class="flex justify-between items-center mt-1">
                        <div class="text-xs text-gray-500 dark:text-gray-400">
                            Spent: <span class="font-bold text-text dark:text-text-dark">{{ formatCurrency(b.spent) }}</span>
                            <span class="mx-1">/</span>
                            {{ formatCurrency(b.effectiveAmount) }}
                        </div>
                        <div class="text-xs font-medium" :class="b.status === 'danger' ? 'text-red-500' : 'text-gray-500 dark:text-gray-400'">
                            {{ b.status === 'danger' ? `Overspent by ${formatCurrency(b.spent - b.effectiveAmount)}` : `Remaining ${formatCurrency(b.remaining)}` }}
                        </div>
                    </div>
                </div>
            </div>
        </div>
    </template>

    <BudgetModal 
        :show="showItemModal" 
        :item="editingItem"
        :expense-categories="expenseCategories"
        :existing-budget-categories="existingBudgetCategories"
        :current-month="currentMonth"
        @close="showItemModal = false"
        @save="handleSaveItem"
        @delete="handleDeleteItem"
    />
  </div>
</template>
