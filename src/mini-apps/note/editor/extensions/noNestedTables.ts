import { Extension } from '@tiptap/core';
import { Plugin, TextSelection } from '@tiptap/pm/state';
import { Fragment, Slice, type Node as ProseMirrorNode, type Schema } from '@tiptap/pm/model';
import {
  __pastedCells as pastedCells,
  cellAround,
  handlePaste as handleTablePaste,
  isInTable,
} from '@tiptap/pm/tables';

/**
 * Keep a table out of a table cell.
 *
 * A cell holds blocks, and a table is a block, so ProseMirror will happily put
 * one inside the other. Nothing in the app ever asks for that, and two ways in
 * reached it by accident:
 *
 *   - Dragging cells onto another cell — filling the columns at the right of a
 *     row and then moving them left is enough. prosemirror-tables does not look
 *     at drops at all, and a drop that it did not start carries the cells as a
 *     whole table, which fits inside the cell under the mouse. What the writer
 *     sees is the columns they just filled in vanishing into a small table
 *     sprouting in the cell they dropped on.
 *   - Pasting anything table-shaped that is not *only* a table — a table with
 *     the blank line after it, which is what copying a table out of a web page
 *     usually puts on the clipboard. prosemirror-tables maps a clean table onto
 *     the cells it is pasted over and steps aside for everything else, and
 *     everything else lands inside the cell.
 *
 * Markdown cannot write the result down either: a cell holding two blocks sends
 * the whole table out as raw HTML.
 *
 * So: a drop into a table behaves like a paste into a table — cells onto cells
 * — and a paste that the table logic will not take is flattened, each row
 * becoming a line of text with its links intact, instead of nesting.
 */
export const NoNestedTables = Extension.create({
  name: 'noNestedTables',
  addProseMirrorPlugins() {
    return [
      new Plugin({
        props: {
          transformPasted(slice, view) {
            if (!isInTable(view.state)) return slice;
            if (!holdsTable(slice.content)) return slice;
            // A clean table is prosemirror-tables' business: it fills the
            // cells it is pasted over. Only what it declines gets flattened.
            if (pastedCells(slice)) return slice;
            return withoutTables(slice, view.state.schema);
          },

          handleDrop(view, event, slice, moved) {
            if (!slice || !holdsTable(slice.content)) return false;
            const at = view.posAtCoords({ left: event.clientX, top: event.clientY });
            if (!at) return false;
            if (!cellAround(view.state.doc.resolve(at.pos))) return false;

            event.preventDefault();

            // Take the dragged content out first, as ProseMirror's own drop
            // does for a move, and leave the cursor where it was dropped.
            const opening = view.state.tr;
            if (moved) opening.deleteSelection();
            const dropAt = opening.mapping.map(at.pos);
            opening.setSelection(TextSelection.near(opening.doc.resolve(dropAt)));
            view.dispatch(opening);

            // From here it is a paste, and takes the paste's two answers:
            // cells onto cells, or flattened into the cell.
            // `handlePaste` names its event parameter `_`: it wants the
            // slice and the selection, and never reads the event.
            const asPaste = event as unknown as ClipboardEvent;
            if (handleTablePaste(view, asPaste, slice)) return true;
            const flat = withoutTables(slice, view.state.schema);
            if (flat.content.size) {
              view.dispatch(view.state.tr.replaceSelection(flat).scrollIntoView());
            }
            return true;
          },
        },
      }),
    ];
  },
});

function isTable(node: ProseMirrorNode): boolean {
  return node.type.spec.tableRole === 'table';
}

function holdsTable(fragment: Fragment): boolean {
  let found = false;
  fragment.forEach((node) => {
    if (found) return;
    found = isTable(node) || holdsTable(node.content);
  });
  return found;
}

/** A cell's blocks as one run of inline content, so its links survive. */
function inlineOf(cell: ProseMirrorNode, schema: Schema): Fragment {
  let out = Fragment.empty;
  cell.forEach((block) => {
    const piece = block.isTextblock
      ? block.content
      : block.textContent
        ? Fragment.from(schema.text(block.textContent))
        : Fragment.empty;
    if (!piece.size) return;
    out = out.size ? out.append(Fragment.from(schema.text(' '))).append(piece) : piece;
  });
  return out;
}

/** A table as one paragraph per row, cells separated by a space. */
function rowsOf(table: ProseMirrorNode, schema: Schema): ProseMirrorNode[] {
  const lines: ProseMirrorNode[] = [];
  table.forEach((row) => {
    let line = Fragment.empty;
    row.forEach((cell) => {
      const piece = inlineOf(cell, schema);
      if (!piece.size) return;
      line = line.size ? line.append(Fragment.from(schema.text(' '))).append(piece) : piece;
    });
    if (line.size) lines.push(schema.nodes.paragraph.create(null, line));
  });
  return lines;
}

function flatten(fragment: Fragment, schema: Schema): Fragment {
  const out: ProseMirrorNode[] = [];
  fragment.forEach((node) => {
    if (isTable(node)) out.push(...rowsOf(node, schema));
    else if (holdsTable(node.content)) out.push(node.copy(flatten(node.content, schema)));
    else out.push(node);
  });
  return Fragment.fromArray(out);
}

function withoutTables(slice: Slice, schema: Schema): Slice {
  const content = flatten(slice.content, schema);
  // The shape has changed, so the slice's own open depths no longer describe
  // it. Open a textblock at either end, which is what makes a paste join the
  // line the cursor is on instead of starting a second paragraph in the cell.
  const open = (node: ProseMirrorNode | null | undefined) => (node?.isTextblock ? 1 : 0);
  return new Slice(content, open(content.firstChild), open(content.lastChild));
}
