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

export interface Budget {
  categoryId: string; // The expense category this budget applies to
  amount: number;     // The maximum amount allowed per month
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
export const DEFAULT_INCOME_CATEGORIES = ['Salary', 'Bonus', 'Interest', 'Gift', ...SYSTEM_INCOME_CATEGORIES, 'Other'];

export const SYSTEM_EXPENSE_CATEGORIES = ['Lending', 'Debt Repayment'];
export const DEFAULT_EXPENSE_CATEGORIES = ['Food & Dining', 'Transportation', 'Shopping', 'Bills & Utilities', 'Entertainment', 'Health', 'Education', ...SYSTEM_EXPENSE_CATEGORIES, 'Other'];

export const DEFAULT_ACCOUNTS: FinanceAccount[] = [
  { id: 'acc-1', name: 'Cash', initialBalance: 0 },
  { id: 'acc-2', name: 'Bank Account', initialBalance: 0 },
  { id: 'acc-3', name: 'Credit Card', initialBalance: 0 }
];
