export interface NoteItem {
  id: string;
  title: string;
  summary: string;
  date: string;
  tags: string[];
  path: string;
  pinned: boolean;
  full_width: boolean;
  content: string;
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
