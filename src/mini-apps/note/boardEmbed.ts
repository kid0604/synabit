/**
 * A board shown in a note, as the note's Markdown keeps it: one `div` the
 * editor reads back as a live embed (`WhiteboardExtension`). Here on its own
 * so that what writes a note outside the editor — the Whiteboard app saving a
 * mind map as a note — writes the same thing the editor would.
 */

/** A value made safe to put between the double quotes of an HTML attribute. */
export function escapeAttribute(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/"/g, '&quot;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

export interface BoardEmbed {
  boardId: string;
  boardPath: string;
  title?: string;
  width?: string;
  height?: string;
  align?: string;
}

/**
 * The embed's Markdown, one line. Escaped, because these land inside double
 * quotes: a board called `Plan "B"` closed the attribute early, and
 * everything after it — the path included — was lost from the note on the
 * next save.
 */
export function boardEmbedHtml(a: BoardEmbed): string {
  const attr = (value: unknown) => escapeAttribute(String(value ?? ''));
  return `<div data-type="whiteboard" data-board-id="${attr(a.boardId)}" data-board-path="${attr(a.boardPath)}" data-title="${attr(a.title || '')}" data-width="${attr(a.width || '100%')}" data-height="${attr(a.height || '240px')}" data-align="${attr(a.align || 'center')}"></div>\n`;
}
