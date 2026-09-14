import { ref, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { logger } from '../../../utils/logger';

/** What `syn::telegram::Status` serialises to. */
export interface TelegramStatus {
  /** False on a phone: the bot runs on a computer. */
  supported: boolean;
  has_token: boolean;
  bot_username: string | null;
  running: boolean;
  problem: 'conflict' | 'unauthorized' | 'network' | 'keychain' | null;
  detail: string | null;
  paired: { name: string; since: string } | null;
  pending: number;
  /** Whether reminders are sent to the paired chat as well as shown here. */
  reminders: boolean;
}

export interface PairingLink {
  link: string;
  expires_at: string;
}

/** Sent by Rust whenever something this screen shows has changed. */
export const TELEGRAM_STATUS_EVENT = 'telegram-status';

const asMessage = (e: unknown) => (e as { message?: string })?.message ?? String(e);

/**
 * The Telegram bot, as the settings screen drives it.
 *
 * Every action answers with the status afterwards, and the bot announces what
 * it changes on its own — a phone pairing, a problem clearing — so the screen
 * draws what is true rather than what it expected to happen.
 */
export function useTelegram() {
  const status = ref<TelegramStatus | null>(null);
  const tokenDraft = ref('');
  const busy = ref(false);
  const error = ref<string | null>(null);
  const pairing = ref<PairingLink | null>(null);
  let stopListening: UnlistenFn | undefined;

  const refresh = async () => {
    try {
      status.value = await invoke<TelegramStatus>('telegram_status');
      // Paired from the phone: the link has done its job.
      if (status.value.paired) pairing.value = null;
    } catch (e) {
      logger.error('[Telegram] Could not read the bot status', e);
      error.value = asMessage(e);
    }
  };

  const act = async (run: () => Promise<void>) => {
    busy.value = true;
    error.value = null;
    try {
      await run();
    } catch (e) {
      logger.error('[Telegram] A bot action failed', e);
      error.value = asMessage(e);
    } finally {
      busy.value = false;
    }
  };

  const connect = () =>
    act(async () => {
      status.value = await invoke<TelegramStatus>('telegram_set_token', { token: tokenDraft.value });
      tokenDraft.value = '';
    });

  const disconnect = () =>
    act(async () => {
      status.value = await invoke<TelegramStatus>('telegram_clear_token');
      pairing.value = null;
    });

  const startPairing = () =>
    act(async () => {
      pairing.value = await invoke<PairingLink>('telegram_start_pairing');
    });

  const unpair = () =>
    act(async () => {
      status.value = await invoke<TelegramStatus>('telegram_unpair');
    });

  const retry = () =>
    act(async () => {
      status.value = await invoke<TelegramStatus>('telegram_retry');
    });

  const setReminders = (on: boolean) =>
    act(async () => {
      status.value = await invoke<TelegramStatus>('telegram_set_reminders', { on });
    });

  onMounted(async () => {
    await refresh();
    try {
      stopListening = await listen(TELEGRAM_STATUS_EVENT, () => {
        void refresh();
      });
    } catch (e) {
      logger.error('[Telegram] Could not listen for status changes', e);
    }
  });

  onUnmounted(() => stopListening?.());

  return {
    status,
    tokenDraft,
    busy,
    error,
    pairing,
    refresh,
    connect,
    disconnect,
    startPairing,
    unpair,
    retry,
    setReminders,
  };
}
