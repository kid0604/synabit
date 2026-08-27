<script setup lang="ts">
import { ref, watch } from 'vue';
import { X, Plus, Trash2, Edit2, Check, Lock, Download, Upload } from 'lucide-vue-next';
import { type FinanceAccount, type Category, type AccountType, ACCOUNT_TYPES, SYSTEM_INCOME_CATEGORIES, SYSTEM_EXPENSE_CATEGORIES } from './types';
import { nameIsTaken, newCategoryId, toCategories } from './categories';
import { COMMON_CURRENCIES, allCurrencies, allowRateLookup, currencyScale, formatAmountInput, formatCurrency, parseAmountInput } from './currency';

const props = defineProps<{
  show: boolean;
  initialIncomeCategories: Category[];
  initialExpenseCategories: Category[];
  initialAccounts: FinanceAccount[];
  initialCurrency?: string;
  currentBalances?: { id: string, name: string, balance: number }[];
  /** How many transactions name each account, keyed by account id. */
  accountUsage?: Record<string, number>;
  /** How many transactions are filed under each category, keyed by name. */
  categoryUsage?: Record<string, number>;
  /** Categories a budget allocates to, which cannot be removed either. */
  budgetedCategories?: string[];
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'save', config: { incomeCategories: Category[], expenseCategories: Category[], accounts: FinanceAccount[], currency: string }): void;
  (e: 'exportCsv'): void;
  (e: 'importCsv'): void;
}>();

const incomeCategories = ref<Category[]>([]);
const expenseCategories = ref<Category[]>([]);

/** Which category is being renamed, and to what. */
const renamingId = ref<string | null>(null);
const renameDraft = ref('');
const accounts = ref<FinanceAccount[]>([]);
const selectedCurrency = ref('USD');

const newIncomeCategory = ref('');
const newExpenseCategory = ref('');
const newAccountName = ref('');
const newAccountType = ref<AccountType>('cash');
const newAccountBalance = ref('');

const editingAccountId = ref<string | null>(null);
const otherCurrencies = allCurrencies().filter(c => !COMMON_CURRENCIES.includes(c));

/**
 * Why something cannot be removed, or `null` if it can.
 *
 * Shown rather than enforced silently: a disabled button with no explanation
 * is indistinguishable from a broken one. Removing an account used to be a
 * single `splice` with no check at all, which left every transaction that named
 * it unreachable — out of the account list, and out of any total built from it.
 */
const blockedMessage = ref<string | null>(null);

const say = (message: string) => {
    blockedMessage.value = message;
    window.setTimeout(() => {
        if (blockedMessage.value === message) blockedMessage.value = null;
    }, 6000);
};

const transactionsOn = (accountId: string) => props.accountUsage?.[accountId] ?? 0;
const transactionsIn = (category: string) => props.categoryUsage?.[category] ?? 0;

// Format number input with commas
/** Grouped and read the way the vault's own currency is written. */
const formatAmount = (val: string) => formatAmountInput(val);

const handleBalanceInput = (e: Event) => {
    const target = e.target as HTMLInputElement;
    newAccountBalance.value = formatAmount(target.value);
};

// formatCurrency is imported from ./currency

const getCurrentBalance = (id: string, fallbackInitial: number) => {
    if (!props.currentBalances) return fallbackInitial;
    const found = props.currentBalances.find(b => b.id === id);
    return found ? found.balance : fallbackInitial;
};

watch(() => props.show, (newVal) => {
    if (newVal) {
        incomeCategories.value = toCategories(props.initialIncomeCategories).map(c => ({ ...c }));
        expenseCategories.value = toCategories(props.initialExpenseCategories).map(c => ({ ...c }));
        // Deep clone accounts
        accounts.value = props.initialAccounts.map(a => ({ ...a }));
        selectedCurrency.value = props.initialCurrency || 'USD';
        newIncomeCategory.value = '';
        newExpenseCategory.value = '';
        newAccountName.value = '';
        newAccountBalance.value = '';
        editingAccountId.value = null;
        renamingId.value = null;
        blockedMessage.value = null;
        newAccountType.value = 'cash';
    }
});

const addIncomeCategory = () => {
    const name = newIncomeCategory.value.trim();
    if (!name) return;
    if (nameIsTaken(incomeCategories.value, name)) {
        say(`There is already an income category called "${name}".`);
        return;
    }
    incomeCategories.value.push({ id: newCategoryId(), name });
    newIncomeCategory.value = '';
};

const removeIncomeCategory = (idx: number) => {
    const category = incomeCategories.value[idx];
    if (SYSTEM_INCOME_CATEGORIES.includes(category.id)) {
        say(`"${category.name}" is used by the debts ledger, so it cannot be removed.`);
        return;
    }
    if (!canRemoveCategory(category)) return;
    incomeCategories.value.splice(idx, 1);
};

const addExpenseCategory = () => {
    const name = newExpenseCategory.value.trim();
    if (!name) return;
    if (nameIsTaken(expenseCategories.value, name)) {
        say(`There is already an expense category called "${name}".`);
        return;
    }
    expenseCategories.value.push({ id: newCategoryId(), name });
    newExpenseCategory.value = '';
};

// ---------------------------------------------------------------------------
// Renaming
// ---------------------------------------------------------------------------

/**
 * Renaming a category, which used to be impossible.
 *
 * A category was a bare string, and a transaction named the one it belonged to
 * by that string — so "renaming" removed one category and added another, and
 * every transaction filed under the old name dropped out of every breakdown
 * while still counting towards the totals beside it. The id stays; only the
 * name changes; the history follows.
 */
const startRename = (category: Category) => {
    renamingId.value = category.id;
    renameDraft.value = category.name;
};

const commitRename = (list: Category[]) => {
    const id = renamingId.value;
    const name = renameDraft.value.trim();
    renamingId.value = null;
    if (!id || !name) return;

    const category = list.find(c => c.id === id);
    if (!category || category.name === name) return;

    if (nameIsTaken(list, name, id)) {
        say(`There is already a category called "${name}".`);
        return;
    }
    category.name = name;
};

const removeExpenseCategory = (idx: number) => {
    const category = expenseCategories.value[idx];
    if (SYSTEM_EXPENSE_CATEGORIES.includes(category.id)) {
        say(`"${category.name}" is used by the debts ledger, so it cannot be removed.`);
        return;
    }
    if (!canRemoveCategory(category)) return;
    expenseCategories.value.splice(idx, 1);
};

/**
 * Whether a category is free to go.
 *
 * A category still filed against transactions cannot be: the transactions keep
 * the name, and it would stop appearing in any breakdown while still counting
 * towards the totals — two figures on the same screen that no longer add up.
 */
const canRemoveCategory = (category: Category): boolean => {
    const used = transactionsIn(category.id);
    if (used > 0) {
        say(`"${category.name}" is on ${used} transaction${used === 1 ? '' : 's'}. Rename it instead, or refile those first.`);
        return false;
    }
    if (props.budgetedCategories?.includes(category.id)) {
        say(`"${category.name}" has a budget allocated to it. Remove it from the budget first.`);
        return false;
    }
    return true;
};

const addAccount = () => {
    const name = newAccountName.value.trim();
    const balanceNum = parseAmountInput(newAccountBalance.value);
    
    if (name) {
        if (!accounts.value.some(a => a.name === name)) {
            accounts.value.push({
                id: `acc-${Date.now()}-${Math.floor(Math.random()*1000)}`,
                name,
                initialBalance: balanceNum,
                type: newAccountType.value
            });
            newAccountName.value = '';
            newAccountBalance.value = '';
        }
    }
};

const removeAccount = (idx: number) => {
    const account = accounts.value[idx];
    const used = transactionsOn(account.id);

    if (used > 0) {
        say(`"${account.name}" still has ${used} transaction${used === 1 ? '' : 's'}. Move or delete them first, and the account can go.`);
        return;
    }
    if (accounts.value.length === 1) {
        say('A transaction has to belong to an account, so there has to be at least one.');
        return;
    }

    accounts.value.splice(idx, 1);
};

const save = () => {
    editingAccountId.value = null; // Exit edit mode
    emit('save', {
        incomeCategories: [...incomeCategories.value],
        expenseCategories: [...expenseCategories.value],
        accounts: accounts.value.map(a => ({ ...a })),
        currency: selectedCurrency.value
    });
    emit('close');
};

</script>

<template>
  <div v-if="show" class="fixed inset-0 z-[60] flex items-center justify-center p-4 bg-black/50 dark:bg-black/70 backdrop-blur-sm" @click.self="emit('close')">
    <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl shadow-xl w-full max-w-md overflow-hidden animate-in zoom-in-95 duration-200 flex flex-col max-h-[85vh]">
      
      <!-- Header -->
      <div class="flex items-center justify-between p-4 border-b border-border dark:border-border-dark shrink-0">
        <h3 class="font-bold text-lg text-text dark:text-text-dark">Finance Settings</h3>
        <button @click="emit('close')" class="p-1 rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors" aria-label="More Options">
            <X class="w-5 h-5" />
        </button>
      </div>

      <!-- Body -->
      <div class="p-5 overflow-y-auto space-y-6">
          
        <!-- General Settings -->
        <div>
            <h4 class="text-sm font-semibold text-text dark:text-text-dark mb-3">General Settings</h4>
            <div
                v-if="blockedMessage"
                class="flex items-start gap-2 p-3 rounded-xl border border-amber-300 bg-amber-50 text-sm text-amber-900 dark:border-amber-700/60 dark:bg-amber-950/40 dark:text-amber-200"
                role="status"
            >
                <Lock class="w-4 h-4 shrink-0 mt-0.5" />
                <span>{{ blockedMessage }}</span>
            </div>

            <div class="flex items-center justify-between p-3 bg-gray-50 dark:bg-gray-800/50 rounded-xl border border-border dark:border-border-dark">
                <div class="flex flex-col pr-4">
                    <span class="font-medium text-sm text-text dark:text-text-dark">Currency</span>
                    <span class="text-xs text-gray-500">Base currency for your transactions. Existing amounts are not converted.</span>
                    <!-- Amounts are stored in the currency's smallest unit, so
                         moving between a currency with subunits and one without
                         reinterprets every stored figure by a factor of a
                         hundred. Said plainly, before it happens. -->
                    <span
                        v-if="currencyScale(selectedCurrency) !== currencyScale(initialCurrency || 'USD')"
                        class="text-xs text-amber-600 dark:text-amber-400 mt-1 max-w-xs"
                    >
                        {{ selectedCurrency }} and {{ initialCurrency || 'USD' }} hold a different number of decimal places, so every existing amount will read differently after this change.
                    </span>
                </div>
                <select v-model="selectedCurrency" class="bg-white dark:bg-gray-900 border border-border dark:border-border-dark rounded-lg px-3 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 text-text dark:text-text-dark cursor-pointer">
                    <optgroup label="Common">
                        <option v-for="c in COMMON_CURRENCIES" :key="c" :value="c">{{ c }}</option>
                    </optgroup>
                    <optgroup label="All">
                        <option v-for="c in otherCurrencies" :key="c" :value="c">{{ c }}</option>
                    </optgroup>
                </select>
            </div>

            <!-- Roadmap 1.4. The app says it sends nothing anywhere; a rate
                 lookup is a request to a CDN, so it is asked for rather than
                 assumed. -->
            <div class="flex items-center justify-between p-3 bg-gray-50 dark:bg-gray-800/50 rounded-xl border border-border dark:border-border-dark">
                <div class="flex flex-col pr-4">
                    <span class="font-medium text-sm text-text dark:text-text-dark">Look up exchange rates online</span>
                    <span class="text-xs text-gray-500">Off by default. When off, type the rate yourself; rates you have looked up before are still remembered.</span>
                </div>
                <label class="relative inline-flex items-center cursor-pointer shrink-0">
                    <input type="checkbox" v-model="allowRateLookup" class="sr-only peer" />
                    <div class="w-11 h-6 bg-gray-300 dark:bg-gray-600 peer-focus:ring-2 peer-focus:ring-blue-500 rounded-full peer peer-checked:after:translate-x-full after:content-[''] after:absolute after:top-0.5 after:left-[2px] after:bg-white after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-blue-500"></div>
                </label>
            </div>
        </div>

        <hr class="border-border dark:border-border-dark" />

        <!-- Income Categories -->
        <div>
            <h4 class="text-sm font-semibold text-green-600 dark:text-green-400 mb-3">Income Categories</h4>
            <div class="flex gap-2 mb-3">
                <input type="text" v-model="newIncomeCategory" @keyup.enter="addIncomeCategory" class="flex-1 bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500" :placeholder="$t('finance.new_income_cat')" />
                <button @click="addIncomeCategory" class="p-2 bg-blue-500 text-white rounded-xl hover:bg-blue-600 transition-colors" aria-label="Add Income Category">
                    <Plus class="w-5 h-5" />
                </button>
            </div>
            <div class="flex flex-wrap gap-2">
                <div v-for="(cat, idx) in incomeCategories" :key="cat.id" class="flex items-center gap-1.5 px-3 py-1.5 bg-green-50 dark:bg-green-900/10 border border-green-200 dark:border-green-900/30 rounded-lg text-sm text-green-700 dark:text-green-400">
                    <input
                        v-if="renamingId === cat.id"
                        v-model="renameDraft"
                        @keyup.enter="commitRename(incomeCategories)"
                        @keyup.escape="renamingId = null"
                        @blur="commitRename(incomeCategories)"
                        class="bg-transparent border-b border-green-400 focus:outline-none w-24 text-green-700 dark:text-green-400"
                        autofocus
                    />
                    <span v-else @dblclick="startRename(cat)" class="cursor-text" :title="'Double-click to rename'">{{ cat.name }}</span>
                    <button v-if="!SYSTEM_INCOME_CATEGORIES.includes(cat.id) && renamingId !== cat.id" @click="startRename(cat)" class="text-green-500/50 hover:text-green-700 transition-colors" aria-label="Rename category">
                        <Edit2 class="w-3 h-3" />
                    </button>
                    <button v-if="!SYSTEM_INCOME_CATEGORIES.includes(cat.id)" @click="removeIncomeCategory(idx)" class="text-green-500/50 hover:text-red-500 transition-colors" aria-label="Remove Income Category">
                        <X class="w-3.5 h-3.5" />
                    </button>
                    <div v-else class="text-green-500/30 ml-1">
                        <Lock class="w-3 h-3" />
                    </div>
                </div>
                <div v-if="!incomeCategories.length" class="text-sm text-gray-400 italic">No categories yet.</div>
            </div>
        </div>

        <hr class="border-border dark:border-border-dark" />

        <!-- Expense Categories -->
        <div>
            <h4 class="text-sm font-semibold text-red-600 dark:text-red-400 mb-3">Expense Categories</h4>
            <div class="flex gap-2 mb-3">
                <input type="text" v-model="newExpenseCategory" @keyup.enter="addExpenseCategory" class="flex-1 bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500" :placeholder="$t('finance.new_expense_cat')" />
                <button @click="addExpenseCategory" class="p-2 bg-blue-500 text-white rounded-xl hover:bg-blue-600 transition-colors" aria-label="More Options">
                    <Plus class="w-5 h-5" />
                </button>
            </div>
            <div class="flex flex-wrap gap-2">
                <div v-for="(cat, idx) in expenseCategories" :key="cat.id" class="flex items-center gap-1.5 px-3 py-1.5 bg-red-50 dark:bg-red-900/10 border border-red-200 dark:border-red-900/30 rounded-lg text-sm text-red-700 dark:text-red-400">
                    <input
                        v-if="renamingId === cat.id"
                        v-model="renameDraft"
                        @keyup.enter="commitRename(expenseCategories)"
                        @keyup.escape="renamingId = null"
                        @blur="commitRename(expenseCategories)"
                        class="bg-transparent border-b border-red-400 focus:outline-none w-24 text-red-700 dark:text-red-400"
                        autofocus
                    />
                    <span v-else @dblclick="startRename(cat)" class="cursor-text" :title="'Double-click to rename'">{{ cat.name }}</span>
                    <button v-if="!SYSTEM_EXPENSE_CATEGORIES.includes(cat.id) && renamingId !== cat.id" @click="startRename(cat)" class="text-red-500/50 hover:text-red-700 transition-colors" aria-label="Rename category">
                        <Edit2 class="w-3 h-3" />
                    </button>
                    <button v-if="!SYSTEM_EXPENSE_CATEGORIES.includes(cat.id)" @click="removeExpenseCategory(idx)" class="text-red-500/50 hover:text-red-500 transition-colors" aria-label="More Options">
                        <X class="w-3.5 h-3.5" />
                    </button>
                    <div v-else class="text-red-500/30 ml-1">
                        <Lock class="w-3 h-3" />
                    </div>
                </div>
                <div v-if="!expenseCategories.length" class="text-sm text-gray-400 italic">No categories yet.</div>
            </div>
        </div>

        <hr class="border-border dark:border-border-dark" />

        <!-- Accounts -->
        <!-- The way out and the way in. Sits with the rest of the settings
             because that is where somebody looks for it, and because leaving
             should not be harder to find than arriving. -->
        <div>
            <h4 class="text-sm font-semibold text-text dark:text-text-dark mb-1">{{ $t('finance.data_section') }}</h4>
            <p class="text-xs text-gray-500 dark:text-gray-400 mb-3">{{ $t('finance.export_hint') }}</p>
            <div class="flex gap-2">
                <button @click="emit('exportCsv')" class="flex items-center justify-center gap-2 flex-1 px-4 py-2.5 rounded-xl bg-gray-50 dark:bg-gray-800/50 border border-border dark:border-border-dark text-sm font-medium text-text dark:text-text-dark hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors">
                    <Download class="w-4 h-4" />
                    {{ $t('finance.export_csv') }}
                </button>
                <button @click="emit('importCsv')" class="flex items-center justify-center gap-2 flex-1 px-4 py-2.5 rounded-xl bg-gray-50 dark:bg-gray-800/50 border border-border dark:border-border-dark text-sm font-medium text-text dark:text-text-dark hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors">
                    <Upload class="w-4 h-4" />
                    {{ $t('finance.import_csv') }}
                </button>
            </div>
        </div>

        <hr class="border-border dark:border-border-dark" />

        <div>
            <h4 class="text-sm font-semibold text-text dark:text-text-dark mb-3">Accounts & Balances</h4>
            
            <div class="flex flex-col gap-2 mb-4 p-3 bg-gray-50 dark:bg-gray-800/50 rounded-xl border border-border dark:border-border-dark">
                <input type="text" v-model="newAccountName" class="w-full bg-white dark:bg-gray-800 border border-border dark:border-border-dark rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500" :placeholder="$t('finance.new_acc_name')" />
                <div class="flex gap-2">
                    <div class="relative flex-1">
                        <input type="text" inputmode="decimal" :value="newAccountBalance" @input="handleBalanceInput" class="w-full bg-white dark:bg-gray-800 border border-border dark:border-border-dark rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 pr-4" :placeholder="$t('finance.initial_balance')" />
                    </div>
                    <!-- A credit card is not a wallet, and the app used to ship
                         an account called "Credit Card" with no way to say so. -->
                    <select v-model="newAccountType" class="bg-white dark:bg-gray-800 border border-border dark:border-border-dark rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 text-text dark:text-text-dark">
                        <option v-for="t in ACCOUNT_TYPES" :key="t" :value="t">{{ $t(`finance.account_type_${t}`) }}</option>
                    </select>
                    <button @click="addAccount" :disabled="!newAccountName" class="px-4 bg-blue-500 text-white rounded-lg hover:bg-blue-600 transition-colors font-medium text-sm whitespace-nowrap disabled:opacity-50">
                        Add
                    </button>
                </div>
                <p v-if="newAccountType === 'credit'" class="text-xs text-gray-500 dark:text-gray-400">
                    A card you owe money on starts below zero — enter its balance with a minus sign.
                </p>
            </div>

            <div class="flex flex-col gap-2">
                <div v-for="(acc, idx) in accounts" :key="acc.id" class="flex flex-col px-3 py-2.5 bg-gray-50 dark:bg-gray-800/50 border border-border dark:border-border-dark rounded-xl text-sm">
                    
                    <div v-if="editingAccountId !== acc.id" class="flex items-center justify-between text-text dark:text-text-dark">
                        <div class="flex flex-col">
                            <span class="font-medium">{{ acc.name }}</span>
                            <span class="text-xs text-gray-500">
                                <span v-if="acc.type">{{ $t(`finance.account_type_${acc.type}`) }} · </span>Current Balance:
                                <span class="font-semibold text-gray-700 dark:text-gray-300">{{ formatCurrency(getCurrentBalance(acc.id, acc.initialBalance)) }}</span>
                            </span>
                        </div>
                        <div class="flex items-center gap-1 shrink-0">
                            <button @click="editingAccountId = acc.id" class="text-gray-400 hover:text-blue-500 transition-colors p-2 rounded-lg hover:bg-gray-200 dark:hover:bg-gray-700" aria-label="More Options">
                                <Edit2 class="w-4 h-4" />
                            </button>
                            <button @click="removeAccount(idx)" class="text-gray-400 hover:text-red-500 transition-colors p-2 rounded-lg hover:bg-gray-200 dark:hover:bg-gray-700" aria-label="Remove Account">
                                <Trash2 class="w-4 h-4" />
                            </button>
                        </div>
                    </div>
                    
                    <div v-else class="flex flex-col gap-2">
                        <input type="text" v-model="acc.name" class="w-full bg-white dark:bg-gray-800 border border-border dark:border-border-dark rounded-lg px-2 py-1 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500" placeholder="Account Name" />
                        <select v-model="acc.type" class="w-full bg-white dark:bg-gray-800 border border-border dark:border-border-dark rounded-lg px-2 py-1 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 text-text dark:text-text-dark">
                            <option :value="undefined">{{ $t('finance.account_type_unset') }}</option>
                            <option v-for="t in ACCOUNT_TYPES" :key="t" :value="t">{{ $t(`finance.account_type_${t}`) }}</option>
                        </select>
                        <div class="flex items-center justify-between text-xs text-gray-500">
                            <span>Current Balance: <span class="font-semibold">{{ formatCurrency(getCurrentBalance(acc.id, acc.initialBalance)) }}</span></span>
                            <button @click="editingAccountId = null" class="px-3 py-1 bg-green-500 text-white rounded-lg hover:bg-green-600 transition-colors flex items-center justify-center" aria-label="More Options">
                                <Check class="w-4 h-4" />
                            </button>
                        </div>
                    </div>
                    
                </div>
                <div v-if="!accounts.length" class="text-sm text-gray-400 italic">No accounts yet.</div>
            </div>
        </div>

      </div>

      <!-- Footer -->
      <div class="p-4 border-t border-border dark:border-border-dark flex justify-end gap-3 shrink-0 bg-gray-50/50 dark:bg-gray-800/50">
        <button @click="emit('close')" class="px-4 py-2 rounded-xl text-sm font-medium text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors">
            Cancel
        </button>
        <button @click="save" class="px-5 py-2 rounded-xl text-sm font-medium bg-blue-500 hover:bg-blue-600 text-white shadow-sm transition-colors">
            Save Changes
        </button>
      </div>

    </div>
  </div>
</template>
