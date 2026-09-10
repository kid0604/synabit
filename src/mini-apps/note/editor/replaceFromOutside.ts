import { createDocument, type Content, type Editor } from '@tiptap/core';

/**
 * Put a document that came from outside the editor on screen, as nobody's edit.
 *
 * `setContent` swaps the whole document in one step and records it in the undo
 * history like a keystroke. For a note that changed somewhere else — synced
 * from another device, rewritten on disk, edited by Syn, put back from its
 * history — that made Cmd+Z undo the change rather than what the person had
 * just typed, and the autosave then wrote the old text back over it. The other
 * device's edit was gone, and nothing on screen said so.
 *
 * So the step is kept out of the history, and it is only as wide as what
 * actually differs. Undo steps are mapped through every change, and one that
 * replaces the whole document takes every one of them with it; replacing just
 * the changed stretch leaves the history of the rest of the note intact, the
 * way a collaborative editor handles an edit that arrives from a peer. The
 * cursor is mapped the same way, so it stays where it was.
 *
 * The document is built exactly as `setContent` builds it, so the result on
 * screen is the one it would have produced.
 */
export function replaceFromOutside(editor: Editor, content: Content): void {
  const current = editor.state.doc;
  const next = createDocument(content, editor.schema);

  const start = current.content.findDiffStart(next.content);
  // The same document: nothing to do, and no transaction to wake anything up.
  if (start === null) return;

  let { a: endA, b: endB } = current.content.findDiffEnd(next.content)!;
  // Where a run repeats — `aab` becoming `ab` — the scan from the front and
  // the scan from the back both claim the shared letter, and the ends land
  // before the start. Pushing both ends past it keeps the range whole.
  const overlap = start - Math.min(endA, endB);
  if (overlap > 0) {
    endA += overlap;
    endB += overlap;
  }

  editor.view.dispatch(
    editor.state.tr
      .replace(start, endA, next.slice(start, endB))
      .setMeta('addToHistory', false),
  );
}
