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
}

export function buildNotePayload(note: NoteItem, content: string) {
  return {
    relPath: note.id,
    title: note.title,
    nodeType: 'note' as const,
    properties: {
      pinned: note.pinned,
      full_width: note.full_width,
      tags: note.tags,
      linked_projects: note.linked_projects,
    },
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
