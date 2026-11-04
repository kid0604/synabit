/**
 * A folder for Syn's rooms, chosen in the system's dialog.
 *
 * The dialog is put up by Rust (`syn_pick_door`, `syn_add_door`), so the path
 * is the person's answer to it and never a string from this page; all this
 * sends is the dialog's title in the screen's language. What comes back is
 * said here, in i18n: a folder Rust refused, and — once one opened — how many
 * names in it the backstop hides, because the person has to be told that a
 * key-shaped file is in what they just opened (design §8).
 */
import { message } from '@tauri-apps/plugin-dialog';
import { i18n } from '../../../i18n';
import { logger } from '../../../utils/logger';

export interface Door {
  id: string;
  name: string;
  path: string;
  opened: string;
  away: boolean;
  hidden: number;
}

export type Refused = 'not_a_folder' | 'too_wide' | 'vault' | 'app_data' | 'library' | 'hidden';

export type Picked =
  | { outcome: 'opened'; door: Door; examples: string[] }
  | { outcome: 'refused'; why: Refused }
  | { outcome: 'cancelled' };

const t = (key: string, values?: Record<string, unknown>, plural?: number) =>
  plural === undefined ? i18n.global.t(key, values ?? {}) : i18n.global.t(key, values ?? {}, plural);

export const dialogTitle = () => t('syn.door_dialog_title');

/** Why Rust would not open it, in a sentence. */
export const refusedText = (why: Refused) => t(`syn.door_refused_${why}`);

/** Tell the person what the backstop hid, in a window of the system's own. */
export const warnAboutHidden = async (picked: Picked) => {
  if (picked.outcome !== 'opened' || picked.door.hidden === 0) return;
  const text = t('syn.door_hidden', { n: picked.door.hidden, examples: picked.examples.join(', ') }, picked.door.hidden);
  try {
    await message(text, { title: picked.door.name, kind: 'warning' });
  } catch (e) {
    logger.warn('[Syn] Could not say what the folder hides', e);
  }
};
