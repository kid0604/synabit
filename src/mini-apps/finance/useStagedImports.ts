/**
 * Transactions Syn staged from a statement, waiting here for the person.
 *
 * Syn never writes them into the ledger. Rust files the rows it was pointed
 * at in `.synabit/staging/`; this lists them, and Finance shows each through
 * the same import preview a file picked by hand goes through — one place that
 * reads an amount by the currency, one place that decides what a duplicate
 * is. See `syn::staging` in Rust.
 */
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { logger } from '../../utils/logger';

export interface Staged {
  id: string;
  /** Syn's words, shown as Syn's. */
  why: string;
  source: string;
  rows: number;
  created_at: string;
  conversation_id?: string | null;
}

export function useStagedImports(vaultPath: () => string) {
  const staged = ref<Staged[]>([]);

  const refresh = async () => {
    if (!vaultPath()) return;
    try {
      staged.value = await invoke<Staged[]>('syn_staged_list', { vaultPath: vaultPath() });
    } catch (e) {
      logger.error('[Finance] Could not list what Syn staged', e);
    }
  };

  const text = (id: string) => invoke<string>('syn_staged_text', { vaultPath: vaultPath(), id });

  const discard = async (id: string) => {
    try {
      await invoke('syn_staged_discard', { vaultPath: vaultPath(), id });
    } catch (e) {
      logger.error('[Finance] Could not set a staging aside', e);
    }
    await refresh();
  };

  let stop: (() => void) | null = null;
  const start = async () => {
    await refresh();
    stop = await listen('syn-staged', () => void refresh()).catch((e) => {
      logger.error('[Finance] Could not listen for stagings', e);
      return null;
    });
  };

  return { staged, refresh, text, discard, start, stop: () => stop?.() };
}
