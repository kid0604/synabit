export type TransactionType = 'income' | 'expense' | 'transfer';

export interface Transaction {
  id: string;
  type: TransactionType;
  amount: number;
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

export interface FinanceAccount {
  id: string;
  name: string;
  initialBalance: number;
}

export interface FinanceConfig {
  title: string;
  type: 'finance_config';
  metadata: {
    incomeCategories: string[];
    expenseCategories: string[];
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
