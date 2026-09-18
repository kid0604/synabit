import { createDocument, type Content, type Editor } from '@tiptap/core';
import { Fragment, type Node as ProseMirrorNode } from '@tiptap/pm/model';

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
 * ── Why the difference is looked for node by node ──────────────────────────
 *
 * This used to work in flat document positions: `findDiffStart` for one end,
 * `findDiffEnd` for the other, and `doc.slice()` between them. Both parts of
 * that were wrong on a note holding a table.
 *
 * `findDiffEnd` walks back from the last child and stops at the first node
 * whose markup does not match, and the editor's own document ends with a
 * trailing paragraph that `createDocument` does not produce. The two ends
 * therefore never matched, so "only as wide as what differs" was in truth
 * always "from the first difference to the end of the note" — and a note is
 * reconciled on nearly every save, because the body that comes back has been
 * through markdown, which trims what the writer typed.
 *
 * A range that wide starts wherever the cursor is — mid-sentence, inside a
 * table cell — and the slice taken to fill it is cut at the same arbitrary
 * depth. ProseMirror then has to make the two fit, and it is allowed to wrap
 * content in new nodes to do it: the rows after the cursor came back as a
 * table *inside* the cell the writer was typing in, with the columns they had
 * just filled in swallowed by it. That is the nested-table bug.
 *
 * So the difference is found by walking the two documents together, one node
 * at a time, descending only into nodes that are the same kind of thing, and
 * the change is applied with whole nodes — or, inside a paragraph, the run of
 * characters that actually changed. There is nothing left to fit.
 */
export function replaceFromOutside(editor: Editor, content: Content): void {
  const current = editor.state.doc;
  const next = createDocument(content, editor.schema);

  const change = changeBetween(current.content, withSameTail(current, next), 0);
  // The same document: nothing to do, and no transaction to wake anything up.
  if (!change) return;

  editor.view.dispatch(
    editor.state.tr
      .replaceWith(change.from, change.to, change.content)
      .setMeta('addToHistory', false),
  );
}

/** The stretch that differs, and what belongs there instead. */
type Change = { from: number; to: number; content: Fragment };

/**
 * The editor keeps an empty paragraph after a table so there is somewhere to
 * put the cursor; a document parsed from the note's text has no such thing.
 * Left alone, that lone difference at the very end would be read as "the whole
 * tail of the note changed".
 */
function withSameTail(current: ProseMirrorNode, next: ProseMirrorNode): Fragment {
  const tail = current.lastChild;
  const theirs = next.lastChild;
  const trailingBlank = tail?.isTextblock && tail.content.size === 0 && tail.attrs.textAlign == null;
  if (!trailingBlank || (theirs?.isTextblock && theirs.content.size === 0)) return next.content;
  return next.content.addToEnd(tail);
}

/**
 * Where two fragments part company, as a range and a replacement.
 *
 * Equal children are skipped from both ends. When what is left is one node on
 * each side and they are the same kind of thing, the difference is inside it,
 * so the walk goes in — down to the characters of a single piece of text.
 * Otherwise the differing children are swapped out whole.
 */
function changeBetween(before: Fragment, after: Fragment, offset: number): Change | null {
  let start = 0;
  const shared = Math.min(before.childCount, after.childCount);
  while (start < shared && before.child(start).eq(after.child(start))) start++;

  let endBefore = before.childCount;
  let endAfter = after.childCount;
  while (endBefore > start && endAfter > start && before.child(endBefore - 1).eq(after.child(endAfter - 1))) {
    endBefore--;
    endAfter--;
  }
  if (start === endBefore && start === endAfter) return null;

  let from = offset;
  for (let i = 0; i < start; i++) from += before.child(i).nodeSize;

  if (endBefore - start === 1 && endAfter - start === 1) {
    const mine = before.child(start);
    const theirs = after.child(start);
    if (mine.sameMarkup(theirs)) {
      if (mine.isText) return changeInText(mine, theirs, from);
      if (!mine.isLeaf) return changeBetween(mine.content, theirs.content, from + 1);
    }
  }

  let to = from;
  for (let i = start; i < endBefore; i++) to += before.child(i).nodeSize;
  let content = Fragment.empty;
  for (let i = start; i < endAfter; i++) content = content.addToEnd(after.child(i));
  return { from, to, content };
}

/** The characters that changed, so a cursor in the same line does not move. */
function changeInText(before: ProseMirrorNode, after: ProseMirrorNode, offset: number): Change {
  const mine = before.text ?? '';
  const theirs = after.text ?? '';

  let head = 0;
  while (head < mine.length && head < theirs.length && mine[head] === theirs[head]) head++;

  let endMine = mine.length;
  let endTheirs = theirs.length;
  while (endMine > head && endTheirs > head && mine[endMine - 1] === theirs[endTheirs - 1]) {
    endMine--;
    endTheirs--;
  }

  const middle = theirs.slice(head, endTheirs);
  return {
    from: offset + head,
    to: offset + endMine,
    content: middle ? Fragment.from(before.type.schema.text(middle, before.marks)) : Fragment.empty,
  };
}
