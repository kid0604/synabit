<script setup lang="ts">
import { ref, onMounted, computed } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { ask } from '@tauri-apps/plugin-dialog';
import { Plus, Settings, ChevronLeft, ChevronRight, Wallet, Scale, Search, ChevronDown, PieChart, Target, BookOpen } from 'lucide-vue-next';
import { logger } from '../../utils/logger';


import FinanceReports from './components/FinanceReports.vue';
import FinanceDebts from './components/FinanceDebts.vue';
import FinanceBudgets from './components/FinanceBudgets.vue';
import TransactionModal from './TransactionModal.vue';
import FinanceSettingsModal from './FinanceSettingsModal.vue';
import FinanceOnboarding from './FinanceOnboarding.vue';
import AdjustBalanceModal from './AdjustBalanceModal.vue';
import { type Transaction, type FinanceAccount, type Debt, type Budget, DEFAULT_INCOME_CATEGORIES, DEFAULT_EXPENSE_CATEGORIES, DEFAULT_ACCOUNTS, SYSTEM_INCOME_CATEGORIES, SYSTEM_EXPENSE_CATEGORIES } from './types';

const props = defineProps<{
  vaultPath: string;
}>();

// --- State ---
const currentView = ref<'transactions' | 'reports' | 'debts' | 'budgets'>('transactions');
const months = ref<{ id: string, label: string, date: Date, node: any }[]>([]);
const currentMonthIdx = ref(-1);

const configNode = ref<any>(null);
const incomeCategories = ref<string[]>([...DEFAULT_INCOME_CATEGORIES]);
const expenseCategories = ref<string[]>([...DEFAULT_EXPENSE_CATEGORIES]);
const accounts = ref<FinanceAccount[]>([...DEFAULT_ACCOUNTS]);

const debtsNode = ref<any>(null);
const debts = ref<Debt[]>([]);

const budgets = ref<Budget[]>([]);

const searchQuery = ref('');
const filterType = ref<'all' | 'income' | 'expense' | 'transfer'>('all');
const filterAccount = ref<string>('all');

const showTxModal = ref(false);
const editingTx = ref<Transaction | null>(null);
const showSettingsModal = ref(false);
const showAdjustModal = ref(false);
const adjustingAccount = ref<{id: string, name: string, balance: number} | null>(null);

const needsOnboarding = ref(false);
const loading = ref(true);

// --- Computed ---
const currentMonth = computed(() => {
    if (currentMonthIdx.value >= 0 && currentMonthIdx.value < months.value.length) {
        return months.value[currentMonthIdx.value];
    }
    return null;
});

const currentTransactions = computed<Transaction[]>(() => {
    if (!currentMonth.value || !currentMonth.value.node.properties?.transactions) return [];
    
    // Sort descending by date
    const txs = [...currentMonth.value.node.properties.transactions] as Transaction[];
    return txs.sort((a, b) => new Date(b.date).getTime() - new Date(a.date).getTime());
});

const totalIncome = computed(() => {
    return currentTransactions.value
        .filter(t => t.type === 'income')
        .reduce((sum, t) => sum + t.amount, 0);
});

const totalExpense = computed(() => {
    return currentTransactions.value
        .filter(t => t.type === 'expense')
        .reduce((sum, t) => sum + t.amount, 0);
});

const filteredTransactions = computed(() => {
    return currentTransactions.value.filter(tx => {
        if (searchQuery.value) {
            const query = searchQuery.value.toLowerCase();
            const categoryMatch = tx.category.toLowerCase().includes(query);
            const noteMatch = tx.note && tx.note.toLowerCase().includes(query);
            if (!categoryMatch && !noteMatch) return false;
        }
        if (filterType.value !== 'all' && tx.type !== filterType.value) return false;
        if (filterAccount.value !== 'all' && tx.accountId !== filterAccount.value && tx.toAccountId !== filterAccount.value) return false;
        
        return true;
    });
});

const groupedTransactions = computed(() => {
    const groups: Record<string, { dateStr: string, date: Date, transactions: Transaction[], totalIncome: number, totalExpense: number }> = {};
    filteredTransactions.value.forEach(tx => {
        const d = new Date(tx.date);
        const dateStr = `${d.getDate().toString().padStart(2, '0')}/${(d.getMonth()+1).toString().padStart(2, '0')}/${d.getFullYear()}`;
        if (!groups[dateStr]) {
            groups[dateStr] = { dateStr, date: new Date(d.getFullYear(), d.getMonth(), d.getDate()), transactions: [], totalIncome: 0, totalExpense: 0 };
        }
        groups[dateStr].transactions.push(tx);
        if (tx.type === 'income') groups[dateStr].totalIncome += tx.amount;
        if (tx.type === 'expense') groups[dateStr].totalExpense += tx.amount;
    });
    
    return Object.values(groups).sort((a, b) => b.date.getTime() - a.date.getTime());
});

const balance = computed(() => totalIncome.value - totalExpense.value);

const globalNetWorth = computed(() => {
    // 1. Sum Initial Balances
    let initialSum = 0;
    accounts.value.forEach(acc => {
        initialSum += acc.initialBalance;
    });
    
    // 2. Sum All-time transactions
    let allTimeIncome = 0;
    let allTimeExpense = 0;
    
    months.value.forEach(m => {
        const txs: Transaction[] = m.node.properties?.transactions || [];
        txs.forEach(t => {
            if (t.type === 'income') allTimeIncome += t.amount;
            else if (t.type === 'expense') allTimeExpense += t.amount;
        });
    });
    
    return initialSum + allTimeIncome - allTimeExpense;
});

const accountBalances = computed(() => {
    return accounts.value.map(acc => {
        let income = 0;
        let expense = 0;
        months.value.forEach(m => {
            const txs: Transaction[] = m.node.properties?.transactions || [];
            txs.forEach(t => {
                if (t.accountId === acc.id) {
                    if (t.type === 'income') income += t.amount;
                    else if (t.type === 'expense') expense += t.amount;
                    else if (t.type === 'transfer') expense += t.amount;
                }
                if (t.toAccountId === acc.id) {
                    if (t.type === 'transfer') income += t.amount;
                }
            });
        });
        return {
            id: acc.id,
            name: acc.name,
            balance: acc.initialBalance + income - expense
        };
    });
});



// --- Methods ---

const formatCurrency = (val: number) => {
    return new Intl.NumberFormat('vi-VN', { style: 'currency', currency: 'VND' }).format(val);
};



const getAccountName = (id: string) => {
    const acc = accounts.value.find(a => a.id === id);
    return acc ? acc.name : 'Không rõ';
};

const ensureCurrentMonthNodeExists = async () => {
    const now = new Date();
    const mm = (now.getMonth() + 1).toString().padStart(2, '0');
    const yyyy = now.getFullYear();
    const expectedId = `Finance/${yyyy}-${mm}.json`;
    
    const existing = months.value.find(m => m.id === expectedId);
    if (!existing) {
        // Create new node
        const nodeProps = { transactions: [] };
        try {
            await invoke('write_node_file', {
                vaultPath: props.vaultPath,
                relPath: expectedId,
                title: `Tháng ${mm}/${yyyy}`,
                nodeType: 'finance_month',
                properties: nodeProps,
                content: ''
            });
            await loadData();
        } catch (e) {
            logger.error('Failed to create current month node', e);
        }
    } else {
        // Just select it
        currentMonthIdx.value = months.value.findIndex(m => m.id === expectedId);
    }
};

const loadData = async () => {
    if (!props.vaultPath) return;
    loading.value = true;
    try {
        // Load config
        const configs: any[] = await invoke('get_nodes', { nodeType: 'finance_config' });
        if (configs.length > 0) {
            configNode.value = configs[0];
            if (configNode.value.properties) {
                if (configNode.value.properties.categories) {
                    // Auto-migrate legacy config
                    const oldCats = configNode.value.properties.categories;
                    incomeCategories.value = [...DEFAULT_INCOME_CATEGORIES];
                    // Keep old ones that are not in income default
                    expenseCategories.value = Array.from(new Set([...DEFAULT_EXPENSE_CATEGORIES, ...oldCats.filter((c: string) => !DEFAULT_INCOME_CATEGORIES.includes(c))]));
                    
                    delete configNode.value.properties.categories;
                    configNode.value.properties.incomeCategories = incomeCategories.value;
                    configNode.value.properties.expenseCategories = expenseCategories.value;
                    
                    saveConfig({ incomeCategories: incomeCategories.value, expenseCategories: expenseCategories.value, accounts: configNode.value.properties.accounts || [...DEFAULT_ACCOUNTS] });
                } else {
                    incomeCategories.value = configNode.value.properties.incomeCategories || [...DEFAULT_INCOME_CATEGORIES];
                    expenseCategories.value = configNode.value.properties.expenseCategories || [...DEFAULT_EXPENSE_CATEGORIES];
                    budgets.value = configNode.value.properties.budgets || [];
                    
                    // Ensure system categories exist in loaded arrays
                    SYSTEM_INCOME_CATEGORIES.forEach(sysCat => {
                        if (!incomeCategories.value.includes(sysCat)) {
                            incomeCategories.value.push(sysCat);
                        }
                    });
                    SYSTEM_EXPENSE_CATEGORIES.forEach(sysCat => {
                        if (!expenseCategories.value.includes(sysCat)) {
                            expenseCategories.value.push(sysCat);
                        }
                    });
                }
                accounts.value = configNode.value.properties.accounts || [...DEFAULT_ACCOUNTS];
            }
            needsOnboarding.value = false;
        } else {
            // First time user!
            needsOnboarding.value = true;
            loading.value = false;
            return;
        }

        // Load months
        const monthNodes: any[] = await invoke('get_nodes', { nodeType: 'finance_month' });
        
        // Auto-migration for legacy transactions
        
        months.value = monthNodes.map(node => {
            // Check for legacy transactions
            let nodeModified = false;
            if (node.properties && node.properties.transactions) {
                node.properties.transactions.forEach((tx: any) => {
                    if (tx.account && !tx.accountId) {
                        // Find matching account id by name
                        const matched = accounts.value.find(a => a.name === tx.account);
                        tx.accountId = matched ? matched.id : accounts.value[0]?.id;
                        delete tx.account; // clean up
                        nodeModified = true;
                    }
                });
            }
            
            if (nodeModified) {
                // Save migrated node back to disk
                invoke('write_node_file', {
                    vaultPath: props.vaultPath,
                    relPath: node.id,
                    title: node.title,
                    nodeType: 'finance_month',
                    properties: node.properties,
                    content: ''
                }).catch(e => logger.error('Auto-migration save failed', e));
            }
            
            // Extract YYYY-MM from title or id
            const match = node.id.match(/(\d{4})-(\d{2})\.json/);
            let date = new Date();
            if (match) {
                date = new Date(parseInt(match[1]), parseInt(match[2]) - 1, 1);
            }
            return {
                id: node.id,
                label: node.title,
                date,
                node
            };
        }).sort((a, b) => a.date.getTime() - b.date.getTime()); // Chronological
        
        if (months.value.length === 0) {
            await ensureCurrentMonthNodeExists();
        } else if (currentMonthIdx.value === -1 || currentMonthIdx.value >= months.value.length) {
            currentMonthIdx.value = months.value.length - 1;
        }

        // Load debts
        const debtNodes: any[] = await invoke('get_nodes', { nodeType: 'finance_debts' });
        if (debtNodes.length > 0) {
            debtsNode.value = debtNodes[0];
            debts.value = debtsNode.value.properties.debts || [];
        } else {
            // Create default debts node
            const newProps = { debts: [] };
            try {
                await invoke('write_node_file', {
                    vaultPath: props.vaultPath,
                    relPath: 'Finance/Debts.json',
                    title: 'Sổ Nợ',
                    nodeType: 'finance_debts',
                    properties: newProps,
                    content: ''
                });
                const loaded: any[] = await invoke('get_nodes', { nodeType: 'finance_debts' });
                if (loaded.length > 0) {
                    debtsNode.value = loaded[0];
                    debts.value = debtsNode.value.properties.debts || [];
                }
            } catch(e) {
                logger.error('Failed to create default debts node', e);
            }
        }
        
    } catch (e) {
        logger.error('Failed to load finance data', e);
    } finally {
        loading.value = false;
    }
};

const prevMonth = () => {
    if (currentMonthIdx.value > 0) currentMonthIdx.value--;
};

const saveDebts = async (updatedDebts: Debt[]) => {
    debts.value = updatedDebts;
    if (debtsNode.value) {
        debtsNode.value.properties.debts = updatedDebts;
        try {
            await invoke('write_node_file', {
                vaultPath: props.vaultPath,
                relPath: debtsNode.value.id,
                title: debtsNode.value.title,
                nodeType: 'finance_debts',
                properties: debtsNode.value.properties,
                content: debtsNode.value.content || ''
            });
        } catch(e) {
            logger.error('Failed to save debts node', e);
        }
    }
};

const nextMonth = () => {
    if (currentMonthIdx.value < months.value.length - 1) currentMonthIdx.value++;
};

const openAddTx = () => {
    editingTx.value = null;
    showTxModal.value = true;
};

const openEditTx = (tx: Transaction) => {
    editingTx.value = tx;
    showTxModal.value = true;
};

const handleBalanceAdjust = async (diff: number) => {
    if (!adjustingAccount.value) return;
    
    // Auto-add "Điều chỉnh số dư" to categories if missing
    let needSave = false;
    if (!expenseCategories.value.includes('Điều chỉnh số dư')) {
        expenseCategories.value.push('Điều chỉnh số dư');
        needSave = true;
    }
    if (!incomeCategories.value.includes('Điều chỉnh số dư')) {
        incomeCategories.value.push('Điều chỉnh số dư');
        needSave = true;
    }
    if (needSave) {
        await saveConfig({ incomeCategories: incomeCategories.value, expenseCategories: expenseCategories.value, accounts: accounts.value });
    }

    const tx: Transaction = {
        id: `tx-${Date.now()}-${Math.floor(Math.random()*1000)}`,
        type: diff > 0 ? 'income' : 'expense',
        amount: Math.abs(diff),
        category: 'Điều chỉnh số dư',
        accountId: adjustingAccount.value.id,
        date: new Date().toISOString(),
        note: 'Tự động điều chỉnh số dư lệch'
    };

    await saveTransaction(tx);
};

const saveTransaction = async (tx: Transaction) => {
    if (!currentMonth.value) return;
    
    // Auto-create debt if standalone borrow/lend
    if (['Đi vay', 'Cho vay'].includes(tx.category) && !tx.debtId) {
        const newDebt: Debt = {
            id: `debt-${Date.now()}-${Math.floor(Math.random()*1000)}`,
            type: tx.category === 'Đi vay' ? 'borrow' : 'lend',
            person: tx.note.trim() ? tx.note.trim() : 'Người giấu tên',
            totalAmount: tx.amount,
            paidAmount: 0,
            startDate: tx.date,
            status: 'active',
            accountId: tx.accountId,
            note: ''
        };
        tx.debtId = newDebt.id;
        const newDebts = [...debts.value, newDebt];
        await saveDebts(newDebts);
    }
    
    // Instead of using the currently viewed month, we should ideally put it in the correct month's node based on tx.date
    // For simplicity in v1, we ensure it goes to the correct month file.
    const d = new Date(tx.date);
    const mm = (d.getMonth() + 1).toString().padStart(2, '0');
    const yyyy = d.getFullYear();
    const expectedId = `Finance/${yyyy}-${mm}.json`;
    
    let targetNode = months.value.find(m => m.id === expectedId)?.node;
    
    // If saving to a month that doesn't exist yet
    if (!targetNode) {
        targetNode = {
            id: expectedId,
            title: `Tháng ${mm}/${yyyy}`,
            node_type: 'finance_month',
            properties: { transactions: [] }
        };
    }
    
    // Make sure properties.transactions exists
    if (!targetNode.properties) targetNode.properties = {};
    if (!targetNode.properties.transactions) targetNode.properties.transactions = [];
    
    const txs: Transaction[] = targetNode.properties.transactions;
    const existingIdx = txs.findIndex(t => t.id === tx.id);
    
    if (existingIdx >= 0) {
        txs[existingIdx] = tx; // Update
    } else {
        txs.push(tx); // Add
    }
    
    if (!months.value.some(m => m.id === expectedId)) {
        months.value.push({
            id: expectedId,
            label: `Tháng ${mm}/${yyyy}`,
            date: new Date(yyyy, parseInt(mm) - 1, 1),
            node: targetNode
        });
        // Sort chronologically
        months.value.sort((a, b) => a.date.getTime() - b.date.getTime());
    }
    
    // Also if we edited an existing transaction and changed its month, we need to remove it from the old month!
    // Edge case handling: check if it existed in the current viewing month but we moved it to another month
    if (currentMonth.value && currentMonth.value.id !== expectedId) {
        const currTxs = currentMonth.value.node.properties?.transactions as Transaction[] || [];
        const oldIdx = currTxs.findIndex(t => t.id === tx.id);
        if (oldIdx >= 0) {
            currTxs.splice(oldIdx, 1);
            // Save the old month to remove it
            await invoke('write_node_file', {
                vaultPath: props.vaultPath,
                relPath: currentMonth.value.id,
                title: currentMonth.value.label,
                nodeType: 'finance_month',
                properties: currentMonth.value.node.properties,
                content: ''
            });
        }
    }
    
    try {
        await invoke('write_node_file', {
            vaultPath: props.vaultPath,
            relPath: targetNode.id,
            title: targetNode.title,
            nodeType: 'finance_month',
            properties: targetNode.properties,
            content: ''
        });
        showTxModal.value = false;
        
        // Always jump to the month where the transaction was added
        const targetIdx = months.value.findIndex(m => m.id === expectedId);
        if (targetIdx >= 0) {
            currentMonthIdx.value = targetIdx;
        }
    } catch (e) {
        logger.error('Failed to save transaction', e);
    }
};

const deleteTransaction = async (txId: string) => {
    if (!currentMonth.value) return;
    
    const confirmed = await ask('This transaction will be permanently removed. This action cannot be undone.', {
        title: 'Delete transaction?',
        kind: 'warning',
        okLabel: 'Delete',
        cancelLabel: 'Cancel'
    });
    
    if (!confirmed) return;
    
    const txs: Transaction[] = currentMonth.value.node.properties?.transactions || [];
    const idx = txs.findIndex(t => t.id === txId);
    if (idx >= 0) {
        txs.splice(idx, 1);
        try {
            await invoke('write_node_file', {
                vaultPath: props.vaultPath,
                relPath: currentMonth.value.id,
                title: currentMonth.value.label,
                nodeType: 'finance_month',
                properties: currentMonth.value.node.properties,
                content: ''
            });
        } catch (e) {
            logger.error('Failed to delete transaction', e);
        }
    }
};

const saveConfig = async (config: { incomeCategories: string[], expenseCategories: string[], accounts: FinanceAccount[], budgets?: Budget[] }) => {
    if (config.budgets) {
        budgets.value = config.budgets;
    }
    
    const propsToSave = {
        incomeCategories: config.incomeCategories,
        expenseCategories: config.expenseCategories,
        accounts: config.accounts,
        budgets: budgets.value
    };
    
    try {
        await invoke('write_node_file', {
            vaultPath: props.vaultPath,
            relPath: 'Finance/Config.json',
            title: 'Cấu hình Tài chính',
            nodeType: 'finance_config',
            properties: propsToSave,
            content: ''
        });
        await loadData();
    } catch (e) {
        logger.error('Failed to save config', e);
    }
};

const finishOnboarding = async (config: { incomeCategories: string[], expenseCategories: string[], accounts: FinanceAccount[] }) => {
    loading.value = true;
    await saveConfig(config);
};

const openMonthById = async (id: string) => {
    if (months.value.length === 0) await loadData();
    const idx = months.value.findIndex(m => m.id === id);
    if (idx >= 0) currentMonthIdx.value = idx;
};

// Lifecycle
onMounted(() => {
    loadData();
    
    listen('vault-file-modified', () => {
        // Reload data if background sync changes finance files
        // We can do a quick loadData here
        loadData();
    });
});

defineExpose({ openMonthById });

</script>

<template>
  <div class="flex-1 flex flex-col h-full bg-base dark:bg-base-dark overflow-hidden relative">
      <!-- Loading Overlay -->
      <div v-if="loading && months.length === 0" class="absolute inset-0 flex items-center justify-center z-[100] bg-base/50 dark:bg-base-dark/50">
          <div class="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-500"></div>
      </div>
      
      <!-- Onboarding -->
      <FinanceOnboarding v-if="needsOnboarding" @complete="finishOnboarding" />
      
      <!-- Topbar -->
      <div v-else class="flex items-center justify-between p-6 shrink-0">
          <div>
              <h1 class="text-2xl font-bold flex items-center gap-2">
                  <Wallet class="w-6 h-6 text-blue-500" />
                  Tài chính
              </h1>
              <p class="text-sm text-gray-500 dark:text-gray-400">Quản lý thu chi và ngân sách cá nhân</p>
          </div>
          
          <div class="flex items-center gap-3">
              <button @click="showSettingsModal = true" class="p-2.5 rounded-xl bg-surface dark:bg-surface-dark border border-border dark:border-border-dark text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors shadow-sm">
                  <Settings class="w-5 h-5" />
              </button>
              <button @click="openAddTx" class="flex items-center gap-2 px-4 py-2.5 rounded-xl bg-blue-500 text-white hover:bg-blue-600 transition-colors shadow-sm font-medium">
                  <Plus class="w-5 h-5" />
                  <span>Thêm giao dịch</span>
              </button>
          </div>
      </div>

      <!-- Main Content Area -->
      <div v-if="currentMonth || currentView === 'reports' || currentView === 'debts'" class="flex-1 flex gap-6 px-6 pb-6 overflow-hidden">
          
          <!-- Sidebar (Global Context) -->
          <div class="w-[280px] flex flex-col gap-6 shrink-0 overflow-y-auto hidden-scrollbar pr-2">
              
              <!-- Navigation Menu -->
              <div class="flex flex-col gap-1">
                  <button 
                      @click="currentView = 'transactions'" 
                      :class="['flex items-center gap-3 px-4 py-2.5 rounded-xl font-medium text-sm transition-colors w-full text-left', currentView === 'transactions' ? 'bg-blue-50 text-blue-600 dark:bg-blue-900/20 dark:text-blue-400' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800']"
                  >
                      <Wallet class="w-5 h-5" />
                      Sổ Thu Chi
                  </button>
                  <button 
                      @click="currentView = 'reports'" 
                      :class="['flex items-center gap-3 px-4 py-2.5 rounded-xl font-medium text-sm transition-colors w-full text-left', currentView === 'reports' ? 'bg-blue-50 text-blue-600 dark:bg-blue-900/20 dark:text-blue-400' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800']"
                  >
                      <PieChart class="w-5 h-5" />
                      Báo Cáo & Phân Tích
                  </button>
                  <button 
                      @click="currentView = 'debts'" 
                      :class="['flex items-center gap-3 px-4 py-2.5 rounded-xl font-medium text-sm transition-colors w-full text-left', currentView === 'debts' ? 'bg-blue-50 text-blue-600 dark:bg-blue-900/20 dark:text-blue-400' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800']"
                  >
                      <BookOpen class="w-5 h-5" />
                      Sổ Nợ
                  </button>
                  <button 
                      @click="currentView = 'budgets'" 
                      :class="['flex items-center gap-3 px-4 py-2.5 rounded-xl font-medium text-sm transition-colors w-full text-left', currentView === 'budgets' ? 'bg-blue-50 text-blue-600 dark:bg-blue-900/20 dark:text-blue-400' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800']"
                  >
                      <Target class="w-5 h-5" />
                      Ngân sách
                  </button>
              </div>

              <!-- Global Net Worth -->
              <div class="bg-gradient-to-br from-blue-500 to-indigo-600 rounded-2xl p-5 text-white shadow-lg relative overflow-hidden shrink-0">
                  <div class="absolute right-0 top-0 opacity-10 pointer-events-none">
                      <Wallet class="w-32 h-32 -mt-4 -mr-4" />
                  </div>
                  <p class="text-blue-100 text-sm font-medium mb-1">Tổng Tài Sản</p>
                  <h2 class="text-3xl font-bold tracking-tight">{{ formatCurrency(globalNetWorth) }}</h2>
              </div>
              
              <!-- Account Balances -->
              <div class="flex flex-col gap-2">
                  <h3 class="font-bold text-sm text-gray-500 dark:text-gray-400 uppercase tracking-wider pl-2">Tài khoản của tôi</h3>
                  <div class="flex flex-col gap-1.5">
                      <div v-for="acc in accountBalances" :key="acc.id" class="flex items-center gap-3 p-3 rounded-xl hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors group relative">
                          <div class="p-2.5 rounded-xl bg-white dark:bg-gray-900 text-blue-500 shadow-sm border border-gray-100 dark:border-gray-800 shrink-0">
                              <Wallet class="w-5 h-5" />
                          </div>
                          <div class="flex flex-col flex-1 min-w-0">
                              <span class="text-sm font-medium text-gray-500 dark:text-gray-400 truncate">{{ acc.name }}</span>
                              <span class="text-base font-bold text-text dark:text-text-dark truncate">{{ formatCurrency(acc.balance) }}</span>
                          </div>
                          <button @click="adjustingAccount = acc; showAdjustModal = true" class="absolute right-3 p-1.5 text-gray-400 hover:text-blue-500 opacity-0 group-hover:opacity-100 transition-opacity bg-white dark:bg-gray-800 rounded-lg shadow-sm border border-gray-100 dark:border-gray-700" title="Điều chỉnh số dư">
                              <Scale class="w-4 h-4" />
                          </button>
                      </div>
                  </div>
              </div>
          </div>

          <!-- Main Content (Transactions) -->
          <div v-if="currentView === 'transactions' && currentMonth" class="flex-1 flex flex-col gap-6 overflow-hidden">
              
              <!-- Monthly Dashboard Header -->
              <div class="flex gap-4 shrink-0">
                  <!-- Month Selector -->
                  <div class="w-[220px] bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl p-4 shadow-sm flex items-center justify-between shrink-0">
                      <button @click="prevMonth" :disabled="currentMonthIdx <= 0" class="p-2 rounded-full hover:bg-gray-100 dark:hover:bg-gray-800 disabled:opacity-30 disabled:pointer-events-none transition-colors text-gray-500">
                          <ChevronLeft class="w-5 h-5" />
                      </button>
                      <div class="flex flex-col items-center">
                          <span class="text-xs text-gray-500 font-medium mb-0.5">Tháng</span>
                          <span class="font-bold text-lg text-text dark:text-text-dark">{{ currentMonth.date.getMonth() + 1 }}/{{ currentMonth.date.getFullYear() }}</span>
                      </div>
                      <button @click="nextMonth" :disabled="currentMonthIdx >= months.length - 1" class="p-2 rounded-full hover:bg-gray-100 dark:hover:bg-gray-800 disabled:opacity-30 disabled:pointer-events-none transition-colors text-gray-500">
                          <ChevronRight class="w-5 h-5" />
                      </button>
                  </div>
                  
                  <!-- Summary Stats -->
                  <div class="flex-1 grid grid-cols-3 gap-4">
                      <!-- Income -->
                      <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl p-4 shadow-sm flex flex-col justify-center">
                          <div class="flex items-center gap-2 mb-1">
                              <div class="w-2 h-2 rounded-full bg-green-500"></div>
                              <span class="text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">Tổng Thu</span>
                          </div>
                          <p class="text-xl font-bold text-green-600 dark:text-green-400">{{ formatCurrency(totalIncome) }}</p>
                      </div>
                      <!-- Expense -->
                      <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl p-4 shadow-sm flex flex-col justify-center">
                          <div class="flex items-center gap-2 mb-1">
                              <div class="w-2 h-2 rounded-full bg-red-500"></div>
                              <span class="text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">Tổng Chi</span>
                          </div>
                          <p class="text-xl font-bold text-red-600 dark:text-red-400">{{ formatCurrency(totalExpense) }}</p>
                      </div>
                      <!-- Net Flow -->
                      <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl p-4 shadow-sm flex flex-col justify-center">
                          <div class="flex items-center gap-2 mb-1">
                              <div class="w-2 h-2 rounded-full bg-blue-500"></div>
                              <span class="text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">Số Dư Tháng</span>
                          </div>
                          <p :class="['text-xl font-bold', balance >= 0 ? 'text-text dark:text-text-dark' : 'text-red-500']">{{ balance > 0 ? '+' : '' }}{{ formatCurrency(balance) }}</p>
                      </div>
                  </div>
              </div>

              <!-- Transaction List -->
              <div class="flex-1 bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl shadow-sm flex flex-col overflow-hidden">
              <div class="p-4 border-b border-border dark:border-border-dark bg-gray-50/50 dark:bg-gray-800/50 shrink-0 flex flex-col gap-3">
                  <div class="flex items-center justify-between">
                      <h3 class="font-bold text-lg text-text dark:text-text-dark">Lịch sử Giao dịch</h3>
                  </div>
                  <!-- Search and Filters -->
                  <div class="flex items-center gap-2">
                      <div class="relative flex-1">
                          <Search class="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-gray-400" />
                          <input v-model="searchQuery" type="text" placeholder="Tìm kiếm..." class="w-full pl-9 pr-3 py-1.5 bg-white dark:bg-gray-900 border border-border dark:border-border-dark rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 transition-shadow" />
                      </div>
                      <div class="relative">
                          <select v-model="filterType" class="appearance-none bg-gray-100 dark:bg-gray-800 hover:bg-gray-200 dark:hover:bg-gray-700 border-none rounded-xl pl-3 pr-8 py-1.5 text-sm font-medium text-gray-700 dark:text-gray-300 focus:outline-none focus:ring-2 focus:ring-blue-500 cursor-pointer transition-colors">
                              <option value="all">Tất cả</option>
                              <option value="income">Thu nhập</option>
                              <option value="expense">Chi tiêu</option>
                              <option value="transfer">Chuyển khoản</option>
                          </select>
                          <ChevronDown class="w-4 h-4 text-gray-500 absolute right-2.5 top-1/2 -translate-y-1/2 pointer-events-none" />
                      </div>
                      
                      <div class="relative">
                          <select v-model="filterAccount" class="appearance-none bg-gray-100 dark:bg-gray-800 hover:bg-gray-200 dark:hover:bg-gray-700 border-none rounded-xl pl-3 pr-8 py-1.5 text-sm font-medium text-gray-700 dark:text-gray-300 focus:outline-none focus:ring-2 focus:ring-blue-500 max-w-[150px] truncate cursor-pointer transition-colors">
                              <option value="all">Mọi tài khoản</option>
                              <option v-for="acc in accounts" :key="acc.id" :value="acc.id">{{ acc.name }}</option>
                          </select>
                          <ChevronDown class="w-4 h-4 text-gray-500 absolute right-2.5 top-1/2 -translate-y-1/2 pointer-events-none" />
                      </div>
                  </div>
              </div>
              
              <div class="flex-1 overflow-y-auto relative hidden-scrollbar bg-gray-50/30 dark:bg-gray-900/10">
                  <div v-if="filteredTransactions.length === 0" class="h-full flex flex-col items-center justify-center text-gray-400 p-6 text-center">
                      <div class="w-16 h-16 bg-gray-100 dark:bg-gray-800 rounded-full flex items-center justify-center mb-3">
                          <Search v-if="searchQuery || filterType !== 'all' || filterAccount !== 'all'" class="w-8 h-8 opacity-50" />
                          <Wallet v-else class="w-8 h-8 opacity-50" />
                      </div>
                      <p v-if="searchQuery || filterType !== 'all' || filterAccount !== 'all'">Không tìm thấy giao dịch nào phù hợp với bộ lọc.</p>
                      <template v-else>
                          <p>Không có giao dịch nào trong tháng này.</p>
                          <button @click="openAddTx" class="mt-4 text-blue-500 hover:underline text-sm">Thêm giao dịch ngay</button>
                      </template>
                  </div>
                  
                  <div v-else class="flex flex-col pb-4">
                      <div v-for="group in groupedTransactions" :key="group.dateStr" class="mb-2">
                          <!-- Sticky Date Header -->
                          <div class="sticky top-0 z-10 px-4 py-2 bg-gray-100/95 dark:bg-gray-800/95 backdrop-blur-md border-y border-border dark:border-border-dark flex items-center justify-between shadow-sm">
                              <span class="font-bold text-sm text-text dark:text-text-dark">{{ group.dateStr }}</span>
                              <div class="flex items-center gap-3 text-xs font-semibold">
                                  <span v-if="group.totalIncome > 0" class="text-green-500">+{{ formatCurrency(group.totalIncome) }}</span>
                                  <span v-if="group.totalExpense > 0" class="text-red-500">-{{ formatCurrency(group.totalExpense) }}</span>
                              </div>
                          </div>
                          
                          <!-- Transactions in Group -->
                          <div class="flex flex-col px-2 py-1">
                              <div v-for="tx in group.transactions" :key="tx.id" class="group flex items-center gap-4 p-3 mx-2 my-1 rounded-xl bg-white dark:bg-surface-dark border border-transparent hover:border-gray-200 dark:hover:border-gray-700 hover:shadow-sm transition-all cursor-pointer relative" @click="openEditTx(tx)">
                                  
                                  <div :class="['w-10 h-10 rounded-full flex items-center justify-center shrink-0', tx.type === 'income' ? 'bg-green-100 dark:bg-green-900/30 text-green-500' : tx.type === 'expense' ? 'bg-red-100 dark:bg-red-900/30 text-red-500' : 'bg-blue-100 dark:bg-blue-900/30 text-blue-500']">
                                      <TrendingUp v-if="tx.type === 'income'" class="w-5 h-5" />
                                      <TrendingDown v-else-if="tx.type === 'expense'" class="w-5 h-5" />
                                      <RefreshCw v-else class="w-5 h-5" />
                                  </div>
                                  
                                  <div class="flex-1 min-w-0">
                                      <p class="font-semibold text-text dark:text-text-dark truncate">{{ tx.type === 'transfer' ? 'Chuyển khoản nội bộ' : tx.category }}</p>
                                      <div class="flex items-center gap-2 text-xs text-gray-500 mt-0.5">
                                          <span>{{ new Date(tx.date).getHours().toString().padStart(2, '0') }}:{{ new Date(tx.date).getMinutes().toString().padStart(2, '0') }}</span>
                                          <span>•</span>
                                          <span class="truncate">{{ tx.type === 'transfer' && tx.toAccountId ? getAccountName(tx.accountId) + ' ➡️ ' + getAccountName(tx.toAccountId) : getAccountName(tx.accountId) }}</span>
                                          <span v-if="tx.note" class="truncate">• {{ tx.note }}</span>
                                      </div>
                                  </div>
                                  
                                  <div class="text-right shrink-0 pr-8">
                                      <p :class="['font-bold', tx.type === 'income' ? 'text-green-500' : tx.type === 'expense' ? 'text-text dark:text-text-dark' : 'text-blue-500']">
                                          {{ tx.type === 'income' ? '+' : tx.type === 'expense' ? '-' : '' }}{{ formatCurrency(tx.amount) }}
                                      </p>
                                  </div>
                                  
                                  <!-- Action Buttons overlay -->
                                  <div class="absolute right-3 top-1/2 -translate-y-1/2 opacity-0 group-hover:opacity-100 transition-opacity">
                                      <button @click.stop="deleteTransaction(tx.id)" class="p-1.5 text-gray-400 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-900/30 rounded-lg transition-colors border border-transparent hover:border-red-200 dark:hover:border-red-800" title="Xóa giao dịch">
                                          <Trash2 class="w-4 h-4" />
                                      </button>
                                  </div>
                              </div>
                          </div>
                      </div>
                      </div>
                  </div>
              </div>
          </div>
          
          <!-- Reports View -->
          <div v-else-if="currentView === 'reports'" class="flex-1 overflow-hidden">
              <FinanceReports :months="months" :global-net-worth="globalNetWorth" :accounts="accounts" :account-balances="accountBalances" />
          </div>

          <!-- Debts View -->
          <div v-else-if="currentView === 'debts'" class="flex-1 overflow-hidden">
              <FinanceDebts 
                  :debts="debts"
                  :accounts="accounts"
                  @save-debts="saveDebts"
                  @create-transaction="(tx) => { saveTransaction(tx) }"
              />
          </div>

          <!-- Budgets View -->
          <div v-else-if="currentView === 'budgets'" class="flex-1 overflow-hidden">
              <FinanceBudgets 
                  :budgets="budgets"
                  :transactions="currentTransactions"
                  :expense-categories="expenseCategories"
                  @save-budgets="(newBudgets) => { saveConfig({ incomeCategories, expenseCategories, accounts, budgets: newBudgets }) }"
              />
          </div>
      </div>

      <TransactionModal :show="showTxModal" :transaction="editingTx" :income-categories="incomeCategories" :expense-categories="expenseCategories" :accounts="accounts" @close="showTxModal = false" @save="saveTransaction" />
      <FinanceSettingsModal :show="showSettingsModal" :initial-income-categories="incomeCategories" :initial-expense-categories="expenseCategories" :initial-accounts="accounts" :current-balances="accountBalances" @close="showSettingsModal = false" @save="saveConfig" />
      <AdjustBalanceModal v-if="adjustingAccount" :show="showAdjustModal" :account-id="adjustingAccount.id" :account-name="adjustingAccount.name" :current-balance="adjustingAccount.balance" @close="showAdjustModal = false" @adjust="handleBalanceAdjust" />
  </div>
</template>
