import { invoke } from '@tauri-apps/api/core';

/**
 * Writing a board file without writing over somebody else's.
 *
 * A board has writers besides the one doing the writing — the app, the board
 * shown in a note, the pane beside a conversation, Syn, sync. Each reads the
 * file, changes it and writes it back, and two of them overlapping used to
 * mean the second wrote over the first. Now a write says which version of the
 * file it was made from, and `update_whiteboard` refuses it (`STALE`) if the
 * file has been written since; the writer reads again, combines, and retries.
 */

/** The version of a board's text, as `board_version` in Rust names it. */
export async function versionOf(text: string): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text));
  return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, '0')).join('');
}

/** Whether a failed write failed only because the file changed underneath it. */
export function isStale(err: unknown): boolean {
  return !!err && typeof err === 'object' && (err as { code?: string }).code === 'STALE';
}

/**
 * A board as text for its file.
 *
 * Indented, so a board stays readable and a change to it reads as a change in
 * a diff — except the short runs of numbers (a stroke's points, a waypoint),
 * which indented put every number on a line of its own and made a drawing's
 * file three or four times the size of the drawing.
 */
export function boardText(data: unknown): string {
  return JSON.stringify(data, null, 2).replace(
    /\[\s+(-?[\d.eE+-]+(?:,\s+-?[\d.eE+-]+)*)\s+\]/g,
    (_, run: string) => `[${run.split(/,\s+/).join(',')}]`,
  );
}

export const MAX_WRITE_TRIES = 4;

/**
 * Read, change, write — again from the read when somebody wrote in between.
 *
 * `change` gets the text on disk and returns the text to write, or null to
 * write nothing. Returns what was written, or null.
 */
export async function rewriteBoard(
  vaultPath: string,
  path: string,
  change: (onDisk: string) => string | null,
  meta: { title?: string; tags?: string[] } = {},
): Promise<string | null> {
  for (let attempt = 1; ; attempt++) {
    const onDisk = await invoke<string>('read_whiteboard', { vaultPath, path });
    const next = change(onDisk);
    if (next === null) return null;
    try {
      await invoke('update_whiteboard', {
        vaultPath,
        path,
        title: meta.title ?? '',
        tags: meta.tags ?? [],
        content: next,
        expected: await versionOf(onDisk),
      });
      return next;
    } catch (err) {
      if (!isStale(err) || attempt >= MAX_WRITE_TRIES) throw err;
    }
  }
}
