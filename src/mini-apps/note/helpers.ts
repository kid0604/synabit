import { logger } from '../../utils/logger';

/**
 * A note as the list holds it.
 *
 * There is deliberately no `content`. The list shows a title, a date, tags and
 * `summary`; carrying every note's body as well was the bulk of what loading
 * the list cost. Anything that needs a body — opening a note, rewriting its
 * file — fetches that one note with `getNode`.
 */
export interface NoteItem {
  id: string;
  title: string;
  /** The opening of the body, for display and for interim search matching. */
  summary: string;
  date: string;
  tags: string[];
  path: string;
  pinned: boolean;
  full_width: boolean;
  linked_projects?: string[];
  /**
   * The note's sync identity, carried so a save can hand it back.
   *
   * A save is a patch now, so the identity in the file survives a payload that
   * does not mention it. This is the second line of the same defence, and it
   * is worth keeping: when a save creates the file — a note reached by link
   * that has never been written — there is no frontmatter to preserve, and an
   * identity minted fresh there is how one note becomes two documents. The
   * copy already published under the old id comes back claiming a path that
   * now belongs to the new one, and every autosave lays down another
   * `(conflict …)` file.
   */
  node_id?: string;
  /** Likewise, for the same reason: a note's creation date is not renewable. */
  created_at?: string;
}

/** Where the sidebar's "recent" ordering is kept between launches. */
export const RECENT_NOTES_KEY = 'synabit_recent_notes';

/**
 * Persist the recent-notes list, and shrug if the store will not have it.
 *
 * Reading it is already wrapped this way. The writes were not, and two of the
 * three run inside a watcher or an async rename, so a failure surfaced as an
 * unhandled rejection instead of anything anyone could act on. Which order the
 * sidebar lists notes in is not worth a thrown error under any circumstances.
 */
export function rememberRecentNotes(ids: string[]) {
  try {
    localStorage.setItem(RECENT_NOTES_KEY, JSON.stringify(ids));
  } catch (e) {
    logger.warn('Could not remember the recently opened notes', e);
  }
}

export function buildNotePayload(note: NoteItem, content: string) {
  const properties: Record<string, unknown> = {
    pinned: note.pinned,
    full_width: note.full_width,
    tags: note.tags,
    linked_projects: note.linked_projects,
  };

  // Only when we have them. Sending `node_id: undefined` would write the key
  // with a null value, which reads as an identity rather than the absence of
  // one; a genuinely new note has neither and should be assigned both.
  if (note.node_id) properties.node_id = note.node_id;
  if (note.created_at) properties.created_at = note.created_at;

  return {
    relPath: note.id,
    title: note.title,
    nodeType: 'note' as const,
    properties,
    content,
  };
}

export const formatDate = (dateStr: string): string => {
    if (!dateStr) return '';
    if (!dateStr.includes('T')) return dateStr;
    try {
        const d = new Date(dateStr);
        if (isNaN(d.getTime())) return dateStr;
        const pad = (n: number) => String(n).padStart(2, '0');
        return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
    } catch (e) {
        return dateStr;
    }
};
