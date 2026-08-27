import { invoke } from '@tauri-apps/api/core';

import { logger } from '../../utils/logger';

import { toCategories } from './categories';
import { currencyScaleTable } from './currency';
import { schemaStamp } from './schema';
import type { Category, FinanceAccount, Transaction } from './types';

/**
 * Writing a single transaction into the ledger, from outside the Finance app.
 *
 * QuickCap can promote a cap into a transaction, and that needs the same two
 * facts FinanceApp works from: which file a given date belongs in, and what
 * shape the transactions inside it take. Both were previously spelled out
 * inline wherever they were needed.
 *
 * The month path especially: `Finance/YYYY-MM.json` appears in several places,
 * and a second spelling of it would not fail — it would quietly create a
 * parallel month file that neither screen shows in full.
 */

/** Where the repeating rules live. */
export const RECURRING_PATH = 'Finance/Recurring.json';

/** The node a transaction on this date belongs to. */
export function monthNodePath(date: Date): string {
  const year = date.getFullYear();
  const month = `${date.getMonth() + 1}`.padStart(2, '0');
  return `Finance/${year}-${month}.json`;
}

/** What that node is called when it has to be created. */
export function monthNodeTitle(date: Date): string {
  const year = date.getFullYear();
  const month = `${date.getMonth() + 1}`.padStart(2, '0');
  return `Month ${month}/${year}`;
}

/**
 * Bring an older vault up to the shape Finance reads, once per vault.
 *
 * Lives here rather than in `FinanceApp.vue` because Finance is not the only
 * thing that writes to the ledger: QuickCap can promote a captured note into a
 * transaction without Finance ever having been opened. Writing a minor-unit row
 * into a month still stored in whole units is refused by the backend, so
 * whoever writes first has to be the one that repairs.
 *
 * Returns the number of files that could not be repaired. Zero is the happy
 * answer and also the answer for a vault that needed nothing.
 */
export async function repairFinanceStorage(vaultPath: string): Promise<number> {
  if (!vaultPath) return 0;
  const key = `finance-storage-v2:${vaultPath}`;

  if (await invoke<string | null>('get_migration_flag', { key })) return 0;

  const report = await invoke<{ changed: number; unchanged: number; failed: number }>(
    'migrate_finance_storage',
    {
      vaultPath,
      // One table, shared with the migration. See `currencyScaleTable`.
      scales: currencyScaleTable(),
      defaultScale: 2,
    },
  );

  logger.info(
    `Finance storage repair: ${report.changed} repaired, ${report.unchanged} already current, ${report.failed} failed`,
  );

  // Only a clean pass is recorded, so an interrupted one is retried.
  if (report.failed === 0) {
    await invoke('set_migration_flag', { key, value: new Date().toISOString() });
  }
  return report.failed;
}

export interface FinanceSetup {
  accounts: FinanceAccount[];
  /**
   * `Category[]`, not `string[]`: the transaction form keys its options on
   * `id` and labels them with `name`. Declared as strings, this handed
   * `TransactionModal` a list it renders as `<option :value="undefined">` —
   * a dropdown of blank rows that files every transaction under no category
   * at all. See `./categories`.
   */
  incomeCategories: Category[];
  expenseCategories: Category[];
  /**
   * The vault's currency, which decides how many digits every amount has.
   *
   * Carried out of here because a caller that opens Finance's transaction form
   * without it would read what the user typed at the wrong scale — a hundred
   * dollars entered against a đồng ledger, or the reverse.
   */
  currency: string;
}

/**
 * The accounts and categories a transaction form needs, or `null` if this
 * vault has no Finance configuration yet.
 *
 * `null` is a real answer rather than an error: somebody who has never opened
 * Finance has no accounts, and a transaction cannot exist without one. The
 * caller is expected to offer nothing rather than offer something that will
 * be refused.
 */
export async function loadFinanceSetup(): Promise<FinanceSetup | null> {
  const configs = await invoke<{ properties?: Record<string, unknown> }[]>('get_nodes', {
    nodeType: 'finance_config',
  });
  const properties = configs[0]?.properties as
    | {
        accounts?: FinanceAccount[];
        incomeCategories?: unknown;
        expenseCategories?: unknown;
        currency?: string;
      }
    | undefined;

  const currency = properties?.currency ?? 'USD';

  // The config node may predate minor units, in which case its opening
  // balances are whole units. Only the currency is read here, so nothing needs
  // scaling — but the caller has to be told the currency before it shows a
  // form that reads amounts.
  const accounts = properties?.accounts ?? [];
  if (accounts.length === 0) return null;

  // Through `toCategories` for the same reason FinanceApp reads them that way:
  // a vault the storage repair has not reached still holds bare strings, and
  // this is the one place that converts them for every caller outside Finance.
  return {
    accounts,
    incomeCategories: toCategories(properties?.incomeCategories),
    expenseCategories: toCategories(properties?.expenseCategories),
    currency,
  };
}

/**
 * Change some rows of a Finance file without sending the rest of it back.
 *
 * The screen holds a month in memory. A sync can refresh that month on disk at
 * any moment, and a save that sends the whole array back would write the copy
 * the screen has been holding over the copy that arrived — taking out every row
 * that landed in between. Sending only what changed cannot do that: a row this
 * caller never mentions is a row the file keeps.
 *
 * `upserts` are whole rows, each carrying its `id`. `removals` are ids.
 */
export async function writeFinanceRows(params: {
  relPath: string;
  title: string;
  nodeType: 'finance_month' | 'finance_debts' | 'finance_recurring';
  upserts?: Record<string, unknown>[];
  removals?: string[];
  metadata?: Record<string, unknown>;
}): Promise<void> {
  await invoke('upsert_finance_rows', {
    relPath: params.relPath,
    title: params.title,
    nodeType: params.nodeType,
    upserts: params.upserts ?? [],
    removals: params.removals ?? [],
    metadata: { ...schemaStamp(), ...(params.metadata ?? {}) },
  });
}

/**
 * Which rows changed between two versions of a list the caller was handed
 * whole.
 *
 * Some screens still think in whole lists — the debts ledger hands its parent
 * an entire array. Translating that into row changes keeps the write narrow:
 * a row in neither list belongs to somebody else's device and is left alone.
 */
export function rowChanges<T extends { id: string }>(
  before: T[],
  after: T[],
): { upserts: T[]; removals: string[] } {
  const kept = new Set(after.map((row) => row.id));
  return {
    upserts: after,
    removals: before.map((row) => row.id).filter((id) => !kept.has(id)),
  };
}

/**
 * Add one transaction to the month it belongs to, creating that month if it
 * is the first entry in it.
 *
 * Returns the path written, so the caller can prove the write landed before
 * retiring whatever produced it.
 *
 * The month is not read here at all. It used to be — read the array, append,
 * write it back — which is the pattern that loses whatever arrived between the
 * read and the write. One row goes down; the file keeps everything else.
 */
export async function appendTransaction(transaction: Transaction): Promise<string> {
  const date = new Date(transaction.date);
  const relPath = monthNodePath(date);

  const existing = await invoke<{ title?: string } | null>('get_node', { id: relPath });

  await writeFinanceRows({
    relPath,
    title: existing?.title ?? monthNodeTitle(date),
    nodeType: 'finance_month',
    upserts: [transaction as unknown as Record<string, unknown>],
  });

  return relPath;
}
