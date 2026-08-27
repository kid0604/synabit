import { describe, it, expect } from 'vitest';
import {
  accountBalances,
  budgetSpent,
  byDateDescending,
  dayLabel,
  effectiveBudgetAmount,
  filterTransactions,
  groupByDate,
  isBudgetOverridden,
  monthNetFlow,
  netWorth,
  netWorthTrend,
  totals,
  transactionsBetween,
  transactionsInMonth,
  type MonthBucket,
} from '../calc';
import { accountUsage, accountsTotal, budgetedCategories, categoryUsage, debtTotals, isDebtTransaction } from '../calc';
import type { BudgetItem, Debt, FinanceAccount, Transaction } from '../types';

const debt = (over: Partial<Debt> & Pick<Debt, 'type' | 'totalAmount'>): Debt => ({
  id: `debt-${++seq}`,
  person: 'Mai',
  paidAmount: 0,
  startDate: '2026-08-01T00:00:00',
  accountId: 'cash',
  note: '',
  status: 'active',
  ...over,
});

/**
 * There are no `it.fails` left here. The two that were — net worth disagreeing
 * with the accounts under it, and borrowing money making the user look richer
 * — are ordinary tests now; they were roadmap 3.1 and 3.4.
 */

let seq = 0;
const tx = (over: Partial<Transaction> & Pick<Transaction, 'type' | 'amount'>): Transaction => ({
  id: `tx-${++seq}`,
  category: 'Food & Dining',
  accountId: 'cash',
  date: '2026-08-15T10:00:00',
  note: '',
  ...over,
});

const account = (id: string, initialBalance = 0): FinanceAccount => ({
  id,
  name: id,
  initialBalance,
});

const month = (id: string, year: number, m: number, transactions: Transaction[]): MonthBucket => ({
  id,
  date: new Date(year, m - 1, 1),
  transactions,
});

describe('accountBalances', () => {
  it('starts from what the account opened with', () => {
    const balances = accountBalances([account('cash', 500)], []);
    expect(balances).toEqual([{ id: 'cash', name: 'cash', balance: 500 }]);
  });

  it('adds income and takes away expense', () => {
    const balances = accountBalances(
      [account('cash', 100)],
      [
        tx({ type: 'income', amount: 60 }),
        tx({ type: 'expense', amount: 25 }),
      ],
    );
    expect(balances[0].balance).toBe(135);
  });

  it('moves a transfer out of one account and into the other', () => {
    const balances = accountBalances(
      [account('cash', 1000), account('bank', 0)],
      [tx({ type: 'transfer', amount: 300, accountId: 'cash', toAccountId: 'bank' })],
    );
    expect(balances.map((b) => b.balance)).toEqual([700, 300]);
  });

  it('only counts a transaction against the account that names it', () => {
    const balances = accountBalances(
      [account('cash'), account('bank')],
      [tx({ type: 'expense', amount: 40, accountId: 'bank' })],
    );
    expect(balances.map((b) => b.balance)).toEqual([0, -40]);
  });

  /**
   * The account is gone; the transactions that named it are not. Nothing here
   * can show that money, which is half of why the two totals drift apart —
   * see the net worth suite below.
   */
  it('ignores a transaction whose account no longer exists', () => {
    const balances = accountBalances(
      [account('cash', 100)],
      [tx({ type: 'expense', amount: 40, accountId: 'deleted-account' })],
    );
    expect(balances[0].balance).toBe(100);
  });
});

describe('netWorth', () => {
  it('is what every account opened with, plus income, less expense', () => {
    const worth = netWorth(
      [account('cash', 100), account('bank', 900)],
      [tx({ type: 'income', amount: 200 }), tx({ type: 'expense', amount: 50 })],
    );
    expect(worth).toBe(1150);
  });

  it('does not move when money is transferred between accounts', () => {
    const accounts = [account('cash', 1000), account('bank', 0)];
    const before = netWorth(accounts, []);
    const after = netWorth(accounts, [
      tx({ type: 'transfer', amount: 400, accountId: 'cash', toAccountId: 'bank' }),
    ]);
    expect(after).toBe(before);
  });

  it('agrees with the sum of the account balances', () => {
    const accounts = [account('cash', 100), account('bank', 900)];
    const transactions = [
      tx({ type: 'income', amount: 200, accountId: 'bank' }),
      tx({ type: 'expense', amount: 50, accountId: 'cash' }),
      tx({ type: 'transfer', amount: 300, accountId: 'bank', toAccountId: 'cash' }),
    ];

    const summed = accountBalances(accounts, transactions).reduce((n, a) => n + a.balance, 0);
    expect(netWorth(accounts, transactions)).toBe(summed);
  });

  /**
   * Roadmap 3.4. Deleting an account leaves its transactions behind. They used
   * to stop counting towards any account balance while still counting towards
   * net worth, so the headline figure and the list under it disagreed for good.
   */
  it('agrees with the sum of the account balances after an account is deleted', () => {
    const accounts = [account('cash', 100)];
    const transactions = [tx({ type: 'expense', amount: 40, accountId: 'deleted-account' })];

    const summed = accountBalances(accounts, transactions).reduce((n, a) => n + a.balance, 0);
    expect(netWorth(accounts, transactions)).toBe(summed);
  });

  /**
   * Roadmap 3.1. The money arrives in an account and the obligation arrives in
   * the debts ledger, and the two cancel. Before this, only the first half was
   * counted, so borrowing five thousand made the user five thousand richer.
   */
  it('is unchanged by borrowing money', () => {
    const accounts = [account('cash', 1_000)];
    const before = netWorth(accounts, [], []);

    const after = netWorth(
      accounts,
      [tx({ type: 'income', amount: 5_000, category: 'Borrowing', accountId: 'cash' })],
      [debt({ type: 'borrow', totalAmount: 5_000 })],
    );

    expect(after).toBe(before);
  });

  /** Lending is the same trade in the other direction. */
  it('is unchanged by lending money', () => {
    const accounts = [account('cash', 5_000)];
    const before = netWorth(accounts, [], []);

    const after = netWorth(
      accounts,
      [tx({ type: 'expense', amount: 2_000, category: 'Lending', accountId: 'cash' })],
      [debt({ type: 'lend', totalAmount: 2_000 })],
    );

    expect(after).toBe(before);
  });

  /** Repaying moves money out of an account and the same amount off the debt. */
  it('is unchanged by repaying a debt', () => {
    const accounts = [account('cash', 6_000)];
    const borrowed = [tx({ type: 'income', amount: 5_000, category: 'Borrowing', accountId: 'cash' })];

    const owing = netWorth(accounts, borrowed, [debt({ type: 'borrow', totalAmount: 5_000 })]);

    const repaid = netWorth(
      accounts,
      [...borrowed, tx({ type: 'expense', amount: 1_000, category: 'Debt Repayment', accountId: 'cash' })],
      [debt({ type: 'borrow', totalAmount: 5_000, paidAmount: 1_000 })],
    );

    expect(repaid).toBe(owing);
  });

  it('counts a debt the user has settled as settled', () => {
    const accounts = [account('cash', 1_000)];
    const settled = debt({ type: 'borrow', totalAmount: 5_000, status: 'completed' });

    expect(netWorth(accounts, [], [settled])).toBe(1_000);
  });
});

describe('debtTotals', () => {
  it('separates what is owed to the user from what the user owes', () => {
    const totals = debtTotals([
      debt({ type: 'lend', totalAmount: 3_000 }),
      debt({ type: 'borrow', totalAmount: 8_000 }),
    ]);
    expect(totals).toEqual({ receivable: 3_000, payable: 8_000 });
  });

  it('counts only what is still outstanding', () => {
    const totals = debtTotals([debt({ type: 'lend', totalAmount: 3_000, paidAmount: 1_200 })]);
    expect(totals.receivable).toBe(1_800);
  });

  /** Somebody repaying more than they owed does not put the user in credit. */
  it('never goes negative on an overpaid debt', () => {
    const totals = debtTotals([debt({ type: 'lend', totalAmount: 1_000, paidAmount: 1_500 })]);
    expect(totals.receivable).toBe(0);
  });

  it('leaves a settled debt out', () => {
    const totals = debtTotals([
      debt({ type: 'borrow', totalAmount: 5_000, status: 'completed' }),
    ]);
    expect(totals.payable).toBe(0);
  });

  it('answers zero for a ledger with nothing in it', () => {
    expect(debtTotals([])).toEqual({ receivable: 0, payable: 0 });
  });
});

describe('accountsTotal', () => {
  it('is the sum of what every account holds', () => {
    const accounts = [account('cash', 100), account('bank', 900)];
    const transactions = [tx({ type: 'expense', amount: 50, accountId: 'cash' })];

    expect(accountsTotal(accounts, transactions)).toBe(950);
  });
});

describe('isDebtTransaction', () => {
  /**
   * Roadmap 3.2. The old answer read the category for words like "lend", which
   * matched a category called "Calendar" and missed a debt filed under a name
   * the user invented.
   */
  it('goes by the link, not by what the category is called', () => {
    expect(isDebtTransaction(tx({ type: 'expense', amount: 1, debtId: 'debt-1' }))).toBe(true);
    expect(isDebtTransaction(tx({ type: 'expense', amount: 1, category: 'Calendar' }))).toBe(false);
    expect(isDebtTransaction(tx({ type: 'expense', amount: 1, category: 'Lending' }))).toBe(false);
  });

  it('does not count an empty link as a link', () => {
    expect(isDebtTransaction(tx({ type: 'expense', amount: 1, debtId: '' }))).toBe(false);
  });
});

describe('accountUsage', () => {
  it('counts what names each account', () => {
    const counts = accountUsage([
      tx({ type: 'expense', amount: 1, accountId: 'cash' }),
      tx({ type: 'expense', amount: 2, accountId: 'cash' }),
      tx({ type: 'income', amount: 3, accountId: 'bank' }),
    ]);
    expect(counts).toEqual({ cash: 2, bank: 1 });
  });

  /** A transfer would be left pointing at nothing if either end went. */
  it('counts both ends of a transfer', () => {
    const counts = accountUsage([
      tx({ type: 'transfer', amount: 1, accountId: 'cash', toAccountId: 'bank' }),
    ]);
    expect(counts).toEqual({ cash: 1, bank: 1 });
  });

  it('says nothing about an account nothing touches', () => {
    expect(accountUsage([])['cash']).toBeUndefined();
  });
});

describe('categoryUsage', () => {
  it('counts what is filed under each category', () => {
    const counts = categoryUsage([
      tx({ type: 'expense', amount: 1, category: 'Food & Dining' }),
      tx({ type: 'expense', amount: 2, category: 'Food & Dining' }),
      tx({ type: 'income', amount: 3, category: 'Salary' }),
    ]);
    expect(counts).toEqual({ 'Food & Dining': 2, Salary: 1 });
  });
});

describe('budgetedCategories', () => {
  it('gathers every category any budget allocates to', () => {
    const used = budgetedCategories([
      { items: [{ id: 'a', name: 'Out', categories: ['Food & Dining', 'Entertainment'], amount: 1 }] },
      { items: [{ id: 'b', name: 'Travel', categories: ['Transportation'], amount: 1 }] },
    ]);
    expect([...used].sort()).toEqual(['Entertainment', 'Food & Dining', 'Transportation']);
  });

  it('is empty when nothing is budgeted', () => {
    expect(budgetedCategories([]).size).toBe(0);
    expect(budgetedCategories([{ items: [] }]).size).toBe(0);
  });
});

describe('totals', () => {
  it('separates money in from money out', () => {
    expect(
      totals([
        tx({ type: 'income', amount: 300 }),
        tx({ type: 'expense', amount: 120 }),
        tx({ type: 'expense', amount: 30 }),
      ]),
    ).toEqual({ income: 300, expense: 150, balance: 150 });
  });

  it('counts a transfer as neither', () => {
    expect(totals([tx({ type: 'transfer', amount: 500, toAccountId: 'bank' })])).toEqual({
      income: 0,
      expense: 0,
      balance: 0,
    });
  });

  it('answers zero for an empty ledger', () => {
    expect(totals([])).toEqual({ income: 0, expense: 0, balance: 0 });
  });
});

describe('transactionsInMonth', () => {
  const ledger = [
    tx({ type: 'expense', amount: 1, date: '2026-07-31T12:00:00' }),
    tx({ type: 'expense', amount: 2, date: '2026-08-01T12:00:00' }),
    tx({ type: 'expense', amount: 3, date: '2026-08-31T12:00:00' }),
    tx({ type: 'expense', amount: 4, date: '2026-09-01T12:00:00' }),
    tx({ type: 'expense', amount: 5, date: '2025-08-15T12:00:00' }),
  ];

  it('takes the whole month and nothing on either side of it', () => {
    expect(transactionsInMonth(ledger, 8, 2026).map((t) => t.amount).sort()).toEqual([2, 3]);
  });

  it('does not confuse the same month in another year', () => {
    expect(transactionsInMonth(ledger, 8, 2025).map((t) => t.amount)).toEqual([5]);
  });

  it('returns them newest first', () => {
    const august = transactionsInMonth(ledger, 8, 2026);
    expect(august.map((t) => t.amount)).toEqual([3, 2]);
  });

  /**
   * A transaction is filed in the month its date names, and the month *file*
   * it physically sits in can be a different one — editing a date moves it,
   * and until that write lands the two disagree. The date is what counts.
   */
  it('selects by the date on the transaction, not by any month file', () => {
    const misfiled = tx({ type: 'income', amount: 99, date: '2026-08-05T12:00:00' });
    expect(transactionsInMonth([misfiled], 8, 2026)).toHaveLength(1);
  });
});

describe('byDateDescending', () => {
  it('does not disturb the array it was given', () => {
    const original = [
      tx({ type: 'expense', amount: 1, date: '2026-08-01T12:00:00' }),
      tx({ type: 'expense', amount: 2, date: '2026-08-09T12:00:00' }),
    ];
    const snapshot = [...original];
    byDateDescending(original);
    expect(original).toEqual(snapshot);
  });
});

describe('groupByDate', () => {
  it('puts two transactions from the same day in one group', () => {
    const groups = groupByDate([
      tx({ type: 'expense', amount: 10, date: '2026-08-15T08:00:00' }),
      tx({ type: 'expense', amount: 20, date: '2026-08-15T22:00:00' }),
    ]);
    expect(groups).toHaveLength(1);
    expect(groups[0].transactions).toHaveLength(2);
  });

  it('totals each day separately', () => {
    const groups = groupByDate([
      tx({ type: 'income', amount: 100, date: '2026-08-15T08:00:00' }),
      tx({ type: 'expense', amount: 40, date: '2026-08-15T09:00:00' }),
      tx({ type: 'expense', amount: 7, date: '2026-08-14T09:00:00' }),
    ]);
    expect(groups[0]).toMatchObject({ totalIncome: 100, totalExpense: 40 });
    expect(groups[1]).toMatchObject({ totalIncome: 0, totalExpense: 7 });
  });

  it('leaves a transfer out of both day totals', () => {
    const groups = groupByDate([
      tx({ type: 'transfer', amount: 500, date: '2026-08-15T08:00:00', toAccountId: 'bank' }),
    ]);
    expect(groups[0]).toMatchObject({ totalIncome: 0, totalExpense: 0 });
  });

  it('puts the newest day first', () => {
    const groups = groupByDate([
      tx({ type: 'expense', amount: 1, date: '2026-08-10T12:00:00' }),
      tx({ type: 'expense', amount: 2, date: '2026-08-20T12:00:00' }),
    ]);
    expect(groups.map((g) => g.dateStr)).toEqual(['20/08/2026', '10/08/2026']);
  });
});

describe('dayLabel', () => {
  it('pads the day and the month', () => {
    expect(dayLabel(new Date(2026, 7, 5))).toBe('05/08/2026');
  });
});

describe('filterTransactions', () => {
  const ledger = [
    tx({ type: 'expense', amount: 50, category: 'Food & Dining', note: 'lunch with Mai', accountId: 'cash' }),
    tx({ type: 'income', amount: 900, category: 'Salary', note: '', accountId: 'bank' }),
    tx({ type: 'transfer', amount: 200, category: 'Transfer', accountId: 'cash', toAccountId: 'bank' }),
  ];

  it('returns everything when nothing is asked of it', () => {
    expect(filterTransactions(ledger, {})).toHaveLength(3);
  });

  it('matches the category and the note, ignoring case', () => {
    expect(filterTransactions(ledger, { query: 'FOOD' })).toHaveLength(1);
    expect(filterTransactions(ledger, { query: 'mai' })).toHaveLength(1);
  });

  it('narrows to one kind of transaction', () => {
    expect(filterTransactions(ledger, { type: 'income' })).toHaveLength(1);
  });

  /** A transfer belongs to both ends, so filtering by either account finds it. */
  it('finds a transfer from either side of it', () => {
    expect(filterTransactions(ledger, { accountId: 'cash', type: 'transfer' })).toHaveLength(1);
    expect(filterTransactions(ledger, { accountId: 'bank', type: 'transfer' })).toHaveLength(1);
  });

  it('applies every filter at once', () => {
    expect(
      filterTransactions(ledger, { query: 'lunch', type: 'expense', accountId: 'cash' }),
    ).toHaveLength(1);
    expect(
      filterTransactions(ledger, { query: 'lunch', type: 'expense', accountId: 'bank' }),
    ).toHaveLength(0);
  });
});

describe('budgets', () => {
  const item = (over: Partial<BudgetItem> = {}): BudgetItem => ({
    id: 'bi-1',
    name: 'Eating out',
    categories: ['Food & Dining'],
    amount: 3_000,
    ...over,
  });

  it('spends only against the categories the item names', () => {
    const spent = budgetSpent(item(), [
      tx({ type: 'expense', amount: 500, category: 'Food & Dining' }),
      tx({ type: 'expense', amount: 900, category: 'Transportation' }),
    ]);
    expect(spent).toBe(500);
  });

  it('does not count income filed under a budgeted category', () => {
    const spent = budgetSpent(item(), [
      tx({ type: 'income', amount: 500, category: 'Food & Dining' }),
    ]);
    expect(spent).toBe(0);
  });

  it('uses this month`s limit when one was set', () => {
    const withOverride = item({ monthlyOverrides: { '2026-08': 5_000 } });
    expect(effectiveBudgetAmount(withOverride, '2026-08', false)).toBe(5_000);
    expect(effectiveBudgetAmount(withOverride, '2026-09', false)).toBe(3_000);
  });

  /** A budget running over its own dates does not think in months at all. */
  it('ignores monthly limits on a custom-period budget', () => {
    const withOverride = item({ monthlyOverrides: { '2026-08': 5_000 } });
    expect(effectiveBudgetAmount(withOverride, '2026-08', true)).toBe(3_000);
  });

  /**
   * A limit of zero means "nothing this month", which is a thing people
   * budget. The two functions used to disagree about it: the badge said the
   * month was overridden while the figure beside it came from the default.
   */
  it('applies a limit of zero, and says it is overridden', () => {
    const zeroed = item({ monthlyOverrides: { '2026-08': 0 } });
    expect(effectiveBudgetAmount(zeroed, '2026-08', false)).toBe(0);
    expect(isBudgetOverridden(zeroed, '2026-08', false)).toBe(true);
  });
});

describe('transactionsBetween', () => {
  it('includes both ends of the window', () => {
    const ledger = [
      tx({ type: 'expense', amount: 1, date: '2026-08-01T00:00:00' }),
      tx({ type: 'expense', amount: 2, date: '2026-08-15T12:00:00' }),
      tx({ type: 'expense', amount: 3, date: '2026-08-31T23:59:59' }),
      tx({ type: 'expense', amount: 4, date: '2026-09-01T00:00:01' }),
    ];
    const window = transactionsBetween(
      ledger,
      new Date('2026-08-01T00:00:00'),
      new Date('2026-08-31T23:59:59'),
    );
    expect(window.map((t) => t.amount)).toEqual([1, 2, 3]);
  });
});

describe('monthNetFlow', () => {
  const ledger = [
    tx({ type: 'income', amount: 900, accountId: 'bank' }),
    tx({ type: 'expense', amount: 100, accountId: 'cash' }),
    tx({ type: 'transfer', amount: 300, accountId: 'bank', toAccountId: 'cash' }),
  ];

  it('sees a transfer as nothing at all across the whole vault', () => {
    expect(monthNetFlow(ledger, 'all')).toBe(800);
  });

  it('sees a transfer leave one account and arrive at the other', () => {
    expect(monthNetFlow(ledger, 'bank')).toBe(600);
    expect(monthNetFlow(ledger, 'cash')).toBe(200);
  });
});

describe('netWorthTrend', () => {
  const july = month('Finance/2026-07.json', 2026, 7, [tx({ type: 'income', amount: 200 })]);
  const august = month('Finance/2026-08.json', 2026, 8, [tx({ type: 'expense', amount: 50 })]);

  it('ends at the balance it was given', () => {
    const points = netWorthTrend([july, august], 1_000, 'all');
    expect(points[points.length - 1]).toMatchObject({ id: august.id, value: 1_000 });
  });

  /** August lost 50, so the month before it must have closed 50 higher. */
  it('walks backwards, undoing each month in turn', () => {
    const points = netWorthTrend([july, august], 1_000, 'all');
    expect(points.map((p) => p.value)).toEqual([1_050, 1_000]);
  });

  it('returns the months oldest first, whatever order they arrived in', () => {
    const points = netWorthTrend([august, july], 1_000, 'all');
    expect(points.map((p) => p.id)).toEqual([july.id, august.id]);
  });

  it('is flat across a month in which nothing happened', () => {
    const quiet = month('Finance/2026-09.json', 2026, 9, []);
    const points = netWorthTrend([august, quiet], 1_000, 'all');
    expect(points[0].value).toBe(points[1].value);
  });
});
