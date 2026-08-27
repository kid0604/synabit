<script setup lang="ts">
import { ref, onMounted, computed, watch } from 'vue';
import { useRouter, useRoute } from 'vue-router';
import { useEventBus } from '../../composables/useEventBus';
import { useNodeService } from '../../composables/useNodeService';
import { ask, open as openFileDialog, save as saveFileDialog } from '@tauri-apps/plugin-dialog';
import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';
import { openPath } from '@tauri-apps/plugin-opener';
import { Plus, Settings, Wallet, Scale, Search, ChevronDown, PieChart, Target, BookOpen, PanelLeft, TrendingUp, TrendingDown, RefreshCw, Trash2, AlertTriangle, X, Landmark, CreditCard, Repeat, Paperclip } from 'lucide-vue-next';
import { logger } from '../../utils/logger';
import { RECURRING_PATH, monthNodePath, monthNodeTitle, repairFinanceStorage, rowChanges, writeFinanceRows } from './ledger';
import { pendingByMonth, todayStr, type RecurringRule } from './recurring';
import {
    exportCsv,
    exportFilename,
    importRows,
    matchColumns,
    missingColumns,
    parseCsv,
    type ColumnMap,
    type ImportResult,
} from './csv';
import NavButtons from '../../shared/components/NavButtons.vue';


import FinanceReports from './components/FinanceReports.vue';
import FinanceDebts from './components/FinanceDebts.vue';
import FinanceBudgets from './components/FinanceBudgets.vue';
import FinanceRecurring from './components/FinanceRecurring.vue';
import FinanceImportModal from './components/FinanceImportModal.vue';
import TransactionModal from './TransactionModal.vue';
import FinanceSettingsModal from './FinanceSettingsModal.vue';
import FinanceOnboarding from './FinanceOnboarding.vue';
import AdjustBalanceModal from './AdjustBalanceModal.vue';
import { type Transaction, type FinanceAccount, type Category, type Debt, type Budget, DEFAULT_INCOME_CATEGORIES, DEFAULT_EXPENSE_CATEGORIES, DEFAULT_ACCOUNTS, SYSTEM_INCOME_CATEGORIES, SYSTEM_EXPENSE_CATEGORIES } from './types';
import { categoryName, newCategoryId, toCategories } from './categories';
import { allowRateLookup, currentCurrency, formatCurrency, formatMinorForInput } from './currency';
import * as calc from './calc';
import { normalizeConfigNode, normalizeDebtsNode, normalizeMonthNode, schemaStamp } from './schema';

const props = defineProps<{
  vaultPath: string;
}>();

const router = useRouter();
const route = useRoute();
const bus = useEventBus();
const ns = useNodeService();

// --- State ---
const currentView = ref<'transactions' | 'reports' | 'debts' | 'budgets' | 'recurring'>('transactions');
const months = ref<{ id: string, label: string, date: Date, node: any }[]>([]);
const currentMonthIdx = ref(-1);

const configNode = ref<any>(null);
const incomeCategories = ref<Category[]>(toCategories(DEFAULT_INCOME_CATEGORIES));
const expenseCategories = ref<Category[]>(toCategories(DEFAULT_EXPENSE_CATEGORIES));
const accounts = ref<FinanceAccount[]>([...DEFAULT_ACCOUNTS]);

const debtsNode = ref<any>(null);
const debts = ref<Debt[]>([]);

const recurringRules = ref<RecurringRule[]>([]);

const budgets = ref<Budget[]>([]);
const projects = ref<{id: string, title: string}[]>([]);
const people = ref<{id: string, title: string}[]>([]);

const searchQuery = ref('');
const filterType = ref<'all' | 'income' | 'expense' | 'transfer'>('all');
const filterAccount = ref<string>('all');

// Month/Year selector for summary stats
const nowDate = new Date();
const selectedMonthNum = ref(nowDate.getMonth() + 1); // 1-12
const selectedYear = ref(nowDate.getFullYear());

const showTxModal = ref(false);
const editingTx = ref<Transaction | null>(null);
const showSettingsModal = ref(false);
const showAdjustModal = ref(false);
const adjustingAccount = ref<{id: string, name: string, balance: number} | null>(null);

const needsOnboarding = ref(false);
const storageError = ref<string | null>(null);
const loading = ref(true);

const isMobile = ref(window.innerWidth < 768);
const isSidebarOpen = ref(false);
const showSummaryStats = ref(window.innerWidth >= 768);

// --- Computed ---
const currentMonth = computed(() => {
    if (currentMonthIdx.value >= 0 && currentMonthIdx.value < months.value.length) {
        return months.value[currentMonthIdx.value];
    }
    return null;
});

// Collect ALL transactions across all month nodes
const allTransactionsFlat = computed<Transaction[]>(() => {
    const all: Transaction[] = [];
    months.value.forEach(m => {
        if (m.node.properties?.transactions) {
            all.push(...(m.node.properties.transactions as Transaction[]));
        }
    });
    return all;
});

// Transactions for the selected month (for summary stats) — filtered by actual transaction date
const selectedMonthTransactions = computed<Transaction[]>(() =>
    calc.transactionsInMonth(allTransactionsFlat.value, selectedMonthNum.value, selectedYear.value)
);

const monthTotals = computed(() => calc.totals(selectedMonthTransactions.value));
const totalIncome = computed(() => monthTotals.value.income);
const totalExpense = computed(() => monthTotals.value.expense);
const balance = computed(() => monthTotals.value.balance);

const filteredTransactions = computed(() =>
    calc.filterTransactions(selectedMonthTransactions.value, {
        query: searchQuery.value,
        type: filterType.value,
        accountId: filterAccount.value,
    })
);

const groupedTransactions = computed(() => calc.groupByDate(filteredTransactions.value));

const globalNetWorth = computed(() =>
    calc.netWorth(accounts.value, allTransactionsFlat.value, debts.value)
);

/** What the accounts hold, before anything is owed either way. */
const accountsTotal = computed(() => calc.accountsTotal(accounts.value, allTransactionsFlat.value));

/** The two sides of the debts ledger, for showing what net worth is made of. */
const debtSummary = computed(() => calc.debtTotals(debts.value));

// What is still in use, so Settings can refuse to remove it.
const accountUsage = computed(() => calc.accountUsage(allTransactionsFlat.value));
const categoryUsage = computed(() => calc.categoryUsage(allTransactionsFlat.value));
const budgetedCategories = computed(() => [...calc.budgetedCategories(budgets.value)]);

const accountBalances = computed(() =>
    calc.accountBalances(accounts.value, allTransactionsFlat.value)
);



// --- Methods ---
// formatCurrency is imported from ./currency



const getAccountName = (id: string) => {
    const acc = accounts.value.find(a => a.id === id);
    return acc ? acc.name : 'Unknown';
};

/**
 * What a transaction's category is called now.
 *
 * The transaction stores an id; the name can have changed since, and for a
 * category that has been deleted the id is the only record of what it was.
 */
/**
 * The icon that says what kind of account this is.
 *
 * A wallet for everything was the only option before accounts had a kind; a
 * credit card and a savings account are not the same thing and should not look
 * the same. An account nobody has classified keeps the wallet.
 */
const ACCOUNT_ICONS = { cash: Wallet, bank: Landmark, credit: CreditCard, investment: TrendingUp, other: Wallet };
const accountIcon = (id: string) => {
    const type = accounts.value.find(a => a.id === id)?.type;
    return type ? ACCOUNT_ICONS[type] ?? Wallet : Wallet;
};

/**
 * Open a receipt in whatever the system uses for pictures.
 *
 * Rather than a viewer of its own: the operating system already has one, it is
 * the one the user knows, and it can zoom into the line of a receipt in a way a
 * modal in a sidebar cannot.
 */
const openReceipt = async (relPath: string) => {
    try {
        await openPath(`${props.vaultPath}/${relPath}`);
    } catch (e) {
        logger.error('Could not open the receipt', e);
        storageError.value = 'That receipt could not be opened. The file may have been moved.';
    }
};

const displayCategory = (id: string) =>
    categoryName([...incomeCategories.value, ...expenseCategories.value], id);

const getPersonName = (id: string) => {
    const p = people.value.find(p => p.id === id);
    return p ? p.title : 'Unknown';
};

const goToPerson = (id: string) => {
    router.push({ name: 'people', query: { id } });
};

const ensureCurrentMonthNodeExists = async () => {
    const now = new Date();
    const mm = (now.getMonth() + 1).toString().padStart(2, '0');
    const yyyy = now.getFullYear();
    const expectedId = monthNodePath(new Date(Number(yyyy), Number(mm) - 1, 1));
    
    const existing = months.value.find(m => m.id === expectedId);
    if (!existing) {
        // Create new node
        const nodeProps = { transactions: [], ...schemaStamp() };
        try {
            await ns.writeNode({
                relPath: expectedId,
                title: `Month ${mm}/${yyyy}`,
                nodeType: 'finance_month',
                properties: nodeProps,
                content: '',
                eventType: 'created',
                silent: true,
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

/**
 * The month a path names, put back in the list without re-reading the vault.
 *
 * `loadData` fetches every month, every debt, every project and every person.
 * That is the right thing on open and far too much when one transaction was
 * saved — or when a sync delivered one month and four separate bus events fired
 * because of it. A ledger with five years in it re-parsed all sixty months
 * every time.
 */
const reloadMonth = async (id: string) => {
    const node = await ns.getNode(id);
    if (!node) {
        // Gone, which is what a month emptied and removed looks like.
        months.value = months.value.filter(m => m.id !== id);
        return;
    }

    normalizeMonthNode(node, currentCurrency.value);

    const match = id.match(/(\d{4})-(\d{2})\.json/);
    const date = match
        ? new Date(parseInt(match[1]), parseInt(match[2]) - 1, 1)
        : new Date();

    const entry = { id, label: node.title, date, node };
    const existing = months.value.findIndex(m => m.id === id);

    if (existing >= 0) {
        months.value[existing] = entry;
        return;
    }

    // A month that did not exist a moment ago. Keep the list chronological and
    // keep looking at whatever was being looked at.
    const selected = currentMonth.value?.id;
    months.value = [...months.value, entry].sort((a, b) => a.date.getTime() - b.date.getTime());
    if (selected) {
        currentMonthIdx.value = months.value.findIndex(m => m.id === selected);
    }
};

/** Whether a path is a month of the ledger rather than the config or debts. */
const isMonthPath = (id: string) => /Finance[\\/]\d{4}-\d{2}\.json$/.test(id);

const loadData = async () => {
    if (!props.vaultPath) return;
    loading.value = true;
    try {
        // Load config
        const configs: any[] = await ns.getNodes('finance_config');
        if (configs.length > 0) {
            configNode.value = configs[0];
            if (configNode.value.properties) {
                // The currency first: everything below is read in terms of it.
                currentCurrency.value = configNode.value.properties.currency || 'USD';
                allowRateLookup.value = configNode.value.properties.allowRateLookup === true;

                // A vault the storage repair could not reach still holds whole
                // units. Scaling it here is what keeps a failed migration from
                // showing every amount at a hundredth of its value.
                normalizeConfigNode(configNode.value, currentCurrency.value);

                // Everything the old shapes needed doing to them is done by
                // `repairStorageOnce` before this runs. What is left here is
                // reading, plus the defaults a vault written by an older
                // version may still be missing on disk.
                // `toCategories` reads both shapes, so a vault the repair
                // could not reach still shows its categories rather than a
                // column of blanks.
                const storedIncome = configNode.value.properties.incomeCategories;
                const storedExpense = configNode.value.properties.expenseCategories;
                incomeCategories.value = storedIncome
                    ? toCategories(storedIncome)
                    : toCategories(DEFAULT_INCOME_CATEGORIES);
                expenseCategories.value = storedExpense
                    ? toCategories(storedExpense)
                    : toCategories(DEFAULT_EXPENSE_CATEGORIES);
                budgets.value = configNode.value.properties.budgets || [];

                // The repair pass adds these to the file. This keeps the two
                // ledger categories on screen even on a vault where it could
                // not run — without them the debts screen has nothing to file
                // a repayment under.
                SYSTEM_INCOME_CATEGORIES.forEach(sysCat => {
                    if (!incomeCategories.value.some(c => c.id === sysCat)) {
                        incomeCategories.value.push({ id: sysCat, name: sysCat });
                    }
                });
                SYSTEM_EXPENSE_CATEGORIES.forEach(sysCat => {
                    if (!expenseCategories.value.some(c => c.id === sysCat)) {
                        expenseCategories.value.push({ id: sysCat, name: sysCat });
                    }
                });

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
        const monthNodes: any[] = await ns.getNodes('finance_month');

        months.value = monthNodes.map(node => {
            normalizeMonthNode(node, currentCurrency.value);

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
        const debtNodes: any[] = await ns.getNodes('finance_debts');
        if (debtNodes.length > 0) {
            debtsNode.value = debtNodes[0];
            normalizeDebtsNode(debtsNode.value, currentCurrency.value);
            debts.value = debtsNode.value.properties.debts || [];
        } else {
            // Create default debts node
            const newProps = { debts: [], ...schemaStamp() };
            try {
                await ns.writeNode({
                    relPath: 'Finance/Debts.json',
                    title: 'Debts Ledger',
                    nodeType: 'finance_debts',
                    properties: newProps,
                    content: '',
                    eventType: 'created',
                    silent: true,
                });
                const loaded: any[] = await ns.getNodes('finance_debts');
                if (loaded.length > 0) {
                    debtsNode.value = loaded[0];
                    debts.value = debtsNode.value.properties.debts || [];
                }
            } catch(e) {
                logger.error('Failed to create default debts node', e);
            }
        }
        
        // Load the repeating rules. Absent is normal: a vault where nobody has
        // set one up has no file, and creating an empty one would be noise.
        try {
            const recurringNodes: any[] = await ns.getNodes('finance_recurring');
            recurringRules.value = recurringNodes[0]?.properties?.rules ?? [];
        } catch (e) {
            logger.error('Failed to load recurring rules', e);
            recurringRules.value = [];
        }

        // Load projects for linking
        try {
            const projectNodes: any[] = await ns.getNodes('project');
            projects.value = projectNodes
                .filter(n => n.properties?.status !== 'completed' && n.properties?.status !== 'archived')
                .map(n => ({ id: n.id, title: n.title }));
        } catch(e) {
            logger.error('Failed to load projects', e);
        }
        
        // Load people for linking
        try {
            const peopleNodes: any[] = await ns.getNodes('person');
            people.value = peopleNodes.map(n => ({ id: n.id, title: n.title }));
        } catch(e) {
            logger.error('Failed to load people', e);
        }
        
    } catch (e) {
        logger.error('Failed to load finance data', e);
    } finally {
        loading.value = false;
    }
};



/**
 * Write out whatever the repeating rules owe.
 *
 * Runs after every load, and catches up rather than only producing what has
 * fallen due since last time: a vault nobody opened for three months comes
 * back with three months of rent in it.
 *
 * Safe to run again because it is not additive. Each occurrence's id is its
 * rule and its date, so a second pass upserts the same rows — two devices, two
 * launches, one rent payment.
 */
const materialiseRecurring = async () => {
    if (recurringRules.value.length === 0) return;

    const months = pendingByMonth(recurringRules.value, todayStr());
    if (months.length === 0) return;

    try {
        for (const month of months) {
            await writeFinanceRows({
                relPath: month.relPath,
                title: month.title,
                nodeType: 'finance_month',
                upserts: month.transactions as unknown as Record<string, unknown>[],
            });
        }
        for (const month of months) await reloadMonth(month.relPath);
    } catch (e) {
        logger.error('Failed to record repeating transactions', e);
        storageError.value = 'Some repeating transactions could not be recorded. Finance will try again next time you open it.';
    }
};

/** Save the repeating rules, row by row like everything else. */
const saveRules = async (updated: RecurringRule[]) => {
    const changes = rowChanges(recurringRules.value, updated);
    recurringRules.value = updated;

    try {
        await writeFinanceRows({
            relPath: RECURRING_PATH,
            title: 'Repeating Transactions',
            nodeType: 'finance_recurring',
            upserts: changes.upserts as unknown as Record<string, unknown>[],
            removals: changes.removals,
        });
        bus.emit('node:updated', { nodeType: 'finance_recurring', id: RECURRING_PATH, title: 'Repeating Transactions' });
        await materialiseRecurring();
    } catch (e) {
        logger.error('Failed to save repeating rules', e);
        storageError.value = 'That repeating transaction could not be saved. Please try again.';
    }
};

const saveDebts = async (updatedDebts: Debt[]) => {
    if (!debtsNode.value) return;

    // The debts screen hands over the whole list. Sending it back whole would
    // overwrite a debt recorded on another device between this screen's last
    // read and now; sending only what this list changed cannot.
    const changes = rowChanges(debts.value, updatedDebts);
    debts.value = updatedDebts;

    try {
        await writeFinanceRows({
            relPath: debtsNode.value.id,
            title: debtsNode.value.title,
            nodeType: 'finance_debts',
            upserts: changes.upserts as unknown as Record<string, unknown>[],
            removals: changes.removals,
        });
        bus.emit('node:updated', { nodeType: 'finance_debts', id: debtsNode.value.id, title: debtsNode.value.title });
    } catch (e) {
        logger.error('Failed to save debts', e);
        storageError.value = 'The debts ledger could not be saved. Please try again.';
    }
};

const openAddTx = () => {
    editingTx.value = null;
    editingRule.value = null;
    defaultRecurrence.value = 'none';
    showTxModal.value = true;
};

const openEditTx = (tx: Transaction) => {
    editingTx.value = tx;
    editingRule.value = null;
    defaultRecurrence.value = 'none';
    showTxModal.value = true;
};

// The transaction dialog does double duty: the same form plus one question,
// rather than a second screen for "the same thing but every month".
const editingRule = ref<RecurringRule | null>(null);
const defaultRecurrence = ref('none');

const openAddRule = () => {
    editingTx.value = null;
    editingRule.value = null;
    defaultRecurrence.value = 'monthly';
    showTxModal.value = true;
};

const openEditRule = (rule: RecurringRule) => {
    editingTx.value = null;
    editingRule.value = rule;
    defaultRecurrence.value = rule.recurrence;
    showTxModal.value = true;
};

/** Save one rule, whether it is new or an edit of an existing one. */
const saveRule = async (rule: RecurringRule) => {
    const others = recurringRules.value.filter(r => r.id !== rule.id);
    showTxModal.value = false;
    await saveRules([...others, rule]);
};

const handleBalanceAdjust = async (diff: number) => {
    if (!adjustingAccount.value) return;
    
    // The category an adjustment is filed under, added the first time one is
    // made. Its id is its name, matching every category that predates ids — so
    // an adjustment made before this still lands in the same place.
    const ADJUSTMENT = 'Balance Adjustment';
    let needSave = false;
    if (!expenseCategories.value.some(c => c.id === ADJUSTMENT)) {
        expenseCategories.value.push({ id: ADJUSTMENT, name: ADJUSTMENT });
        needSave = true;
    }
    if (!incomeCategories.value.some(c => c.id === ADJUSTMENT)) {
        incomeCategories.value.push({ id: ADJUSTMENT, name: ADJUSTMENT });
        needSave = true;
    }
    if (needSave) {
        await saveConfig({ incomeCategories: incomeCategories.value, expenseCategories: expenseCategories.value, accounts: accounts.value });
    }

    const tx: Transaction = {
        id: `tx-${Date.now()}-${Math.floor(Math.random()*1000)}`,
        type: diff > 0 ? 'income' : 'expense',
        amount: Math.abs(diff),
        category: ADJUSTMENT,
        accountId: adjustingAccount.value.id,
        date: new Date().toISOString(),
        note: 'Automatic balance adjustment'
    };

    await saveTransaction(tx);
};

const saveTransaction = async (tx: Transaction) => {
    // Recording a debt is something the user does on the Debts screen, which
    // asks who it is with and when it is due. This used to guess from the
    // category name instead — anything containing "vay", "lend", "debt" — and
    // invent a debt owed to whatever was in the note, or to "Anonymous". A
    // category called "Calendar" was enough to create one.

    // A transaction belongs to the month its own date names, not to the month
    // being looked at. Editing the date moves it, which means taking it out of
    // wherever it used to be.
    const date = new Date(tx.date);
    const targetId = monthNodePath(date);
    const previous = months.value.find(m =>
        ((m.node.properties?.transactions as Transaction[]) || []).some(t => t.id === tx.id)
    );

    try {
        await writeFinanceRows({
            relPath: targetId,
            title: monthNodeTitle(date),
            nodeType: 'finance_month',
            upserts: [tx as unknown as Record<string, unknown>],
        });

        if (previous && previous.id !== targetId) {
            await writeFinanceRows({
                relPath: previous.id,
                title: previous.label,
                nodeType: 'finance_month',
                removals: [tx.id],
            });
        }

        showTxModal.value = false;

        // Read the month back rather than patch memory: the file now holds
        // whatever arrived from other devices as well as this row.
        await reloadMonth(targetId);
        if (previous && previous.id !== targetId) await reloadMonth(previous.id);

        const targetIdx = months.value.findIndex(m => m.id === targetId);
        if (targetIdx >= 0) {
            currentMonthIdx.value = targetIdx;
        }

        // `upsert_finance_rows` does not go through `writeNode`, so the event
        // other apps listen for has to be raised here.
        bus.emit('node:updated', { nodeType: 'finance_month', id: targetId, title: monthNodeTitle(date) });
    } catch (e) {
        logger.error('Failed to save transaction', e);
        storageError.value = 'That transaction could not be saved. Nothing was changed — please try again.';
    }
};

const deleteTransaction = async (txId: string) => {
    // Which month holds it, so the removal goes to the right file.
    const holder = months.value.find(m =>
        ((m.node.properties?.transactions as Transaction[]) || []).some(t => t.id === txId)
    );
    if (!holder) return;

    const confirmed = await ask('This transaction will be permanently removed. This action cannot be undone.', {
        title: 'Delete transaction?',
        kind: 'warning',
        okLabel: 'Delete',
        cancelLabel: 'Cancel'
    });

    if (!confirmed) return;

    try {
        await writeFinanceRows({
            relPath: holder.id,
            title: holder.label,
            nodeType: 'finance_month',
            removals: [txId],
        });
        showTxModal.value = false;
        await reloadMonth(holder.id);
        bus.emit('node:updated', { nodeType: 'finance_month', id: holder.id, title: holder.label });
    } catch (e) {
        logger.error('Failed to delete transaction', e);
        storageError.value = 'That transaction could not be deleted. Nothing was changed — please try again.';
    }
};

// ---------------------------------------------------------------------------
// Import and export
// ---------------------------------------------------------------------------

/**
 * The whole ledger, in a file anything can open.
 *
 * The app's own description promises no vendor lock-in, and until this the only
 * way out was to read the JSON by hand. Every transaction goes, not the month
 * being looked at: an export somebody has to run twelve times is not an exit.
 */
const exportLedger = async () => {
    try {
        const path = await saveFileDialog({
            defaultPath: exportFilename(),
            filters: [{ name: 'CSV', extensions: ['csv'] }],
        });
        if (!path) return;

        const text = exportCsv(allTransactionsFlat.value, {
            accounts: accounts.value,
            categories: [...incomeCategories.value, ...expenseCategories.value],
            currency: currentCurrency.value,
        });
        await writeTextFile(path, text);
    } catch (e) {
        logger.error('Failed to export the ledger', e);
        storageError.value = 'The ledger could not be exported. Nothing was changed.';
    }
};

// What an import is about to do, held while the user looks at it.
const showImportModal = ref(false);
const importFileName = ref('');
const importHeader = ref<string[]>([]);
const importMap = ref<ColumnMap | null>(null);
const importMissing = ref<string[]>([]);
const importResult = ref<ImportResult | null>(null);

/**
 * Read a file and work out what importing it would do — without doing it.
 *
 * Import is the one thing here that puts somebody else's data into the user's
 * ledger, so it shows its working first. Nothing is written until the preview
 * is confirmed.
 */
const chooseImportFile = async () => {
    try {
        const picked = await openFileDialog({
            multiple: false,
            filters: [{ name: 'CSV', extensions: ['csv'] }],
        });
        const path = Array.isArray(picked) ? picked[0] : picked;
        if (!path) return;

        const rows = parseCsv(await readTextFile(path));
        if (rows.length === 0) {
            storageError.value = 'That file has nothing in it.';
            return;
        }

        const [header, ...body] = rows;
        const map = matchColumns(header);

        importFileName.value = path.split(/[\\/]/).pop() ?? path;
        importHeader.value = header;
        importMap.value = map;
        importMissing.value = missingColumns(map);
        importResult.value = missingColumns(map).length > 0
            ? { ready: [], duplicates: 0, problems: [] }
            : importRows(body, map, {
                accounts: accounts.value,
                categories: [...incomeCategories.value, ...expenseCategories.value],
                currency: currentCurrency.value,
                existing: allTransactionsFlat.value,
            });
        showImportModal.value = true;
    } catch (e) {
        logger.error('Failed to read the file', e);
        storageError.value = 'That file could not be read.';
    }
};

/** Write what the preview showed, one month at a time. */
const confirmImport = async () => {
    const rows = importResult.value?.ready ?? [];
    showImportModal.value = false;
    if (rows.length === 0) return;

    // Grouped by month, because writing is per file.
    const byMonth = new Map<string, Transaction[]>();
    for (const tx of rows) {
        const relPath = monthNodePath(new Date(tx.date));
        byMonth.set(relPath, [...(byMonth.get(relPath) ?? []), tx]);
    }

    try {
        for (const [relPath, transactions] of byMonth) {
            await writeFinanceRows({
                relPath,
                title: monthNodeTitle(new Date(transactions[0].date)),
                nodeType: 'finance_month',
                upserts: transactions as unknown as Record<string, unknown>[],
            });
        }
        await loadData();
    } catch (e) {
        logger.error('Failed to import transactions', e);
        storageError.value = 'Some transactions could not be imported. Nothing else was changed.';
    }
};

const saveConfig = async (config: { incomeCategories: Category[], expenseCategories: Category[], accounts: FinanceAccount[], budgets?: Budget[], currency?: string }) => {
    if (config.budgets) {
        budgets.value = config.budgets;
    }
    if (config.currency) {
        currentCurrency.value = config.currency;
    }
    
    const propsToSave = {
        incomeCategories: config.incomeCategories,
        expenseCategories: config.expenseCategories,
        accounts: config.accounts,
        budgets: budgets.value,
        currency: currentCurrency.value,
        allowRateLookup: allowRateLookup.value,
        ...schemaStamp()
    };
    
    try {
        await ns.writeNode({
            relPath: 'Finance/Config.json',
            title: 'Finance Config',
            nodeType: 'finance_config',
            properties: propsToSave,
            content: '',
        });
        await loadData();
    } catch (e) {
        logger.error('Failed to save config', e);
    }
};

const finishOnboarding = async (config: { incomeCategories: Category[], expenseCategories: Category[], accounts: FinanceAccount[], currency: string }) => {
    loading.value = true;
    await saveConfig(config);
};

const handleAddCategory = async (payload: { type: 'income' | 'expense', name: string }) => {
    const name = payload.name.trim();
    if (!name) return;

    const config = {
        incomeCategories: [...incomeCategories.value],
        expenseCategories: [...expenseCategories.value],
        accounts: accounts.value,
        budgets: budgets.value,
        currency: currentCurrency.value
    };

    const list = payload.type === 'income' ? config.incomeCategories : config.expenseCategories;
    if (list.some(c => c.name.trim().toLowerCase() === name.toLowerCase())) return;

    list.push({ id: newCategoryId(), name });

    await saveConfig(config);
    // Refresh the local lists so the dialog sees the new category without
    // waiting for the reload.
    incomeCategories.value = config.incomeCategories;
    expenseCategories.value = config.expenseCategories;
};

const openMonthById = async (id: string) => {
    if (months.value.length === 0) await loadData();
    const idx = months.value.findIndex(m => m.id === id);
    if (idx >= 0) currentMonthIdx.value = idx;
};

const handleRouteQuery = () => {
    if (route.query.view === 'debts') {
        currentView.value = 'debts';
    } else if (route.query.txId) {
        currentView.value = 'transactions';
        const txId = route.query.txId as string;
        for (let i = 0; i < months.value.length; i++) {
            const txs = months.value[i].node.properties?.transactions || [];
            const tx = txs.find((t: any) => t.id === txId);
            if (tx) {
                currentMonthIdx.value = i;
                openEditTx(tx);
                break;
            }
        }
    }
    // Clear query so it doesn't trigger on refresh
    if (route.query.txId || route.query.view) {
        router.replace({ query: {} });
    }
};

watch(() => route.query, () => {
    if (route.query.txId || route.query.view) {
        handleRouteQuery();
    }
});

// Debounce wrapper: coalesces rapid-fire events (e.g. node:updated + vault:file-modified)
let _debounceTimer: ReturnType<typeof setTimeout> | null = null;
const debouncedLoad = (fn: () => void, ms = 300) => {
    if (_debounceTimer) clearTimeout(_debounceTimer);
    _debounceTimer = setTimeout(fn, ms);
};

// Lifecycle
/**
 * The one-time repair that brings an older vault up to the shape this screen
 * reads. The pass itself is `repairFinanceStorage`; see
 * `src-tauri/src/utils/finance_storage.rs` for the transforms and
 * `commands/migration.rs` for why they do not go through the ordinary save
 * path.
 *
 * This used to happen inside `loadData`, on every launch, writing back through
 * the ordinary save path and swallowing failures into the log. The screen then
 * drew the repaired copy from memory while the disk still held the old one, so
 * a vault that could not be written looked migrated until the next launch.
 */
const repairStorageOnce = async () => {
    try {
        const failed = await repairFinanceStorage(props.vaultPath);
        if (failed > 0) {
            // Say so rather than carry on looking fine. The files that failed
            // are untouched, not half-written, so the ledger is still readable
            // — it is the repair that did not happen.
            storageError.value = `${failed} Finance file(s) could not be updated. Your data is unchanged; Finance will try again next time you open it.`;
        }
    } catch (e) {
        logger.error('Finance storage repair failed', e);
        storageError.value = 'Finance could not update its stored files. Your data is unchanged; it will try again next time you open Finance.';
    }
};

onMounted(async () => {
    // Before the first read, so the ledger is never drawn from the old shape
    // and then redrawn from the new one.
    await repairStorageOnce();
    await loadData();
    // After the load, so the rules are known — and before the user looks at a
    // month that is missing this month's rent.
    await materialiseRecurring();
    handleRouteQuery();
    
    bus.on('vault:file-modified', ({ paths }) => {
        // A sync that touched only months can refresh only those months.
        // Anything else — the config, the debts ledger, a path we cannot read —
        // still needs the full pass.
        const finance = (paths || []).filter(p => p.includes('Finance'));
        if (finance.length > 0 && finance.every(isMonthPath)) {
            finance.forEach(p => reloadMonth(p.replace(/\\/g, '/')));
            return;
        }
        debouncedLoad(() => loadData());
    });

    const handleResize = () => { isMobile.value = window.innerWidth < 768; };
    window.addEventListener('resize', handleResize);

    bus.on('vault:file-created-deleted', () => {
        debouncedLoad(() => loadData());
    });

    bus.on('vault:sync-completed', () => {
        debouncedLoad(() => loadData());
    });

    // Cross-app: refresh when finance data changes from other apps (e.g., TaskApp saves a finance_month)
    bus.on('node:created', ({ nodeType, id }) => {
        if (nodeType === 'finance_month' && id && isMonthPath(id)) {
            reloadMonth(id);
            return;
        }
        if (nodeType === 'finance_config' || nodeType === 'finance_debts') debouncedLoad(() => loadData());
    });

    bus.on('node:updated', ({ nodeType, id }) => {
        if (nodeType === 'finance_month' && id && isMonthPath(id)) {
            reloadMonth(id);
            return;
        }
        if (nodeType === 'finance_config' || nodeType === 'finance_debts') debouncedLoad(() => loadData());
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

      <!-- A repair that could not finish. Said out loud, because the ledger
           looks perfectly normal either way. -->
      <div
          v-if="storageError"
          class="mx-4 md:mx-6 mt-4 flex items-start gap-3 rounded-xl border border-amber-300 bg-amber-50 px-4 py-3 text-sm text-amber-900 dark:border-amber-700/60 dark:bg-amber-950/40 dark:text-amber-200"
          role="status"
      >
          <AlertTriangle class="w-5 h-5 shrink-0 mt-0.5" />
          <p class="flex-1">{{ storageError }}</p>
          <button
              @click="storageError = null"
              class="shrink-0 rounded-lg p-1 hover:bg-amber-200/60 dark:hover:bg-amber-900/60 transition-colors"
              aria-label="Dismiss"
          >
              <X class="w-4 h-4" />
          </button>
      </div>

      <!-- Topbar -->
      <div v-else class="flex items-center justify-between p-4 md:p-6 shrink-0 relative z-10">
          <div class="flex items-center gap-3">
              <button @click="isSidebarOpen = true" class="md:hidden p-1.5 -ml-1 text-gray-500 hover:text-blue-500 rounded-lg hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors" aria-label="Is Sidebar Open = true">
                  <PanelLeft class="w-6 h-6" />
              </button>
              <div>
                  <h1 class="text-xl md:text-2xl font-bold flex items-center gap-2">
                      <NavButtons class="hidden md:flex" />
                      <Wallet class="w-6 h-6 text-blue-500 hidden md:block" />
                      Finance
                  </h1>
                  <p class="text-xs md:text-sm text-gray-500 dark:text-gray-400">{{ $t('finance.subtitle') }}</p>
              </div>
          </div>
          
          <div class="flex items-center gap-2 md:gap-3">
              <button @click="showSettingsModal = true" class="p-2 md:p-2.5 rounded-xl bg-surface dark:bg-surface-dark border border-border dark:border-border-dark text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors shadow-sm" aria-label="Show Settings Modal = true">
                  <Settings class="w-5 h-5" />
              </button>
              <button @click="openAddTx" class="hidden md:flex items-center gap-2 px-4 py-2.5 rounded-xl bg-blue-500 text-white hover:bg-blue-600 transition-colors shadow-sm font-medium">
                  <Plus class="w-5 h-5" />
                  <span>{{ $t('finance.add_transaction') }}</span>
              </button>
          </div>
      </div>

      <!-- Main Content Area -->
      <div v-if="currentMonth || currentView === 'reports' || currentView === 'debts'" class="flex-1 flex gap-6 px-2 md:px-6 pb-2 md:pb-6 overflow-hidden relative">
          
          <!-- Backdrop -->
          <div v-if="isMobile && isSidebarOpen" class="md:hidden absolute inset-0 bg-black/20 dark:bg-black/40 z-[48]" @click="isSidebarOpen = false" />

          <!-- Sidebar (Global Context) -->
          <div v-show="!isMobile || isSidebarOpen" class="w-[280px] flex flex-col gap-6 shrink-0 overflow-y-auto hidden-scrollbar pr-2 absolute md:relative z-[49] h-full bg-base dark:bg-base-dark md:bg-transparent shadow-lg md:shadow-none p-4 md:p-0 pt-0 md:pt-0">
              
              <!-- Navigation Menu -->
              <div class="flex flex-col gap-1">
                  <button 
                      @click="currentView = 'transactions'; if(isMobile) isSidebarOpen = false;" 
                      :class="['flex items-center gap-3 px-4 py-2.5 rounded-xl font-medium text-sm transition-colors w-full text-left', currentView === 'transactions' ? 'bg-blue-50 text-blue-600 dark:bg-blue-900/20 dark:text-blue-400' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800']"
                  >
                      <Wallet class="w-5 h-5" />
                      Ledger
                  </button>
                  <button 
                      @click="currentView = 'reports'; if(isMobile) isSidebarOpen = false;" 
                      :class="['flex items-center gap-3 px-4 py-2.5 rounded-xl font-medium text-sm transition-colors w-full text-left', currentView === 'reports' ? 'bg-blue-50 text-blue-600 dark:bg-blue-900/20 dark:text-blue-400' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800']"
                  >
                      <PieChart class="w-5 h-5" />
                      Reports & Analytics
                  </button>
                  <button 
                      @click="currentView = 'debts'; if(isMobile) isSidebarOpen = false;" 
                      :class="['flex items-center gap-3 px-4 py-2.5 rounded-xl font-medium text-sm transition-colors w-full text-left', currentView === 'debts' ? 'bg-blue-50 text-blue-600 dark:bg-blue-900/20 dark:text-blue-400' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800']"
                  >
                      <BookOpen class="w-5 h-5" />
                      Debts
                  </button>
                  <button 
                      @click="currentView = 'budgets'; if(isMobile) isSidebarOpen = false;" 
                      :class="['flex items-center gap-3 px-4 py-2.5 rounded-xl font-medium text-sm transition-colors w-full text-left', currentView === 'budgets' ? 'bg-blue-50 text-blue-600 dark:bg-blue-900/20 dark:text-blue-400' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800']"
                  >
                      <Target class="w-5 h-5" />
                      Budgets
                  </button>
                  <button
                      @click="currentView = 'recurring'; if(isMobile) isSidebarOpen = false;"
                      :class="['flex items-center gap-3 px-4 py-2.5 rounded-xl font-medium text-sm transition-colors w-full text-left', currentView === 'recurring' ? 'bg-blue-50 text-blue-600 dark:bg-blue-900/20 dark:text-blue-400' : 'text-gray-600 dark:text-gray-400 hover:bg-gray-100 dark:hover:bg-gray-800']"
                  >
                      <Repeat class="w-5 h-5" />
                      {{ $t('finance.recurring') }}
                  </button>
              </div>

              <!-- Global Net Worth -->
              <div class="bg-gradient-to-br from-blue-500 to-indigo-600 rounded-2xl p-5 text-white shadow-lg relative overflow-hidden shrink-0">
                  <div class="absolute right-0 top-0 opacity-10 pointer-events-none">
                      <Wallet class="w-32 h-32 -mt-4 -mr-4" />
                  </div>
                  <p class="text-blue-100 text-sm font-medium mb-1">{{ $t('finance.total_net_worth') }}</p>
                  <h2 class="text-3xl font-bold tracking-tight">{{ formatCurrency(globalNetWorth) }}</h2>
                  <!-- What the one figure is made of. Net worth now nets off
                       what is owed either way, and a headline that moves when
                       a debt is recorded should say why. -->
                  <div v-if="debtSummary.receivable > 0 || debtSummary.payable > 0" class="flex flex-wrap gap-x-3 gap-y-0.5 mt-2 text-[11px] text-blue-100/90">
                      <span>{{ $t('finance.in_accounts') }} {{ formatCurrency(accountsTotal) }}</span>
                      <span v-if="debtSummary.receivable > 0">+ {{ $t('finance.owed_to_you') }} {{ formatCurrency(debtSummary.receivable) }}</span>
                      <span v-if="debtSummary.payable > 0">− {{ $t('finance.you_owe') }} {{ formatCurrency(debtSummary.payable) }}</span>
                  </div>
              </div>
              
              <!-- Account Balances -->
              <div class="flex flex-col gap-2">
                  <h3 class="font-bold text-sm text-gray-500 dark:text-gray-400 uppercase tracking-wider pl-2">{{ $t('finance.my_accounts') }}</h3>
                  <div class="flex flex-col gap-1.5">
                      <div v-for="acc in accountBalances" :key="acc.id" class="flex items-center gap-3 p-3 rounded-xl hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors group relative">
                          <div class="p-2.5 rounded-xl bg-white dark:bg-gray-900 text-blue-500 shadow-sm border border-gray-100 dark:border-gray-800 shrink-0">
                              <component :is="accountIcon(acc.id)" class="w-5 h-5" />
                          </div>
                          <div class="flex flex-col flex-1 min-w-0">
                              <span class="text-sm font-medium text-gray-500 dark:text-gray-400 truncate">{{ acc.name }}</span>
                              <span class="text-base font-bold text-text dark:text-text-dark truncate">{{ formatCurrency(acc.balance) }}</span>
                          </div>
                          <button @click="adjustingAccount = acc; showAdjustModal = true" class="absolute right-3 p-1.5 text-gray-400 hover:text-blue-500 opacity-0 group-hover:opacity-100 transition-opacity bg-white dark:bg-gray-800 rounded-lg shadow-sm border border-gray-100 dark:border-gray-700" title="Adjust Balance">
                              <Scale class="w-4 h-4" />
                          </button>
                      </div>
                  </div>
              </div>
          </div>

          <!-- Main Content (Transactions) -->
          <div v-if="currentView === 'transactions'" class="flex-1 flex flex-col gap-6 overflow-hidden">
              
              <!-- Monthly Dashboard Header -->
              <div class="flex flex-col md:flex-row gap-4 shrink-0 px-2 md:px-0">
                  <!-- Month Selector -->
                  <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl p-4 shadow-sm flex flex-col md:items-center justify-center gap-3 shrink-0">
                      <div class="flex items-center justify-between md:justify-center w-full">
                          <span class="text-xs text-gray-500 font-medium uppercase tracking-wider mr-1">Summary for</span>
                          <div class="flex items-center gap-2">
                              <div class="relative">
                                  <select v-model.number="selectedMonthNum" class="appearance-none bg-gray-100 dark:bg-gray-800 hover:bg-gray-200 dark:hover:bg-gray-700 border-none rounded-xl pl-3 pr-7 py-2 text-base md:text-lg font-bold text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500 cursor-pointer transition-colors">
                                      <option v-for="m in 12" :key="m" :value="m">{{ m.toString().padStart(2, '0') }}</option>
                                  </select>
                                  <ChevronDown class="w-3.5 h-3.5 text-gray-500 absolute right-2 top-1/2 -translate-y-1/2 pointer-events-none" />
                              </div>
                              <span class="text-lg font-bold text-gray-400">/</span>
                              <div class="relative">
                                  <select v-model.number="selectedYear" class="appearance-none bg-gray-100 dark:bg-gray-800 hover:bg-gray-200 dark:hover:bg-gray-700 border-none rounded-xl pl-3 pr-7 py-2 text-base md:text-lg font-bold text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500 cursor-pointer transition-colors">
                                      <option v-for="y in 10" :key="y" :value="nowDate.getFullYear() - 5 + y">{{ nowDate.getFullYear() - 5 + y }}</option>
                                  </select>
                                  <ChevronDown class="w-3.5 h-3.5 text-gray-500 absolute right-2 top-1/2 -translate-y-1/2 pointer-events-none" />
                              </div>
                          </div>
                      </div>
                      
                      <!-- Toggle Summary Button -->
                      <button @click="showSummaryStats = !showSummaryStats" class="md:hidden flex items-center justify-center gap-2 w-full px-4 py-2 bg-gray-50 dark:bg-gray-800/50 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-xl transition-colors border border-border dark:border-border-dark">
                          <span class="text-sm font-medium text-gray-600 dark:text-gray-300">
                              {{ showSummaryStats ? 'Hide Summary' : 'Show Summary' }}
                          </span>
                          <ChevronDown :class="['w-4 h-4 text-gray-500 transition-transform', showSummaryStats ? 'rotate-180' : '']" />
                      </button>
                  </div>
                  
                  <!-- Summary Stats -->
                  <div v-show="showSummaryStats" class="flex-1 grid grid-cols-1 md:grid-cols-3 gap-2 md:gap-4">
                      <!-- Income -->
                      <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl p-4 shadow-sm flex flex-col justify-center">
                          <div class="flex items-center gap-2 mb-1">
                              <div class="w-2 h-2 rounded-full bg-green-500"></div>
                              <span class="text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">Total Income</span>
                          </div>
                          <p class="text-xl font-bold text-green-600 dark:text-green-400">{{ formatCurrency(totalIncome) }}</p>
                      </div>
                      <!-- Expense -->
                      <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl p-4 shadow-sm flex flex-col justify-center">
                          <div class="flex items-center gap-2 mb-1">
                              <div class="w-2 h-2 rounded-full bg-red-500"></div>
                              <span class="text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">Total Expense</span>
                          </div>
                          <p class="text-lg md:text-xl font-bold text-red-600 dark:text-red-400">{{ formatCurrency(totalExpense) }}</p>
                      </div>
                      <!-- Net Flow -->
                      <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl p-4 shadow-sm flex flex-col justify-center">
                          <div class="flex items-center gap-2 mb-1">
                              <div class="w-2 h-2 rounded-full bg-blue-500"></div>
                              <span class="text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">Monthly Balance</span>
                          </div>
                          <p :class="['text-lg md:text-xl font-bold', balance >= 0 ? 'text-text dark:text-text-dark' : 'text-red-500']">{{ balance > 0 ? '+' : '' }}{{ formatCurrency(balance) }}</p>
                      </div>
                  </div>
              </div>

              <!-- Transaction List -->
              <div class="flex-1 bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl shadow-sm flex flex-col overflow-hidden mx-2 md:mx-0">
              <div class="p-3 md:p-4 border-b border-border dark:border-border-dark bg-gray-50/50 dark:bg-gray-800/50 shrink-0 flex flex-col gap-3">
                      <div class="flex items-center justify-between">
                      <h3 class="font-bold text-lg text-text dark:text-text-dark">{{ $t('finance.transaction_history') }}</h3>
                  </div>
                  <!-- Search and Filters -->
                  <div class="flex items-center gap-2">
                      <div class="relative flex-1">
                          <Search class="w-4 h-4 absolute left-3 top-1/2 -translate-y-1/2 text-gray-400" />
                          <input v-model="searchQuery" type="text" :placeholder="$t('finance.search')" class="w-full pl-9 pr-3 py-1.5 bg-white dark:bg-gray-900 border border-border dark:border-border-dark rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 transition-shadow" />
                      </div>
                      <div class="relative">
                          <select v-model="filterType" class="appearance-none bg-gray-100 dark:bg-gray-800 hover:bg-gray-200 dark:hover:bg-gray-700 border-none rounded-xl pl-3 pr-8 py-1.5 text-sm font-medium text-gray-700 dark:text-gray-300 focus:outline-none focus:ring-2 focus:ring-blue-500 cursor-pointer transition-colors">
                              <option value="all">All</option>
                              <option value="income">Income</option>
                              <option value="expense">Expense</option>
                              <option value="transfer">Transfer</option>
                          </select>
                          <ChevronDown class="w-4 h-4 text-gray-500 absolute right-2.5 top-1/2 -translate-y-1/2 pointer-events-none" />
                      </div>
                      
                      <div class="relative">
                          <select v-model="filterAccount" class="appearance-none bg-gray-100 dark:bg-gray-800 hover:bg-gray-200 dark:hover:bg-gray-700 border-none rounded-xl pl-3 pr-8 py-1.5 text-sm font-medium text-gray-700 dark:text-gray-300 focus:outline-none focus:ring-2 focus:ring-blue-500 max-w-[150px] truncate cursor-pointer transition-colors">
                              <option value="all">{{ $t('finance.all_accounts') }}</option>
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
                      <p v-if="searchQuery || filterType !== 'all' || filterAccount !== 'all'">No transactions found matching the filters.</p>
                      <template v-else>
                          <p>No transactions this month.</p>
                          <button @click="openAddTx" class="mt-4 text-blue-500 hover:underline text-sm">{{ $t('finance.add_tx_now') }}</button>
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
                                      <p class="font-semibold text-text dark:text-text-dark truncate">{{ tx.type === 'transfer' ? 'Internal Transfer' : displayCategory(tx.category) }}</p>
                                      <button
                                          v-if="tx.receipt"
                                          @click.stop="openReceipt(tx.receipt)"
                                          class="p-1 rounded text-gray-400 hover:text-blue-500 transition-colors shrink-0"
                                          :aria-label="$t('finance.receipt_open')"
                                          :title="$t('finance.receipt_open')"
                                      >
                                          <Paperclip class="w-3.5 h-3.5" />
                                      </button>
                                      <div class="flex items-center gap-2 text-xs text-gray-500 mt-0.5">
                                          <span>{{ new Date(tx.date).getHours().toString().padStart(2, '0') }}:{{ new Date(tx.date).getMinutes().toString().padStart(2, '0') }}</span>
                                          <span>•</span>
                                          <span class="truncate">{{ tx.type === 'transfer' && tx.toAccountId ? getAccountName(tx.accountId) + ' ➡️ ' + getAccountName(tx.toAccountId) : getAccountName(tx.accountId) }}</span>
                                          <span v-if="tx.personId" @click.stop="goToPerson(tx.personId)" class="px-1.5 py-0.5 bg-blue-100 text-blue-700 dark:bg-blue-900/30 dark:text-blue-300 rounded text-[10px] font-medium hover:bg-blue-200 dark:hover:bg-blue-900/50 transition-colors ml-1">@{{ getPersonName(tx.personId) }}</span>
                                          <span v-if="tx.note" class="truncate">• {{ tx.note }}</span>
                                      </div>
                                  </div>
                                  
                                  <div class="text-right shrink-0 pr-8">
                                      <p :class="['font-bold', tx.type === 'income' ? 'text-green-500' : tx.type === 'expense' ? 'text-text dark:text-text-dark' : 'text-blue-500']">
                                          {{ tx.type === 'income' ? '+' : tx.type === 'expense' ? '-' : '' }}{{ formatCurrency(tx.amount) }}
                                      </p>
                                      <p v-if="tx.originalCurrency && tx.originalCurrency !== currentCurrency" class="text-xs text-gray-400 mt-0.5 font-medium">
                                          {{ tx.type === 'income' ? '+' : tx.type === 'expense' ? '-' : '' }}{{ formatMinorForInput(tx.originalAmount ?? 0, tx.originalCurrency) }} {{ tx.originalCurrency }}
                                      </p>
                                  </div>
                                  
                                  <!-- Action Buttons overlay -->
                                  <div class="absolute right-3 top-1/2 -translate-y-1/2 opacity-0 group-hover:opacity-100 transition-opacity">
                                      <button @click.stop="deleteTransaction(tx.id)" class="p-1.5 text-gray-400 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-900/30 rounded-lg transition-colors border border-transparent hover:border-red-200 dark:hover:border-red-800" :title="$t('finance.delete_tx')">
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
              <FinanceReports :months="months" :accounts-total="accountsTotal" :categories="[...incomeCategories, ...expenseCategories]" :accounts="accounts" :account-balances="accountBalances" />
          </div>

          <!-- Debts View -->
          <div v-else-if="currentView === 'debts'" class="flex-1 overflow-hidden">
              <FinanceDebts 
                  :debts="debts"
                  :accounts="accounts"
                  :people="people"
                  @save-debts="saveDebts"
                  @create-transaction="(tx) => { saveTransaction(tx) }"
              />
          </div>

          <!-- Repeating View -->
          <div v-else-if="currentView === 'recurring'" class="flex-1 overflow-hidden flex flex-col">
              <FinanceRecurring
                  :rules="recurringRules"
                  :accounts="accounts"
                  :categories="[...incomeCategories, ...expenseCategories]"
                  @save-rules="saveRules"
                  @add-rule="openAddRule"
                  @edit-rule="openEditRule"
              />
          </div>

          <!-- Budgets View -->
          <div v-else-if="currentView === 'budgets'" class="flex-1 overflow-hidden">
              <FinanceBudgets 
                  :budgets="budgets"
                  :transactions="selectedMonthTransactions"
                  :all-transactions="allTransactionsFlat"
                  :current-month="`${selectedYear}-${selectedMonthNum.toString().padStart(2, '0')}`"
                  :expense-categories="expenseCategories"
                  :selected-month-num="selectedMonthNum"
                  :selected-year="selectedYear"
                  :base-year="nowDate.getFullYear()"
                  @save-budgets="(newBudgets) => { saveConfig({ incomeCategories, expenseCategories, accounts, budgets: newBudgets }) }"
                  @change-month="(m: number, y: number) => { selectedMonthNum = m; selectedYear = y; }"
              />
          </div>
      </div>

      <!-- Mobile FAB -->
      <button v-if="isMobile && !needsOnboarding && currentView === 'transactions'" @click="openAddTx" class="md:hidden absolute bottom-6 right-6 p-4 rounded-full bg-blue-500 text-white hover:bg-blue-600 transition-colors shadow-lg z-[40]" :title="$t('finance.add_transaction')">
          <Plus class="w-6 h-6" />
      </button>

      <!-- Modals -->
      <TransactionModal 
          :show="showTxModal"
          :accounts="accounts"
          :income-categories="incomeCategories"
          :expense-categories="expenseCategories"
          :transaction="editingTx"
          :vault-path="vaultPath"
          :rule="editingRule"
          :default-recurrence="defaultRecurrence"
          :projects="projects"
          :people="people"
          @close="showTxModal = false"
          @save="saveTransaction"
          @save-rule="saveRule"
          @delete="deleteTransaction"
          @add-category="handleAddCategory"
      /><FinanceSettingsModal :show="showSettingsModal" :initial-income-categories="incomeCategories" :initial-expense-categories="expenseCategories" :initial-accounts="accounts" :current-balances="accountBalances" :account-usage="accountUsage" :category-usage="categoryUsage" :budgeted-categories="budgetedCategories" :initial-currency="currentCurrency" @close="showSettingsModal = false" @save="saveConfig" @export-csv="exportLedger" @import-csv="chooseImportFile" />
      <FinanceImportModal
          :show="showImportModal"
          :file-name="importFileName"
          :header="importHeader"
          :map="importMap"
          :missing="importMissing"
          :result="importResult"
          @close="showImportModal = false"
          @confirm="confirmImport"
      />
      <AdjustBalanceModal v-if="adjustingAccount" :show="showAdjustModal" :account-id="adjustingAccount.id" :account-name="adjustingAccount.name" :current-balance="adjustingAccount.balance" @close="showAdjustModal = false" @adjust="handleBalanceAdjust" />
  </div>
</template>
