import { invoke } from '@tauri-apps/api/core';

/**
 * Save and folder dialogs for commands that write outside the vault.
 *
 * Opened by Rust rather than by `@tauri-apps/plugin-dialog`, because the
 * command that writes the file takes the path from this window — and a path
 * from a window is only trustworthy if Rust saw the dialog it came from. The
 * answer is good for one call naming it (`app_shell::gate::CHOSEN_ARGS`), so
 * pick right before the write, not ahead of time.
 *
 * A save that this window writes itself, through the fs plugin, keeps using
 * the plugin's own dialog: the plugin grants exactly that file, and nothing
 * Rust does is involved.
 */

export interface DialogFilter {
  name: string;
  extensions: string[];
}

/** Where to save a file, or null when the dialog was closed. */
export function pickSavePath(defaultName: string, filters: DialogFilter[]): Promise<string | null> {
  return invoke<string | null>('pick_save_path', { defaultName, filters });
}

/** A folder to add as a file source, or null when the dialog was closed. */
export function pickFolder(title: string): Promise<string | null> {
  return invoke<string | null>('pick_folder', { title });
}
