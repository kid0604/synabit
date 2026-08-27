/**
 * Which of the two things a stored amount can mean, and how to tell.
 *
 * Finance used to store amounts as whole units — 12 for twelve dollars, and no
 * way at all to say twelve fifty. It now stores minor units, so the same
 * twelve dollars is 1250. The number on disk looks identical either way, so a
 * file has to say which it is; that is what `financeSchema` is for.
 *
 * The conversion itself is done once, on disk, by
 * `src-tauri/src/utils/finance_storage.rs`. This module is the safety net
 * underneath it: a vault the repair could not reach is read correctly anyway,
 * scaled as it is loaded. Without that, a failed migration would show every
 * amount at a hundredth of its value, which looks like the ledger lost its
 * money rather than like a repair that did not run.
 *
 * Nothing here rounds: multiplying by a power of ten is exact.
 */

import { currencyScale } from './currency';
import type { Budget, Debt, FinanceAccount, Transaction } from './types';

/** Amounts are minor units, and the file says so. */
export const FINANCE_SCHEMA = 2;

/** What a node's `properties` look like from here. Deliberately loose. */
type Properties = Record<string, unknown> | null | undefined;

/**
 * Whether this node's amounts are already minor units.
 *
 * A file written before the marker existed has no `financeSchema`, and that
 * absence is the answer: it is schema 1.
 */
export const isCurrentSchema = (properties: Properties): boolean => {
  const stamped = (properties as { financeSchema?: unknown } | null | undefined)?.financeSchema;
  return typeof stamped === 'number' && stamped >= FINANCE_SCHEMA;
};

/** The marker to write alongside whatever is being saved. */
export const schemaStamp = () => ({ financeSchema: FINANCE_SCHEMA });

const scaleUp = (major: unknown, factor: number): number =>
  typeof major === 'number' && Number.isFinite(major) ? major * factor : 0;

/**
 * Bring one month node's transactions up to minor units, in place.
 *
 * In place rather than copied, because the app writes the whole node back when
 * a transaction is saved. A copy would leave the original — the thing that
 * actually gets written — still in whole units, and the next save would put
 * the old numbers back.
 */
export function normalizeMonthNode(node: { properties?: Properties }, currency: string): boolean {
  const properties = node.properties as { transactions?: Transaction[] } | undefined;
  if (!properties || isCurrentSchema(properties)) return false;

  const factor = 10 ** currencyScale(currency);
  for (const tx of properties.transactions ?? []) {
    tx.amount = scaleUp(tx.amount, factor);
    if (typeof tx.originalAmount === 'number') {
      // Recorded in the currency it was typed in, which may round differently
      // from the vault's own.
      tx.originalAmount = scaleUp(tx.originalAmount, 10 ** currencyScale(tx.originalCurrency ?? currency));
    }
    // `exchangeRate` is a ratio between whole units and is not touched.
  }

  (properties as Record<string, unknown>).financeSchema = FINANCE_SCHEMA;
  return true;
}

/** The same for the config node: opening balances and budget limits. */
export function normalizeConfigNode(node: { properties?: Properties }, currency: string): boolean {
  const properties = node.properties as
    | { accounts?: FinanceAccount[]; budgets?: Budget[] }
    | undefined;
  if (!properties || isCurrentSchema(properties)) return false;

  const factor = 10 ** currencyScale(currency);

  for (const account of properties.accounts ?? []) {
    account.initialBalance = scaleUp(account.initialBalance, factor);
  }

  for (const budget of properties.budgets ?? []) {
    for (const item of budget.items ?? []) {
      item.amount = scaleUp(item.amount, factor);
      if (item.monthlyOverrides) {
        for (const month of Object.keys(item.monthlyOverrides)) {
          item.monthlyOverrides[month] = scaleUp(item.monthlyOverrides[month], factor);
        }
      }
    }
  }

  (properties as Record<string, unknown>).financeSchema = FINANCE_SCHEMA;
  return true;
}

/** And the debts ledger. */
export function normalizeDebtsNode(node: { properties?: Properties }, currency: string): boolean {
  const properties = node.properties as { debts?: Debt[] } | undefined;
  if (!properties || isCurrentSchema(properties)) return false;

  const factor = 10 ** currencyScale(currency);
  for (const debt of properties.debts ?? []) {
    debt.totalAmount = scaleUp(debt.totalAmount, factor);
    debt.paidAmount = scaleUp(debt.paidAmount, factor);
  }

  (properties as Record<string, unknown>).financeSchema = FINANCE_SCHEMA;
  return true;
}
