import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { Editor } from '@tiptap/core';
import StarterKit from '@tiptap/starter-kit';
import { Table, TableRow } from '@tiptap/extension-table';
import Link from '@tiptap/extension-link';
import { CustomTableCell, CustomTableHeader } from '../extensions/customTable';
import { NoNestedTables } from '../extensions/noNestedTables';

let editor: Editor;

/** A change log two columns wide, with the cursor after "PSSv2 -". */
const TABLE = `<table><tbody>
  <tr><th><p>Môi trường</p></th><th><p>Nội dung</p></th></tr>
  <tr><td><p>PSSv2 -</p></td><td><p>x</p></td></tr>
</tbody></table><p>after</p>`;

function inFirstCell() {
  const cell = editor.view.dom.querySelectorAll('tbody tr')[1].children[0];
  const pos = editor.view.posAtDOM(cell, 0);
  editor.commands.setTextSelection(pos + editor.state.doc.nodeAt(pos - 1)!.content.size);
}

beforeEach(() => {
  editor = new Editor({
    element: document.createElement('div'),
    extensions: [
      StarterKit,
      Link.configure({ openOnClick: false }),
      Table, TableRow, CustomTableCell, CustomTableHeader,
      NoNestedTables,
    ],
    content: TABLE,
  });
  inFirstCell();
});

afterEach(() => editor.destroy());

function paste(html: string) {
  const event = new Event('paste', { bubbles: true, cancelable: true });
  Object.defineProperty(event, 'clipboardData', {
    value: { getData: (t: string) => (t === 'text/html' ? html : ''), types: ['text/html'], items: [] },
  });
  editor.view.dom.dispatchEvent(event);
}

function drop(html: string) {
  const cell = editor.view.dom.querySelectorAll('tbody tr')[1].children[0];
  const at = editor.view.posAtDOM(cell, 0);
  const event = new Event('drop', { bubbles: true, cancelable: true });
  Object.defineProperties(event, {
    clientX: { value: 0 }, clientY: { value: 0 },
    dataTransfer: {
      value: { getData: (t: string) => (t === 'text/html' ? html : ''), types: ['text/html'], files: [] },
    },
  });
  // jsdom lays nothing out, so the drop point cannot come from coordinates.
  editor.view.posAtCoords = () => ({ pos: at, inside: at - 1 });
  editor.view.dom.dispatchEvent(event);
}

const nested = () => /<t[dh][^>]*>(?:(?!<\/t[dh]>)[\s\S])*?<table/.test(editor.getHTML());

function tables() {
  const found: import('@tiptap/pm/model').Node[] = [];
  editor.state.doc.forEach((node) => { if (node.type.name === 'table') found.push(node); });
  return found;
}

// What copying a table out of a web page actually puts on the clipboard: the
// table, and the blank line that followed it.
const TABLE_AND_MORE = '<table><tbody><tr><td>ben</td><td><a href="http://x">dang</a></td></tr></tbody></table><p>tail</p>';
const CLEAN_TABLE = '<table><tbody><tr><td>ben</td><td>dang</td></tr></tbody></table>';

describe('a table pasted into a table cell', () => {
  it('does not nest, and keeps the text and its links', () => {
    paste(TABLE_AND_MORE);
    expect(nested()).toBe(false);
    expect(editor.getText()).toContain('ben dang');
    expect(editor.getText()).toContain('tail');
    expect(editor.getHTML()).toContain('href="http://x"');
  });

  it('still fills the cells when it is a table and nothing else', () => {
    paste(CLEAN_TABLE);
    expect(nested()).toBe(false);
    const row = tables()[0].child(1);
    expect([row.child(0).textContent, row.child(1).textContent]).toEqual(['ben', 'dang']);
  });
});

describe('a table dropped on a table cell', () => {
  // Dragging cells from one column to another is where this shows up: the
  // drop arrives without the marker ProseMirror puts on its own drags, so the
  // cells read back as a whole table, and a whole table fits inside a cell.
  it('fills the cells from the one it was dropped on, rather than nesting', () => {
    drop(CLEAN_TABLE);
    expect(nested()).toBe(false);
    const row = tables()[0].child(1);
    expect([row.child(0).textContent, row.child(1).textContent]).toEqual(['ben', 'dang']);
  });

  it('does not nest when the drop carries more than the table', () => {
    drop(TABLE_AND_MORE);
    expect(nested()).toBe(false);
    expect(editor.getText()).toContain('ben dang');
  });
});

describe('everything else', () => {
  it('leaves a table pasted outside a table alone', () => {
    editor.commands.setTextSelection(editor.state.doc.content.size - 1);
    paste(CLEAN_TABLE);
    expect(nested()).toBe(false);
    expect(tables()).toHaveLength(2);
  });
});
