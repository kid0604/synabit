/**
 * A moment in a recording, as a citation carries it: `Files/<hash>.md#t=192,230`.
 *
 * The `t=START,END` shape is the W3C media fragment, in seconds. Transcripts
 * tell Syn to cite that way (`src-tauri/src/timeline/media.rs`), and the Files
 * app opens a recording there. Read by hand rather than left to the `<audio>`
 * element: the fragment would have to survive `convertFileSrc`, and a player
 * that ignores it would open at 0:00 with nothing to say it had.
 */

export interface TimeFragment {
  start: number;
  end?: number;
}

const FRAGMENT = /^#?t=(\d+(?:\.\d+)?)(?:,(\d+(?:\.\d+)?))?$/;

/** `t=192,230`, `#t=192` or nothing. An end before the start is dropped. */
export function parseTimeFragment(text: string | null | undefined): TimeFragment | null {
  if (!text) return null;
  const match = FRAGMENT.exec(text.trim());
  if (!match) return null;
  const start = Number(match[1]);
  const end = match[2] === undefined ? undefined : Number(match[2]);
  return end === undefined || end <= start ? { start } : { start, end };
}

export function timeFragment(start: number, end?: number): string {
  const round = (n: number) => String(Math.round(n * 10) / 10);
  return end === undefined ? `t=${round(start)}` : `t=${round(start)},${round(end)}`;
}

/** A node id and the moment after its `#`, if one is there. */
export function splitMediaLink(link: string): { id: string; fragment: TimeFragment | null } {
  const at = link.indexOf('#');
  if (at < 0) return { id: link, fragment: null };
  return { id: link.slice(0, at), fragment: parseTimeFragment(link.slice(at + 1)) };
}

/** `3:12`, or `1:03:12` past the hour. */
export function clock(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  const pad = (n: number) => String(n).padStart(2, '0');
  if (s >= 3600) return `${Math.floor(s / 3600)}:${pad(Math.floor(s / 60) % 60)}:${pad(s % 60)}`;
  return `${Math.floor(s / 60)}:${pad(s % 60)}`;
}
