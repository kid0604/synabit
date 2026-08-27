/**
 * Exporting and restoring the vault as a single zip.
 *
 * Lives outside any one component because two places need it: the Settings
 * screen, where the user goes looking for it, and the reminder banner, where it
 * finds them. Both must record the same "last exported" timestamp, or the
 * reminder would keep firing after an export done from the other one.
 *
 * The file dialog returns an ordinary path on desktop and a `content://` URI on
 * Android. Neither is inspected here — the Rust side resolves both.
 */

import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';
import { load } from '@tauri-apps/plugin-store';
import { logger } from '../utils/logger';

export const LAST_EXPORT_KEY = 'vaultLastExportedAt';

/** How long a backup is treated as recent enough not to mention. */
export const EXPORT_REMINDER_DAYS = 30;

export interface ArchiveSummary { files: number; bytes: number }
export interface RestoreSummary { files: number; bytes: number; rejected: string[] }

/** Why the backup reminder is showing, or null when it should not. */
export type BackupReminder = 'never' | 'stale' | null;

/**
 * Decide whether to nag, kept as a plain function so the rule is testable
 * without a store, a platform, or a mounted component.
 *
 * Only on a mobile OS. On the desktop the vault is a folder the user picked and
 * can see, and uninstalling the app does not touch it — there is nothing to
 * warn about, and a warning with nothing behind it is how a product teaches
 * people to ignore its warnings.
 */
export function backupReminderReason(
  lastExport: Date | null,
  now: Date,
  onMobile: boolean,
): BackupReminder {
  if (!onMobile) return null;
  if (!lastExport) return 'never';

  const days = Math.floor((now.getTime() - lastExport.getTime()) / 86_400_000);
  // A clock that has gone backwards, or a timestamp from the future, reads as
  // a negative age. That is not staleness and must not be reported as it.
  return days >= EXPORT_REMINDER_DAYS ? 'stale' : null;
}

/** Whole days between an export and now, floored at zero. */
export function daysSince(lastExport: Date, now: Date): number {
  return Math.max(0, Math.floor((now.getTime() - lastExport.getTime()) / 86_400_000));
}

/** When the vault was last exported, or null if it never has been. */
export async function lastExportedAt(): Promise<Date | null> {
  try {
    const store = await load('settings.json', { autoSave: true } as any);
    const stamp = await store.get<string>(LAST_EXPORT_KEY);
    return stamp ? new Date(stamp) : null;
  } catch (e) {
    logger.warn('Could not read the last export time', e);
    return null;
  }
}

async function recordExport(): Promise<void> {
  try {
    const store = await load('settings.json', { autoSave: true } as any);
    await store.set(LAST_EXPORT_KEY, new Date().toISOString());
    await store.save();
  } catch (e) {
    // The export itself succeeded; only the reminder loses its memory.
    logger.warn('Could not record the export time', e);
  }
}

const busy = ref(false);

export function useVaultArchive() {
  /**
   * Returns the summary, or null when the user cancelled.
   *
   * Cancelling is not a failure and must not be reported as one — a dialog the
   * user closed on purpose producing a red error is how people learn to ignore
   * errors.
   */
  async function exportVault(vaultPath: string): Promise<ArchiveSummary | null> {
    busy.value = true;
    try {
      const defaultPath = await invoke<string>('suggested_archive_name');
      const destination = await save({
        defaultPath,
        filters: [{ name: 'Zip', extensions: ['zip'] }],
      });
      if (!destination) return null;

      const summary = await invoke<ArchiveSummary>('export_vault_archive', {
        vaultPath,
        destination,
      });
      await recordExport();
      return summary;
    } finally {
      busy.value = false;
    }
  }

  async function importVault(vaultPath: string): Promise<RestoreSummary | null> {
    busy.value = true;
    try {
      const source = await open({
        multiple: false,
        directory: false,
        filters: [{ name: 'Zip', extensions: ['zip'] }],
      });
      if (!source) return null;

      return await invoke<RestoreSummary>('import_vault_archive', {
        vaultPath,
        source: source as string,
      });
    } finally {
      busy.value = false;
    }
  }

  /** Save the application log somewhere the user can attach it to a report. */
  async function exportDiagnostics(): Promise<number | null> {
    busy.value = true;
    try {
      const defaultPath = await invoke<string>('suggested_diagnostics_name');
      const destination = await save({
        defaultPath,
        filters: [{ name: 'Text', extensions: ['txt'] }],
      });
      if (!destination) return null;
      return await invoke<number>('export_diagnostics', { destination });
    } finally {
      busy.value = false;
    }
  }

  return { busy, exportVault, importVault, exportDiagnostics };
}

/** Bytes as something a person reads. */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
