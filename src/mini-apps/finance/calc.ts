/**
 * Every number Finance puts on screen, as functions rather than as `computed`.
 *
 * These lived inside `FinanceApp.vue` and `FinanceBudgets.vue`, which meant the
 * one part of the app that adds up money was the one part no test could reach.
 *
 * **Every amount here is an integer number of minor units** — cents, xu. That
 * is what makes plain `+` safe over ten thousand rows. Turning one back into
 * the 12.50 a person reads is `formatCurrency`'s job and nobody else's; see
 * `currency.ts`.
 *
 * `netWorth` is the one figure here that is not a plain sum, and it is worth
 * knowing why. It used to add up every transaction ever recorded, which meant
 * two things were wrong at once: borrowing money made the user look richer by
 * exactly the amount they now owed, and a transaction naming a deleted account
 * still counted even though no account balance included it. It is now built
 * from the account balances themselves, plus what is owed to the user and less
 * what the user owes — so both of those stop being possible.
 */

import type { BudgetItem, Debt, FinanceAccount, Transaction } from './types';

/** One month's worth of the ledger, without the node it came out of. */
export interface MonthBucket {
  id: string;
  date: Date;
  transactions: Transaction[];
}

export interface AccountBalance {
  id: string;
  name: string;
  balance: number;
}

export interface Totals {
  income: number;
  expense: number;
  balance: number;
}

export interface DayGroup {
  /** `YYYY-MM-DD` — stable, sortable, never shown to anyone. */
  key: string;
  /** What the day is called on screen. */
  dateStr: string;
  date: Date;
  transactions: Transaction[];
  totalIncome: number;
  totalExpense: number;
}

export interface TrendPoint {
  id: string;
  date: Date;
  value: number;
}

/** Every account a transfer can name, matched against one account. */
const touches = (tx: Transaction, accountId: string): boolean =>
  tx.accountId === accountId || tx.toAccountId === accountId;

/**
 * The balance of each account: what it opened with, plus everything that has
 * moved through it since.
 *
 * A transfer leaves the account it names and arrives at `toAccountId`, so it
 * is subtracted from one side and added to the other. That is also why it does
 * not appear in `netWorth` — the money never left.
 */
export function accountBalances(
  accounts: FinanceAccount[],
  transactions: Transaction[],
): AccountBalance[] {
  return accounts.map((acc) => {
    let income = 0;
    let expense = 0;

    for (const tx of transactions) {
      if (tx.accountId === acc.id) {
        if (tx.type === 'income') income += tx.amount;
        else if (tx.type === 'expense') expense += tx.amount;
        else if (tx.type === 'transfer') expense += tx.amount;
      }
      if (tx.toAccountId === acc.id && tx.type === 'transfer') {
        income += tx.amount;
      }
    }

    return { id: acc.id, name: acc.name, balance: acc.initialBalance + income - expense };
  });
}

/** Everything sitting in accounts, which is the sum of what each one holds. */
export function accountsTotal(
  accounts: FinanceAccount[],
  transactions: Transaction[],
): number {
  return accountBalances(accounts, transactions).reduce((total, a) => total + a.balance, 0);
}

export interface DebtTotals {
  /** Lent out and not yet repaid: money the user is owed. */
  receivable: number;
  /** Borrowed and not yet repaid: money the user owes. */
  payable: number;
}

/**
 * What is still outstanding on both sides of the debts ledger.
 *
 * Only active debts count. Marking a debt completed is the user saying it is
 * settled, whatever the arithmetic says — somebody forgave the last of it, or
 * it was written off — and a screen that kept counting it would be arguing
 * with them.
 */
export function debtTotals(debts: Debt[]): DebtTotals {
  let receivable = 0;
  let payable = 0;

  for (const debt of debts) {
    if (debt.status !== 'active') continue;
    const outstanding = Math.max(0, debt.totalAmount - debt.paidAmount);
    if (debt.type === 'lend') receivable += outstanding;
    else payable += outstanding;
  }

  return { receivable, payable };
}

/**
 * Everything the user owns, less what they owe.
 *
 * Built from the account balances rather than from the transactions directly,
 * which is what makes the headline figure and the list of accounts underneath
 * it agree: a transaction naming an account that no longer exists is in
 * neither.
 *
 * Borrowing lands in an account *and* in the debts ledger, so the two cancel
 * and borrowing money leaves this unchanged — which is the whole point. Lending
 * is the same in reverse: the cash leaves an account and becomes a receivable.
 */
export function netWorth(
  accounts: FinanceAccount[],
  transactions: Transaction[],
  debts: Debt[] = [],
): number {
  const { receivable, payable } = debtTotals(debts);
  return accountsTotal(accounts, transactions) + receivable - payable;
}

/**
 * Whether this transaction is one side of a debt.
 *
 * Answered by the link the transaction carries, not by reading its category
 * for words like "lend" or "vay". That guess was wrong in both directions: a
 * category called "Calendar" contains "lend", and a debt filed under a name
 * the user invented contained none of them.
 */
export function isDebtTransaction(tx: Transaction): boolean {
  return typeof tx.debtId === 'string' && tx.debtId.length > 0;
}

/**
 * How many transactions name each account.
 *
 * Both ends of a transfer count, because deleting either account would leave
 * the transfer pointing at nothing. Used to stop an account being removed out
 * from under the money that moved through it — which used to be possible, and
 * left those transactions unreachable and uncounted for good.
 */
export function accountUsage(transactions: Transaction[]): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const tx of transactions) {
    if (tx.accountId) counts[tx.accountId] = (counts[tx.accountId] ?? 0) + 1;
    if (tx.toAccountId) counts[tx.toAccountId] = (counts[tx.toAccountId] ?? 0) + 1;
  }
  return counts;
}

/** How many transactions are filed under each category. */
export function categoryUsage(transactions: Transaction[]): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const tx of transactions) {
    if (tx.category) counts[tx.category] = (counts[tx.category] ?? 0) + 1;
  }
  return counts;
}

/** Which categories a budget has allocations for, and cannot lose. */
export function budgetedCategories(budgets: { items?: BudgetItem[] }[]): Set<string> {
  const used = new Set<string>();
  for (const budget of budgets) {
    for (const item of budget.items ?? []) {
      for (const category of item.categories ?? []) used.add(category);
    }
  }
  return used;
}

/** Money in, money out, and the difference. Transfers are neither. */
export function totals(transactions: Transaction[]): Totals {
  let income = 0;
  let expense = 0;
  for (const tx of transactions) {
    if (tx.type === 'income') income += tx.amount;
    else if (tx.type === 'expense') expense += tx.amount;
  }
  return { income, expense, balance: income - expense };
}

/** Newest first, which is the order every list in the app shows. */
export function byDateDescending(transactions: Transaction[]): Transaction[] {
  return [...transactions].sort(
    (a, b) => new Date(b.date).getTime() - new Date(a.date).getTime(),
  );
}

/**
 * The transactions that fall in one calendar month, newest first.
 *
 * Selected by the date on the transaction, not by the month file it is stored
 * in. Those can differ: editing a transaction's date moves it between files,
 * and until that write lands the two disagree.
 */
export function transactionsInMonth(
  transactions: Transaction[],
  month: number,
  year: number,
): Transaction[] {
  return byDateDescending(
    transactions.filter((tx) => {
      const d = new Date(tx.date);
      return d.getMonth() + 1 === month && d.getFullYear() === year;
    }),
  );
}

/**
 * How a day is labelled in the ledger.
 *
 * Day-first, because the app was written for a day-first audience. This is the
 * single place that decision is made, so making it follow the locale later is
 * a change to one function rather than a hunt through templates.
 */
export function dayLabel(date: Date): string {
  const dd = date.getDate().toString().padStart(2, '0');
  const mm = (date.getMonth() + 1).toString().padStart(2, '0');
  return `${dd}/${mm}/${date.getFullYear()}`;
}

/** The key a day is grouped under: sortable, and never shown. */
function dayKey(date: Date): string {
  const mm = (date.getMonth() + 1).toString().padStart(2, '0');
  const dd = date.getDate().toString().padStart(2, '0');
  return `${date.getFullYear()}-${mm}-${dd}`;
}

/** Transactions gathered into days, newest day first. */
export function groupByDate(transactions: Transaction[]): DayGroup[] {
  const groups = new Map<string, DayGroup>();

  for (const tx of transactions) {
    const d = new Date(tx.date);
    const key = dayKey(d);
    let group = groups.get(key);
    if (!group) {
      group = {
        key,
        dateStr: dayLabel(d),
        date: new Date(d.getFullYear(), d.getMonth(), d.getDate()),
        transactions: [],
        totalIncome: 0,
        totalExpense: 0,
      };
      groups.set(key, group);
    }
    group.transactions.push(tx);
    if (tx.type === 'income') group.totalIncome += tx.amount;
    if (tx.type === 'expense') group.totalExpense += tx.amount;
  }

  return [...groups.values()].sort((a, b) => b.date.getTime() - a.date.getTime());
}

/**
 * The limit a budget item carries in one month.
 *
 * A monthly budget can be overridden month by month; a custom-period budget
 * runs on one figure throughout, so its overrides are ignored rather than
 * applied to a month it does not think in.
 *
 * A limit of zero is a limit — "nothing this month" — so the override is
 * looked for by presence and not by truthiness. Reading it truthily is what
 * used to make the badge say a month was overridden while the figure beside it
 * came from the default.
 */
export function effectiveBudgetAmount(
  item: BudgetItem,
  monthKey: string,
  isCustomPeriod: boolean,
): number {
  const override = item.monthlyOverrides?.[monthKey];
  if (!isCustomPeriod && override !== undefined) return override;
  return item.amount;
}

/** Whether this month's limit was set by hand. */
export function isBudgetOverridden(
  item: BudgetItem,
  monthKey: string,
  isCustomPeriod: boolean,
): boolean {
  return (
    !isCustomPeriod &&
    !!item.monthlyOverrides &&
    item.monthlyOverrides[monthKey] !== undefined
  );
}

/** What has been spent against a budget item, out of the transactions given. */
export function budgetSpent(item: BudgetItem, transactions: Transaction[]): number {
  let spent = 0;
  for (const tx of transactions) {
    if (tx.type === 'expense' && item.categories.includes(tx.category)) {
      spent += tx.amount;
    }
  }
  return spent;
}

/** Transactions between two instants, inclusive at both ends. */
export function transactionsBetween(
  transactions: Transaction[],
  start: Date,
  end: Date,
): Transaction[] {
  const from = start.getTime();
  const to = end.getTime();
  return transactions.filter((tx) => {
    const t = new Date(tx.date).getTime();
    return t >= from && t <= to;
  });
}

/**
 * How much one month moved the balance, from the point of view of one account
 * or of the whole vault.
 *
 * Seen from the whole vault a transfer is invisible; seen from one account it
 * is a departure or an arrival. That is the only difference between the two
 * branches.
 */
export function monthNetFlow(transactions: Transaction[], accountId: string | 'all'): number {
  let flow = 0;
  for (const tx of transactions) {
    if (accountId === 'all') {
      if (tx.type === 'income') flow += tx.amount;
      else if (tx.type === 'expense') flow -= tx.amount;
      continue;
    }
    if (tx.accountId === accountId) {
      if (tx.type === 'income') flow += tx.amount;
      else if (tx.type === 'expense' || tx.type === 'transfer') flow -= tx.amount;
    }
    if (tx.toAccountId === accountId && tx.type === 'transfer') {
      flow += tx.amount;
    }
  }
  return flow;
}

/**
 * The balance at the end of each month, worked out backwards from today.
 *
 * Forwards would need an opening balance nobody records; backwards needs only
 * the balance now, which the app already knows exactly. So the walk starts at
 * `closingBalance` and un-does each month in turn.
 *
 * Returned oldest first, which is the order a chart draws.
 */
export function netWorthTrend(
  months: MonthBucket[],
  closingBalance: number,
  accountId: string | 'all',
): TrendPoint[] {
  const newestFirst = [...months].sort((a, b) => b.date.getTime() - a.date.getTime());
  const points: TrendPoint[] = [];
  let running = closingBalance;

  for (const month of newestFirst) {
    points.unshift({ id: month.id, date: month.date, value: running });
    running -= monthNetFlow(month.transactions, accountId);
  }

  return points;
}

/**
 * Filter a list the way the ledger's search box does.
 *
 * Matches the category and the note, and nothing else — an amount typed into
 * the box finds nothing, which is what the box does today.
 */
export function filterTransactions(
  transactions: Transaction[],
  filters: {
    query?: string;
    type?: 'all' | 'income' | 'expense' | 'transfer';
    accountId?: string;
  },
): Transaction[] {
  const query = filters.query?.trim().toLowerCase() ?? '';
  const type = filters.type ?? 'all';
  const accountId = filters.accountId ?? 'all';

  return transactions.filter((tx) => {
    if (query) {
      const inCategory = tx.category.toLowerCase().includes(query);
      const inNote = !!tx.note && tx.note.toLowerCase().includes(query);
      if (!inCategory && !inNote) return false;
    }
    if (type !== 'all' && tx.type !== type) return false;
    if (accountId !== 'all' && !touches(tx, accountId)) return false;
    return true;
  });
}
