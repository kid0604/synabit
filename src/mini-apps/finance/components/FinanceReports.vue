<script setup lang="ts">
import { ref, computed } from 'vue';
import type { Transaction, FinanceAccount } from '../types';
import FinanceChart from './FinanceChart.vue';
import { Filter, Calendar, Wallet } from 'lucide-vue-next';
import { formatCurrency } from '../currency';

const props = defineProps<{
    months: { id: string, label: string, date: Date, node: any }[];
    globalNetWorth: number;
    accounts: FinanceAccount[];
    accountBalances: { id: string, name: string, balance: number }[];
}>();

// formatCurrency is imported from ../currency

const formatShort = (val: number) => {
    const abs = Math.abs(val);
    if (abs >= 1000000) return (val / 1000000).toFixed(1) + 'M';
    if (abs >= 1000) return (val / 1000).toFixed(0) + 'K';
    return val.toString();
};

import { SYSTEM_INCOME_CATEGORIES, SYSTEM_EXPENSE_CATEGORIES } from '../types';

// --- Filters State ---
const timeRange = ref<'all' | 'this_month' | 'last_3' | 'last_6' | 'this_year' | 'custom'>('last_6');
const customStartDate = ref<string>('');
const customEndDate = ref<string>('');
const selectedAccount = ref<string>('all');
const excludeDebts = ref<boolean>(true);
const pieChartType = ref<'expense' | 'income'>('expense');

// --- Helper: Target Months ---
const targetMonths = computed(() => {
    let start = new Date(0);
    let end = new Date(8640000000000000);
    const now = new Date();
    
    if (timeRange.value === 'this_month') {
        start = new Date(now.getFullYear(), now.getMonth(), 1);
        end = new Date(now.getFullYear(), now.getMonth() + 1, 0, 23, 59, 59);
    } else if (timeRange.value === 'last_3') {
        start = new Date(now.getFullYear(), now.getMonth() - 2, 1);
        end = new Date(now.getFullYear(), now.getMonth() + 1, 0, 23, 59, 59);
    } else if (timeRange.value === 'last_6') {
        start = new Date(now.getFullYear(), now.getMonth() - 5, 1);
        end = new Date(now.getFullYear(), now.getMonth() + 1, 0, 23, 59, 59);
    } else if (timeRange.value === 'this_year') {
        start = new Date(now.getFullYear(), 0, 1);
        end = new Date(now.getFullYear(), 11, 31, 23, 59, 59);
    } else if (timeRange.value === 'custom') {
        if (customStartDate.value) start = new Date(customStartDate.value);
        if (customEndDate.value) {
            end = new Date(customEndDate.value);
            end.setHours(23, 59, 59, 999);
        }
    }
    
    return props.months.filter(m => {
        const mStart = new Date(m.date.getFullYear(), m.date.getMonth(), 1);
        const mEnd = new Date(m.date.getFullYear(), m.date.getMonth() + 1, 0, 23, 59, 59);
        return mStart <= end && mEnd >= start;
    }).sort((a, b) => a.date.getTime() - b.date.getTime());
});

// --- Filtered Transactions (for CashFlow and PieChart) ---
const filteredTransactions = computed(() => {
    let start = new Date(0);
    let end = new Date(8640000000000000);
    const now = new Date();
    
    if (timeRange.value === 'this_month') {
        start = new Date(now.getFullYear(), now.getMonth(), 1);
        end = new Date(now.getFullYear(), now.getMonth() + 1, 0, 23, 59, 59);
    } else if (timeRange.value === 'last_3') {
        start = new Date(now.getFullYear(), now.getMonth() - 2, 1);
        end = new Date(now.getFullYear(), now.getMonth() + 1, 0, 23, 59, 59);
    } else if (timeRange.value === 'last_6') {
        start = new Date(now.getFullYear(), now.getMonth() - 5, 1);
        end = new Date(now.getFullYear(), now.getMonth() + 1, 0, 23, 59, 59);
    } else if (timeRange.value === 'this_year') {
        start = new Date(now.getFullYear(), 0, 1);
        end = new Date(now.getFullYear(), 11, 31, 23, 59, 59);
    } else if (timeRange.value === 'custom') {
        if (customStartDate.value) start = new Date(customStartDate.value);
        if (customEndDate.value) {
            end = new Date(customEndDate.value);
            end.setHours(23, 59, 59, 999);
        }
    }
    
    const txs: (Transaction & { monthLabel: string })[] = [];
    props.months.forEach(m => {
        const t: Transaction[] = m.node.properties?.transactions || [];
        t.forEach(tx => txs.push({ ...tx, monthLabel: m.label }));
    });
    
    return txs.filter(tx => {
        const txDate = new Date(tx.date);
        if (txDate < start || txDate > end) return false;
        if (selectedAccount.value !== 'all') {
            if (tx.accountId !== selectedAccount.value && tx.toAccountId !== selectedAccount.value) return false;
        }
        if (excludeDebts.value && (SYSTEM_INCOME_CATEGORIES.includes(tx.category) || SYSTEM_EXPENSE_CATEGORIES.includes(tx.category))) {
            return false;
        }
        return true;
    });
});

// --- Cash Flow Data ---
const cashFlowData = computed(() => {
    const groups: Record<string, { label: string, fullLabel: string, income: number, expense: number, date: Date }> = {};
    
    targetMonths.value.forEach(m => {
        groups[m.label] = {
            label: m.date.getMonth() + 1 + '/' + m.date.getFullYear().toString().slice(-2),
            fullLabel: m.label,
            income: 0,
            expense: 0,
            date: m.date
        };
    });
    
    filteredTransactions.value.forEach(tx => {
        const key = tx.monthLabel;
        if (!groups[key]) return; // Fallback
        
        if (tx.type === 'income') {
            groups[key].income += tx.amount;
        } else if (tx.type === 'expense') {
            groups[key].expense += tx.amount;
        } else if (tx.type === 'transfer' && selectedAccount.value !== 'all') {
            if (tx.accountId === selectedAccount.value) groups[key].expense += tx.amount;
            if (tx.toAccountId === selectedAccount.value) groups[key].income += tx.amount;
        }
    });
    
    return Object.values(groups)
        .map(g => ({ ...g, net: g.income - g.expense }))
        .sort((a, b) => a.date.getTime() - b.date.getTime());
});

const maxCashFlow = computed(() => {
    if (cashFlowData.value.length === 0) return 1;
    return Math.max(...cashFlowData.value.map(d => Math.max(d.income, d.expense)), 1000);
});

// --- Net Worth Trend Data ---
const netWorthTrendData = computed(() => {
    let runningNetWorth = 0;
    if (selectedAccount.value === 'all') {
        runningNetWorth = props.globalNetWorth;
    } else {
        const acc = props.accountBalances.find(a => a.id === selectedAccount.value);
        if (acc) runningNetWorth = acc.balance;
    }
    
    const allMonthsDesc = [...props.months].sort((a, b) => b.date.getTime() - a.date.getTime());
    const result = [];
    
    for (const m of allMonthsDesc) {
        let netFlow = 0;
        const txs: Transaction[] = m.node.properties?.transactions || [];
        txs.forEach(t => {
            if (selectedAccount.value === 'all') {
                if (t.type === 'income') netFlow += t.amount;
                if (t.type === 'expense') netFlow -= t.amount;
            } else {
                if (t.accountId === selectedAccount.value) {
                    if (t.type === 'income') netFlow += t.amount;
                    if (t.type === 'expense' || t.type === 'transfer') netFlow -= t.amount;
                }
                if (t.toAccountId === selectedAccount.value && t.type === 'transfer') {
                    netFlow += t.amount;
                }
            }
        });
        
        result.unshift({
            id: m.id,
            label: m.date.getMonth() + 1 + '/' + m.date.getFullYear().toString().slice(-2),
            fullLabel: m.label,
            value: runningNetWorth,
            date: m.date
        });
        
        runningNetWorth -= netFlow;
    }
    
    const targetIds = new Set(targetMonths.value.map(m => m.id));
    return result.filter(r => targetIds.has(r.id));
});

const maxNetWorth = computed(() => {
    if (netWorthTrendData.value.length === 0) return 1;
    return Math.max(...netWorthTrendData.value.map(d => d.value), 1000) * 1.1; 
});
const minNetWorth = computed(() => {
    if (netWorthTrendData.value.length === 0) return 0;
    const min = Math.min(...netWorthTrendData.value.map(d => d.value));
    return min > 0 ? 0 : min * 1.1;
});

const getLinePoints = computed(() => {
    const data = netWorthTrendData.value;
    if (data.length === 0) return '';
    if (data.length === 1) return `0,50 100,50`; 
    
    const range = Math.max(maxNetWorth.value - minNetWorth.value, 1);
    return data.map((d, i) => {
        const x = (i / (data.length - 1)) * 100;
        const y = 100 - ((d.value - minNetWorth.value) / range) * 100;
        return `${x},${y}`;
    }).join(' ');
});

const getAreaPoints = computed(() => {
    const line = getLinePoints.value;
    if (!line) return '';
    if (netWorthTrendData.value.length === 1) return `0,100 0,50 100,50 100,100`;
    return `0,100 ${line} 100,100`;
});

// --- Category Breakdown Data ---
const pieChartData = computed(() => {
    const data: Record<string, number> = {};
    filteredTransactions.value.forEach(t => {
        if (t.type === pieChartType.value) {
            data[t.category] = (data[t.category] || 0) + t.amount;
        }
    });
    
    return Object.keys(data)
        .map(label => ({ label, value: data[label] }))
        .sort((a, b) => b.value - a.value);
});

const pieChartTotal = computed(() => {
    return pieChartData.value.reduce((sum, item) => sum + item.value, 0);
});

</script>

<template>
    <div class="h-full overflow-y-auto hidden-scrollbar flex flex-col gap-6">
        
        <!-- Filter Control Panel -->
        <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl shadow-sm p-4 flex flex-wrap gap-4 items-center shrink-0">
            <div class="flex items-center gap-2 text-text dark:text-text-dark font-medium mr-2">
                <Filter class="w-5 h-5 text-blue-500" />
                Filters
            </div>
            
            <!-- Time Range -->
            <div class="flex items-center bg-gray-100/80 dark:bg-gray-800/80 hover:bg-gray-200 dark:hover:bg-gray-700 border border-transparent dark:border-gray-700 rounded-xl transition-colors focus-within:ring-2 focus-within:ring-blue-500 group pl-3 relative">
                <Calendar class="w-4 h-4 text-gray-500 dark:text-gray-400 group-hover:text-blue-500 transition-colors shrink-0" />
                <select v-model="timeRange" class="bg-transparent border-none py-1.5 pl-2 pr-8 text-sm font-medium focus:ring-0 cursor-pointer outline-none text-text dark:text-text-dark">
                    <option value="this_month">This month</option>
                    <option value="last_3">Last 3 months</option>
                    <option value="last_6">Last 6 months</option>
                    <option value="this_year">This year</option>
                    <option value="all">All time</option>
                    <option value="custom">Custom...</option>
                </select>
            </div>
            
            <div v-if="timeRange === 'custom'" class="flex items-center gap-2 bg-gray-100/80 dark:bg-gray-800/80 border border-transparent dark:border-gray-700 rounded-xl px-3 py-1.5 focus-within:ring-2 focus-within:ring-blue-500">
                <input v-model="customStartDate" type="date" class="bg-transparent border-none text-sm font-medium p-0 focus:ring-0 text-text dark:text-text-dark" />
                <span class="text-gray-400">-</span>
                <input v-model="customEndDate" type="date" class="bg-transparent border-none text-sm font-medium p-0 focus:ring-0 text-text dark:text-text-dark" />
            </div>

            <!-- Account Filter -->
            <div class="flex items-center bg-gray-100/80 dark:bg-gray-800/80 hover:bg-gray-200 dark:hover:bg-gray-700 border border-transparent dark:border-gray-700 rounded-xl transition-colors focus-within:ring-2 focus-within:ring-blue-500 group pl-3 relative">
                <Wallet class="w-4 h-4 text-gray-500 dark:text-gray-400 group-hover:text-blue-500 transition-colors shrink-0" />
                <select v-model="selectedAccount" class="bg-transparent border-none py-1.5 pl-2 pr-8 text-sm font-medium focus:ring-0 cursor-pointer outline-none text-text dark:text-text-dark max-w-[200px] truncate">
                    <option value="all">All accounts</option>
                    <option v-for="acc in accounts" :key="acc.id" :value="acc.id">{{ acc.name }}</option>
                </select>
            </div>
            
            <!-- Exclude Debts Toggle -->
            <label class="flex items-center gap-2 cursor-pointer group ml-2">
                <input type="checkbox" v-model="excludeDebts" class="w-4 h-4 text-blue-500 bg-gray-100 border-gray-300 rounded focus:ring-blue-500 dark:focus:ring-blue-600 dark:ring-offset-gray-800 focus:ring-2 dark:bg-gray-700 dark:border-gray-600 cursor-pointer" />
                <span class="text-sm font-medium text-gray-600 dark:text-gray-300 group-hover:text-text dark:group-hover:text-text-dark transition-colors">Exclude Debts</span>
            </label>
        </div>
        
        <!-- Cash Flow Bar Chart -->
        <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl shadow-sm flex flex-col overflow-hidden relative shrink-0">
            <div class="p-6 border-b border-border dark:border-border-dark flex justify-between items-center">
                <div>
                    <h3 class="font-bold text-lg text-text dark:text-text-dark">Cash Flow</h3>
                    <p class="text-sm text-gray-500 mt-1">Compare Income and Expense over time</p>
                </div>
                <!-- Legend -->
                <div class="flex gap-4">
                    <div class="flex items-center gap-2">
                        <div class="w-3 h-3 rounded-full bg-gradient-to-t from-green-500 to-green-400"></div>
                        <span class="text-xs font-medium text-gray-600 dark:text-gray-300">Total Income</span>
                    </div>
                    <div class="flex items-center gap-2">
                        <div class="w-3 h-3 rounded-full bg-gradient-to-t from-red-500 to-red-400"></div>
                        <span class="text-xs font-medium text-gray-600 dark:text-gray-300">Total Expense</span>
                    </div>
                </div>
            </div>
            
            <div class="p-6">
                <div v-if="cashFlowData.length === 0" class="h-[250px] flex items-center justify-center text-gray-400">
                    No data available
                </div>
                
                <div v-else class="h-[300px] w-full flex flex-col mt-4">
                    <!-- Chart Area -->
                    <div class="flex-1 relative flex items-end justify-around gap-2 sm:gap-6 z-10 w-full">
                        <!-- Background Grid -->
                        <div class="absolute inset-0 flex flex-col justify-between pointer-events-none z-0">
                            <div class="border-t border-dashed border-gray-200 dark:border-gray-800/50 w-full"></div>
                            <div class="border-t border-dashed border-gray-200 dark:border-gray-800/50 w-full"></div>
                            <div class="border-t border-dashed border-gray-200 dark:border-gray-800/50 w-full"></div>
                            <div class="border-t border-solid border-gray-200 dark:border-gray-800 w-full"></div>
                        </div>
                        
                        <!-- Bars -->
                        <div v-for="(data, idx) in cashFlowData" :key="idx" class="flex items-end justify-center w-full max-w-[80px] h-full relative z-10 group">
                            <!-- Tooltip -->
                            <div class="absolute -top-16 opacity-0 group-hover:opacity-100 transition-opacity duration-200 bg-gray-900/90 dark:bg-white text-white dark:text-gray-900 text-xs py-2 px-3 rounded-xl shadow-xl whitespace-nowrap z-20 pointer-events-none transform -translate-y-2 group-hover:translate-y-0 backdrop-blur-sm">
                                <p class="font-bold mb-1">{{ data.fullLabel }}</p>
                                <p class="text-green-400 dark:text-green-600">Income: {{ formatCurrency(data.income) }}</p>
                                <p class="text-red-400 dark:text-red-600">Expense: {{ formatCurrency(data.expense) }}</p>
                            </div>
                            
                            <div class="flex items-end justify-center gap-1 sm:gap-2 w-full h-full">
                                <div class="w-1/2 bg-gradient-to-t from-green-500 to-green-400 rounded-t-md hover:brightness-110 transition-all cursor-pointer shadow-sm relative group-hover:from-green-400 group-hover:to-green-300" :style="{ height: `${(data.income / maxCashFlow) * 100}%`, minHeight: data.income > 0 ? '4px' : '0' }"></div>
                                <div class="w-1/2 bg-gradient-to-t from-red-500 to-red-400 rounded-t-md hover:brightness-110 transition-all cursor-pointer shadow-sm relative group-hover:from-red-400 group-hover:to-red-300" :style="{ height: `${(data.expense / maxCashFlow) * 100}%`, minHeight: data.expense > 0 ? '4px' : '0' }"></div>
                            </div>
                        </div>
                    </div>
                    
                    <!-- X Axis Labels -->
                    <div class="h-[60px] w-full flex items-start justify-around gap-2 sm:gap-6 mt-2 z-10">
                        <div v-for="(data, idx) in cashFlowData" :key="`label-${idx}`" class="w-full max-w-[80px] flex flex-col items-center">
                            <p class="text-xs font-semibold text-gray-500 dark:text-gray-400 mt-2">{{ data.label }}</p>
                            <span :class="['mt-1 text-[10px] font-bold px-1.5 py-0.5 rounded-full', data.net >= 0 ? 'text-green-600 bg-green-50 dark:bg-green-900/20' : 'text-red-600 bg-red-50 dark:bg-red-900/20']">
                                {{ data.net > 0 ? '+' : ''}}{{ formatShort(data.net) }}
                            </span>
                        </div>
                    </div>
                </div>
            </div>
        </div>

        <!-- Net Worth Trend Area Chart -->
        <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl shadow-sm flex flex-col overflow-hidden relative shrink-0">
            <div class="p-6 border-b border-border dark:border-border-dark flex justify-between items-center bg-gradient-to-r from-blue-50/50 to-transparent dark:from-blue-900/10">
                <div>
                    <h3 class="font-bold text-lg text-text dark:text-text-dark">Net Worth Trend</h3>
                    <p class="text-sm text-gray-500 mt-1">Cumulative net worth trend</p>
                </div>
                <div class="text-right">
                    <p class="text-2xl font-bold text-blue-600 dark:text-blue-400">{{ formatCurrency(globalNetWorth) }}</p>
                    <p class="text-xs font-medium text-gray-500 uppercase tracking-wider mt-0.5">Current</p>
                </div>
            </div>
            
            <div class="p-6">
                <div v-if="netWorthTrendData.length === 0" class="h-[250px] flex items-center justify-center text-gray-400">
                    No data available
                </div>
                
                <div v-else class="h-[300px] w-full flex flex-col mt-4">
                    <!-- Chart Area -->
                    <div class="flex-1 relative z-10 w-full">
                        <!-- Background Grid -->
                        <div class="absolute inset-0 flex flex-col justify-between pointer-events-none z-0">
                            <div class="border-t border-dashed border-gray-200 dark:border-gray-800/50 w-full"></div>
                            <div class="border-t border-dashed border-gray-200 dark:border-gray-800/50 w-full"></div>
                            <div class="border-t border-dashed border-gray-200 dark:border-gray-800/50 w-full"></div>
                            <div class="border-t border-solid border-gray-200 dark:border-gray-800 w-full"></div>
                        </div>
                        
                        <!-- SVG Area & Line -->
                        <svg class="absolute inset-0 w-full h-full z-10 overflow-visible" preserveAspectRatio="none">
                            <defs>
                                <linearGradient id="netWorthGrad" x1="0" y1="0" x2="0" y2="1">
                                    <stop offset="0%" stop-color="#3b82f6" stop-opacity="0.25" />
                                    <stop offset="100%" stop-color="#3b82f6" stop-opacity="0" />
                                </linearGradient>
                            </defs>
                            <svg viewBox="0 0 100 100" preserveAspectRatio="none" class="w-full h-full overflow-visible">
                                <polygon :points="getAreaPoints" fill="url(#netWorthGrad)" />
                                <polyline :points="getLinePoints" fill="none" stroke="#3b82f6" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" vector-effect="non-scaling-stroke" />
                            </svg>
                        </svg>

                        <!-- HTML Data Points -->
                        <div class="absolute inset-0 z-20 pointer-events-none">
                            <div v-for="(data, idx) in netWorthTrendData" :key="idx" 
                                class="absolute top-0 bottom-0 flex flex-col items-center justify-end pointer-events-none"
                                :style="{ 
                                    left: netWorthTrendData.length === 1 ? '50%' : `${(idx / (netWorthTrendData.length - 1)) * 100}%`,
                                    transform: 'translateX(-50%)'
                                }">
                                
                                <div class="absolute inset-y-0 w-8 md:w-16 flex justify-center group pointer-events-auto cursor-pointer">
                                    
                                    <div class="absolute -top-6 opacity-0 group-hover:opacity-100 transition-all duration-200 bg-blue-600 dark:bg-blue-500 text-white text-xs py-1.5 px-3 rounded-lg shadow-lg whitespace-nowrap z-30 transform -translate-y-2 group-hover:translate-y-0 font-medium">
                                        {{ formatCurrency(data.value) }}
                                    </div>
                                    
                                    <div class="h-full w-px bg-blue-500/20 opacity-0 group-hover:opacity-100 transition-opacity hidden md:block"></div>
                                    
                                    <div class="absolute w-3.5 h-3.5 rounded-full bg-white dark:bg-gray-900 border-2 border-blue-500 shadow-md group-hover:scale-125 group-hover:border-4 transition-all"
                                         :style="{ bottom: netWorthTrendData.length === 1 ? '50%' : `${((data.value - minNetWorth) / Math.max(maxNetWorth - minNetWorth, 1)) * 100}%`, transform: 'translateY(50%)' }">
                                    </div>
                                </div>
                            </div>
                        </div>
                    </div>
                    
                    <!-- X Axis Labels -->
                    <div class="h-[60px] w-full mt-2 z-10 relative">
                        <div v-for="(data, idx) in netWorthTrendData" :key="`label-${idx}`" 
                            class="absolute top-0 flex flex-col items-center w-20"
                            :style="{ 
                                left: netWorthTrendData.length === 1 ? '50%' : `${(idx / (netWorthTrendData.length - 1)) * 100}%`,
                                transform: 'translateX(-50%)'
                            }">
                            <p class="text-xs font-semibold text-gray-500 dark:text-gray-400 mt-2">{{ data.label }}</p>
                        </div>
                    </div>
                    
                </div>
            </div>
        </div>

        <!-- Category Breakdown Chart -->
        <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl shadow-sm flex flex-col overflow-hidden relative shrink-0">
            <div class="p-6 border-b border-border dark:border-border-dark flex justify-between items-center bg-gray-50/50 dark:bg-gray-800/50">
                <div>
                    <h3 class="font-bold text-lg text-text dark:text-text-dark">{{ pieChartType === 'expense' ? 'Expense' : 'Income' }} Breakdown</h3>
                    <p class="text-sm text-gray-500 mt-1">Category distribution for selected period</p>
                </div>
                
                <!-- Pie Chart Toggle -->
                <div class="flex bg-gray-200 dark:bg-gray-700 rounded-lg p-1">
                    <button @click="pieChartType = 'expense'" :class="['px-4 py-1.5 rounded-md text-sm font-medium transition-colors', pieChartType === 'expense' ? 'bg-white dark:bg-gray-600 text-text dark:text-white shadow-sm' : 'text-gray-500 hover:text-gray-700 dark:hover:text-gray-300']">Expense</button>
                    <button @click="pieChartType = 'income'" :class="['px-4 py-1.5 rounded-md text-sm font-medium transition-colors', pieChartType === 'income' ? 'bg-white dark:bg-gray-600 text-text dark:text-white shadow-sm' : 'text-gray-500 hover:text-gray-700 dark:hover:text-gray-300']">Income</button>
                </div>
            </div>
            <div class="p-6 flex items-center justify-center">
                <div v-if="pieChartTotal > 0" class="w-full h-[300px]">
                    <FinanceChart :data="pieChartData" :total="pieChartTotal" :title="pieChartType === 'expense' ? 'Total Expense' : 'Total Income'" />
                </div>
                <div v-else class="h-[300px] flex items-center justify-center text-gray-400">No data in this period</div>
            </div>
        </div>

    </div>
</template>
