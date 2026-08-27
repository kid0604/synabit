<script setup lang="ts">
import { ref, watch, computed } from 'vue';
import { X, RefreshCw, Plus, Check, Trash2, Paperclip } from 'lucide-vue-next';
import { convertFileSrc, invoke } from '@tauri-apps/api/core';
import { open as openFileDialog } from '@tauri-apps/plugin-dialog';
import type { Transaction, TransactionType, FinanceAccount, Category } from './types';
import { FINANCE_RECURRENCES, type RecurringRule } from './recurring';
import { COMMON_CURRENCIES, allCurrencies, allowRateLookup, convertMinor, currentCurrency, fetchExchangeRate, formatAmountInput, formatCurrency, formatMinorForInput, parseAmountInput } from './currency';

const props = defineProps<{
  show: boolean;
  transaction?: Transaction | null;
  incomeCategories: Category[];
  expenseCategories: Category[];
  accounts: FinanceAccount[];
  projects?: {id: string, title: string}[];
  people?: {id: string, title: string}[];
  defaultProjectId?: string;
  /** Needed to turn a vault-relative receipt path into something renderable. */
  vaultPath?: string;
  /** The repeating rule being edited, if this dialog was opened on one. */
  rule?: RecurringRule | null;
  /** What the Repeats field starts on, so "Add repeating" opens repeating. */
  defaultRecurrence?: string;
}>();

const emit = defineEmits<{
  (e: 'close'): void;
  (e: 'save', tx: Transaction): void;
  (e: 'saveRule', rule: RecurringRule): void;
  (e: 'delete', txId: string): void;
  (e: 'addCategory', payload: { type: 'income' | 'expense', name: string }): void;
}>();

const type = ref<TransactionType>('expense');
const amount = ref<string>('');
const category = ref<string>('');
const accountId = ref<string>('');
const toAccountId = ref<string>('');
const date = ref<string>('');
const note = ref<string>('');
const projectId = ref<string>('');
const personId = ref<string>('');
const showErrors = ref(false);

/**
 * How often this happens.
 *
 * `none` saves a transaction; anything else saves a rule, and the rule makes
 * the transactions. The two share this dialog because they are the same form
 * with one extra question — offering a separate screen for "the same thing but
 * every month" is how apps end up with two ways to enter a purchase.
 */
const recurrence = ref<string>('none');
const endDate = ref<string>('');
const repeats = computed(() => recurrence.value !== 'none');

const inputCurrency = ref(currentCurrency.value);
const isFetchingRate = ref(false);
const exchangeRate = ref<number | null>(null);
const exchangeRateStr = ref<string>('');
const calculatedBaseAmount = ref<number>(0);
/** The common few first, then everything this runtime can format. */
const CURRENCIES = computed(() => {
    const rest = allCurrencies().filter(c => !COMMON_CURRENCIES.includes(c));
    return { common: COMMON_CURRENCIES, rest };
});

const isAddingCategory = ref(false);
const newCategoryName = ref('');

const pendingCategoryName = ref<string | null>(null);

// ---------------------------------------------------------------------------
// The receipt
// ---------------------------------------------------------------------------

/** A vault-relative `assets/…` path, or nothing. */
const receipt = ref<string>('');
const attaching = ref(false);

/** What the picture is called, which is all the row has space to say. */
const receiptName = computed(() => receipt.value.split('/').pop() ?? '');

/** Where the picture actually is, for the webview to render. */
const receiptSrc = computed(() =>
    receipt.value && props.vaultPath
        ? convertFileSrc(`${props.vaultPath}/${receipt.value}`)
        : ''
);

/**
 * Copy a picture into the vault and remember where it landed.
 *
 * Copied rather than referenced: the assets folder is what sync carries, so a
 * receipt left in Downloads is a receipt only this device will ever see. The
 * copy is named after its own contents, so photographing the same receipt
 * twice stores one file.
 */
const attachReceipt = async () => {
    if (!props.vaultPath) return;
    try {
        const picked = await openFileDialog({
            multiple: false,
            filters: [{ name: 'Image', extensions: ['jpg', 'jpeg', 'png', 'webp', 'heic', 'gif', 'pdf'] }],
        });
        const sourcePath = Array.isArray(picked) ? picked[0] : picked;
        if (!sourcePath) return;

        attaching.value = true;
        receipt.value = await invoke<string>('copy_asset_to_vault', {
            vaultPath: props.vaultPath,
            sourcePath,
        });
    } catch (e) {
        console.error('Could not attach the receipt', e);
    } finally {
        attaching.value = false;
    }
};

/**
 * Forget the receipt on this transaction.
 *
 * The file itself stays. It is named after its contents and may well be
 * attached to something else, and deleting a picture because one transaction
 * stopped pointing at it is how the other one loses it.
 */
const removeReceipt = () => {
    receipt.value = '';
};

const availableCategories = computed(() => {
    if (type.value === 'income') return props.incomeCategories;
    if (type.value === 'expense') return props.expenseCategories;
    // For transfers, we can either hide the category field or use a fixed category.
    // In V1, we'll just return expenseCategories or empty
    return [];
});

/**
 * A category asked for but not yet created.
 *
 * The parent owns the list and mints the id, so the dialog cannot select the
 * new category until it comes back. It waits by name and selects by id — a
 * name is not an id, and treating it as one is how the old code filed a
 * transaction under a category that did not exist.
 */
watch(availableCategories, (list) => {
    if (!pendingCategoryName.value) return;
    const arrived = list.find(c => c.name === pendingCategoryName.value);
    if (arrived) {
        category.value = arrived.id;
        pendingCategoryName.value = null;
    }
});

watch(type, (newType, oldType) => {
    if (newType !== oldType) {
        category.value = availableCategories.value[0]?.id ?? '';
    }
});

// Format number input with grouping, in whatever currency is being typed.
const formatAmount = (val: string) => formatAmountInput(val, inputCurrency.value);

const handleAmountInput = (e: Event) => {
    const target = e.target as HTMLInputElement;
    amount.value = formatAmount(target.value);
};

/** What was typed, in the minor units of the currency it was typed in. */
const typedAmountMinor = () => parseAmountInput(amount.value, inputCurrency.value);

/**
 * A rate as text, keeping enough of it to be worth having.
 *
 * The old code rounded any rate above 1 to a whole number, which turned a
 * euro-to-dollar rate of 1.08 into 1 and quietly made the conversion a no-op.
 */
const rateAsText = (rate: number) =>
    rate >= 1000 ? rate.toFixed(0) : Number(rate.toPrecision(6)).toString();

/** Recompute the vault-currency amount from what is typed and the rate. */
const recalculateBase = () => {
    const typed = typedAmountMinor();
    if (inputCurrency.value === currentCurrency.value) {
        calculatedBaseAmount.value = typed;
        return;
    }
    calculatedBaseAmount.value = exchangeRate.value
        ? convertMinor(typed, inputCurrency.value, currentCurrency.value, exchangeRate.value)
        : 0;
};

watch([amount, inputCurrency], async ([, newCurrency], [, oldCurrency]) => {
    if (newCurrency === currentCurrency.value) {
        exchangeRate.value = null;
        exchangeRateStr.value = '';
        recalculateBase();
        return;
    }

    // Only when the currency itself changed: re-asking on every keystroke
    // would be a request per character.
    if (newCurrency !== oldCurrency) {
        isFetchingRate.value = true;
        const rate = await fetchExchangeRate(newCurrency, currentCurrency.value);
        isFetchingRate.value = false;

        if (rate) {
            exchangeRate.value = rate;
            exchangeRateStr.value = rateAsText(rate);
        }
    }

    recalculateBase();
});

const handleRateInput = (e: Event) => {
    const target = e.target as HTMLInputElement;
    // A rate is not money: it is a plain decimal, in whatever precision the
    // user has. Stored as the exact string so that typing "1.0" then "8"
    // is not reformatted out from under the caret.
    const cleanStr = target.value.replace(/[^\d.]/g, '');
    exchangeRateStr.value = cleanStr;
    exchangeRate.value = Number(cleanStr) || 0;
    recalculateBase();
};

const initForm = () => {
    if (props.rule) {
        const template = props.rule.template;
        type.value = template.type;
        category.value = template.category;
        accountId.value = template.accountId;
        toAccountId.value = template.toAccountId || '';
        inputCurrency.value = currentCurrency.value;
        amount.value = formatMinorForInput(template.amount, currentCurrency.value);
        calculatedBaseAmount.value = template.amount;
        exchangeRate.value = null;
        exchangeRateStr.value = '';
        date.value = `${props.rule.startDate}T12:00`;
        note.value = template.note;
        projectId.value = template.projectId || '';
        personId.value = template.personId || '';
        receipt.value = '';
        recurrence.value = props.rule.recurrence;
        endDate.value = props.rule.endDate || '';
        showErrors.value = false;
        isAddingCategory.value = false;
        newCategoryName.value = '';
        pendingCategoryName.value = null;
        return;
    }

    recurrence.value = props.defaultRecurrence ?? 'none';
    endDate.value = '';

    if (props.transaction) {
        type.value = props.transaction.type;
        category.value = props.transaction.category;
        accountId.value = props.transaction.accountId;
        toAccountId.value = props.transaction.toAccountId || '';
        
        if (props.transaction.originalCurrency && props.transaction.originalCurrency !== currentCurrency.value) {
            inputCurrency.value = props.transaction.originalCurrency;
            amount.value = props.transaction.originalAmount
                ? formatMinorForInput(props.transaction.originalAmount, inputCurrency.value)
                : '';
            exchangeRate.value = props.transaction.exchangeRate || null;
            exchangeRateStr.value = exchangeRate.value ? rateAsText(exchangeRate.value) : '';
            calculatedBaseAmount.value = props.transaction.amount;
        } else {
            inputCurrency.value = currentCurrency.value;
            amount.value = formatMinorForInput(props.transaction.amount, inputCurrency.value);
            exchangeRate.value = null;
            exchangeRateStr.value = '';
            calculatedBaseAmount.value = props.transaction.amount;
        }

        const d = new Date(props.transaction.date);
        date.value = new Date(d.getTime() - d.getTimezoneOffset() * 60000).toISOString().slice(0, 16);
        note.value = props.transaction.note;
        receipt.value = props.transaction.receipt || '';
        projectId.value = props.transaction.projectId || '';
        personId.value = props.transaction.personId || '';
    } else {
        type.value = 'expense';
        inputCurrency.value = currentCurrency.value;
        amount.value = '';
        exchangeRate.value = null;
        exchangeRateStr.value = '';
        calculatedBaseAmount.value = 0;
        
        category.value = props.expenseCategories[0]?.id ?? '';
        accountId.value = props.accounts.length ? props.accounts[0].id : '';
        toAccountId.value = props.accounts.length > 1 ? props.accounts[1].id : '';
        const now = new Date();
        date.value = new Date(now.getTime() - now.getTimezoneOffset() * 60000).toISOString().slice(0, 16);
        note.value = '';
        receipt.value = '';
        projectId.value = props.defaultProjectId || '';
        personId.value = '';
    }
    showErrors.value = false;
    isAddingCategory.value = false;
    newCategoryName.value = '';
    pendingCategoryName.value = null;
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
    
    // Prevent saving if it's a transfer between the same account
    if (type.value === 'transfer' && accountId.value === toAccountId.value) {
        showErrors.value = true;
        return;
    }
    
    if (repeats.value) {
        // A repeating transaction is a rule, and the rule makes the
        // transactions. Its start date is the anchor every later occurrence is
        // measured from, so it is stored as a plain day.
        emit('saveRule', {
            id: props.rule?.id || `rule-${Date.now()}-${Math.floor(Math.random()*1000)}`,
            recurrence: recurrence.value as RecurringRule['recurrence'],
            startDate: date.value.slice(0, 10),
            endDate: endDate.value || undefined,
            paused: props.rule?.paused,
            template: {
                type: type.value,
                amount: calculatedBaseAmount.value,
                category: type.value === 'transfer' ? 'Transfer' : category.value,
                accountId: accountId.value,
                toAccountId: type.value === 'transfer' ? toAccountId.value : undefined,
                note: note.value.trim(),
                projectId: type.value === 'expense' && projectId.value ? projectId.value : undefined,
                personId: personId.value ? personId.value : undefined,
            },
        });
        return;
    }

    const tx: Transaction = {
        id: props.transaction?.id || `tx-${Date.now()}-${Math.floor(Math.random()*1000)}`,
        type: type.value,
        amount: calculatedBaseAmount.value,
        category: type.value === 'transfer' ? 'Transfer' : category.value,
        accountId: accountId.value,
        date: new Date(date.value).toISOString(),
        note: note.value.trim(),
        projectId: type.value === 'expense' && projectId.value ? projectId.value : undefined,
        personId: personId.value ? personId.value : undefined,
        receipt: receipt.value || undefined
    };
    
    if (inputCurrency.value !== currentCurrency.value) {
        tx.originalCurrency = inputCurrency.value;
        tx.originalAmount = typedAmountMinor();
        tx.exchangeRate = exchangeRate.value || 1;
    }
    
    if (type.value === 'transfer') {
        tx.toAccountId = toAccountId.value;
    }
    
    emit('save', tx);
};

const saveNewCategory = () => {
    const name = newCategoryName.value.trim();
    if (name) {
        emit('addCategory', { type: type.value as 'income' | 'expense', name });
        // The parent mints the id and hands the list back; until then the
        // dialog has nothing to select, so it waits rather than selecting a
        // name as though it were an id.
        pendingCategoryName.value = name;
    }
    isAddingCategory.value = false;
    newCategoryName.value = '';
};

// Computed property for save validation
const canSave = computed(() => {
    const numericAmount = typedAmountMinor();
    if (!numericAmount || numericAmount <= 0) return false;
    if (!accountId.value) return false;
    if (type.value === 'transfer' && (!toAccountId.value || accountId.value === toAccountId.value)) return false;
    return true;
});


const personSearch = ref('');
const isPersonDropdownOpen = ref(false);

const filteredPeople = computed(() => {
    if (!props.people) return [];
    if (!personSearch.value) return props.people;
    const q = personSearch.value.toLowerCase();
    return props.people.filter(p => p.title.toLowerCase().includes(q));
});

const getPersonName = (id: string) => {
    return props.people?.find(p => p.id === id)?.title || 'No person';
};

const openPersonDropdown = () => {
    personSearch.value = '';
    isPersonDropdownOpen.value = true;
};

const closePersonDropdown = () => {
    setTimeout(() => {
        isPersonDropdownOpen.value = false;
    }, 200);
};

</script>

<template>
  <div v-if="show" class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/50 dark:bg-black/70 backdrop-blur-sm" @click.self="emit('close')">
    <div class="bg-surface dark:bg-surface-dark border border-border dark:border-border-dark rounded-2xl shadow-xl w-full max-w-md overflow-hidden animate-in zoom-in-95 duration-200">
      
      <!-- Header -->
      <div class="flex items-center justify-between p-4 border-b border-border dark:border-border-dark">
        <h3 class="font-bold text-lg text-text dark:text-text-dark">
            {{ transaction ? 'Edit Transaction' : 'New Transaction' }}
        </h3>
        <button @click="emit('close')" class="p-1 rounded-lg text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors" aria-label="More Options">
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
            <label class="block text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-1">Amount <span v-if="showErrors && calculatedBaseAmount <= 0" class="text-red-500 normal-case font-normal ml-1">{{ $t('finance.must_be_gt_0') }}</span></label>
            <div class="flex gap-2">
                <div :class="['relative rounded-xl transition-all flex-1', showErrors && calculatedBaseAmount <= 0 ? 'ring-2 ring-red-500' : '']">
                    <input type="text" inputmode="decimal" :value="amount" @input="handleAmountInput" class="w-full bg-transparent border border-border dark:border-border-dark rounded-xl px-4 py-3 text-2xl font-bold text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500 transition-all pr-4" placeholder="0" />
                </div>
                <select v-model="inputCurrency" class="bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl px-3 py-3 font-bold text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500 appearance-none min-w-[80px] text-center cursor-pointer">
                    <optgroup label="Common">
                        <option v-for="c in CURRENCIES.common" :key="c" :value="c">{{ c }}</option>
                    </optgroup>
                    <optgroup label="All">
                        <option v-for="c in CURRENCIES.rest" :key="c" :value="c">{{ c }}</option>
                    </optgroup>
                </select>
            </div>
            
            <!-- Exchange Rate UI -->
            <div v-if="inputCurrency !== currentCurrency" class="mt-3 p-3 bg-blue-50 dark:bg-blue-900/10 rounded-xl border border-blue-100 dark:border-blue-900/30 flex flex-col gap-2">
                <div class="flex items-center justify-between">
                    <span class="text-xs font-semibold text-blue-600 dark:text-blue-400">Exchange Rate ({{ inputCurrency }} &rarr; {{ currentCurrency }})</span>
                    <span v-if="isFetchingRate" class="text-xs text-blue-500 animate-pulse flex items-center gap-1"><RefreshCw class="w-3 h-3 animate-spin" /> Fetching...</span>
                </div>
                <div class="flex gap-2 items-center">
                    <input type="text" inputmode="decimal" :value="exchangeRateStr" @input="handleRateInput" class="w-full bg-white dark:bg-gray-900 border border-blue-200 dark:border-blue-800 rounded-lg px-3 py-2 text-sm font-bold text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500" :placeholder="$t('finance.custom_rate')" :disabled="isFetchingRate" />
                    <span class="text-sm font-bold text-blue-700 dark:text-blue-300 whitespace-nowrap">
                        ≈ {{ formatCurrency(calculatedBaseAmount) }}
                    </span>
                </div>
                <!-- Why nothing was looked up. Said here rather than left as an
                     empty field the user has to guess the meaning of. -->
                <p v-if="!allowRateLookup && !exchangeRate" class="text-xs text-blue-600/80 dark:text-blue-400/80">
                    Enter today's rate. Synabit does not look rates up online unless you turn that on in Finance settings.
                </p>
            </div>
        </div>

        <div class="grid grid-cols-2 gap-4">
            <!-- Category (Hidden for Transfer) -->
            <div v-if="type !== 'transfer'">
                <label class="block text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-1">{{ $t('finance.category') }}</label>
                <div class="flex items-center gap-2">
                    <template v-if="!isAddingCategory">
                        <select v-model="category" class="w-full bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl px-3 py-2.5 text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500 appearance-none">
                            <option v-for="cat in availableCategories" :key="cat.id" :value="cat.id">{{ cat.name }}</option>
                        </select>
                        <button @click="isAddingCategory = true" class="p-2.5 text-gray-400 hover:text-blue-500 hover:bg-blue-50 dark:hover:bg-blue-900/30 rounded-xl transition-colors shrink-0 border border-border dark:border-border-dark bg-gray-50 dark:bg-gray-800" title="Add new category">
                            <Plus class="w-4 h-4" />
                        </button>
                    </template>
                    <template v-else>
                        <input type="text" v-model="newCategoryName" @keyup.enter="saveNewCategory" class="w-full bg-white dark:bg-gray-900 border border-blue-300 dark:border-blue-700 rounded-xl px-3 py-2.5 text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500" :placeholder="$t('finance.type_new_category')" autofocus />
                        <button @click="saveNewCategory" class="p-2.5 text-white bg-blue-500 hover:bg-blue-600 rounded-xl transition-colors shrink-0" title="Save category">
                            <Check class="w-4 h-4" />
                        </button>
                        <button @click="isAddingCategory = false; newCategoryName = ''" class="p-2.5 text-gray-500 bg-gray-100 dark:bg-gray-800 hover:bg-gray-200 dark:hover:bg-gray-700 rounded-xl transition-colors shrink-0" title="Cancel">
                            <X class="w-4 h-4" />
                        </button>
                    </template>
                </div>
            </div>
            
            <!-- From Account -->
            <div>
                <label class="block text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-1">{{ type === 'transfer' ? 'From Account' : 'Account' }} <span v-if="showErrors && !accountId" class="text-red-500 normal-case font-normal ml-1">{{ $t('finance.required') }}</span></label>
                <select v-model="accountId" :class="['w-full bg-gray-50 dark:bg-gray-800 border rounded-xl px-3 py-2.5 text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500 appearance-none', showErrors && !accountId ? 'border-red-500' : 'border-border dark:border-border-dark']">
                    <option v-for="acc in accounts" :key="acc.id" :value="acc.id">{{ acc.name }}</option>
                </select>
            </div>
            
            <!-- To Account (Only for Transfer) -->
            <div v-if="type === 'transfer'">
                <label class="block text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-1">To Account <span v-if="showErrors && (!toAccountId || accountId === toAccountId)" class="text-red-500 normal-case font-normal ml-1">{{ $t('finance.invalid') }}</span></label>
                <select v-model="toAccountId" :class="['w-full bg-gray-50 dark:bg-gray-800 border rounded-xl px-3 py-2.5 text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500 appearance-none', showErrors && (!toAccountId || accountId === toAccountId) ? 'border-red-500' : 'border-border dark:border-border-dark']">
                    <option v-for="acc in accounts" :key="acc.id" :value="acc.id">{{ acc.name }}</option>
                </select>
            </div>
        </div>

        <!-- Date -->
        <div>
            <label class="block text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-1">{{ $t('finance.date') }}</label>
            <input type="datetime-local" v-model="date" class="w-full bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl px-3 py-2.5 text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500" />
        </div>

        <!-- How often. `none` saves one transaction; anything else saves a
             rule that keeps making them. -->
        <div>
            <label class="block text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-1">{{ $t('finance.repeats') }}</label>
            <select v-model="recurrence" class="w-full bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl px-3 py-2.5 text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500">
                <option value="none">{{ $t('finance.repeats_none') }}</option>
                <option v-for="r in FINANCE_RECURRENCES" :key="r" :value="r">{{ $t(`finance.repeats_${r}`) }}</option>
            </select>

            <div v-if="repeats" class="mt-2 flex flex-col gap-2">
                <p class="text-xs text-blue-600 dark:text-blue-400">{{ $t('finance.repeats_hint') }}</p>
                <label class="block text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider">{{ $t('finance.repeats_until') }}</label>
                <input type="date" v-model="endDate" class="w-full bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl px-3 py-2.5 text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500" />
            </div>
        </div>

        <!-- Note -->
        <div>
            <label class="block text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-1">{{ $t('finance.note') }}</label>
            <input type="text" v-model="note" class="w-full bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl px-3 py-2.5 text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500" :placeholder="$t('finance.tx_details_ph')" />
        </div>

        <!-- The receipt. Copied into the vault so it travels with the ledger,
             rather than pointing at wherever the photo happened to be. -->
        <div v-if="vaultPath">
            <label class="block text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-1">{{ $t('finance.receipt') }}</label>

            <button
                v-if="!receipt"
                @click="attachReceipt"
                :disabled="attaching"
                class="flex items-center gap-2 w-full px-3 py-2.5 rounded-xl border border-dashed border-border dark:border-border-dark text-sm text-gray-500 dark:text-gray-400 hover:border-blue-400 hover:text-blue-500 transition-colors disabled:opacity-50"
            >
                <Paperclip class="w-4 h-4" />
                {{ attaching ? $t('finance.receipt_attaching') : $t('finance.receipt_attach') }}
            </button>

            <div v-else class="flex items-center gap-3 p-2 rounded-xl bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark">
                <img
                    v-if="!receiptName.toLowerCase().endsWith('.pdf')"
                    :src="receiptSrc"
                    :alt="$t('finance.receipt')"
                    class="w-12 h-12 rounded-lg object-cover shrink-0 bg-white dark:bg-gray-900"
                />
                <div v-else class="w-12 h-12 rounded-lg shrink-0 flex items-center justify-center bg-white dark:bg-gray-900 text-gray-400">
                    <Paperclip class="w-5 h-5" />
                </div>
                <span class="text-xs text-gray-500 truncate flex-1">{{ receiptName }}</span>
                <button @click="removeReceipt" class="p-2 rounded-lg text-gray-400 hover:text-red-500 hover:bg-gray-200 dark:hover:bg-gray-700 transition-colors shrink-0" :aria-label="$t('finance.receipt_remove')">
                    <Trash2 class="w-4 h-4" />
                </button>
            </div>
        </div>
        
        <!-- Project Link (Only for Expense) -->
        <div v-if="type === 'expense' && projects && projects.length > 0">
            <label class="block text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-1">{{ $t('finance.link_project') }}</label>
            <select v-model="projectId" class="w-full bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl px-3 py-2.5 text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500 appearance-none">
                <option value="">{{ $t('finance.no_project') }}</option>
                <option v-for="p in projects" :key="p.id" :value="p.id">{{ p.title }}</option>
            </select>
        </div>

        <!-- Person Link (Only for Debt categories) -->
        <div v-if="people && people.length > 0">
            <label class="block text-xs font-semibold text-gray-500 dark:text-gray-400 uppercase tracking-wider mb-1">{{ $t('finance.link_person') }}</label>
            <div class="relative">
                <input 
                    type="text" 
                    v-model="personSearch" 
                    @focus="openPersonDropdown"
                    @blur="closePersonDropdown"
                    class="w-full bg-gray-50 dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl px-3 py-2.5 text-sm text-text dark:text-text-dark focus:outline-none focus:ring-2 focus:ring-blue-500" 
                    :placeholder="personId ? getPersonName(personId) : 'Search person...'" 
                />
                <X v-if="personId" @click="personId = ''; personSearch = ''" class="w-4 h-4 absolute right-3 top-1/2 -translate-y-1/2 text-gray-400 cursor-pointer hover:text-red-500 transition-colors" />
                
                <div v-if="isPersonDropdownOpen" class="absolute z-10 w-full mt-1 bg-white dark:bg-gray-800 border border-border dark:border-border-dark rounded-xl shadow-lg max-h-48 overflow-y-auto hidden-scrollbar py-1">
                    <div 
                        class="px-3 py-2 text-sm text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-700 cursor-pointer transition-colors"
                        @click="personId = ''; personSearch = ''; isPersonDropdownOpen = false"
                    >
                        No person
                    </div>
                    <div 
                        v-for="p in filteredPeople" 
                        :key="p.id" 
                        class="px-3 py-2 text-sm text-text dark:text-text-dark hover:bg-blue-50 dark:hover:bg-blue-900/20 cursor-pointer transition-colors"
                        @click="personId = p.id; personSearch = ''; isPersonDropdownOpen = false"
                    >
                        {{ p.title }}
                    </div>
                    <div v-if="filteredPeople.length === 0" class="px-3 py-2 text-sm text-gray-400 italic">
                        No matching people
                    </div>
                </div>
            </div>
        </div>

      </div>

      <!-- Footer -->
      <div class="p-4 border-t border-border dark:border-border-dark flex justify-between gap-3 bg-gray-50/50 dark:bg-gray-800/50">
        <div>
            <button v-if="transaction" @click="emit('delete', transaction.id)" class="px-3 py-2 rounded-xl text-sm font-medium text-red-500 hover:bg-red-50 dark:hover:bg-red-900/20 transition-colors flex items-center gap-1">
                <Trash2 class="w-4 h-4" />
                Delete
            </button>
        </div>
        <div class="flex gap-3">
            <button @click="emit('close')" class="px-4 py-2 rounded-xl text-sm font-medium text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 transition-colors">
                Cancel
            </button>
            <button @click="save" class="px-5 py-2 rounded-xl text-sm font-medium bg-blue-500 hover:bg-blue-600 text-white shadow-sm transition-colors">
                Save Transaction
            </button>
        </div>
      </div>

    </div>
  </div>
</template>
