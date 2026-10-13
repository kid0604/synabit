import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { logger } from '../../../utils/logger';
import { errorText } from '../../../shared/errorText';
import type { Choices, Connected, DeclaredView, Problem } from '../declared';

/**
 * Declared connectors, as the settings screen drives them. Every command
 * answers with the whole list afterwards, so the screen draws what Rust found.
 */
export function useSynDeclared(vaultPath: () => string) {
  const declared = ref<DeclaredView[]>([]);
  const busy = ref(false);
  const error = ref<string | null>(null);

  const run = async <T>(what: string, call: () => Promise<T>): Promise<T | null> => {
    busy.value = true;
    error.value = null;
    try {
      return await call();
    } catch (e) {
      logger.error(`[Syn] declared connectors: could not ${what}`, e);
      error.value = errorText(e);
      return null;
    } finally {
      busy.value = false;
    }
  };

  const take = (views: DeclaredView[] | null) => {
    if (views) declared.value = views;
    return views !== null;
  };

  const load = async () =>
    take(await run('list', () => invoke<DeclaredView[]>('syn_manifest_list', { vaultPath: vaultPath() })));

  const validate = (yaml: string) => run('check a declaration', () => invoke<Problem[]>('syn_manifest_validate', { yaml }));

  const save = async (yaml: string, about: string | null = null) =>
    take(await run('save a declaration', () => invoke<DeclaredView[]>('syn_manifest_save', { vaultPath: vaultPath(), yaml, about })));



  const approve = async (manifestId: string, choices: Choices, secret: string) =>
    take(
      await run('approve', () =>
        invoke<DeclaredView[]>('syn_instance_approve', {
          vaultPath: vaultPath(),
          manifestId,
          choices,
          // Straight to the keychain; nothing keeps it here.
          secret: secret.trim() ? secret : null,
        }),
      ),
    );

  const remove = async (manifestId: string) =>
    take(await run('remove an approval', () => invoke<DeclaredView[]>('syn_instance_remove', { vaultPath: vaultPath(), manifestId })));

  const peekCertificate = (url: string) => run('read the certificate', () => invoke<string>('syn_instance_peek_cert', { url }));

  /** This device's side of a device connector. The folder is the person's to pick, never a declaration's. */
  const device = {
    chooseFolder: async (): Promise<string | null> => {
      const picked = await openDialog({ directory: true, multiple: false }).catch((e) => {
        logger.error('[Syn] Could not open the folder dialog', e);
        return null;
      });
      return typeof picked === 'string' ? picked : null;
    },
    access: () => invoke<boolean>('syn_device_notification_access').catch(() => false),
    openAccess: async () => {
      await invoke('syn_device_open_notification_access').catch((e) => logger.error('[Syn] Could not open notification access', e));
    },
  };

  /** The connect card's one button: keep the key, approve here, try it. See `syn_connect`. */
  const connect = async (
    manifestId: string,
    inputs: Record<string, string>,
    secrets: Record<string, string>,
    trustCert: string | null = null,
    folder: string | null = null,
  ): Promise<Connected | null> => {
    const out = await run('connect', () =>
      invoke<Connected>('syn_connect', { vaultPath: vaultPath(), manifestId, inputs, secrets, trustCert, folder }),
    );
    await load();
    return out;
  };

  return { declared, busy, error, load, validate, save, approve, remove, peekCertificate, device, connect };
}
