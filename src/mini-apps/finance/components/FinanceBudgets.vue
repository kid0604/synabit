<script setup lang="ts">
import { ref, computed } from 'vue';
import { Target, Plus, TrendingUp, AlertCircle, CheckCircle2 } from 'lucide-vue-next';
import type { Budget, Transaction } from '../types';
import BudgetModal from './BudgetModal.vue';

const props = defineProps<{
    budgets: Budget[];
    transactions: Transaction[];
    expenseCategories: string[];
}>();

const emit = defineEmits<{
    (e: 'save-budgets', budgets: Budget[]): void;
}>();

const showBudgetModal = ref(false);
const editingBudget = ref<Budget | null>(null);

// Calculate spent amounts for each category based on current month's transactions
const spentByCategory = computed(() => {
    const spent: Record<string, number> = {};
    props.transactions.forEach(tx => {
        if (tx.type === 'expense') {
            if (!spent[tx.category]) spent[tx.category] = 0;
            spent[tx.category] += tx.amount;
        }
    });
    return spent;
});

// Enriched budgets with spent amounts and percentages
const budgetStats = computed(() => {
    return props.budgets.map(b => {
        const spent = spentByCategory.value[b.categoryId] || 0;
        const percent = b.amount > 0 ? Math.min(Math.round((spent / b.amount) * 100), 100) : 0;
        const realPercent = b.amount > 0 ? (spent / b.amount) * 100 : 0;
        
        let status: 'safe' | 'warning' | 'danger' = 'safe';
        if (realPercent >= 100) status = 'danger';
        else if (realPercent >= 80) status = 'warning';

        return {
            ...b,
            spent,
            percent,
            realPercent,
            status,
            remaining: Math.max(0, b.amount - spent)
        };
    }).sort((a, b) => b.percent - a.percent); // Show ones closest to limit first
});

// Overall stats
const totalBudget = computed(() => props.budgets.reduce((acc, b) => acc + b.amount, 0));
const totalSpentOnBudgets = computed(() => budgetStats.value.reduce((acc, b) => acc + b.spent, 0));
const overallPercent = computed(() => totalBudget.value > 0 ? Math.min(Math.round((totalSpentOnBudgets.value / totalBudget.value) * 100), 100) : 0);

const formatCurrency = (val: number) => {
    return '$' + val.toLocaleString('en-US');
};

const openAddBudget = () => {
    editingBudget.value = null;
    showBudgetModal.value = true;
};

const openEditBudget = (budget: Budget) => {
    editingBudget.value = budget;
    showBudgetModal.value = true;
};

const handleSaveBudget = (budget: Budget) => {
    const newBudgets = [...props.budgets];
    const idx = newBudgets.findIndex(b => b.categoryId === budget.categoryId);
    
    if (idx >= 0) {
        newBudgets[idx] = budget;
    } else {
        newBudgets.push(budget);
    }
    
    emit('save-budgets', newBudgets);
    showBudgetModal.value = false;
};

const handleDeleteBudget = (categoryId: string) => {
    const newBudgets = props.budgets.filter(b => b.categoryId !== categoryId);
    emit('save-budgets', newBudgets);
    showBudgetModal.value = false;
};

const existingBudgetCategories = computed(() => props.budgets.map(b => b.categoryId));

</script>

<template>
  <div class="h-full flex flex-col gap-6 overflow-y-auto hidden-scrollbar pb-10 pr-2">
    
    <!-- Top Summary Card -->
    <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl p-6 shadow-sm flex flex-col gap-5 shrink-0">
        <div class="flex items-center justify-between">
            <div class="flex items-center gap-3">
                <div class="p-2.5 bg-blue-50 dark:bg-blue-900/20 text-blue-500 rounded-xl">
                    <Target class="w-6 h-6" />
                </div>
                <div>
                    <h2 class="text-lg font-bold text-text dark:text-text-dark">Total Budgets</h2>
                    <p class="text-sm text-gray-500 dark:text-gray-400">Tracked categories</p>
                </div>
            </div>
            
            <button @click="openAddBudget" class="flex items-center gap-2 px-4 py-2 bg-blue-500 text-white rounded-xl text-sm font-medium hover:bg-blue-600 transition-colors shadow-sm">
                <Plus class="w-4 h-4" />
                Add budget
            </button>
        </div>

        <div v-if="budgets.length > 0" class="flex flex-col gap-3 mt-2">
            <div class="flex justify-between items-end">
                <div>
                    <div class="text-sm font-medium text-gray-500 dark:text-gray-400 mb-1">Spent</div>
                    <div class="text-2xl font-bold tracking-tight text-text dark:text-text-dark">
                        {{ formatCurrency(totalSpentOnBudgets) }}
                        <span class="text-lg font-medium text-gray-400 dark:text-gray-500">/ {{ formatCurrency(totalBudget) }}</span>
                    </div>
                </div>
                <div class="text-sm font-bold" :class="overallPercent >= 100 ? 'text-red-500' : overallPercent >= 80 ? 'text-orange-500' : 'text-green-500'">
                    {{ overallPercent }}%
                </div>
            </div>

            <!-- Global Progress Bar -->
            <div class="h-3 w-full bg-gray-100 dark:bg-gray-800 rounded-full overflow-hidden">
                <div 
                    class="h-full transition-all duration-500 ease-out"
                    :class="overallPercent >= 100 ? 'bg-red-500' : overallPercent >= 80 ? 'bg-orange-500' : 'bg-green-500'"
                    :style="{ width: `${overallPercent}%` }"
                ></div>
            </div>
        </div>
        <div v-else class="text-center py-6 text-gray-500 dark:text-gray-400">
            You haven't set any budgets. Add a budget to manage your expenses effectively!
        </div>
    </div>

    <!-- Budgets List -->
    <div v-if="budgetStats.length > 0" class="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <div 
            v-for="b in budgetStats" 
            :key="b.categoryId"
            @click="openEditBudget(b)"
            class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl p-5 hover:border-blue-300 dark:hover:border-blue-700/50 transition-colors cursor-pointer group shadow-sm flex flex-col gap-4 relative overflow-hidden"
        >
            <div class="absolute right-0 top-0 opacity-5 pointer-events-none transition-transform group-hover:scale-110">
                <Target class="w-24 h-24 -mt-4 -mr-4" />
            </div>

            <div class="flex items-center justify-between">
                <div class="font-bold text-lg text-text dark:text-text-dark flex items-center gap-2">
                    {{ b.categoryId }}
                    <CheckCircle2 v-if="b.status === 'safe'" class="w-4 h-4 text-green-500" />
                    <AlertCircle v-else-if="b.status === 'warning'" class="w-4 h-4 text-orange-500" />
                    <TrendingUp v-else-if="b.status === 'danger'" class="w-4 h-4 text-red-500" />
                </div>
                <div class="text-sm font-medium" :class="b.status === 'danger' ? 'text-red-500' : b.status === 'warning' ? 'text-orange-500' : 'text-green-500'">
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
                        {{ formatCurrency(b.amount) }}
                    </div>
                    <div class="text-xs font-medium" :class="b.status === 'danger' ? 'text-red-500' : 'text-gray-500 dark:text-gray-400'">
                        {{ b.status === 'danger' ? `Overspent by ${formatCurrency(b.spent - b.amount)}` : `Remaining ${formatCurrency(b.remaining)}` }}
                    </div>
                </div>
            </div>
        </div>
    </div>

    <BudgetModal 
        :show="showBudgetModal" 
        :budget="editingBudget"
        :expense-categories="expenseCategories"
        :existing-budget-categories="existingBudgetCategories"
        @close="showBudgetModal = false"
        @save="handleSaveBudget"
        @delete="handleDeleteBudget"
    />
  </div>
</template>
