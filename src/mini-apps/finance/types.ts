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
}

export interface Debt {
  id: string;
  type: 'borrow' | 'lend';
  person: string;
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
  };
}

export const SYSTEM_INCOME_CATEGORIES = ['Đi vay', 'Thu nợ'];
export const DEFAULT_INCOME_CATEGORIES = ['Lương', 'Thưởng', 'Tiền lãi', 'Được tặng', ...SYSTEM_INCOME_CATEGORIES, 'Khác'];

export const SYSTEM_EXPENSE_CATEGORIES = ['Cho vay', 'Trả nợ'];
export const DEFAULT_EXPENSE_CATEGORIES = ['Ăn uống', 'Di chuyển', 'Mua sắm', 'Hóa đơn & Tiện ích', 'Giải trí', 'Sức khỏe', 'Giáo dục', ...SYSTEM_EXPENSE_CATEGORIES, 'Khác'];

export const DEFAULT_ACCOUNTS: FinanceAccount[] = [
  { id: 'acc-1', name: 'Tiền mặt', initialBalance: 0 },
  { id: 'acc-2', name: 'Tài khoản Ngân hàng', initialBalance: 0 },
  { id: 'acc-3', name: 'Thẻ Tín dụng', initialBalance: 0 }
];
