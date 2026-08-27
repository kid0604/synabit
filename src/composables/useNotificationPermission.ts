/**
 * Ask for notification permission, once, at a moment that makes sense.
 *
 * From Android 13 `POST_NOTIFICATIONS` is a runtime permission. The manifest
 * declares it — the Tauri notification plugin contributes that — but nothing
 * ever asked the user for it, so on every recent phone the system dropped every
 * notification this app produced. No error, no log: task reminders simply never
 * appeared, and the feature looked broken rather than blocked.
 *
 * Timing is the whole design here. Asking during first launch, before the user
 * has a vault or any tasks, is asking about something they cannot yet picture,
 * and Android stops offering the dialog after two refusals — a badly timed
 * prompt costs the permission permanently. So the request waits until a vault
 * exists, which is the point at which task reminders can actually happen.
 */

import { isPermissionGranted, requestPermission } from '@tauri-apps/plugin-notification';
import { load } from '@tauri-apps/plugin-store';
import { logger } from '../utils/logger';

const ASKED_KEY = 'notificationPermissionAsked';

let inFlight: Promise<boolean> | null = null;

async function ask(): Promise<boolean> {
  try {
    if (await isPermissionGranted()) return true;

    // Android shows the dialog at most twice, ever. Once it has stopped, asking
    // again returns immediately and achieves nothing, so remember that we tried
    // rather than calling into it on every launch.
    const store = await load('settings.json', { autoSave: true } as any);
    if (await store.get(ASKED_KEY)) return false;

    const outcome = await requestPermission();
    await store.set(ASKED_KEY, true);
    await store.save();

    const granted = outcome === 'granted';
    logger.info(`Notification permission ${granted ? 'granted' : 'not granted'}`);
    return granted;
  } catch (e) {
    // Desktop platforms with no such concept, or a plugin that is unavailable.
    // Notifications are a convenience; failing to obtain them must never stop
    // the app from starting.
    logger.warn('Could not establish notification permission', e);
    return false;
  }
}

/**
 * Request the permission if it has not been settled already.
 *
 * Safe to call more than once — concurrent callers share one request, and a
 * decision already made is not revisited.
 */
export function ensureNotificationPermission(): Promise<boolean> {
  if (!inFlight) {
    inFlight = ask().finally(() => {
      inFlight = null;
    });
  }
  return inFlight;
}
