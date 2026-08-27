export type TransactionType = 'income' | 'expense' | 'transfer';

export interface Transaction {
  id: string;
  type: TransactionType;
  /** In the vault currency's minor units. See `currency.ts`. */
  amount: number;
  /**
   * The **id** of a category, not its name.
   *
   * For every category that existed before categories had ids, the two are the
   * same string — that is what the migration chose, precisely so that no
   * transaction had to be rewritten to gain one. Renaming a category changes
   * its `name` and leaves this alone, which is how a year of history stays
   * attached to a category the user decided to call something else.
   *
   * Read it through `categoryName`; falling back to the id itself displays the
   * original name, which is the right answer for a category that is gone.
   */
  category: string;
  accountId: string;
  toAccountId?: string;
  date: string; // ISO string
  note: string;
  debtId?: string;
  projectId?: string;
  personId?: string;
  originalCurrency?: string;
  originalAmount?: number;
  exchangeRate?: number;
  /** The repeating rule that produced this, if one did. See `recurring.ts`. */
  recurringRuleId?: string;
  /**
   * A photo of the receipt, as a vault-relative `assets/…` path.
   *
   * Stored in the vault's own assets folder rather than as a path to wherever
   * the picture happened to be, because that folder is what sync carries. A
   * receipt that lives in Downloads is a receipt only one device has.
   */
  receipt?: string;
}

export interface Debt {
  id: string;
  type: 'borrow' | 'lend';
  person: string;
  personId?: string;
  totalAmount: number;
  paidAmount: number;
  startDate: string;
  dueDate?: string;
  accountId: string; // Account that the money was sent from / received to
  note: string;
  status: 'active' | 'completed';
}

export type BudgetType = 'monthly' | 'custom';

export interface BudgetItem {
  id: string;
  name: string;
  categories: string[];
  amount: number;
  monthlyOverrides?: Record<string, number>; // key: "YYYY-MM" → override amount for that month
}

export interface Budget {
  id: string;
  name: string;           // e.g. "Monthly Budget", "Business 2026"
  type?: BudgetType;      // 'monthly' (default) or 'custom'
  items: BudgetItem[];    // Sub-items (category allocations)
  startDate?: string;     // ISO date — only for type 'custom'
  endDate?: string;       // ISO date — only for type 'custom'
}

export interface FinanceMonth {
  title: string;
  type: 'finance_month';
  metadata: {
    transactions: Transaction[];
  };
}

/**
 * What kind of thing an account is.
 *
 * Absent means nobody has said. The app shipped a default account called
 * "Credit Card" with no concept of a credit card behind it, and guessing the
 * kind back from the name would be the same kind of guess that used to decide
 * whether a transaction was a debt.
 */
export type AccountType = 'cash' | 'bank' | 'credit' | 'investment' | 'other';

export const ACCOUNT_TYPES: AccountType[] = ['cash', 'bank', 'credit', 'investment', 'other'];

export interface FinanceAccount {
  id: string;
  name: string;
  /** In minor units, and allowed to be negative — a credit card starts there. */
  initialBalance: number;
  type?: AccountType;
}

/**
 * A category, as something that can be renamed.
 *
 * Categories used to be bare strings, which meant a transaction referred to one
 * by its name. Renaming "Food & Dining" to "Ăn uống" therefore did not rename
 * anything: it removed one category and added another, and every transaction
 * ever filed under the old name stopped appearing in any breakdown while still
 * counting towards the totals beside it.
 */
export interface Category {
  id: string;
  name: string;
}

export interface FinanceConfig {
  title: string;
  type: 'finance_config';
  metadata: {
    incomeCategories: Category[];
    expenseCategories: Category[];
    accounts: FinanceAccount[];
    budgets?: Budget[];
    currency?: string;
  };
}

export const SYSTEM_INCOME_CATEGORIES = ['Borrowing', 'Debt Collection'];
export const DEFAULT_INCOME_CATEGORIES = ['Salary', 'Bonus', 'Allowance', 'Savings Interest', 'Investment Return', 'Gift', 'Business', 'Freelance', ...SYSTEM_INCOME_CATEGORIES, 'Other Income'];

export const SYSTEM_EXPENSE_CATEGORIES = ['Lending', 'Debt Repayment'];
export const DEFAULT_EXPENSE_CATEGORIES = ['Food & Dining', 'Transportation', 'Bills & Utilities', 'Housing', 'Gifts & Donations', 'Health & Medical', 'Clothing', 'Entertainment', 'Education', 'Family & Kids', 'Investment', 'Insurance', ...SYSTEM_EXPENSE_CATEGORIES, 'Other Expense'];

export const DEFAULT_ACCOUNTS: FinanceAccount[] = [
  { id: 'acc-1', name: 'Cash', initialBalance: 0 },
  { id: 'acc-2', name: 'Bank Account', initialBalance: 0 },
  { id: 'acc-3', name: 'Credit Card', initialBalance: 0 }
];
