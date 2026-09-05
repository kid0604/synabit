/**
 * What Syn has been allowed to do, and what it has done with it.
 *
 * Two lists that answer two different questions and are shown together because
 * somebody worried is asking both at once: *what can this thing do?* and *what
 * has it done?* Neither is much use alone — a permission with no record of its
 * use is a promise, and a record with no way to withdraw the permission is a
 * complaint.
 */
import { computed, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { logger } from '../../../utils/logger';
import type { AuditEntry, Grant } from '../types';

/**
 * Whether a grant still counts.
 *
 * An `always` expires; a `never` does not. Shown rather than filtered, because
 * a lapsed permission is something the user chose once and may want to know
 * about — quietly dropping it from the list would make the screen disagree with
 * the file.
 */
export const hasLapsed = (grant: Grant, now = new Date().toISOString()): boolean =>
  !!grant.expires_at && grant.expires_at <= now;

export function useSynAudit(vaultPath: () => string) {
  const grants = ref<Grant[]>([]);
  const entries = ref<AuditEntry[]>([]);
  const isLoading = ref(false);
  const error = ref<string | null>(null);

  const asMessage = (e: unknown) => (e as { message?: string })?.message ?? String(e);

  const load = async () => {
    isLoading.value = true;
    error.value = null;
    try {
      const [granted, log] = await Promise.all([
        invoke<Grant[]>('syn_list_grants', { vaultPath: vaultPath() }),
        invoke<AuditEntry[]>('syn_audit_log', { vaultPath: vaultPath() }),
      ]);
      grants.value = granted;
      entries.value = log;
    } catch (e) {
      logger.error('[Syn] Failed to read permissions', e);
      error.value = asMessage(e);
      grants.value = [];
      entries.value = [];
    } finally {
      isLoading.value = false;
    }
  };

  /** Refusals first — they are the ones somebody is checking are still there. */
  const ordered = computed(() =>
    [...grants.value].sort((a, b) => {
      const refusal = Number(b.answer === 'never') - Number(a.answer === 'never');
      if (refusal) return refusal;
      return b.granted_at.localeCompare(a.granted_at);
    }),
  );

  const revoke = async (grant: Grant) => {
    error.value = null;
    try {
      await invoke('syn_revoke_grant', { vaultPath: vaultPath(), scope: grant.scope });
      await load();
    } catch (e) {
      logger.error('[Syn] Failed to take back a permission', e);
      error.value = asMessage(e);
    }
  };

  return { grants, entries, ordered, isLoading, error, load, revoke };
}
