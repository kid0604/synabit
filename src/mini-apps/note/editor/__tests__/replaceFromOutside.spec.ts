import { describe, it, expect, afterEach, vi } from 'vitest';
import { Editor } from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import { Table, TableRow } from '@tiptap/extension-table';
import { CustomTableCell, CustomTableHeader } from '../extensions/customTable';
import { replaceFromOutside } from '../replaceFromOutside';

let editor: Editor;

const make = (html: string) => {
  editor = new Editor({ element: document.createElement('div'), extensions: [StarterKit], content: html });
  return editor;
};

/**
 * A note holding a table, reaching the editor the way a note's body does:
 * through `setContent`, which leaves the empty paragraph after the table that
 * a parsed document does not have.
 */
const makeNote = (html: string) => {
  editor = new Editor({
    element: document.createElement('div'),
    extensions: [StarterKit, Table, TableRow, CustomTableCell, CustomTableHeader],
    content: '<p></p>',
  });
  editor.commands.setContent(html);
  return editor;
};

const CHANGE_LOG = `<table><tbody>
  <tr><th><p>When</p></th><th><p>Where</p></th><th><p>Who</p></th><th><p>What</p></th></tr>
  <tr><td><p>2026-09-17</p></td><td><p>PSSv2 - DC02</p></td><td><p>FTI</p></td><td><p>a sentence with some length to it</p></td></tr>
  <tr><td><p></p></td><td><p>PSSv2 -</p></td><td><p>ben.dang</p></td><td><p>the row being filled in</p></td></tr>
</tbody></table>`;

/** Where each piece of text ends: where a writer's cursor sits. */
const cursorSpots = () => {
  const spots: number[] = [];
  editor.state.doc.descendants((node, pos) => { if (node.isText) spots.push(pos + node.nodeSize); });
  return spots;
};

const holdsTableInACell = () => /<t[dh][^>]*>(?:(?!<\/t[dh]>)[\s\S])*?<table/.test(editor.getHTML());

/** Type at the end of the first paragraph, the way a keystroke lands. */
const typeInFirstParagraph = (text: string) => {
  const end = editor.state.doc.firstChild!.nodeSize - 1;
  editor.commands.insertContentAt(end, text);
};

afterEach(() => editor.destroy());

describe('replaceFromOutside', () => {
  // The whole point. A change synced in from another device is not something
  // this person did, so Cmd+Z must undo their typing and leave it alone.
  it('undoes what was typed and keeps what arrived', () => {
    make('<p>mine</p><p>theirs</p>');
    typeInFirstParagraph(' typed');

    replaceFromOutside(editor, '<p>mine typed</p><p>theirs, edited on the phone</p>');
    editor.commands.undo();

    expect(editor.getHTML()).toBe('<p>mine</p><p>theirs, edited on the phone</p>');
  });

  // A restore or a file rewritten on disk replaces everything. Undo must not
  // quietly put the old text back for the autosave to write over it.
  it('cannot be undone even when it replaces everything', () => {
    make('<p>the note as it was</p>');

    replaceFromOutside(editor, '<p>the version put back</p>');
    editor.commands.undo();

    expect(editor.getHTML()).toBe('<p>the version put back</p>');
  });

  it('leaves the cursor where it was when the change is elsewhere', () => {
    make('<p>mine</p><p>theirs</p>');
    editor.commands.setTextSelection(3);

    replaceFromOutside(editor, '<p>mine</p><p>theirs, and more of it</p>');

    expect(editor.state.selection.from).toBe(3);
  });

  it('does nothing at all when nothing changed', () => {
    make('<p>same</p>');
    const onUpdate = vi.fn();
    editor.on('update', onUpdate);

    replaceFromOutside(editor, '<p>same</p>');

    expect(onUpdate).not.toHaveBeenCalled();
  });

  // The nested-table bug. The body that comes back from the store has been
  // through markdown, so it never quite matches what is on screen, and the
  // document it parses to has no trailing paragraph. Reconciling the two used
  // to replace everything from the cursor to the end of the note, and what
  // came back to fill that stretch was a table — inside the cell being typed
  // in, swallowing the columns the writer had just filled.
  it('does not put a table inside a cell, wherever the cursor is', () => {
    makeNote(CHANGE_LOG);
    const spots = cursorSpots();
    expect(spots.length).toBeGreaterThan(8);

    for (const spot of spots) {
      makeNote(CHANGE_LOG);
      editor.commands.insertContentAt(spot, ' ');
      replaceFromOutside(editor, CHANGE_LOG);

      expect(holdsTableInACell()).toBe(false);
      expect(editor.getText()).toContain('the row being filled in');
      expect(editor.getText()).toContain('ben.dang');
      editor.destroy();
    }
    make('<p></p>');
  });

  // The empty paragraph the editor keeps after a table is the editor's, not a
  // difference: taking it for one is what made every reconciliation reach the
  // end of the note.
  it('does not read the editor\'s trailing paragraph as a change', () => {
    makeNote(CHANGE_LOG);
    const onUpdate = vi.fn();
    editor.on('update', onUpdate);

    replaceFromOutside(editor, CHANGE_LOG);

    expect(onUpdate).not.toHaveBeenCalled();
    expect(editor.state.doc.lastChild!.type.name).toBe('paragraph');
  });

  // A change in one cell is a change in one cell.
  it('touches only the cell that changed', () => {
    makeNote(CHANGE_LOG);
    const before = editor.state.doc.content.size;
    editor.commands.setTextSelection(4);

    replaceFromOutside(editor, CHANGE_LOG.replace('<p>FTI</p>', '<p>FTI, then someone else</p>'));

    expect(editor.getText()).toContain('FTI, then someone else');
    expect(editor.state.doc.content.size).toBe(before + 'then someone else'.length + 2);
    expect(editor.state.selection.from).toBe(4);
  });

  // Where a run repeats, the scans from each end overlap. The result still has
  // to be exactly the new document.
  it('lands on exactly the new document when letters repeat', () => {
    for (const [from, to] of [['aab', 'ab'], ['ab', 'aab'], ['abba', 'aba'], ['x', 'xxxx']]) {
      make(`<p>${from}</p>`);
      replaceFromOutside(editor, `<p>${to}</p>`);
      expect(editor.getHTML()).toBe(`<p>${to}</p>`);
      editor.destroy();
    }
    make('<p></p>');
  });
});
